use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use crate::adoptium::{AdoptiumClient, host_adoptium_os, normalize_adoptium_architecture, plan_runtime_download};
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
    resolve_modlist, ModLoader, ResolutionResult, ResolutionTarget,
    FailureReason, RuleOutcome,
};
use crate::content_packs::{load_content_list, ContentEntry};
use crate::rules::{ModList, ModSource, Rule, VersionRuleKind, RULES_FILENAME};
use crate::token_storage::KeyringSecretStore;

use std::sync::Mutex;

pub const LAUNCH_PROGRESS_EVENT: &str = "launch-progress";
const DOWNLOAD_PROGRESS_EVENT: &str = "download-progress";
const LAUNCHER_ERROR_EVENT: &str = "launcher-error";

/// Tracks the PID of the currently running Minecraft process so it can be killed.
static ACTIVE_MC_PID: Mutex<Option<u32>> = Mutex::new(None);

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

/// Kills the currently running Minecraft process, if any.
#[tauri::command]
pub fn stop_minecraft_command() -> Result<(), String> {
    let pid = ACTIVE_MC_PID.lock().unwrap().take();
    if let Some(pid) = pid {
        #[cfg(target_os = "windows")]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/PID", &pid.to_string()])
                .output();
        }
        #[cfg(not(target_os = "windows"))]
        {
            unsafe { libc::kill(pid as i32, libc::SIGTERM); }
        }
        Ok(())
    } else {
        Err("No Minecraft process is running.".into())
    }
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

    let selected_mods = collect_selected_mods(&modlist, &resolution, &target);
    let compatible_versions =
        prefetch_compatible_versions_for_selected(&selected_mods, &modrinth_client, &target).await?;
    let parent_versions = collect_resolved_parent_versions(&selected_mods, &compatible_versions);
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
        &build_cached_mod_jars(&selected_mods, &all_remote_versions, &target, &launcher_paths, &modlist_name)?,
    )?;
    prepare_instance_config_directory(
        launcher_paths.configs_cache_dir(),
        &instance_config_dir,
        &Vec::<CachedConfigPlacement>::new(),
    )?;

    // Download and install content packs (resource packs, data packs, shaders)
    resolve_and_install_content_packs(
        &app_handle,
        &launcher_paths,
        &http_client,
        &modrinth_client,
        &modlist_name,
        &target,
        &instance_root,
    ).await?;

    let mut loader_metadata = LoaderMetadataClient::new()
        .fetch_loader_metadata(&target.minecraft_version, target.mod_loader)
        .await?;
    let loader_library_paths =
        materialize_loader_libraries(&http_client, &instance_library_dir, &loader_metadata).await?;

    extract_natives(&mc_data.native_paths, &instance_natives_dir)?;

    if target.mod_loader == ModLoader::Vanilla {
        loader_metadata.main_class = mc_data.main_class.clone();
        loader_metadata.jvm_arguments = mc_data.jvm_arguments.clone();
        loader_metadata.game_arguments = mc_data.game_arguments.clone();
    } else {
        // For modded loaders (Fabric, Forge, etc.): prepend essential MC game
        // arguments (auth, version, directories) but skip quickPlay entries
        // which require specific values and cause conflicts.
        let mut mc_args: Vec<String> = Vec::new();
        let mut skip_next = false;
        for arg in &mc_data.game_arguments {
            if skip_next {
                skip_next = false;
                continue;
            }
            if arg.starts_with("--quickPlay") || arg.starts_with("--demo") {
                skip_next = true; // skip the flag and its value
                continue;
            }
            mc_args.push(arg.clone());
        }
        mc_args.extend(loader_metadata.game_arguments.drain(..));
        loader_metadata.game_arguments = mc_args;
    }

    emit_progress(&app_handle, "resolving", 90, "Authenticating", "Refreshing Minecraft session...")?;
    let player_identity = load_player_identity(&launcher_paths).await?;
    emit_log(&app_handle, ProcessLogStream::Stdout,
        format!("[Auth] username={}, uuid={}, user_type={}, token_len={}",
            player_identity.username, player_identity.uuid, player_identity.user_type, player_identity.access_token.len()))?;
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

    let java_binary_path = select_or_download_java(&app_handle, &launcher_paths, &effective_settings, &target).await?;
    emit_log(
        &app_handle,
        ProcessLogStream::Stdout,
        format!("[Java] Using {}", java_binary_path.display()),
    )?;

    let prepared_command = build_launch_command(&JavaLaunchRequest {
        java_binary_path: java_binary_path.clone(),
        working_directory: instance_root.clone(),
        classpath_entries: {
            // Loader libraries take precedence over MC libraries when they
            // provide the same artifact (e.g. ASM). We deduplicate by jar
            // stem prefix: "asm-9.9.jar" and "asm-9.6.jar" share stem "asm",
            // so the loader version wins.
            let loader_artifacts: std::collections::HashSet<String> = loader_library_paths.iter()
                .filter_map(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| extract_artifact_name(n))
                })
                .collect();
            let mut entries: Vec<PathBuf> = mc_data.library_paths.iter()
                .filter(|p| {
                    let dominated = p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| loader_artifacts.contains(&extract_artifact_name(n)))
                        .unwrap_or(false);
                    !dominated
                })
                .cloned()
                .collect();
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

    let pid = process.pid;
    *ACTIVE_MC_PID.lock().unwrap() = Some(pid);

    emit_progress(
        &app_handle,
        "running",
        100,
        "Launching Minecraft",
        &format!("Minecraft process started with PID {}.", pid),
    )?;
    emit_log(
        &app_handle,
        ProcessLogStream::Stdout,
        format!("[Launch] Spawned Minecraft process with PID {}", pid),
    )?;

    // Wait for exit in background and clear the stored PID.
    let app_for_wait = app_handle.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _ = process.wait();
        *ACTIVE_MC_PID.lock().unwrap() = None;
        let _ = emit_progress(
            &app_for_wait,
            "idle",
            0,
            "Ready",
            "Minecraft has exited.",
        );
    });

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

/// Extracts the artifact name from a jar filename by stripping the version suffix.
/// e.g. "asm-9.6.jar" → "asm", "fabric-loader-0.16.jar" → "fabric-loader"
fn extract_artifact_name(filename: &str) -> String {
    let stem = filename.strip_suffix(".jar").unwrap_or(filename);
    // Find the last '-' followed by a digit — everything before it is the artifact name
    if let Some(pos) = stem.rfind(|c: char| c == '-').and_then(|i| {
        if stem[i + 1..].starts_with(|c: char| c.is_ascii_digit()) { Some(i) } else { None }
    }) {
        stem[..pos].to_string()
    } else {
        stem.to_string()
    }
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

/// Collect mods for launch.  When a primary resolves, only it is included.
/// When a primary fails, all of its DIRECT alternatives that individually
/// pass (exclude_if, requires, version_rules) are included.
fn collect_selected_mods(
    modlist: &ModList,
    resolution: &ResolutionResult,
    target: &ResolutionTarget,
) -> Vec<SelectedMod> {
    let mut selected = Vec::new();
    let active: HashSet<String> = resolution.active_mods.clone();

    for (i, resolved) in resolution.resolved_rules.iter().enumerate() {
        let Some(top_rule) = modlist.rules.get(i) else { continue };

        match &resolved.outcome {
            RuleOutcome::Resolved { resolved_id } if *resolved_id == top_rule.mod_id => {
                // Primary resolved — include only it.
                selected.push(SelectedMod {
                    mod_id: top_rule.mod_id.clone(),
                    source: top_rule.source.clone(),
                });
            }
            _ => {
                // Primary failed — include every DIRECT viable alternative.
                for alt in &top_rule.alternatives {
                    if alt_viable_for_launch(alt, &active, target) {
                        selected.push(SelectedMod {
                            mod_id: alt.mod_id.clone(),
                            source: alt.source.clone(),
                        });
                    }
                }
            }
        }
    }

    selected
}

fn alt_viable_for_launch(
    rule: &Rule,
    active_mods: &HashSet<String>,
    target: &ResolutionTarget,
) -> bool {
    use crate::rules::VersionRuleKind;
    if rule.exclude_if.iter().any(|id| active_mods.contains(id)) {
        return false;
    }
    if rule.requires.iter().any(|id| !active_mods.contains(id)) {
        return false;
    }
    for vr in &rule.version_rules {
        let version_matches = vr.mc_versions.iter().any(|v| v == &target.minecraft_version);
        let vr_loader = vr.loader.to_ascii_lowercase();
        let loader_matches =
            vr_loader == "any" || vr_loader == target.mod_loader.as_modrinth_loader();
        match vr.kind {
            VersionRuleKind::Only => {
                if !(version_matches && loader_matches) { return false; }
            }
            VersionRuleKind::Exclude => {
                if version_matches && loader_matches { return false; }
            }
        }
    }
    true
}

fn collect_resolved_parent_versions(
    selected_mods: &[SelectedMod],
    compatible_versions: &HashMap<String, ModrinthVersion>,
) -> Vec<ModrinthVersion> {
    let mut versions = Vec::new();
    let mut seen_projects = HashSet::new();

    for selected in selected_mods {
        if !matches!(selected.source, ModSource::Modrinth)
            || !seen_projects.insert(selected.mod_id.clone())
        {
            continue;
        }

        // Skip mods that have no compatible version on Modrinth for this
        // target — they were resolved by local rules but simply don't have
        // a release for this MC version + loader.
        if let Some(version) = compatible_versions.get(&selected.mod_id) {
            versions.push(version.clone());
        }
    }

    versions
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

// ── Content packs (resource packs, data packs, shaders) ─────────────────────

/// Checks whether a content entry is active for the current MC version + loader.
fn is_content_entry_active(entry: &ContentEntry, mc_version: &str, loader: &str) -> bool {
    for rule in &entry.version_rules {
        let version_match = rule.mc_versions.is_empty() || rule.mc_versions.iter().any(|v| v == mc_version);
        let loader_match = rule.loader == "any" || rule.loader.eq_ignore_ascii_case(loader);
        match rule.kind {
            VersionRuleKind::Exclude => { if version_match && loader_match { return false; } }
            VersionRuleKind::Only   => { if !(version_match && loader_match) { return false; } }
        }
    }
    true
}

/// Resolve, download and install content packs into the instance.
async fn resolve_and_install_content_packs(
    app_handle: &tauri::AppHandle,
    launcher_paths: &LauncherPaths,
    http_client: &reqwest::Client,
    modrinth_client: &ModrinthClient,
    modlist_name: &str,
    target: &ResolutionTarget,
    instance_root: &Path,
) -> Result<()> {
    let modlist_dir = launcher_paths.modlists_dir().join(modlist_name);
    let cache_dir = launcher_paths.content_packs_cache_dir();
    std::fs::create_dir_all(cache_dir)
        .with_context(|| format!("failed to create content packs cache at {}", cache_dir.display()))?;

    let mc_version = &target.minecraft_version;
    let loader_str = target.mod_loader.as_modrinth_loader();

    for (content_type, instance_subdir) in [
        ("resourcepack", "resourcepacks"),
        ("shader", "shaderpacks"),
    ] {
        let list = load_content_list(&modlist_dir, content_type).unwrap_or_else(|_| {
            crate::content_packs::ContentList {
                content_type: content_type.to_string(),
                entries: vec![],
                groups: vec![],
            }
        });

        // Filter active entries
        let active_entries: Vec<&ContentEntry> = list.entries.iter()
            .filter(|e| is_content_entry_active(e, mc_version, loader_str))
            .collect();

        if active_entries.is_empty() { continue; }

        let instance_dir = instance_root.join(instance_subdir);
        std::fs::create_dir_all(&instance_dir)
            .with_context(|| format!("failed to create {}", instance_dir.display()))?;
        // Clear existing content in instance dir
        crate::instance_mods::clear_instance_mods_directory(&instance_dir)?;

        for entry in &active_entries {
            if entry.source == "modrinth" {
                // Fetch latest compatible version from Modrinth
                match modrinth_client.fetch_content_pack_versions(&entry.id, mc_version).await {
                    Ok(versions) => {
                        // Pick the latest by date
                        let best = versions.into_iter()
                            .max_by(|a, b| a.date_published.cmp(&b.date_published));
                        if let Some(version) = best {
                            if let Some(file) = version.primary_file() {
                                let cached_path = cache_dir.join(&file.filename);
                                let was_cached = cached_path.exists();
                                if !was_cached {
                                    emit_log(app_handle, ProcessLogStream::Stdout,
                                        format!("[Content] Downloading {} ({})", entry.id, file.filename))?;
                                    download_file(http_client, &file.url, &cached_path).await
                                        .with_context(|| format!("failed to download content pack '{}'", entry.id))?;
                                }
                                let target_path = instance_dir.join(&file.filename);
                                crate::instance_mods::create_file_link(&cached_path, &target_path)
                                    .with_context(|| format!("failed to link content pack '{}' into instance", entry.id))?;
                                if was_cached {
                                    emit_log(app_handle, ProcessLogStream::Stdout,
                                        format!("[Content] {} → {} (cached)", entry.id, instance_subdir))?;
                                } else {
                                    emit_log(app_handle, ProcessLogStream::Stdout,
                                        format!("[Content] Downloaded {} → {}", entry.id, instance_subdir))?;
                                }
                            }
                        } else {
                            emit_log(app_handle, ProcessLogStream::Stdout,
                                format!("[Content] No compatible version found for '{}' on {}", entry.id, mc_version))?;
                        }
                    }
                    Err(e) => {
                        emit_log(app_handle, ProcessLogStream::Stdout,
                            format!("[Content] Failed to fetch versions for '{}': {}", entry.id, e))?;
                    }
                }
            }
            // Local content packs: would need to be handled if local upload is implemented
        }
    }

    // Data packs go into saves/*/datapacks — more complex because they're world-specific.
    // For now, place them in a top-level datapacks/ folder that some mods (e.g. Open Loader) support.
    {
        let list = load_content_list(&modlist_dir, "datapack").unwrap_or_else(|_| {
            crate::content_packs::ContentList {
                content_type: "datapack".to_string(),
                entries: vec![],
                groups: vec![],
            }
        });
        let active_entries: Vec<&ContentEntry> = list.entries.iter()
            .filter(|e| is_content_entry_active(e, mc_version, loader_str))
            .collect();

        if !active_entries.is_empty() {
            let instance_dir = instance_root.join("datapacks");
            std::fs::create_dir_all(&instance_dir)
                .with_context(|| format!("failed to create {}", instance_dir.display()))?;
            crate::instance_mods::clear_instance_mods_directory(&instance_dir)?;

            for entry in &active_entries {
                if entry.source == "modrinth" {
                    match modrinth_client.fetch_content_pack_versions(&entry.id, mc_version).await {
                        Ok(versions) => {
                            let best = versions.into_iter()
                                .max_by(|a, b| a.date_published.cmp(&b.date_published));
                            if let Some(version) = best {
                                if let Some(file) = version.primary_file() {
                                    let cached_path = cache_dir.join(&file.filename);
                                    let was_cached = cached_path.exists();
                                    if !was_cached {
                                        emit_log(app_handle, ProcessLogStream::Stdout,
                                            format!("[Content] Downloading {} ({})", entry.id, file.filename))?;
                                        download_file(http_client, &file.url, &cached_path).await
                                            .with_context(|| format!("failed to download data pack '{}'", entry.id))?;
                                    }
                                    let target_path = instance_dir.join(&file.filename);
                                    crate::instance_mods::create_file_link(&cached_path, &target_path)
                                        .with_context(|| format!("failed to link data pack '{}' into instance", entry.id))?;
                                    if was_cached {
                                        emit_log(app_handle, ProcessLogStream::Stdout,
                                            format!("[Content] {} → datapacks (cached)", entry.id))?;
                                    } else {
                                        emit_log(app_handle, ProcessLogStream::Stdout,
                                            format!("[Content] Downloaded {} → datapacks", entry.id))?;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            emit_log(app_handle, ProcessLogStream::Stdout,
                                format!("[Content] Failed to fetch versions for '{}': {}", entry.id, e))?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn build_cached_mod_jars(
    selected_mods: &[SelectedMod],
    versions: &[ModrinthVersion],
    target: &ResolutionTarget,
    launcher_paths: &LauncherPaths,
    modlist_name: &str,
) -> Result<Vec<CachedModJar>> {
    let mut jars = Vec::new();
    let mut seen = HashSet::new();
    let mods_cache_dir = launcher_paths.mods_cache_dir();
    let local_jars_dir = launcher_paths
        .modlists_dir()
        .join(modlist_name)
        .join("local-jars");

    // Local mods: JAR lives at local-jars/{mod_id}.jar — copy to cache/mods/
    for selected in selected_mods {
        if !matches!(selected.source, ModSource::Local) {
            continue;
        }

        let file_name = format!("{}.jar", selected.mod_id);
        if seen.insert(file_name.clone()) {
            let source = local_jars_dir.join(&file_name);
            let dest = mods_cache_dir.join(&file_name);
            if source.exists() && !dest.exists() {
                std::fs::create_dir_all(&mods_cache_dir).ok();
                std::fs::copy(&source, &dest).with_context(|| {
                    format!("failed to copy local JAR '{}' to mod cache", file_name)
                })?;
            }
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
        let relative_path = relative_loader_library_path(library)?;
        let destination_path = library_root.join(&relative_path);

        if !destination_path.exists() {
            // Determine the download URL: prefer explicit download artifact,
            // fall back to constructing from Maven base URL + coordinates.
            let download_url = if let Some(download) = &library.download {
                download.url.clone()
            } else if let Some(base_url) = &library.url {
                let base = base_url.trim_end_matches('/');
                format!("{base}/{}", relative_path.to_string_lossy().replace('\\', "/"))
            } else {
                continue; // no way to obtain this library
            };

            download_file(http_client, &download_url, &destination_path).await?;
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

async fn load_player_identity(launcher_paths: &LauncherPaths) -> Result<PlayerIdentity> {
    let connection = Connection::open(launcher_paths.database_path()).with_context(|| {
        format!(
            "failed to open launcher database at {}",
            launcher_paths.database_path().display()
        )
    })?;

    // Read the raw active account (no decryption needed — we use profile_data).
    // Extract all data from DB BEFORE any async work (Connection is not Send).
    let raw_account = {
        use crate::microsoft_auth::AccountsRepository as RawRepo;
        RawRepo::new(&connection).load_active_account().ok().flatten()
    };
    let db_path = launcher_paths.database_path().to_path_buf();
    drop(connection); // Release connection before async work.

    if let Some(raw) = raw_account {
        let profile = raw.profile_data.as_deref().and_then(|pd| {
            serde_json::from_str::<serde_json::Value>(pd).ok()
        });

        let username = profile.as_ref()
            .and_then(|v| v.get("username").and_then(|u| u.as_str()).map(String::from))
            .or(raw.xbox_gamertag.clone())
            .unwrap_or_else(|| "CubicPlayer".to_string());

        let uuid = format_uuid_with_dashes(
            &raw.minecraft_uuid.unwrap_or_else(|| deterministic_offline_uuid(&username).to_string())
        );

        let microsoft_id = raw.microsoft_id.clone();

        // Try to refresh the Minecraft access token using the stored MS refresh token.
        let ms_refresh = profile.as_ref()
            .and_then(|v| v.get("ms_refresh_token").and_then(|t| t.as_str()).map(String::from));

        if let Some(refresh_token) = ms_refresh {
            if !refresh_token.is_empty() {
                eprintln!("[Auth] Has refresh token (len={}), attempting refresh...", refresh_token.len());

                const DEFAULT_CLIENT_ID: &str = "00000000402b5328";
                let env_path = launcher_paths.root_dir().join(".env");
                let client_id = crate::microsoft_auth::microsoft_client_id_from_env(&env_path)
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string());

                let config = crate::microsoft_auth::MicrosoftOAuthConfig {
                    client_id,
                    redirect_uri: crate::microsoft_auth::DESKTOP_REDIRECT_URI.to_string(),
                    scopes: vec!["XboxLive.signin".into(), "offline_access".into()],
                };
                let oauth_client = crate::microsoft_auth::MicrosoftOAuthClient::new();

                match oauth_client.refresh_access_token(&config, &refresh_token).await {
                    Ok(ms_tokens) => {
                        eprintln!("[Auth] MS token refresh OK, authenticating with Xbox/MC...");
                        let chain = crate::microsoft_auth::MinecraftAuthChain::new();
                        match chain.authenticate(
                            &ms_tokens.access_token,
                            ms_tokens.refresh_token.as_deref(),
                            ms_tokens.user_id.as_deref(),
                        ).await {
                            Ok(login) => {
                                eprintln!("[Auth] Full auth chain OK: username={}", login.minecraft_username);
                        // Update stored tokens in profile_data.
                        let new_profile = serde_json::json!({
                            "username": login.minecraft_username,
                            "uuid": login.minecraft_uuid,
                            "mc_access_token": login.minecraft_access_token,
                            "ms_refresh_token": ms_tokens.refresh_token.as_deref()
                                .unwrap_or(&refresh_token),
                        });
                        if let Ok(conn) = Connection::open(&db_path) {
                            use crate::microsoft_auth::AccountsRepository as RawRepo;
                            let _ = RawRepo::new(&conn)
                                .update_profile_data(&microsoft_id, &new_profile.to_string());
                        }

                        return Ok(PlayerIdentity {
                            username: login.minecraft_username,
                            uuid: format_uuid_with_dashes(&login.minecraft_uuid),
                            access_token: login.minecraft_access_token,
                            user_type: "msa".to_string(),
                            version_type: "Cubic".to_string(),
                        });
                    }
                    Err(e) => eprintln!("[Auth] MC auth chain failed: {e:#}"),
                }
                }
                    Err(e) => eprintln!("[Auth] MS token refresh failed: {e:#}"),
                }
            }
        }

        eprintln!("[Auth] Falling back to offline mode");
        // No refresh token or refresh failed — offline with correct name.
        return Ok(PlayerIdentity {
            username,
            uuid,
            access_token: "0".to_string(),
            user_type: "offline".to_string(),
            version_type: "Cubic".to_string(),
        });
    }

    // No active account at all — fully offline.
    Ok(PlayerIdentity {
        username: "CubicPlayer".to_string(),
        uuid: deterministic_offline_uuid("CubicPlayer").to_string(),
        access_token: "0".to_string(),
        user_type: "offline".to_string(),
        version_type: "Cubic".to_string(),
    })
}

async fn select_or_download_java(
    app_handle: &tauri::AppHandle,
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
        if probe.version < required {
            bail!(
                "Java override '{}' reports version {}, but Minecraft {} requires Java {} or higher",
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

    if let Some(installation) = select_java_for_minecraft(&installations, &target.minecraft_version)? {
        return Ok(installation.path);
    }

    // No suitable Java found — auto-download via Adoptium.
    let required = required_java_version_for_minecraft(&target.minecraft_version)?;
    emit_log(
        app_handle,
        ProcessLogStream::Stdout,
        format!("[Java] No Java {} found, downloading from Adoptium...", required),
    )?;
    emit_progress(
        app_handle,
        "resolving",
        85,
        "Downloading Java",
        &format!("Fetching Java {} runtime from Adoptium.", required),
    )?;

    let adoptium = AdoptiumClient::new();
    let os = host_adoptium_os();
    let arch = normalize_adoptium_architecture(std::env::consts::ARCH);

    let package = adoptium
        .fetch_latest_jre_package(required, os, arch)
        .await?
        .with_context(|| format!("Adoptium has no JRE {} for {}/{}", required, os, arch))?;

    let plan = plan_runtime_download(
        launcher_paths.java_runtimes_dir(),
        required,
        package,
        os,
        arch,
    );

    // Download the archive.
    adoptium.download_package(&plan.package, &plan.archive_path).await?;

    // Extract the archive into the install directory.
    emit_log(
        app_handle,
        ProcessLogStream::Stdout,
        format!("[Java] Extracting to {}", plan.install_dir.display()),
    )?;
    let archive_path = plan.archive_path.clone();
    let install_dir = plan.install_dir.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let file = std::fs::File::open(&archive_path)
            .with_context(|| format!("failed to open Java archive {}", archive_path.display()))?;
        let mut archive = zip::ZipArchive::new(file)
            .with_context(|| format!("failed to read Java archive {}", archive_path.display()))?;
        archive.extract(&install_dir)
            .with_context(|| format!("failed to extract Java archive to {}", install_dir.display()))?;
        // Clean up archive file.
        let _ = std::fs::remove_file(&archive_path);
        Ok(())
    })
    .await
    .context("Java extraction task panicked")??;

    // Re-scan and select.
    let installations = discover_java_installations(launcher_paths.java_runtimes_dir())?;
    persist_java_installations(&connection, &installations)?;

    select_java_for_minecraft(&installations, &target.minecraft_version)?
        .map(|installation| installation.path)
        .with_context(|| {
            format!(
                "Java {} was downloaded but could not be found after extraction. Check {}",
                required,
                launcher_paths.java_runtimes_dir().display()
            )
        })
}

fn format_uuid_with_dashes(uuid: &str) -> String {
    let clean = uuid.replace('-', "");
    if clean.len() == 32 {
        format!("{}-{}-{}-{}-{}", &clean[0..8], &clean[8..12], &clean[12..16], &clean[16..20], &clean[20..32])
    } else {
        uuid.to_string()
    }
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
