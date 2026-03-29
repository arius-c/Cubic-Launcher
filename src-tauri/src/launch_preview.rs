use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use crate::app_shell::{load_shell_snapshot_from_root, ShellGlobalSettings, ShellModListOverrides};
use crate::minecraft_downloader::{ensure_minecraft_version, extract_natives};
use crate::dependencies::{resolve_required_dependencies_with_client, DependencyLink};
use crate::instance_configs::{prepare_instance_config_directory, CachedConfigPlacement};
use crate::instance_mods::{prepare_instance_mods_directory, CachedModJar};
use crate::java_runtime::{
    discover_java_installations, persist_java_installations, required_java_version_for_minecraft,
    select_java_for_minecraft, CommandJavaBinaryInspector, JavaBinaryInspector,
};
use crate::launch_command::{build_launch_command, JavaLaunchRequest, JavaLaunchSettings};
use crate::launcher_paths::LauncherPaths;
use crate::loader_metadata::{LoaderLibrary, LoaderMetadata, LoaderMetadataClient};
use crate::mod_cache::{
    build_mod_acquisition_plan, cache_record_from_version, SqliteModCacheRepository,
};
use crate::modrinth::{ModrinthClient, ModrinthVersion};
use crate::offline_account::{deterministic_offline_uuid, OfflineAccountService};
use crate::process_streaming::{
    spawn_and_stream_process, ProcessEventSink, ProcessLogEvent, ProcessLogStream,
    TauriProcessEventSink, MINECRAFT_LOG_EVENT,
};
use crate::resolver::{
    resolve_modlist, find_resolved_rule, ModLoader, ResolutionResult, ResolutionTarget,
    FailureReason, RuleOutcome,
};
use crate::rules::{ModList, ModSource, RULES_FILENAME};
use crate::token_storage::KeyringSecretStore;

pub const LAUNCH_PROGRESS_EVENT: &str = "launch-progress";
const DOWNLOAD_PROGRESS_EVENT: &str = "download-progress";
const LAUNCHER_ERROR_EVENT: &str = "launcher-error";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRequest {
    pub modlist_name: String,
    pub minecraft_version: String,
    pub mod_loader: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchProgressEvent {
    pub state: String,
    pub progress: u8,
    pub stage: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgressEvent {
    pub filename: String,
    pub percentage: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct LauncherErrorEvent {
    id: String,
    title: String,
    message: String,
    detail: String,
    severity: String,
    scope: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EffectiveLaunchSettings {
    min_ram_mb: u32,
    max_ram_mb: u32,
    custom_jvm_args: String,
    wrapper_command: Option<String>,
    java_path_override: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlayerIdentity {
    username: String,
    uuid: String,
    access_token: String,
    user_type: String,
    version_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LaunchPlaceholders {
    auth_player_name: String,
    version_name: String,
    game_directory: String,
    assets_root: String,
    assets_index_name: String,
    auth_uuid: String,
    auth_access_token: String,
    user_type: String,
    version_type: String,
    library_directory: String,
    natives_directory: String,
    launcher_name: String,
    launcher_version: String,
    classpath_separator: String,
}

/// A selected mod from resolution — carries mod_id + source for downstream processing.
#[derive(Debug, Clone)]
struct SelectedMod {
    mod_id: String,
    source: ModSource,
}

#[tauri::command]
pub fn start_launch_command(
    app_handle: tauri::AppHandle,
    launcher_paths: State<'_, LauncherPaths>,
    request: LaunchRequest,
) -> Result<(), String> {
    let launcher_paths = launcher_paths.inner().clone();

    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_launch_pipeline(app_handle.clone(), launcher_paths, request).await {
            let detail = format!("{error:#}");
            let _ = emit_log(
                &app_handle,
                ProcessLogStream::Stderr,
                format!("[Launch] {detail}"),
            );
            let _ = emit_progress(
                &app_handle,
                "idle",
                0,
                "Launch Aborted",
                "Launch preparation stopped before Minecraft could start.",
            );
            let _ = emit_launcher_error(
                &app_handle,
                "Launch failed",
                "The launcher could not finish preparing the selected mod list.",
                &detail,
            );
        }
    });

    Ok(())
}

async fn run_launch_pipeline(
    app_handle: tauri::AppHandle,
    launcher_paths: LauncherPaths,
    request: LaunchRequest,
) -> Result<()> {
    let target = ResolutionTarget {
        minecraft_version: request.minecraft_version.trim().to_string(),
        mod_loader: parse_mod_loader(&request.mod_loader)?,
    };
    let modlist_name = request.modlist_name.trim().to_string();
    anyhow::ensure!(!modlist_name.is_empty(), "modlist_name cannot be empty");

    emit_progress(
        &app_handle,
        "resolving",
        10,
        "Resolve Rules",
        &format!(
            "Evaluating '{}' for {} / {}.",
            modlist_name, target.minecraft_version, request.mod_loader
        ),
    )?;
    emit_log(
        &app_handle,
        ProcessLogStream::Stdout,
        format!(
            "[Launcher] Starting launch for '{}' on {} / {}",
            modlist_name, target.minecraft_version, request.mod_loader
        ),
    )?;

    let modlist = load_modlist(&launcher_paths, &modlist_name)?;
    let shell_snapshot =
        load_shell_snapshot_from_root(launcher_paths.root_dir(), Some(&modlist_name))?;
    let effective_settings = EffectiveLaunchSettings::from_shell_settings(
        &shell_snapshot.global_settings,
        &shell_snapshot.selected_modlist_overrides,
    );

    let modrinth_client = ModrinthClient::new();
    let http_client = reqwest::Client::new();

    let resolution = resolve_modlist(&modlist, &target)?;
    log_resolution(&app_handle, &resolution)?;

    let selected_mods = collect_selected_mods(&modlist, &resolution);
    let compatible_versions =
        prefetch_compatible_versions_for_selected(&selected_mods, &modrinth_client, &target).await?;
    let parent_versions = collect_resolved_parent_versions(&selected_mods, &compatible_versions)?;
    let dependency_resolution =
        resolve_required_dependencies_with_client(&parent_versions, &target, &modrinth_client)
            .await?;
    let dependency_versions = fetch_dependency_versions(
        &dependency_resolution.resolved_dependencies,
        &modrinth_client,
    )
    .await?;
    let all_remote_versions = deduplicate_versions(parent_versions, dependency_versions);

    emit_progress(
        &app_handle,
        "resolving",
        42,
        "Check Cache",
        "Inspecting cached mods and downloading missing dependencies.",
    )?;

    let acquisition_plan =
        build_remote_acquisition_plan(&launcher_paths, &all_remote_versions, &target)?;
    emit_log(
        &app_handle,
        ProcessLogStream::Stdout,
        format!(
            "[Cache] {} cached, {} pending download",
            acquisition_plan.cached.len(),
            acquisition_plan.to_download.len()
        ),
    )?;

    download_pending_artifacts(
        &app_handle,
        &http_client,
        launcher_paths.mods_cache_dir(),
        &acquisition_plan
            .to_download
            .iter()
            .map(|download| DownloadArtifact {
                filename: download.jar_filename.clone(),
                url: download.download_url.clone(),
                destination_path: launcher_paths.mods_cache_dir().join(&download.jar_filename),
            })
            .collect::<Vec<_>>(),
    )
    .await?;
    persist_remote_versions_and_dependencies(
        &launcher_paths,
        &all_remote_versions,
        &target,
        &dependency_resolution.links,
    )?;

    emit_progress(
        &app_handle,
        "resolving",
        58,
        "Download Minecraft",
        &format!(
            "Ensuring Minecraft {} client, libraries and assets are cached.",
            target.minecraft_version
        ),
    )?;

    let mc_data = ensure_minecraft_version(
        &http_client,
        &target.minecraft_version,
        &launcher_paths,
        |label, detail| {
            let _ = emit_log(
                &app_handle,
                ProcessLogStream::Stdout,
                format!("[MC] {label}: {detail}"),
            );
        },
    )
    .await?;

    emit_progress(
        &app_handle,
        "resolving",
        74,
        "Prepare Instance",
        "Refreshing mods, loader libraries, configs and launch metadata.",
    )?;

    let instance_root = build_instance_root(&launcher_paths, &modlist_name, &target);
    let instance_mods_dir = instance_root.join("mods");
    let instance_config_dir = instance_root.join("config");
    let instance_natives_dir = instance_root.join("natives");
    let instance_library_dir = instance_root.join("libraries");

    std::fs::create_dir_all(&instance_natives_dir)
        .with_context(|| format!("failed to create {}", instance_natives_dir.display()))?;

    prepare_instance_mods_directory(
        launcher_paths.mods_cache_dir(),
        &instance_mods_dir,
        &build_cached_mod_jars(&selected_mods, &all_remote_versions, &target)?,
    )?;
    prepare_instance_config_directory(
        launcher_paths.configs_cache_dir(),
        &instance_config_dir,
        &Vec::<CachedConfigPlacement>::new(),
    )?;

    let mut loader_metadata = LoaderMetadataClient::new()
        .fetch_loader_metadata(&target.minecraft_version, target.mod_loader)
        .await?;
    let loader_library_paths =
        materialize_loader_libraries(&http_client, &instance_library_dir, &loader_metadata).await?;

    extract_natives(&mc_data.native_paths, &instance_natives_dir)?;

    if target.mod_loader == ModLoader::Vanilla {
        loader_metadata.main_class = mc_data.main_class.clone();
        loader_metadata.game_arguments = mc_data.game_arguments.clone();
        loader_metadata.jvm_arguments = mc_data.jvm_arguments.clone();
    }

    let player_identity = load_player_identity(&launcher_paths)?;
    let placeholders = LaunchPlaceholders::new(
        &player_identity,
        &modlist_name,
        &target,
        &instance_root,
        &mc_data.assets_dir,
        &mc_data.asset_index_id,
        &instance_library_dir,
        &instance_natives_dir,
    );
    substitute_loader_placeholders(&mut loader_metadata, &placeholders);

    let java_binary_path = select_java_binary(&launcher_paths, &effective_settings, &target)?;
    emit_log(
        &app_handle,
        ProcessLogStream::Stdout,
        format!("[Java] Using {}", java_binary_path.display()),
    )?;

    let prepared_command = build_launch_command(&JavaLaunchRequest {
        java_binary_path: java_binary_path.clone(),
        working_directory: instance_root.clone(),
        classpath_entries: {
            let mut entries = mc_data.library_paths.clone();
            entries.extend(loader_library_paths);
            entries.push(mc_data.client_jar_path.clone());
            entries
        },
        loader_metadata,
        launch_settings: JavaLaunchSettings {
            min_ram_mb: effective_settings.min_ram_mb,
            max_ram_mb: effective_settings.max_ram_mb,
            custom_jvm_args: effective_settings.custom_jvm_args.clone(),
            profiler: None,
            wrapper_command: effective_settings.wrapper_command.clone(),
        },
        additional_game_arguments: Vec::new(),
        config_attribution: None,
    })?;

    let sink: Arc<dyn ProcessEventSink> = Arc::new(TauriProcessEventSink::new(app_handle.clone()));
    let process = spawn_and_stream_process(prepared_command, sink)?;

    emit_progress(
        &app_handle,
        "running",
        100,
        "Launching Minecraft",
        &format!("Minecraft process started with PID {}.", process.pid),
    )?;
    emit_log(
        &app_handle,
        ProcessLogStream::Stdout,
        format!(
            "[Launch] Spawned Minecraft process with PID {}",
            process.pid
        ),
    )?;

    Ok(())
}

impl EffectiveLaunchSettings {
    fn from_shell_settings(
        global: &ShellGlobalSettings,
        overrides: &ShellModListOverrides,
    ) -> Self {
        let wrapper_command = overrides
            .wrapper_command
            .clone()
            .unwrap_or_else(|| global.wrapper_command.clone())
            .trim()
            .to_string();
        let java_path_override = global.java_path_override.trim();

        Self {
            min_ram_mb: overrides.min_ram_mb.unwrap_or(global.min_ram_mb),
            max_ram_mb: overrides.max_ram_mb.unwrap_or(global.max_ram_mb),
            custom_jvm_args: overrides
                .custom_jvm_args
                .clone()
                .unwrap_or_else(|| global.custom_jvm_args.clone()),
            wrapper_command: if wrapper_command.is_empty() {
                None
            } else {
                Some(wrapper_command)
            },
            java_path_override: if java_path_override.is_empty() {
                None
            } else {
                Some(PathBuf::from(java_path_override))
            },
        }
    }
}

impl LaunchPlaceholders {
    fn new(
        player_identity: &PlayerIdentity,
        modlist_name: &str,
        target: &ResolutionTarget,
        game_directory: &Path,
        assets_root: &Path,
        asset_index_id: &str,
        library_directory: &Path,
        natives_directory: &Path,
    ) -> Self {
        Self {
            auth_player_name: player_identity.username.clone(),
            version_name: format!(
                "{}-{}-{}",
                modlist_name,
                target.minecraft_version,
                target.mod_loader.as_modrinth_loader()
            ),
            game_directory: game_directory.display().to_string(),
            assets_root: assets_root.display().to_string(),
            assets_index_name: asset_index_id.to_string(),
            auth_uuid: player_identity.uuid.clone(),
            auth_access_token: player_identity.access_token.clone(),
            user_type: player_identity.user_type.clone(),
            version_type: player_identity.version_type.clone(),
            library_directory: library_directory.display().to_string(),
            natives_directory: natives_directory.display().to_string(),
            launcher_name: "Cubic Launcher".to_string(),
            launcher_version: env!("CARGO_PKG_VERSION").to_string(),
            classpath_separator: if cfg!(target_os = "windows") {
                ";".to_string()
            } else {
                ":".to_string()
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DownloadArtifact {
    filename: String,
    url: String,
    destination_path: PathBuf,
}

fn parse_mod_loader(value: &str) -> Result<ModLoader> {
    match value.trim().to_ascii_lowercase().as_str() {
        "fabric" => Ok(ModLoader::Fabric),
        "quilt" => Ok(ModLoader::Quilt),
        "forge" => Ok(ModLoader::Forge),
        "neoforge" => Ok(ModLoader::NeoForge),
        "vanilla" => Ok(ModLoader::Vanilla),
        other => bail!("unsupported mod loader '{other}'"),
    }
}

fn load_modlist(launcher_paths: &LauncherPaths, modlist_name: &str) -> Result<ModList> {
    let rules_path = launcher_paths
        .modlists_dir()
        .join(modlist_name)
        .join(RULES_FILENAME);

    ModList::read_from_file(&rules_path).with_context(|| {
        format!(
            "failed to read mod-list '{}' from {}",
            modlist_name,
            rules_path.display()
        )
    })
}


/// Fetch compatible versions only for the mods that were actually selected by resolution.
async fn prefetch_compatible_versions_for_selected(
    selected_mods: &[SelectedMod],
    client: &ModrinthClient,
    target: &ResolutionTarget,
) -> Result<HashMap<String, ModrinthVersion>> {
    let mut versions = HashMap::new();

    for selected in selected_mods {
        if !matches!(selected.source, ModSource::Modrinth) {
            continue;
        }
        if versions.contains_key(&selected.mod_id) {
            continue;
        }
        if let Some(version) = client
            .fetch_latest_compatible_version(&selected.mod_id, target)
            .await?
        {
            versions.insert(selected.mod_id.clone(), version);
        }
    }

    Ok(versions)
}

fn log_resolution(app_handle: &tauri::AppHandle, resolution: &ResolutionResult) -> Result<()> {
    for rule in &resolution.resolved_rules {
        match &rule.outcome {
            RuleOutcome::Resolved { resolved_id } => emit_log(
                app_handle,
                ProcessLogStream::Stdout,
                format!(
                    "[Resolver] {} -> {}",
                    rule.mod_id,
                    resolved_id,
                ),
            )?,
            RuleOutcome::Unresolved { reason } => emit_log(
                app_handle,
                ProcessLogStream::Stdout,
                format!(
                    "[Resolver] {} unresolved ({})",
                    rule.mod_id,
                    describe_failure_reason(*reason)
                ),
            )?,
        }
    }

    Ok(())
}

fn describe_failure_reason(reason: FailureReason) -> &'static str {
    match reason {
        FailureReason::ExcludedByActiveMod => "excluded by already-selected mods",
        FailureReason::RequiredModMissing => "required mod not active",
        FailureReason::IncompatibleVersion => "incompatible version/loader",
        FailureReason::NoOptionAvailable => "no compatible option remained",
    }
}

/// Collect the actually-resolved mods from the resolution, looking up Rule data in the modlist.
fn collect_selected_mods(modlist: &ModList, resolution: &ResolutionResult) -> Vec<SelectedMod> {
    let mut selected = Vec::new();

    for (i, resolved) in resolution.resolved_rules.iter().enumerate() {
        if let Some(top_rule) = modlist.rules.get(i) {
            if let Some(actual_rule) = find_resolved_rule(top_rule, &resolved.outcome) {
                selected.push(SelectedMod {
                    mod_id: actual_rule.mod_id.clone(),
                    source: actual_rule.source.clone(),
                });
            }
        }
    }

    selected
}

fn collect_resolved_parent_versions(
    selected_mods: &[SelectedMod],
    compatible_versions: &HashMap<String, ModrinthVersion>,
) -> Result<Vec<ModrinthVersion>> {
    let mut versions = Vec::new();
    let mut seen_projects = HashSet::new();

    for selected in selected_mods {
        if !matches!(selected.source, ModSource::Modrinth)
            || !seen_projects.insert(selected.mod_id.clone())
        {
            continue;
        }

        let version = compatible_versions
            .get(&selected.mod_id)
            .with_context(|| {
                format!(
                    "resolved Modrinth project '{}' did not have a prefetched compatible version",
                    selected.mod_id
                )
            })?;
        versions.push(version.clone());
    }

    Ok(versions)
}

async fn fetch_dependency_versions(
    resolved_dependencies: &[crate::dependencies::ResolvedDependency],
    client: &ModrinthClient,
) -> Result<Vec<ModrinthVersion>> {
    let mut versions = Vec::new();
    let mut seen_version_ids = HashSet::new();

    for dependency in resolved_dependencies {
        let version_id = dependency.version_id.as_str();
        if !seen_version_ids.insert(version_id.to_string()) {
            continue;
        }

        let version = client
            .fetch_version(version_id)
            .await?
            .with_context(|| format!("dependency version '{}' could not be loaded", version_id))?;
        versions.push(version);
    }

    Ok(versions)
}

fn deduplicate_versions(
    parent_versions: Vec<ModrinthVersion>,
    dependency_versions: Vec<ModrinthVersion>,
) -> Vec<ModrinthVersion> {
    let mut versions = Vec::new();
    let mut seen = HashSet::new();

    for version in parent_versions.into_iter().chain(dependency_versions) {
        if seen.insert(version.id.clone()) {
            versions.push(version);
        }
    }

    versions
}

fn build_remote_acquisition_plan(
    launcher_paths: &LauncherPaths,
    versions: &[ModrinthVersion],
    target: &ResolutionTarget,
) -> Result<crate::mod_cache::ModAcquisitionPlan> {
    let connection = Connection::open(launcher_paths.database_path()).with_context(|| {
        format!(
            "failed to open launcher database at {}",
            launcher_paths.database_path().display()
        )
    })?;
    let repository = SqliteModCacheRepository::new(&connection, launcher_paths.mods_cache_dir());

    build_mod_acquisition_plan(versions, target, &repository)
}

async fn download_pending_artifacts(
    app_handle: &tauri::AppHandle,
    http_client: &reqwest::Client,
    default_directory: &Path,
    artifacts: &[DownloadArtifact],
) -> Result<()> {
    for artifact in artifacts {
        emit_download_progress(app_handle, &artifact.filename, 0)?;
        download_file(http_client, &artifact.url, &artifact.destination_path)
            .await
            .with_context(|| {
                format!(
                    "failed to download '{}' to {}",
                    artifact.url,
                    artifact.destination_path.display()
                )
            })?;
        emit_download_progress(app_handle, &artifact.filename, 100)?;
        emit_log(
            app_handle,
            ProcessLogStream::Stdout,
            format!(
                "[Download] Saved {} to {}",
                artifact.filename,
                artifact
                    .destination_path
                    .strip_prefix(default_directory)
                    .unwrap_or(&artifact.destination_path)
                    .display()
            ),
        )?;
    }

    Ok(())
}

async fn download_file(
    http_client: &reqwest::Client,
    url: &str,
    destination_path: &Path,
) -> Result<()> {
    let response = http_client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to request {url}"))?
        .error_for_status()
        .with_context(|| format!("download request failed for {url}"))?;
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("failed to read downloaded bytes from {url}"))?;

    if let Some(parent) = destination_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    tokio::fs::write(destination_path, bytes)
        .await
        .with_context(|| format!("failed to write {}", destination_path.display()))?;

    Ok(())
}

fn persist_remote_versions_and_dependencies(
    launcher_paths: &LauncherPaths,
    versions: &[ModrinthVersion],
    target: &ResolutionTarget,
    dependency_links: &[DependencyLink],
) -> Result<()> {
    let connection = Connection::open(launcher_paths.database_path()).with_context(|| {
        format!(
            "failed to open launcher database at {}",
            launcher_paths.database_path().display()
        )
    })?;
    let repository = SqliteModCacheRepository::new(&connection, launcher_paths.mods_cache_dir());

    for version in versions {
        repository.upsert_modrinth_version(version, target)?;
    }

    persist_dependency_links(&connection, dependency_links)
}

fn persist_dependency_links(connection: &Connection, links: &[DependencyLink]) -> Result<()> {
    let transaction = connection.unchecked_transaction()?;

    for link in links {
        transaction.execute(
            r#"
            INSERT INTO dependencies (
                mod_parent_id,
                dependency_id,
                dep_type,
                specific_version,
                jar_filename
            ) VALUES (?1, ?2, 'required', ?3, ?4)
            ON CONFLICT(mod_parent_id, dependency_id) DO UPDATE SET
                dep_type = excluded.dep_type,
                specific_version = excluded.specific_version,
                jar_filename = excluded.jar_filename
            "#,
            params![
                &link.parent_mod_id,
                &link.dependency_id,
                &link.specific_version,
                &link.jar_filename,
            ],
        )?;
    }

    transaction.commit()?;
    Ok(())
}

fn build_instance_root(
    launcher_paths: &LauncherPaths,
    modlist_name: &str,
    target: &ResolutionTarget,
) -> PathBuf {
    launcher_paths
        .modlists_dir()
        .join(modlist_name)
        .join("instances")
        .join(format!(
            "{}-{}",
            target.minecraft_version,
            target.mod_loader.as_modrinth_loader()
        ))
}

fn build_cached_mod_jars(
    selected_mods: &[SelectedMod],
    versions: &[ModrinthVersion],
    target: &ResolutionTarget,
) -> Result<Vec<CachedModJar>> {
    let mut jars = Vec::new();
    let mut seen = HashSet::new();

    // Local mods: JAR lives at local-jars/{mod_id}.jar
    for selected in selected_mods {
        if !matches!(selected.source, ModSource::Local) {
            continue;
        }

        let file_name = format!("{}.jar", selected.mod_id);
        if seen.insert(file_name.clone()) {
            jars.push(CachedModJar {
                jar_filename: file_name,
            });
        }
    }

    for version in versions {
        let jar_filename = cache_record_from_version(version, target)?.jar_filename;
        if seen.insert(jar_filename.clone()) {
            jars.push(CachedModJar { jar_filename });
        }
    }

    Ok(jars)
}

async fn materialize_loader_libraries(
    http_client: &reqwest::Client,
    library_root: &Path,
    loader_metadata: &LoaderMetadata,
) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();

    for library in &loader_metadata.libraries {
        let Some(download) = library.download.as_ref() else {
            continue;
        };
        let relative_path = relative_loader_library_path(library)?;
        let destination_path = library_root.join(relative_path);

        if !destination_path.exists() {
            download_file(http_client, &download.url, &destination_path).await?;
        }

        paths.push(destination_path);
    }

    Ok(paths)
}

fn relative_loader_library_path(library: &LoaderLibrary) -> Result<PathBuf> {
    if let Some(download) = &library.download {
        if let Some(path) = &download.path {
            return Ok(PathBuf::from(path));
        }
    }

    maven_artifact_relative_path(&library.name)
}

fn maven_artifact_relative_path(coordinates: &str) -> Result<PathBuf> {
    let parts = coordinates.split(':').collect::<Vec<_>>();
    if parts.len() < 3 {
        bail!("invalid Maven coordinates '{coordinates}'");
    }

    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).copied();
    let file_name = match classifier {
        Some(classifier) if !classifier.trim().is_empty() => {
            format!("{artifact}-{version}-{classifier}.jar")
        }
        _ => format!("{artifact}-{version}.jar"),
    };

    Ok(PathBuf::from(group)
        .join(artifact)
        .join(version)
        .join(file_name))
}

fn load_player_identity(launcher_paths: &LauncherPaths) -> Result<PlayerIdentity> {
    let connection = Connection::open(launcher_paths.database_path()).with_context(|| {
        format!(
            "failed to open launcher database at {}",
            launcher_paths.database_path().display()
        )
    })?;
    let offline_account = OfflineAccountService::new(&connection, KeyringSecretStore::new())
        .active_offline_account()?;

    let username = offline_account
        .as_ref()
        .map(|account| account.username.clone())
        .unwrap_or_else(|| "CubicPlayer".to_string());
    let uuid = offline_account
        .as_ref()
        .map(|account| account.offline_uuid.clone())
        .unwrap_or_else(|| deterministic_offline_uuid(&username).to_string());

    Ok(PlayerIdentity {
        username,
        uuid,
        access_token: "0".to_string(),
        user_type: "offline".to_string(),
        version_type: "Cubic".to_string(),
    })
}

fn select_java_binary(
    launcher_paths: &LauncherPaths,
    settings: &EffectiveLaunchSettings,
    target: &ResolutionTarget,
) -> Result<PathBuf> {
    if let Some(path) = &settings.java_path_override {
        let inspector = CommandJavaBinaryInspector;
        let probe = inspector
            .inspect(path)
            .with_context(|| format!("failed to inspect Java override at {}", path.display()))?
            .with_context(|| format!("Java override '{}' is not runnable", path.display()))?;
        let required = required_java_version_for_minecraft(&target.minecraft_version)?;
        if probe.version != required {
            bail!(
                "Java override '{}' reports version {}, but Minecraft {} requires Java {}",
                path.display(),
                probe.version,
                target.minecraft_version,
                required
            );
        }

        return Ok(path.clone());
    }

    let installations = discover_java_installations(launcher_paths.java_runtimes_dir())?;
    let connection = Connection::open(launcher_paths.database_path()).with_context(|| {
        format!(
            "failed to open launcher database at {}",
            launcher_paths.database_path().display()
        )
    })?;
    persist_java_installations(&connection, &installations)?;

    select_java_for_minecraft(&installations, &target.minecraft_version)?
        .map(|installation| installation.path)
        .with_context(|| {
            format!(
                "no suitable Java runtime was found for Minecraft {}. Set a Java Path Override in Settings.",
                target.minecraft_version
            )
        })
}

fn substitute_loader_placeholders(
    loader_metadata: &mut LoaderMetadata,
    placeholders: &LaunchPlaceholders,
) {
    loader_metadata.jvm_arguments = loader_metadata
        .jvm_arguments
        .iter()
        .map(|argument| substitute_known_placeholders(argument, placeholders))
        .collect();
    loader_metadata.game_arguments = loader_metadata
        .game_arguments
        .iter()
        .map(|argument| substitute_known_placeholders(argument, placeholders))
        .collect();
}

fn substitute_known_placeholders(argument: &str, placeholders: &LaunchPlaceholders) -> String {
    [
        (
            "${auth_player_name}",
            placeholders.auth_player_name.as_str(),
        ),
        ("${version_name}", placeholders.version_name.as_str()),
        ("${game_directory}", placeholders.game_directory.as_str()),
        ("${assets_root}", placeholders.assets_root.as_str()),
        (
            "${assets_index_name}",
            placeholders.assets_index_name.as_str(),
        ),
        ("${auth_uuid}", placeholders.auth_uuid.as_str()),
        (
            "${auth_access_token}",
            placeholders.auth_access_token.as_str(),
        ),
        ("${user_type}", placeholders.user_type.as_str()),
        ("${version_type}", placeholders.version_type.as_str()),
        (
            "${library_directory}",
            placeholders.library_directory.as_str(),
        ),
        (
            "${natives_directory}",
            placeholders.natives_directory.as_str(),
        ),
        ("${launcher_name}", placeholders.launcher_name.as_str()),
        (
            "${launcher_version}",
            placeholders.launcher_version.as_str(),
        ),
        (
            "${classpath_separator}",
            placeholders.classpath_separator.as_str(),
        ),
    ]
    .into_iter()
    .fold(argument.to_string(), |current, (needle, replacement)| {
        current.replace(needle, replacement)
    })
}

fn emit_progress(
    app_handle: &tauri::AppHandle,
    state: &str,
    progress: u8,
    stage: &str,
    detail: &str,
) -> Result<()> {
    app_handle
        .emit(
            LAUNCH_PROGRESS_EVENT,
            LaunchProgressEvent {
                state: state.to_string(),
                progress,
                stage: stage.to_string(),
                detail: detail.to_string(),
            },
        )
        .map_err(|error| anyhow::anyhow!(error.to_string()))
}

fn emit_download_progress(
    app_handle: &tauri::AppHandle,
    filename: &str,
    percentage: u8,
) -> Result<()> {
    app_handle
        .emit(
            DOWNLOAD_PROGRESS_EVENT,
            DownloadProgressEvent {
                filename: filename.to_string(),
                percentage,
            },
        )
        .map_err(|error| anyhow::anyhow!(error.to_string()))
}

fn emit_log(app_handle: &tauri::AppHandle, stream: ProcessLogStream, line: String) -> Result<()> {
    app_handle
        .emit(MINECRAFT_LOG_EVENT, ProcessLogEvent { stream, line })
        .map_err(|error| anyhow::anyhow!(error.to_string()))
}

fn emit_launcher_error(
    app_handle: &tauri::AppHandle,
    title: &str,
    message: &str,
    detail: &str,
) -> Result<()> {
    app_handle
        .emit(
            LAUNCHER_ERROR_EVENT,
            LauncherErrorEvent {
                id: unique_error_id(),
                title: title.to_string(),
                message: message.to_string(),
                detail: detail.to_string(),
                severity: "error".to_string(),
                scope: "launch".to_string(),
            },
        )
        .map_err(|error| anyhow::anyhow!(error.to_string()))
}

fn unique_error_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("launch-error-{timestamp}")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::app_shell::{ShellGlobalSettings, ShellModListOverrides};
    use crate::resolver::{ModLoader, ResolutionTarget};

    use super::{
        build_instance_root, maven_artifact_relative_path, parse_mod_loader,
        substitute_known_placeholders, EffectiveLaunchSettings, LaunchPlaceholders, PlayerIdentity,
    };

    fn global_settings() -> ShellGlobalSettings {
        ShellGlobalSettings {
            min_ram_mb: 2048,
            max_ram_mb: 4096,
            custom_jvm_args: "-Dglobal=true".into(),
            profiler_enabled: false,
            wrapper_command: "gamemoderun".into(),
            java_path_override: "/custom/java".into(),
        }
    }

    fn modlist_overrides() -> ShellModListOverrides {
        ShellModListOverrides {
            modlist_name: Some("Pack".into()),
            min_ram_mb: Some(8192),
            max_ram_mb: None,
            custom_jvm_args: Some("-Dmodlist=true".into()),
            profiler_enabled: Some(false),
            wrapper_command: Some("mangohud".into()),
            minecraft_version: None,
            mod_loader: None,
        }
    }

    #[test]
    fn parses_supported_mod_loader_names() {
        assert_eq!(parse_mod_loader("Fabric").unwrap(), ModLoader::Fabric);
        assert_eq!(parse_mod_loader("quilt").unwrap(), ModLoader::Quilt);
        assert_eq!(parse_mod_loader("Forge").unwrap(), ModLoader::Forge);
        assert_eq!(parse_mod_loader("NeoForge").unwrap(), ModLoader::NeoForge);
    }

    #[test]
    fn effective_launch_settings_prefer_modlist_overrides() {
        let settings =
            EffectiveLaunchSettings::from_shell_settings(&global_settings(), &modlist_overrides());

        assert_eq!(settings.min_ram_mb, 8192);
        assert_eq!(settings.max_ram_mb, 4096);
        assert_eq!(settings.custom_jvm_args, "-Dmodlist=true");
        assert_eq!(settings.wrapper_command, Some("mangohud".into()));
        assert_eq!(
            settings.java_path_override,
            Some(PathBuf::from("/custom/java"))
        );
    }

    #[test]
    fn maven_coordinates_expand_to_standard_artifact_path() {
        assert_eq!(
            maven_artifact_relative_path("net.fabricmc:fabric-loader:0.16.14").unwrap(),
            PathBuf::from("net/fabricmc/fabric-loader/0.16.14/fabric-loader-0.16.14.jar")
        );
        assert_eq!(
            maven_artifact_relative_path("org.lwjgl:lwjgl:3.3.3:natives-windows").unwrap(),
            PathBuf::from("org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar")
        );
    }

    #[test]
    fn instance_root_uses_version_and_loader_suffix() {
        let launcher_paths = crate::launcher_paths::LauncherPaths::new("workspace-root");
        let target = ResolutionTarget {
            minecraft_version: "1.21.1".into(),
            mod_loader: ModLoader::Fabric,
        };

        assert_eq!(
            build_instance_root(&launcher_paths, "Pack", &target),
            PathBuf::from("workspace-root")
                .join("mod-lists")
                .join("Pack")
                .join("instances")
                .join("1.21.1-fabric")
        );
    }

    #[test]
    fn known_launch_placeholders_are_substituted() {
        let placeholders = LaunchPlaceholders::new(
            &PlayerIdentity {
                username: "PlayerOne".into(),
                uuid: "uuid-123".into(),
                access_token: "token-abc".into(),
                user_type: "offline".into(),
                version_type: "Cubic".into(),
            },
            "Pack",
            &ResolutionTarget {
                minecraft_version: "1.21.1".into(),
                mod_loader: ModLoader::Fabric,
            },
            PathBuf::from("game-dir").as_path(),
            PathBuf::from("assets-root").as_path(),
            "1.21",
            PathBuf::from("libraries-root").as_path(),
            PathBuf::from("natives-root").as_path(),
        );

        let substituted = substitute_known_placeholders(
            "${auth_player_name}:${auth_uuid}:${game_directory}:${version_name}",
            &placeholders,
        );

        assert_eq!(
            substituted,
            "PlayerOne:uuid-123:game-dir:Pack-1.21.1-fabric"
        );
    }
}
