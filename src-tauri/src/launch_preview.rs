use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use crate::adoptium::{
    host_adoptium_os, normalize_adoptium_architecture, plan_runtime_download, AdoptiumClient,
};
use crate::app_shell::{load_shell_snapshot_from_root, ShellGlobalSettings, ShellModListOverrides};
use crate::content_packs::{load_content_list, ContentEntry};
use crate::dependencies::{
    collect_required_dependency_requests, resolve_required_dependencies_with_client,
    DependencyLink, DependencyRequest, DependencyResolution, DependencySelector,
    ResolvedDependency,
};
use crate::instance_configs::{prepare_instance_config_directory, CachedConfigPlacement};
use crate::instance_mods::{prepare_instance_mods_directory, CachedModJar};
use crate::java_runtime::{
    discover_java_installations, persist_java_installations, required_java_version_for_minecraft,
    select_java_for_requirement, CommandJavaBinaryInspector, JavaBinaryInspector,
};
use crate::launch_command::{build_launch_command, JavaLaunchRequest, JavaLaunchSettings};
use crate::launcher_paths::LauncherPaths;
use crate::loader_metadata::{LoaderLibrary, LoaderMetadata, LoaderMetadataClient};
use crate::minecraft_downloader::{ensure_minecraft_version, extract_natives};
use crate::mod_cache::{
    build_mod_acquisition_plan, cache_record_from_version, cached_artifact_path_for_pending_download,
    cached_artifact_path_for_record, cached_local_artifact_path, pending_download_from_version,
    ModAcquisitionPlan, ModCacheLookup, ModCacheRecord, SqliteModCacheRepository,
};
use crate::modrinth::{DependencyType, ModrinthClient, ModrinthVersion};
use crate::offline_account::deterministic_offline_uuid;
use crate::process_streaming::{
    spawn_and_stream_process, ProcessEventSink, ProcessExitEvent, ProcessLogEvent,
    ProcessLogStream, TauriProcessEventSink, MINECRAFT_LOG_EVENT,
};
use crate::resolver::{
    resolve_modlist, FailureReason, ModLoader, ResolutionResult, ResolutionTarget, RuleOutcome,
};
use crate::rules::{ModList, ModSource, Rule, VersionRuleKind, RULES_FILENAME};

use std::sync::Mutex;

pub const LAUNCH_PROGRESS_EVENT: &str = "launch-progress";
const LAUNCHER_ERROR_EVENT: &str = "launcher-error";

/// Tracks the PID of the currently running Minecraft process so it can be killed.
static ACTIVE_MC_PID: Mutex<Option<u32>> = Mutex::new(None);
static ACTIVE_LAUNCH_LOG_SESSION: Mutex<Option<Arc<LaunchLogSession>>> = Mutex::new(None);

#[derive(Debug)]
struct LaunchLogSession {
    dir: PathBuf,
}

#[derive(Clone)]
struct LoggingProcessEventSink {
    inner: TauriProcessEventSink,
    session: Arc<LaunchLogSession>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRequest {
    pub modlist_name: String,
    pub minecraft_version: String,
    pub mod_loader: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchVerificationRequest {
    pub modlist_name: String,
    pub minecraft_version: String,
    pub mod_loader: String,
    #[serde(default = "default_verification_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default = "default_success_after_seconds")]
    pub success_after_seconds: u64,
    #[serde(default = "default_terminate_on_success")]
    pub terminate_on_success: bool,
    #[serde(default = "default_terminate_on_timeout")]
    pub terminate_on_timeout: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchVerificationResult {
    pub started: bool,
    pub success: bool,
    pub state: String,
    pub pid: Option<u32>,
    pub launch_log_dir: Option<String>,
    pub duration_ms: u64,
    pub failure_kind: Option<String>,
    pub failure_summary: Option<String>,
    pub minecraft_log_tail: Vec<String>,
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
    cache_only_mode: bool,
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

impl SelectedMod {
    fn source_label(&self) -> &'static str {
        match self.source {
            ModSource::Modrinth => "modrinth",
            ModSource::Local => "local",
        }
    }
}

#[derive(Debug, Clone)]
struct TopLevelVersionCandidates {
    selected_mod_id: String,
    project_id: String,
    candidates: Vec<ModrinthVersion>,
}

#[derive(Debug, Clone)]
enum RemoteArtifact {
    Live(ModrinthVersion),
    Cached(ModCacheRecord),
}

#[derive(Debug, Clone)]
struct DependencyResolutionCandidate {
    parent_mod_id: String,
    selector: DependencySelector,
    resolved_dependency: ResolvedDependency,
    artifact: RemoteArtifact,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct EmbeddedFabricRequirementSet {
    minecraft_predicates: Vec<String>,
    java_predicates: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct EmbeddedFabricRequirements {
    root_entry: Option<EmbeddedFabricRequirementSet>,
    entries: Vec<EmbeddedFabricRequirementSet>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct EmbeddedFabricModMetadata {
    mod_id: String,
    version: String,
    provides: Vec<String>,
    depends: HashMap<String, Vec<String>>,
    breaks: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OwnedEmbeddedFabricModMetadata {
    owner_project_id: String,
    metadata: EmbeddedFabricModMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FabricValidationIssue {
    reason_code: &'static str,
    owner_project_id: String,
    mod_id: String,
    dependency_id: Option<String>,
    detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FabricIssueRetryState {
    signature: String,
    consecutive_attempts: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StartedLaunch {
    pid: u32,
    launch_log_dir: PathBuf,
}

fn default_verification_timeout_seconds() -> u64 {
    45
}

fn default_success_after_seconds() -> u64 {
    15
}

fn default_terminate_on_success() -> bool {
    true
}

fn default_terminate_on_timeout() -> bool {
    true
}

impl LaunchVerificationRequest {
    fn into_launch_request(self) -> LaunchRequest {
        LaunchRequest {
            modlist_name: self.modlist_name,
            minecraft_version: self.minecraft_version,
            mod_loader: self.mod_loader,
        }
    }
}

fn fabric_issue_signature(issue: &FabricValidationIssue) -> String {
    format!(
        "{}|{}|{}|{}",
        issue.reason_code,
        issue.owner_project_id,
        issue.mod_id,
        issue.dependency_id.as_deref().unwrap_or("-"),
    )
}

fn fabric_issue_reason_priority(reason_code: &str) -> u8 {
    match reason_code {
        "exact_dependency_conflict" => 0,
        "breaks_conflict" => 1,
        "incompatible_dependency_version" => 2,
        "missing_dependency" => 3,
        "embedded_version_incompatible" => 4,
        _ => 10,
    }
}

fn choose_primary_fabric_issue<'a>(
    issues: &'a HashMap<String, FabricValidationIssue>,
) -> Option<(&'a String, &'a FabricValidationIssue)> {
    let mut entries = issues.iter().collect::<Vec<_>>();
    entries.sort_by(|(left_project_id, left_issue), (right_project_id, right_issue)| {
        fabric_issue_reason_priority(left_issue.reason_code)
            .cmp(&fabric_issue_reason_priority(right_issue.reason_code))
            .then_with(|| left_project_id.cmp(right_project_id))
    });
    entries.into_iter().next()
}

impl LaunchLogSession {
    fn create(
        launcher_paths: &LauncherPaths,
        modlist_name: &str,
        target: &ResolutionTarget,
    ) -> Result<Arc<Self>> {
        let launch_id = format!(
            "{}-{}-{}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            sanitize_log_path_component(modlist_name),
            sanitize_log_path_component(&target.minecraft_version),
            sanitize_log_path_component(target.mod_loader.as_modrinth_loader()),
        );
        let dir = launcher_paths.launch_logs_dir().join(launch_id);
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create launch log directory {}", dir.display()))?;

        let session = Arc::new(Self { dir });
        session.append_summary_line(&format!("modlist={modlist_name}"))?;
        session.append_summary_line(&format!("minecraft_version={}", target.minecraft_version))?;
        session.append_summary_line(&format!(
            "mod_loader={}",
            target.mod_loader.as_modrinth_loader()
        ))?;
        session.append_summary_line(&format!("log_dir={}", session.dir.display()))?;
        Ok(session)
    }

    fn dir(&self) -> &Path {
        &self.dir
    }

    fn append_launcher_line(&self, stream: ProcessLogStream, line: &str) -> Result<()> {
        let stream_label = match stream {
            ProcessLogStream::Stdout => "stdout",
            ProcessLogStream::Stderr => "stderr",
        };
        self.append_line("all.log", &format!("[launcher:{stream_label}] {line}"))?;
        self.append_line("launcher.log", line)?;

        if let Some(category_file) = launcher_category_file(line) {
            self.append_line(category_file, line)?;
        }

        Ok(())
    }

    fn append_minecraft_log(&self, event: &ProcessLogEvent) -> Result<()> {
        let stream_label = match event.stream {
            ProcessLogStream::Stdout => "stdout",
            ProcessLogStream::Stderr => "stderr",
        };
        self.append_line(
            "all.log",
            &format!("[minecraft:{stream_label}] {}", event.line),
        )?;
        self.append_line("minecraft.log", &format!("[{stream_label}] {}", event.line))
    }

    fn append_exit_event(&self, event: &ProcessExitEvent) -> Result<()> {
        self.append_summary_line(&format!("minecraft_exit_success={}", event.success))?;
        self.append_summary_line(&format!(
            "minecraft_exit_code={}",
            event
                .exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "none".to_string())
        ))?;
        self.append_line(
            "all.log",
            &format!(
                "[minecraft:exit] success={} exit_code={}",
                event.success,
                event
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "none".to_string())
            ),
        )
    }

    fn write_selected_mods(&self, selected_mods: &[SelectedMod]) -> Result<()> {
        let mut lines = Vec::with_capacity(selected_mods.len() + 1);
        lines.push(format!("count={}", selected_mods.len()));
        for selected in selected_mods {
            lines.push(format!(
                "{} | source={}",
                selected.mod_id,
                selected.source_label()
            ));
        }
        self.write_file("selected_mods.log", &lines)
    }

    fn write_dependency_summary(&self, resolution: &DependencyResolution) -> Result<()> {
        let mut lines = vec![
            format!(
                "resolved_dependencies={}",
                resolution.resolved_dependencies.len()
            ),
            format!("links={}", resolution.links.len()),
            format!("excluded_parents={}", resolution.excluded_parents.len()),
            String::new(),
            "[resolved_dependencies]".to_string(),
        ];

        for dependency in &resolution.resolved_dependencies {
            lines.push(format!(
                "{} | version_id={} | jar={}",
                dependency.dependency_id, dependency.version_id, dependency.jar_filename
            ));
        }

        lines.push(String::new());
        lines.push("[links]".to_string());
        for link in &resolution.links {
            lines.push(format!(
                "{} -> {} | specific_version={} | jar={}",
                link.parent_mod_id,
                link.dependency_id,
                link.specific_version.as_deref().unwrap_or("-"),
                link.jar_filename
            ));
        }

        lines.push(String::new());
        lines.push("[excluded_parents]".to_string());
        let mut excluded = resolution
            .excluded_parents
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        excluded.sort();
        for parent in excluded {
            lines.push(parent);
        }

        self.write_file("dependencies.log", &lines)
    }

    fn write_resolved_versions(
        &self,
        remote_versions: &[ModrinthVersion],
        cached_records: &[ModCacheRecord],
    ) -> Result<()> {
        let mut lines = vec![
            format!("live_versions={}", remote_versions.len()),
            format!("cached_records={}", cached_records.len()),
            String::new(),
            "[live_versions]".to_string(),
        ];

        for version in remote_versions {
            let primary_file = version.primary_file();
            lines.push(format!(
                "{} | version_id={} | version_number={} | file={}",
                version.project_id,
                version.id,
                version.version_number,
                primary_file
                    .map(|file| file.filename.as_str())
                    .unwrap_or("<missing>")
            ));
        }

        lines.push(String::new());
        lines.push("[cached_records]".to_string());
        for record in cached_records {
            lines.push(format!(
                "{} | version_id={} | jar={} | local={}",
                record.modrinth_project_id,
                record.modrinth_version_id,
                record.jar_filename,
                record.is_local
            ));
        }

        self.write_file("resolved_versions.log", &lines)
    }

    fn write_cache_plan(&self, plan: &ModAcquisitionPlan) -> Result<()> {
        let mut lines = vec![
            format!("cached={}", plan.cached.len()),
            format!("to_download={}", plan.to_download.len()),
            String::new(),
            "[cached]".to_string(),
        ];

        for record in &plan.cached {
            lines.push(format!(
                "{} | version_id={} | jar={}",
                record.modrinth_project_id, record.modrinth_version_id, record.jar_filename
            ));
        }

        lines.push(String::new());
        lines.push("[to_download]".to_string());
        for pending in &plan.to_download {
            lines.push(format!(
                "{} | version_id={} | jar={}",
                pending.modrinth_project_id, pending.modrinth_version_id, pending.jar_filename
            ));
        }

        self.write_file("cache_plan.log", &lines)
    }

    fn write_final_mod_set(&self, jars: &[CachedModJar]) -> Result<()> {
        let mut lines = vec![format!("count={}", jars.len())];
        for jar in jars {
            lines.push(jar.jar_filename.clone());
        }
        self.write_file("final_mod_set.log", &lines)
    }

    fn append_summary_line(&self, line: &str) -> Result<()> {
        self.append_line("summary.log", line)
    }

    fn append_line(&self, file_name: &str, line: &str) -> Result<()> {
        let path = self.dir.join(file_name);
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        writeln!(file, "{line}").with_context(|| format!("failed to write {}", path.display()))
    }

    fn write_file(&self, file_name: &str, lines: &[String]) -> Result<()> {
        let path = self.dir.join(file_name);
        let mut content = lines.join("\n");
        content.push('\n');
        std::fs::write(&path, content)
            .with_context(|| format!("failed to write {}", path.display()))
    }
}

impl LoggingProcessEventSink {
    fn new(app_handle: tauri::AppHandle, session: Arc<LaunchLogSession>) -> Self {
        Self {
            inner: TauriProcessEventSink::new(app_handle),
            session,
        }
    }
}

impl ProcessEventSink for LoggingProcessEventSink {
    fn emit_log(&self, event: ProcessLogEvent) -> Result<()> {
        let _ = self.session.append_minecraft_log(&event);
        self.inner.emit_log(event)
    }

    fn emit_exit(&self, event: ProcessExitEvent) -> Result<()> {
        let _ = self.session.append_exit_event(&event);
        self.inner.emit_exit(event)
    }
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
            let _ = emit_launch_failure(&app_handle, &detail);
            set_active_launch_log_session(None);
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn verify_launch_command(
    app_handle: tauri::AppHandle,
    launcher_paths: State<'_, LauncherPaths>,
    request: LaunchVerificationRequest,
) -> Result<LaunchVerificationResult, String> {
    run_launch_verification(app_handle, launcher_paths.inner().clone(), request)
        .await
        .map_err(|error| error.to_string())
}

/// Kills the currently running Minecraft process, if any.
#[tauri::command]
pub fn stop_minecraft_command() -> Result<(), String> {
    let pid = ACTIVE_MC_PID.lock().unwrap().take();
    if let Some(pid) = pid {
        terminate_minecraft_pid(pid).map_err(|error| error.to_string())
    } else {
        Err("No Minecraft process is running.".into())
    }
}

pub fn maybe_start_automation_verifier(
    app_handle: tauri::AppHandle,
    launcher_paths: LauncherPaths,
) -> Result<()> {
    let request_json = match std::env::var("CUBIC_AUTOMATION_VERIFY_REQUEST") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => return Ok(()),
    };

    let request: LaunchVerificationRequest =
        serde_json::from_str(&request_json).context("failed to parse automation request JSON")?;
    let output_path = std::env::var("CUBIC_AUTOMATION_VERIFY_OUTPUT")
        .ok()
        .map(PathBuf::from);
    let should_exit = std::env::var("CUBIC_AUTOMATION_VERIFY_EXIT")
        .ok()
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    append_automation_trace(
        &launcher_paths,
        &format!(
            "automation bootstrap requested: output={} exit_on_failure={should_exit}",
            output_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<none>".to_string())
        ),
    );

    tauri::async_runtime::spawn(async move {
        append_automation_trace(&launcher_paths, "automation task started");
        let result = run_launch_pipeline(
            app_handle.clone(),
            launcher_paths.clone(),
            request.into_launch_request(),
        )
        .await;
        let launch_failed = result.is_err();

        let output_result = match result {
            Ok(started_launch) => {
                append_automation_trace(
                    &launcher_paths,
                    &format!(
                        "launch pipeline started minecraft: pid={} log_dir={}",
                        started_launch.pid,
                        started_launch.launch_log_dir.display()
                    ),
                );
                serde_json::to_string_pretty(&serde_json::json!({
                    "started": true,
                    "success": false,
                    "state": "started",
                    "pid": started_launch.pid,
                    "launchLogDir": started_launch.launch_log_dir.display().to_string(),
                }))
                .context("failed to serialize automation launch start")
            }
            Err(error) => {
                let detail = format!("{error:#}");
                let _ = emit_launch_failure(&app_handle, &detail);
                append_automation_trace(
                    &launcher_paths,
                    &format!("launch pipeline failed before spawn: {detail}"),
                );
                serde_json::to_string_pretty(&serde_json::json!({
                    "started": false,
                    "success": false,
                    "state": "launch_failed",
                    "failureKind": "launch_failed",
                    "failureSummary": detail,
                    "launchLogDir": current_launch_log_session().map(|session| session.dir().display().to_string()),
                }))
                .context("failed to serialize automation launch failure")
            }
        };

        if let Some(path) = output_path {
            if let Err(error) = write_automation_output_json(&path, output_result) {
                append_automation_trace(
                    &launcher_paths,
                    &format!(
                        "failed to write automation output {}: {error:#}",
                        path.display()
                    ),
                );
                eprintln!(
                    "failed to write automation verification output {}: {error:#}",
                    path.display()
                );
            } else {
                append_automation_trace(
                    &launcher_paths,
                    &format!("wrote automation output {}", path.display()),
                );
            }
        }

        if launch_failed {
            set_active_launch_log_session(None);
        }

        if should_exit && launch_failed {
            app_handle.exit(1);
        }
    });

    Ok(())
}

fn set_active_launch_log_session(session: Option<Arc<LaunchLogSession>>) {
    *ACTIVE_LAUNCH_LOG_SESSION.lock().unwrap() = session;
}

fn current_launch_log_session() -> Option<Arc<LaunchLogSession>> {
    ACTIVE_LAUNCH_LOG_SESSION.lock().unwrap().clone()
}

fn current_minecraft_pid() -> Option<u32> {
    *ACTIVE_MC_PID.lock().unwrap()
}

pub fn automation_mode_enabled() -> bool {
    std::env::var("CUBIC_AUTOMATION_VERIFY_REQUEST")
        .ok()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn append_automation_trace(launcher_paths: &LauncherPaths, message: &str) {
    let path = launcher_paths.logs_dir().join("automation.log");
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let line = format!("[{timestamp}] {message}\n");
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, line.as_bytes()));
}

fn emit_launch_failure(app_handle: &tauri::AppHandle, detail: &str) -> Result<()> {
    emit_log(
        app_handle,
        ProcessLogStream::Stderr,
        format!("[Launch] {detail}"),
    )?;
    emit_progress(
        app_handle,
        "idle",
        0,
        "Launch Aborted",
        "Launch preparation stopped before Minecraft could start.",
    )?;
    emit_launcher_issue(
        app_handle,
        "Launch failed",
        "The launcher could not finish preparing the selected mod list.",
        detail,
        "error",
        "launch",
    )
}

fn terminate_minecraft_pid(pid: u32) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let output = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .with_context(|| format!("failed to terminate Minecraft PID {pid}"))?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            bail!(
                "failed to terminate Minecraft PID {pid}: {}",
                if stderr.is_empty() {
                    "taskkill returned a non-zero exit code"
                } else {
                    &stderr
                }
            )
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let result = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
        if result == 0 {
            Ok(())
        } else {
            bail!("failed to terminate Minecraft PID {pid}")
        }
    }
}

async fn run_launch_verification(
    app_handle: tauri::AppHandle,
    launcher_paths: LauncherPaths,
    request: LaunchVerificationRequest,
) -> Result<LaunchVerificationResult> {
    anyhow::ensure!(
        current_minecraft_pid().is_none(),
        "cannot verify launch while another Minecraft process is already tracked"
    );

    let started_at = Instant::now();
    let timeout = Duration::from_secs(request.timeout_seconds.max(5));
    let success_after = Duration::from_secs(request.success_after_seconds.max(1));
    let launch_request = request.clone().into_launch_request();

    let started_launch = match run_launch_pipeline(app_handle.clone(), launcher_paths, launch_request).await
    {
        Ok(started_launch) => started_launch,
        Err(error) => {
            let detail = format!("{error:#}");
            let _ = emit_launch_failure(&app_handle, &detail);
            let result = build_failed_verification_result(
                started_at.elapsed(),
                current_launch_log_session(),
                None,
                "launch_failed",
                "launch_failed",
                &detail,
            )?;
            set_active_launch_log_session(None);
            return Ok(result);
        }
    };

    loop {
        let elapsed = started_at.elapsed();
        let log_tail = read_log_tail(&started_launch.launch_log_dir.join("minecraft.log"), 80)?;
        let summary_exit = summary_reports_minecraft_exit(&started_launch.launch_log_dir)?;

        if let Some((failure_kind, failure_summary)) = summarize_launch_failure(&log_tail) {
            if current_minecraft_pid() == Some(started_launch.pid) {
                let _ = terminate_minecraft_pid(started_launch.pid);
            }
            let result = finalize_verification_result(LaunchVerificationResult {
                started: true,
                success: false,
                state: "crashed".to_string(),
                pid: Some(started_launch.pid),
                launch_log_dir: Some(started_launch.launch_log_dir.display().to_string()),
                duration_ms: elapsed.as_millis() as u64,
                failure_kind: Some(failure_kind),
                failure_summary: Some(failure_summary),
                minecraft_log_tail: log_tail,
            })?;
            return Ok(result);
        }

        if current_minecraft_pid() != Some(started_launch.pid) || summary_exit.is_some() {
            let failure_summary = if let Some((_, exit_code)) = summary_exit {
                format!(
                    "Minecraft exited before the verification success window was reached{}.",
                    exit_code
                        .map(|code| format!(" (exit code {code})"))
                        .unwrap_or_default()
                )
            } else {
                "Minecraft exited before the verification success window was reached."
                    .to_string()
            };
            let result = finalize_verification_result(LaunchVerificationResult {
                started: true,
                success: false,
                state: "exited".to_string(),
                pid: Some(started_launch.pid),
                launch_log_dir: Some(started_launch.launch_log_dir.display().to_string()),
                duration_ms: elapsed.as_millis() as u64,
                failure_kind: Some("process_exited".to_string()),
                failure_summary: Some(failure_summary),
                minecraft_log_tail: log_tail,
            })?;
            return Ok(result);
        }

        if elapsed >= success_after && launch_appears_healthy(&log_tail) {
            if request.terminate_on_success {
                let _ = terminate_minecraft_pid(started_launch.pid);
            }
            let result = finalize_verification_result(LaunchVerificationResult {
                started: true,
                success: true,
                state: "running".to_string(),
                pid: Some(started_launch.pid),
                launch_log_dir: Some(started_launch.launch_log_dir.display().to_string()),
                duration_ms: elapsed.as_millis() as u64,
                failure_kind: None,
                failure_summary: None,
                minecraft_log_tail: log_tail,
            })?;
            return Ok(result);
        }

        if elapsed >= timeout {
            if request.terminate_on_timeout && current_minecraft_pid() == Some(started_launch.pid) {
                let _ = terminate_minecraft_pid(started_launch.pid);
            }
            let result = finalize_verification_result(LaunchVerificationResult {
                started: true,
                success: false,
                state: "timed_out".to_string(),
                pid: Some(started_launch.pid),
                launch_log_dir: Some(started_launch.launch_log_dir.display().to_string()),
                duration_ms: elapsed.as_millis() as u64,
                failure_kind: Some("timed_out".to_string()),
                failure_summary: Some(format!(
                    "Minecraft did not reach the verification success window within {} seconds.",
                    timeout.as_secs()
                )),
                minecraft_log_tail: log_tail,
            })?;
            return Ok(result);
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn sanitize_log_path_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => character,
            _ => '-',
        })
        .collect::<String>()
        .trim_matches('-')
        .to_ascii_lowercase();

    if sanitized.is_empty() {
        "launch".to_string()
    } else {
        sanitized
    }
}

fn launcher_category_file(line: &str) -> Option<&'static str> {
    if line.starts_with("[Resolver]") {
        Some("resolver.log")
    } else if line.starts_with("[Cache]") {
        Some("cache.log")
    } else if line.starts_with("[Dependencies]") {
        Some("dependencies.log")
    } else {
        None
    }
}

fn read_log_tail(path: &Path, max_lines: usize) -> Result<Vec<String>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content =
        std::fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut lines = content.lines().map(str::to_string).collect::<Vec<_>>();
    if lines.len() > max_lines {
        let split_point = lines.len() - max_lines;
        lines.drain(0..split_point);
    }
    Ok(lines)
}

fn summary_reports_minecraft_exit(log_dir: &Path) -> Result<Option<(bool, Option<String>)>> {
    let lines = read_log_tail(&log_dir.join("summary.log"), 30)?;
    let mut exit_success = None;
    let mut exit_code = None;

    for line in lines {
        if let Some(value) = line.strip_prefix("minecraft_exit_success=") {
            exit_success = Some(value.trim() == "true");
        } else if let Some(value) = line.strip_prefix("minecraft_exit_code=") {
            let trimmed = value.trim();
            if trimmed != "none" {
                exit_code = Some(trimmed.to_string());
            }
        }
    }

    Ok(exit_success.map(|success| (success, exit_code)))
}

fn summarize_launch_failure(lines: &[String]) -> Option<(String, String)> {
    let joined = lines.join("\n");
    let lowercase = joined.to_ascii_lowercase();

    if lowercase.contains("requires version")
        && lowercase.contains("which is missing")
        && lowercase.contains("mod '")
    {
        return Some((
            "missing_dependency".to_string(),
            "Fabric reported a missing required dependency in the selected mod set.".to_string(),
        ));
    }

    if lowercase.contains("requires version")
        && lowercase.contains("java")
        && lowercase.contains("wrong version is present")
    {
        return Some((
            "wrong_java".to_string(),
            "A selected mod requires a newer Java runtime than the launcher used.".to_string(),
        ));
    }

    if lowercase.contains("noclassdeffounderror")
        || (lowercase.contains("classnotfoundexception")
            && lowercase.contains("caused by:")
            && !lowercase.contains("error loading class:"))
    {
        return Some((
            "missing_class".to_string(),
            "Minecraft crashed because a required class was missing at runtime.".to_string(),
        ));
    }

    if lowercase.contains("nosuchmethoderror") {
        return Some((
            "method_mismatch".to_string(),
            "Minecraft crashed because two mods loaded incompatible method signatures.".to_string(),
        ));
    }

    if lowercase.contains("could not execute entrypoint stage")
        || lowercase.contains("exception caught from launcher")
    {
        return Some((
            "entrypoint_crash".to_string(),
            "Minecraft crashed while loading a mod entrypoint.".to_string(),
        ));
    }

    if lowercase.contains("incompatible mods found") {
        return Some((
            "incompatible_mods".to_string(),
            "Fabric rejected the selected mod set as incompatible.".to_string(),
        ));
    }

    None
}

fn launch_appears_healthy(lines: &[String]) -> bool {
    if lines.is_empty() || summarize_launch_failure(lines).is_some() {
        return false;
    }

    lines.iter().any(|line| {
        line.contains("[Render thread/")
            || line.contains("Loading Minecraft")
            || line.contains("OpenAL initialized")
            || line.contains("Reloading ResourceManager")
    })
}

fn write_verification_result(log_dir: &Path, result: &LaunchVerificationResult) -> Result<()> {
    let json = serde_json::to_string_pretty(result).context("failed to serialize verification result")?;
    std::fs::write(log_dir.join("verification.json"), json).with_context(|| {
        format!(
            "failed to write {}",
            log_dir.join("verification.json").display()
        )
    })
}

fn finalize_verification_result(
    result: LaunchVerificationResult,
) -> Result<LaunchVerificationResult> {
    if let Some(log_dir) = &result.launch_log_dir {
        let path = PathBuf::from(log_dir);
        write_verification_result(&path, &result)?;
        if let Some(session) = current_launch_log_session() {
            let _ = session.append_summary_line(&format!("verification_success={}", result.success));
            let _ = session.append_summary_line(&format!("verification_state={}", result.state));
            if let Some(failure_kind) = &result.failure_kind {
                let _ = session.append_summary_line(&format!("verification_failure_kind={failure_kind}"));
            }
            if let Some(failure_summary) = &result.failure_summary {
                let _ = session.append_summary_line(&format!(
                    "verification_failure_summary={failure_summary}"
                ));
            }
        }
    }

    Ok(result)
}

fn build_failed_verification_result(
    elapsed: Duration,
    session: Option<Arc<LaunchLogSession>>,
    pid: Option<u32>,
    state: &str,
    failure_kind: &str,
    failure_summary: &str,
) -> Result<LaunchVerificationResult> {
    let launch_log_dir = session.as_ref().map(|entry| entry.dir().display().to_string());
    let minecraft_log_tail = if let Some(entry) = &session {
        read_log_tail(&entry.dir().join("minecraft.log"), 80)?
    } else {
        Vec::new()
    };
    let result = LaunchVerificationResult {
        started: false,
        success: false,
        state: state.to_string(),
        pid,
        launch_log_dir,
        duration_ms: elapsed.as_millis() as u64,
        failure_kind: Some(failure_kind.to_string()),
        failure_summary: Some(failure_summary.to_string()),
        minecraft_log_tail,
    };
    finalize_verification_result(result)
}

fn write_automation_output_json(path: &Path, json: Result<String>) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let json = match json {
        Ok(value) => value,
        Err(error) => serde_json::to_string_pretty(&serde_json::json!({
            "started": false,
            "success": false,
            "state": "automation_error",
            "failureKind": "automation_error",
            "failureSummary": format!("{error:#}")
        }))
        .context("failed to serialize automation verification failure")?,
    };

    std::fs::write(path, json)
        .with_context(|| format!("failed to write automation result to {}", path.display()))
}

async fn run_launch_pipeline(
    app_handle: tauri::AppHandle,
    launcher_paths: LauncherPaths,
    request: LaunchRequest,
) -> Result<StartedLaunch> {
    let target = ResolutionTarget {
        minecraft_version: request.minecraft_version.trim().to_string(),
        mod_loader: parse_mod_loader(&request.mod_loader)?,
    };
    let modlist_name = request.modlist_name.trim().to_string();
    anyhow::ensure!(!modlist_name.is_empty(), "modlist_name cannot be empty");
    let launch_log_session = LaunchLogSession::create(&launcher_paths, &modlist_name, &target)?;
    set_active_launch_log_session(Some(launch_log_session.clone()));

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
    emit_log(
        &app_handle,
        ProcessLogStream::Stdout,
        format!(
            "[Launcher] Writing launch logs to {}",
            launch_log_session.dir().display()
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

    // Verify Modrinth availability for resolved mods. If a resolved mod has no
    // compatible version on Modrinth, temporarily disable it and re-resolve so
    // alternatives get a chance. In cache-only mode, a valid cached artifact
    // also counts as available.
    let resolution = if effective_settings.cache_only_mode {
        let selected = collect_selected_mods(&modlist, &resolution, &target);
        let available_artifacts =
            resolve_selected_remote_artifacts(&launcher_paths, &selected, &target).await?;
        let unavailable = selected
            .iter()
            .filter(|selected| matches!(selected.source, ModSource::Modrinth))
            .filter(|selected| !available_artifacts.contains_key(&selected.mod_id))
            .map(|selected| selected.mod_id.clone())
            .collect::<Vec<_>>();

        if unavailable.is_empty() {
            resolution
        } else {
            let mut patched = modlist.clone();
            for uid in &unavailable {
                if let Some(rule) = patched.find_rule_mut(uid) {
                    rule.enabled = false;
                }
            }
            resolve_modlist(&patched, &target)?
        }
    } else {
        let selected = collect_selected_mods(&modlist, &resolution, &target);
        let versions = prefetch_compatible_versions_for_selected(
            &app_handle,
            &launcher_paths,
            &http_client,
            &selected,
            &modrinth_client,
            &target,
        )
        .await?;
        let unavailable = selected
            .iter()
            .filter(|selected| matches!(selected.source, ModSource::Modrinth))
            .filter(|selected| !versions.contains_key(&selected.mod_id))
            .map(|selected| selected.mod_id.clone())
            .collect::<Vec<_>>();

        if unavailable.is_empty() {
            resolution
        } else {
            let mut patched = modlist.clone();
            for uid in &unavailable {
                if let Some(rule) = patched.find_rule_mut(uid) {
                    rule.enabled = false;
                }
            }
            resolve_modlist(&patched, &target)?
        }
    };
    log_resolution(&app_handle, &resolution)?;

    let selected_mods = collect_selected_mods(&modlist, &resolution, &target);
    launch_log_session.write_selected_mods(&selected_mods)?;
    if selected_mods.len() > 300 {
        let detail = if effective_settings.cache_only_mode {
            "You have more than 300 mods. The first launch may still take 1-2 minutes, but cache-only mode is already enabled for future launches."
        } else {
            "You have more than 300 mods. The first launch may take 1-2 minutes due to API rate limits. You can enable \"Cache-Only Mode\" in Settings to skip API checks on future launches."
        };
        emit_launcher_issue(
            &app_handle,
            "Large modpack detected",
            "Launch preparation may take longer than usual.",
            detail,
            "warning",
            "launch",
        )?;
    }

    let (
        all_remote_versions,
        cached_remote_records,
        dependency_resolution,
        effective_required_java,
    ) = if effective_settings.cache_only_mode {
        emit_log(
            &app_handle,
            ProcessLogStream::Stdout,
            "[Cache] Cache-only mode enabled. Reusing cached artifacts only; uncached mods and dependencies will be skipped."
                .to_string(),
        )?;

        let parent_artifacts =
            resolve_selected_remote_artifacts(&launcher_paths, &selected_mods, &target).await?;
        let parent_artifact_values = parent_artifacts.into_values().collect::<Vec<_>>();
        let (parent_versions, cached_parent_records) =
            split_remote_artifacts(&parent_artifact_values);
        let selected_project_ids = collect_selected_project_ids(&parent_versions);

        let mut dependency_requests = collect_required_dependency_requests(&parent_versions)?;
        let cached_parent_ids = cached_parent_records
            .iter()
            .map(|record| record.modrinth_project_id.clone())
            .collect::<Vec<_>>();
        dependency_requests.extend(load_cached_dependency_requests(
            &launcher_paths,
            &cached_parent_ids,
        )?);

        let (dependency_resolution, dependency_artifacts) =
            resolve_dependency_requests_with_cache_fallback(
                &launcher_paths,
                &dependency_requests,
                &selected_project_ids,
                &target,
            )
            .await?;

        if !dependency_resolution.excluded_parents.is_empty() {
            for excluded_id in &dependency_resolution.excluded_parents {
                emit_log(
                        &app_handle,
                        ProcessLogStream::Stdout,
                        format!(
                            "[Launch] skipping mod '{}': a required dependency has no compatible version for this target",
                            excluded_id
                        ),
                    )?;
            }
        }

        let filtered_parent_artifacts = parent_artifact_values
            .into_iter()
            .filter(|artifact| {
                !dependency_resolution
                    .excluded_parents
                    .contains(remote_artifact_project_id(artifact))
            })
            .collect::<Vec<_>>();
        let mut all_artifacts = filtered_parent_artifacts;
        all_artifacts.extend(dependency_artifacts);
        let (live_versions, cached_records) = split_remote_artifacts(&all_artifacts);
        (
            live_versions,
            cached_records,
            dependency_resolution,
            required_java_version_for_minecraft(&target.minecraft_version)?,
        )
    } else {
        let compatible_versions = prefetch_compatible_versions_for_selected(
            &app_handle,
            &launcher_paths,
            &http_client,
            &selected_mods,
            &modrinth_client,
            &target,
        )
        .await?;
        let parent_versions = selected_mods
            .iter()
            .filter(|selected| matches!(selected.source, ModSource::Modrinth))
            .filter_map(|selected| compatible_versions.get(&selected.mod_id))
            .cloned()
            .collect::<Vec<_>>();
        let selected_project_ids = collect_selected_project_ids(&parent_versions);
        let mut dependency_resolution = resolve_required_dependencies_with_client(
            &parent_versions,
            &target,
            &modrinth_client,
            &selected_project_ids,
        )
        .await?;
        for excluded_id in &dependency_resolution.excluded_parents {
            emit_log(
                &app_handle,
                ProcessLogStream::Stdout,
                format!(
                    "[Launch] skipping mod '{}': a required dependency has no compatible version for this target",
                    excluded_id
                ),
            )?;
        }
        let final_parent_versions = parent_versions
            .into_iter()
            .filter(|version| !dependency_resolution.excluded_parents.contains(&version.project_id))
            .collect::<Vec<_>>();
        let allowed_parent_ids = final_parent_versions
            .iter()
            .map(|version| version.project_id.clone())
            .collect::<HashSet<_>>();
        dependency_resolution
            .links
            .retain(|link| allowed_parent_ids.contains(&link.parent_mod_id));
        let allowed_dependency_ids = dependency_resolution
            .links
            .iter()
            .map(|link| link.dependency_id.clone())
            .collect::<HashSet<_>>();
        dependency_resolution
            .resolved_dependencies
            .retain(|dependency| allowed_dependency_ids.contains(&dependency.dependency_id));
        let mut final_dependency_versions = fetch_dependency_versions(
            &dependency_resolution.resolved_dependencies,
            &modrinth_client,
        )
        .await?;
        final_dependency_versions
            .retain(|version| allowed_dependency_ids.contains(&version.project_id));
        launch_log_session.append_summary_line(&format!(
            "excluded_top_level_mods={}",
            dependency_resolution.excluded_parents.len()
        ))?;
        (
            deduplicate_versions(final_parent_versions, final_dependency_versions),
            Vec::new(),
            dependency_resolution,
            required_java_version_for_minecraft(&target.minecraft_version)?,
        )
    };
    launch_log_session.write_dependency_summary(&dependency_resolution)?;
    launch_log_session.write_resolved_versions(&all_remote_versions, &cached_remote_records)?;
    launch_log_session.append_summary_line(&format!(
        "cache_only_mode={}",
        effective_settings.cache_only_mode
    ))?;
    launch_log_session.append_summary_line(&format!("selected_mods={}", selected_mods.len()))?;
    launch_log_session.append_summary_line(&format!(
        "resolved_remote_versions={}",
        all_remote_versions.len()
    ))?;
    launch_log_session.append_summary_line(&format!(
        "resolved_cached_records={}",
        cached_remote_records.len()
    ))?;
    launch_log_session.append_summary_line(&format!(
        "required_java_before_loader={}",
        effective_required_java
    ))?;

    emit_progress(
        &app_handle,
        "resolving",
        42,
        "Check Cache",
        "Inspecting cached mods and downloading missing dependencies.",
    )?;

    let acquisition_plan = if effective_settings.cache_only_mode {
        build_remote_acquisition_plan_from_artifacts(
            &launcher_paths,
            &all_remote_versions,
            &cached_remote_records,
            &target,
        )?
    } else {
        build_remote_acquisition_plan(&launcher_paths, &all_remote_versions, &target)?
    };
    emit_log(
        &app_handle,
        ProcessLogStream::Stdout,
        format!(
            "[Cache] {} cached, {} pending download",
            acquisition_plan.cached.len(),
            acquisition_plan.to_download.len()
        ),
    )?;
    launch_log_session.write_cache_plan(&acquisition_plan)?;

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
                destination_path: cached_artifact_path_for_pending_download(
                    launcher_paths.mods_cache_dir(),
                    download,
                ),
                file_hash: download.file_hash.clone(),
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

    let cached_mod_jars = build_cached_mod_jars(
        &app_handle,
        &selected_mods,
        &all_remote_versions,
        &cached_remote_records,
        &target,
        &launcher_paths,
        &modlist_name,
    )?;
    launch_log_session.write_final_mod_set(&cached_mod_jars)?;
    prepare_instance_mods_directory(
        launcher_paths.mods_cache_dir(),
        &instance_mods_dir,
        &cached_mod_jars,
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
    )
    .await?;

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

    emit_progress(
        &app_handle,
        "resolving",
        90,
        "Authenticating",
        "Refreshing Minecraft session...",
    )?;
    let player_identity = load_player_identity(&launcher_paths).await?;
    emit_log(
        &app_handle,
        ProcessLogStream::Stdout,
        format!(
            "[Auth] username={}, uuid={}, user_type={}, token_len={}",
            player_identity.username,
            player_identity.uuid,
            player_identity.user_type,
            player_identity.access_token.len()
        ),
    )?;
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

    let effective_required_java =
        effective_required_java.max(required_java_for_cached_mod_jars(&cached_mod_jars)?);
    let effective_required_java = effective_required_java.max(
        loader_metadata
            .min_java_version
            .unwrap_or(effective_required_java),
    );
    emit_log(
        &app_handle,
        ProcessLogStream::Stdout,
        format!(
            "[Java] Effective runtime requirement is Java {}",
            effective_required_java
        ),
    )?;
    launch_log_session.append_summary_line(&format!(
        "required_java_after_loader={}",
        effective_required_java
    ))?;
    let java_binary_path = select_or_download_java(
        &app_handle,
        &launcher_paths,
        &effective_settings,
        &target,
        effective_required_java,
    )
    .await?;
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
            let loader_artifacts: std::collections::HashSet<String> = loader_library_paths
                .iter()
                .filter_map(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| extract_artifact_name(n))
                })
                .collect();
            let mut entries: Vec<PathBuf> = mc_data
                .library_paths
                .iter()
                .filter(|p| {
                    let dominated = p
                        .file_name()
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

    let sink: Arc<dyn ProcessEventSink> = Arc::new(LoggingProcessEventSink::new(
        app_handle.clone(),
        launch_log_session.clone(),
    ));
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
        set_active_launch_log_session(None);
        let _ = emit_progress(&app_for_wait, "idle", 0, "Ready", "Minecraft has exited.");
    });

    Ok(StartedLaunch {
        pid,
        launch_log_dir: launch_log_session.dir().to_path_buf(),
    })
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

        let cache_only_mode = automation_cache_only_override().unwrap_or(global.cache_only_mode);

        Self {
            min_ram_mb: overrides.min_ram_mb.unwrap_or(global.min_ram_mb),
            max_ram_mb: overrides.max_ram_mb.unwrap_or(global.max_ram_mb),
            custom_jvm_args: overrides
                .custom_jvm_args
                .clone()
                .unwrap_or_else(|| global.custom_jvm_args.clone()),
            cache_only_mode,
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

fn automation_cache_only_override() -> Option<bool> {
    let value = std::env::var("CUBIC_AUTOMATION_CACHE_ONLY_MODE").ok()?;
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" => Some(true),
        "0" | "false" | "off" => Some(false),
        _ => None,
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
    file_hash: Option<String>,
}

/// Extracts the artifact name from a jar filename by stripping the version suffix.
/// e.g. "asm-9.6.jar" → "asm", "fabric-loader-0.16.jar" → "fabric-loader"
fn extract_artifact_name(filename: &str) -> String {
    let stem = filename.strip_suffix(".jar").unwrap_or(filename);
    // Find the last '-' followed by a digit — everything before it is the artifact name
    if let Some(pos) = stem.rfind(|c: char| c == '-').and_then(|i| {
        if stem[i + 1..].starts_with(|c: char| c.is_ascii_digit()) {
            Some(i)
        } else {
            None
        }
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
/// Network/API errors for individual mods are treated as "no compatible version"
/// so that the re-resolution pass can disable them and try alternatives.
async fn prefetch_compatible_versions_for_selected(
    app_handle: &tauri::AppHandle,
    _launcher_paths: &LauncherPaths,
    _http_client: &reqwest::Client,
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
        match client.fetch_project_versions(&selected.mod_id, target).await {
            Ok(candidate_versions) => {
                if let Some(version) = candidate_versions.into_iter().next() {
                versions.insert(selected.mod_id.clone(), version);
                }
            }
            Err(err) => {
                let _ = emit_log(
                    app_handle,
                    ProcessLogStream::Stderr,
                    format!(
                        "[Launch] skipping mod '{}': failed to query Modrinth ({:#})",
                        selected.mod_id, err
                    ),
                );
            }
        }
    }

    Ok(versions)
}

async fn prefetch_ranked_versions_for_selected(
    app_handle: &tauri::AppHandle,
    selected_mods: &[SelectedMod],
    client: &ModrinthClient,
    target: &ResolutionTarget,
) -> Result<HashMap<String, Vec<ModrinthVersion>>> {
    let mut versions = HashMap::new();

    for selected in selected_mods {
        if !matches!(selected.source, ModSource::Modrinth) || versions.contains_key(&selected.mod_id)
        {
            continue;
        }

        match client.fetch_project_versions(&selected.mod_id, target).await {
            Ok(mut candidate_versions) => {
                crate::modrinth::sort_versions_by_target_preference(&mut candidate_versions, target);
                if !candidate_versions.is_empty() {
                    versions.insert(selected.mod_id.clone(), candidate_versions);
                }
            }
            Err(err) => {
                let _ = emit_log(
                    app_handle,
                    ProcessLogStream::Stderr,
                    format!(
                        "[Launch] skipping mod '{}': failed to query Modrinth ({:#})",
                        selected.mod_id, err
                    ),
                );
            }
        }
    }

    Ok(versions)
}

async fn select_latest_launch_compatible_version(
    app_handle: &tauri::AppHandle,
    launcher_paths: &LauncherPaths,
    http_client: &reqwest::Client,
    client: &ModrinthClient,
    project_id_or_slug: &str,
    target: &ResolutionTarget,
) -> Result<Option<ModrinthVersion>> {
    Ok(select_launch_compatible_versions(
        app_handle,
        launcher_paths,
        http_client,
        client,
        project_id_or_slug,
        target,
    )
    .await?
    .into_iter()
    .next())
}

async fn select_launch_compatible_versions(
    app_handle: &tauri::AppHandle,
    launcher_paths: &LauncherPaths,
    http_client: &reqwest::Client,
    client: &ModrinthClient,
    project_id_or_slug: &str,
    target: &ResolutionTarget,
) -> Result<Vec<ModrinthVersion>> {
    let mut versions = client
        .fetch_project_versions(project_id_or_slug, target)
        .await?;
    crate::modrinth::sort_versions_by_target_preference(&mut versions, target);

    let mut compatible_versions = Vec::new();
    for version in versions {
        let jar_path = ensure_remote_version_cached(http_client, launcher_paths, &version, target)
            .await
            .with_context(|| {
                format!(
                    "failed to cache '{}' candidate version '{}'",
                    project_id_or_slug, version.id
                )
            })?;
        let requirements = read_embedded_fabric_requirements(&jar_path)?;
        if requirements.entries.is_empty()
            || embedded_minecraft_requirements_match(&requirements, target)
        {
            compatible_versions.push(version);
            continue;
        }

        let _ = emit_log(
            app_handle,
            ProcessLogStream::Stdout,
            format!(
                "[Launch] skipping remote version '{}' for '{}': embedded metadata is incompatible with {} / {}",
                version.version_number,
                project_id_or_slug,
                target.minecraft_version,
                target.mod_loader.as_modrinth_loader()
            ),
        );
    }

    Ok(compatible_versions)
}

async fn ensure_remote_version_cached(
    http_client: &reqwest::Client,
    launcher_paths: &LauncherPaths,
    version: &ModrinthVersion,
    target: &ResolutionTarget,
) -> Result<PathBuf> {
    let record = cache_record_from_version(version, target)?;
    let destination_path =
        cached_artifact_path_for_record(launcher_paths.mods_cache_dir(), &record);
    if destination_path.exists() {
        return Ok(destination_path);
    }

    let file = version.primary_file().with_context(|| {
        format!(
            "version '{}' for project '{}' does not expose a primary file",
            version.id, version.project_id
        )
    })?;
    match file.hashes.get("sha1").map(String::as_str) {
        Some(hash) => {
            crate::minecraft_downloader::download_file_verified(
                http_client,
                &file.url,
                &destination_path,
                hash,
            )
            .await?
        }
        None => download_file(http_client, &file.url, &destination_path).await?,
    }

    Ok(destination_path)
}

fn log_resolution(app_handle: &tauri::AppHandle, resolution: &ResolutionResult) -> Result<()> {
    for rule in &resolution.resolved_rules {
        match &rule.outcome {
            RuleOutcome::Resolved { resolved_id } => emit_log(
                app_handle,
                ProcessLogStream::Stdout,
                format!("[Resolver] {} -> {}", rule.mod_id, resolved_id,),
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

/// Collect mods for launch.  When a rule resolves (primary or alternative),
/// include whichever mod was selected by the resolver.
fn collect_selected_mods(
    modlist: &ModList,
    resolution: &ResolutionResult,
    _target: &ResolutionTarget,
) -> Vec<SelectedMod> {
    let mut selected = Vec::new();

    for (i, resolved) in resolution.resolved_rules.iter().enumerate() {
        let Some(top_rule) = modlist.rules.get(i) else {
            continue;
        };

        if let RuleOutcome::Resolved { resolved_id } = &resolved.outcome {
            // Find the actual rule (primary or any nested alternative) to get its source.
            let rule = modlist.find_rule(resolved_id).unwrap_or(top_rule);
            selected.push(SelectedMod {
                mod_id: resolved_id.clone(),
                source: rule.source.clone(),
            });
        }
    }

    selected
}

fn remote_artifact_project_id(artifact: &RemoteArtifact) -> &str {
    match artifact {
        RemoteArtifact::Live(version) => &version.project_id,
        RemoteArtifact::Cached(record) => &record.modrinth_project_id,
    }
}

fn split_remote_artifacts(
    artifacts: &[RemoteArtifact],
) -> (Vec<ModrinthVersion>, Vec<ModCacheRecord>) {
    let mut live_versions = Vec::new();
    let mut cached_records = Vec::new();
    let mut seen_version_ids = HashSet::new();

    for artifact in artifacts {
        match artifact {
            RemoteArtifact::Live(version) => {
                if seen_version_ids.insert(version.id.clone()) {
                    live_versions.push(version.clone());
                }
            }
            RemoteArtifact::Cached(record) => {
                if seen_version_ids.insert(record.modrinth_version_id.clone()) {
                    cached_records.push(record.clone());
                }
            }
        }
    }

    (live_versions, cached_records)
}

async fn resolve_selected_remote_artifacts(
    launcher_paths: &LauncherPaths,
    selected_mods: &[SelectedMod],
    target: &ResolutionTarget,
) -> Result<HashMap<String, RemoteArtifact>> {
    let mut artifacts = HashMap::new();

    for selected in selected_mods {
        if !matches!(selected.source, ModSource::Modrinth)
            || artifacts.contains_key(&selected.mod_id)
        {
            continue;
        }

        if let Some(record) =
            load_cached_mod_record_for_target(launcher_paths, &selected.mod_id, target)?
        {
            artifacts.insert(selected.mod_id.clone(), RemoteArtifact::Cached(record));
        }
    }

    Ok(artifacts)
}

fn alt_viable_for_launch(
    rule: &Rule,
    active_mods: &HashSet<String>,
    target: &ResolutionTarget,
) -> bool {
    use crate::rules::VersionRuleKind;
    if !rule.enabled {
        return false;
    }
    if rule.exclude_if.iter().any(|id| active_mods.contains(id)) {
        return false;
    }
    if rule.requires.iter().any(|id| !active_mods.contains(id)) {
        return false;
    }
    for vr in &rule.version_rules {
        let version_matches = vr
            .mc_versions
            .iter()
            .any(|v| crate::modrinth::mc_version_matches(v, &target.minecraft_version));
        let vr_loader = vr.loader.to_ascii_lowercase();
        let loader_matches =
            vr_loader == "any" || vr_loader == target.mod_loader.as_modrinth_loader();
        match vr.kind {
            VersionRuleKind::Only => {
                if !(version_matches && loader_matches) {
                    return false;
                }
            }
            VersionRuleKind::Exclude => {
                if version_matches && loader_matches {
                    return false;
                }
            }
        }
    }
    true
}

fn collect_resolved_parent_versions(
    selected_mods: &[SelectedMod],
    compatible_versions: &HashMap<String, Vec<ModrinthVersion>>,
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
        if let Some(version) = compatible_versions
            .get(&selected.mod_id)
            .and_then(|versions| versions.first())
        {
            versions.push(version.clone());
        }
    }

    versions
}

fn collect_top_level_version_candidates(
    selected_mods: &[SelectedMod],
    compatible_versions: &HashMap<String, Vec<ModrinthVersion>>,
) -> Vec<TopLevelVersionCandidates> {
    let mut top_level_candidates = Vec::new();
    let mut seen_selected_mod_ids = HashSet::new();

    for selected in selected_mods {
        if !matches!(selected.source, ModSource::Modrinth)
            || !seen_selected_mod_ids.insert(selected.mod_id.clone())
        {
            continue;
        }

        let Some(candidates) = compatible_versions.get(&selected.mod_id) else {
            continue;
        };
        let Some(first_candidate) = candidates.first() else {
            continue;
        };

        top_level_candidates.push(TopLevelVersionCandidates {
            selected_mod_id: selected.mod_id.clone(),
            project_id: first_candidate.project_id.clone(),
            candidates: candidates.clone(),
        });
    }

    top_level_candidates
}

fn collect_selected_project_ids(parent_versions: &[ModrinthVersion]) -> HashSet<String> {
    parent_versions
        .iter()
        .map(|version| version.project_id.clone())
        .collect()
}

async fn inspect_remote_versions_for_launch(
    launcher_paths: &LauncherPaths,
    http_client: &reqwest::Client,
    versions: &[ModrinthVersion],
    target: &ResolutionTarget,
) -> Result<(HashSet<String>, u32)> {
    let mut incompatible_projects = HashSet::new();
    let mut required_java = required_java_version_for_minecraft(&target.minecraft_version)?;

    for version in versions {
        let jar_path =
            ensure_remote_version_cached(http_client, launcher_paths, version, target).await?;
        let requirements = read_embedded_fabric_requirements(&jar_path)?;
        if !embedded_minecraft_requirements_match(&requirements, target) {
            incompatible_projects.insert(version.project_id.clone());
            continue;
        }
        if let Some(min_java) = embedded_min_java_requirement(&requirements) {
            required_java = required_java.max(min_java);
        }
    }

    Ok((incompatible_projects, required_java))
}

async fn resolve_embedded_metadata_dependencies(
    app_handle: &tauri::AppHandle,
    launcher_paths: &LauncherPaths,
    http_client: &reqwest::Client,
    client: &ModrinthClient,
    target: &ResolutionTarget,
    parent_versions: &[ModrinthVersion],
    dependency_versions: &mut Vec<ModrinthVersion>,
    dependency_resolution: &mut DependencyResolution,
) -> Result<()> {
    let mut attempted_dependency_ids = HashSet::new();

    loop {
        let all_versions = deduplicate_versions(parent_versions.to_vec(), dependency_versions.clone());
        let metadata_entries = load_embedded_fabric_metadata_for_versions(
            launcher_paths,
            http_client,
            &all_versions,
            target,
        )
        .await?;
        let missing_dependencies = collect_missing_embedded_dependencies(&metadata_entries);
        if missing_dependencies.is_empty() {
            return Ok(());
        }

        let mut added_any = false;

        for (logical_dependency_id, owners) in missing_dependencies {
            if !attempted_dependency_ids.insert(logical_dependency_id.clone()) {
                continue;
            }

            let existing_project_ids = all_versions
                .iter()
                .map(|version| version.project_id.as_str())
                .collect::<HashSet<_>>();

            let resolved_version = resolve_embedded_dependency_version(
                app_handle,
                launcher_paths,
                http_client,
                client,
                target,
                &logical_dependency_id,
                &existing_project_ids,
            )
            .await?;

            let Some(version) = resolved_version else {
                let _ = emit_log(
                    app_handle,
                    ProcessLogStream::Stdout,
                    format!(
                        "[Dependencies] embedded dependency '{}' could not be resolved for {} / {}",
                        logical_dependency_id,
                        target.minecraft_version,
                        target.mod_loader.as_modrinth_loader()
                    ),
                );
                continue;
            };

            let primary_file = version.primary_file().with_context(|| {
                format!(
                    "embedded dependency '{}' version '{}' is missing a primary file",
                    logical_dependency_id, version.id
                )
            })?;

            if dependency_versions.iter().all(|candidate| candidate.id != version.id) {
                dependency_versions.push(version.clone());
            }

            if dependency_resolution
                .resolved_dependencies
                .iter()
                .all(|dependency| dependency.version_id != version.id)
            {
                dependency_resolution
                    .resolved_dependencies
                    .push(ResolvedDependency {
                        dependency_id: version.project_id.clone(),
                        version_id: version.id.clone(),
                        jar_filename: primary_file.filename.clone(),
                        download_url: primary_file.url.clone(),
                        file_hash: primary_file.hashes.get("sha1").cloned(),
                        date_published: version.date_published.clone(),
                    });
            }

            for owner in owners {
                let already_linked = dependency_resolution.links.iter().any(|link| {
                    link.parent_mod_id == owner && link.dependency_id == version.project_id
                });
                if already_linked {
                    continue;
                }

                dependency_resolution.links.push(DependencyLink {
                    parent_mod_id: owner,
                    dependency_id: version.project_id.clone(),
                    specific_version: None,
                    jar_filename: primary_file.filename.clone(),
                });
            }

            let _ = emit_log(
                app_handle,
                ProcessLogStream::Stdout,
                format!(
                    "[Dependencies] added embedded dependency '{}' as '{}'",
                    logical_dependency_id, version.project_id
                ),
            );
            added_any = true;
        }

        if !added_any {
            return Ok(());
        }
    }
}

async fn suppress_redundant_bundled_dependencies(
    app_handle: &tauri::AppHandle,
    launcher_paths: &LauncherPaths,
    http_client: &reqwest::Client,
    target: &ResolutionTarget,
    parent_versions: &[ModrinthVersion],
    dependency_versions: &mut Vec<ModrinthVersion>,
    dependency_resolution: &mut DependencyResolution,
) -> Result<()> {
    let all_versions = deduplicate_versions(parent_versions.to_vec(), dependency_versions.clone());
    let mut bundled_ids_by_project: HashMap<String, HashSet<String>> = HashMap::new();

    for version in &all_versions {
        let jar_path = ensure_remote_version_cached(http_client, launcher_paths, version, target)
            .await?;
        let bundled_ids = read_bundled_fabric_provided_ids(&jar_path)?;
        if !bundled_ids.is_empty() {
            bundled_ids_by_project.insert(version.project_id.clone(), bundled_ids);
        }
    }

    if bundled_ids_by_project.is_empty() {
        return Ok(());
    }

    let mut dependency_root_ids: HashMap<String, HashSet<String>> = HashMap::new();
    for version in dependency_versions.iter() {
        let jar_path = ensure_remote_version_cached(http_client, launcher_paths, version, target)
            .await?;
        let root_ids = read_root_fabric_provided_ids(&jar_path)?;
        if !root_ids.is_empty() {
            dependency_root_ids.insert(version.project_id.clone(), root_ids);
        }
    }

    let mut removable_dependency_projects = HashSet::new();
    for dependency_version in dependency_versions.iter() {
        let Some(root_ids) = dependency_root_ids.get(&dependency_version.project_id) else {
            continue;
        };

        let links = dependency_resolution
            .links
            .iter()
            .filter(|link| {
                link.dependency_id == dependency_version.project_id && link.specific_version.is_none()
            })
            .collect::<Vec<_>>();
        if links.is_empty() {
            continue;
        }

        let covered_by_bundled_parent = links.iter().all(|link| {
            bundled_ids_by_project
                .get(&link.parent_mod_id)
                .is_some_and(|bundled_ids| root_ids.iter().any(|id| bundled_ids.contains(id)))
        });
        if !covered_by_bundled_parent {
            continue;
        }

        removable_dependency_projects.insert(dependency_version.project_id.clone());
        let _ = emit_log(
            app_handle,
            ProcessLogStream::Stdout,
            format!(
                "[Dependencies] dropped standalone dependency '{}' because all requiring parents already bundle it",
                dependency_version.project_id
            ),
        );
    }

    if removable_dependency_projects.is_empty() {
        return Ok(());
    }

    dependency_versions.retain(|version| !removable_dependency_projects.contains(&version.project_id));
    dependency_resolution
        .resolved_dependencies
        .retain(|dependency| !removable_dependency_projects.contains(&dependency.dependency_id));
    dependency_resolution
        .links
        .retain(|link| !removable_dependency_projects.contains(&link.dependency_id));

    Ok(())
}

async fn load_embedded_fabric_metadata_for_versions(
    launcher_paths: &LauncherPaths,
    http_client: &reqwest::Client,
    versions: &[ModrinthVersion],
    target: &ResolutionTarget,
) -> Result<Vec<OwnedEmbeddedFabricModMetadata>> {
    let mut entries = Vec::new();

    for version in versions {
        let jar_path = ensure_remote_version_cached(http_client, launcher_paths, version, target)
            .await?;
        for metadata in read_embedded_fabric_mod_metadata(&jar_path)? {
            entries.push(OwnedEmbeddedFabricModMetadata {
                owner_project_id: version.project_id.clone(),
                metadata,
            });
        }
    }

    Ok(entries)
}

async fn load_root_fabric_metadata_for_versions(
    launcher_paths: &LauncherPaths,
    http_client: &reqwest::Client,
    versions: &[ModrinthVersion],
    target: &ResolutionTarget,
) -> Result<HashMap<String, EmbeddedFabricModMetadata>> {
    let mut entries = HashMap::new();

    for version in versions {
        let jar_path = ensure_remote_version_cached(http_client, launcher_paths, version, target)
            .await?;
        if let Some(metadata) = read_root_fabric_mod_metadata(&jar_path)? {
            entries.insert(version.project_id.clone(), metadata);
        }
    }

    Ok(entries)
}

fn collect_missing_embedded_dependencies(
    entries: &[OwnedEmbeddedFabricModMetadata],
) -> Vec<(String, Vec<String>)> {
    let mut provided_ids = HashSet::new();
    for entry in entries {
        if !entry.metadata.mod_id.trim().is_empty() {
            provided_ids.insert(entry.metadata.mod_id.clone());
        }
        provided_ids.extend(entry.metadata.provides.iter().cloned());
    }

    let mut missing_by_dependency: HashMap<String, HashSet<String>> = HashMap::new();
    for entry in entries {
        for dependency_id in entry.metadata.depends.keys() {
            if embedded_dependency_is_builtin(dependency_id) || provided_ids.contains(dependency_id) {
                continue;
            }
            missing_by_dependency
                .entry(dependency_id.clone())
                .or_default()
                .insert(entry.owner_project_id.clone());
        }
    }

    let mut missing = missing_by_dependency
        .into_iter()
        .map(|(dependency_id, owners)| {
            let mut owners = owners.into_iter().collect::<Vec<_>>();
            owners.sort();
            (dependency_id, owners)
        })
        .collect::<Vec<_>>();
    missing.sort_by(|left, right| left.0.cmp(&right.0));
    missing
}

fn build_top_level_owner_map(
    parent_versions: &[ModrinthVersion],
    dependency_links: &[DependencyLink],
) -> HashMap<String, HashSet<String>> {
    let mut owners = parent_versions
        .iter()
        .map(|version| {
            (
                version.project_id.clone(),
                HashSet::from([version.project_id.clone()]),
            )
        })
        .collect::<HashMap<_, _>>();

    loop {
        let mut changed = false;

        for link in dependency_links {
            let parent_owners = owners
                .get(&link.parent_mod_id)
                .cloned()
                .unwrap_or_default();
            if parent_owners.is_empty() {
                continue;
            }

            let dependency_owners = owners.entry(link.dependency_id.clone()).or_default();
            let previous_len = dependency_owners.len();
            dependency_owners.extend(parent_owners);
            if dependency_owners.len() != previous_len {
                changed = true;
            }
        }

        if !changed {
            return owners;
        }
    }
}

fn collect_top_level_owner_ids(
    project_ids: &HashSet<String>,
    owner_map: &HashMap<String, HashSet<String>>,
) -> HashSet<String> {
    let mut top_level_ids = HashSet::new();

    for project_id in project_ids {
        if let Some(owners) = owner_map.get(project_id) {
            top_level_ids.extend(owners.iter().cloned());
        }
    }

    top_level_ids
}

fn validate_final_fabric_runtime(
    metadata_entries: &[OwnedEmbeddedFabricModMetadata],
    owner_map: &HashMap<String, HashSet<String>>,
) -> HashMap<String, FabricValidationIssue> {
    let mut providers_by_id: HashMap<String, Vec<&OwnedEmbeddedFabricModMetadata>> = HashMap::new();
    for entry in metadata_entries {
        for provided_id in provided_ids_for_metadata(&entry.metadata) {
            providers_by_id
                .entry(provided_id)
                .or_default()
                .push(entry);
        }
    }

    let mut issues = HashMap::new();
    for entry in metadata_entries {
        let Some(top_level_owners) = owner_map.get(&entry.owner_project_id) else {
            continue;
        };

        for (dependency_id, predicates) in &entry.metadata.depends {
            if embedded_dependency_is_builtin(dependency_id) {
                continue;
            }

            let providers = providers_by_id.get(dependency_id);
            let satisfied = providers.is_some_and(|providers| {
                providers.iter().any(|provider| {
                    fabric_dependency_predicates_match(
                        predicates,
                        &provider.metadata.version,
                    )
                })
            });
            if satisfied {
                continue;
            }

            let reason_code = if providers.is_some() {
                "incompatible_dependency_version"
            } else {
                "missing_dependency"
            };
            let detail = if providers.is_some() {
                format!(
                    "embedded metadata requires '{}' with a compatible version, but only incompatible versions are present",
                    dependency_id
                )
            } else {
                format!("embedded metadata requires '{}', which is missing", dependency_id)
            };

            for top_level_owner in top_level_owners {
                issues.entry(top_level_owner.clone()).or_insert_with(|| FabricValidationIssue {
                    reason_code,
                    owner_project_id: entry.owner_project_id.clone(),
                    mod_id: entry.metadata.mod_id.clone(),
                    dependency_id: Some(dependency_id.clone()),
                    detail: detail.clone(),
                });
            }
        }

        for (dependency_id, predicates) in &entry.metadata.breaks {
            let Some(providers) = providers_by_id.get(dependency_id) else {
                continue;
            };
            let Some(conflicting_provider) = providers.iter().find(|provider| {
                fabric_dependency_predicates_match(predicates, &provider.metadata.version)
            }) else {
                continue;
            };

            let detail = format!(
                "embedded metadata breaks '{}' version {}",
                dependency_id, conflicting_provider.metadata.version
            );
            for top_level_owner in top_level_owners {
                issues.entry(top_level_owner.clone()).or_insert_with(|| FabricValidationIssue {
                    reason_code: "breaks_conflict",
                    owner_project_id: entry.owner_project_id.clone(),
                    mod_id: entry.metadata.mod_id.clone(),
                    dependency_id: Some(dependency_id.clone()),
                    detail: detail.clone(),
                });
            }
        }
    }

    issues
}

fn validate_root_parent_fabric_runtime(
    parent_metadata_by_project: &HashMap<String, EmbeddedFabricModMetadata>,
    all_metadata_entries: &[OwnedEmbeddedFabricModMetadata],
) -> HashMap<String, FabricValidationIssue> {
    let mut providers_by_id: HashMap<String, Vec<&OwnedEmbeddedFabricModMetadata>> = HashMap::new();
    for entry in all_metadata_entries {
        for provided_id in provided_ids_for_metadata(&entry.metadata) {
            providers_by_id
                .entry(provided_id)
                .or_default()
                .push(entry);
        }
    }

    let mut issues = HashMap::new();
    for (project_id, metadata) in parent_metadata_by_project {
        for (dependency_id, predicates) in &metadata.depends {
            if embedded_dependency_is_builtin(dependency_id) {
                continue;
            }

            let providers = providers_by_id.get(dependency_id);
            let satisfied = providers.is_some_and(|providers| {
                providers.iter().any(|provider| {
                    fabric_dependency_predicates_match(
                        predicates,
                        &provider.metadata.version,
                    )
                })
            });
            if satisfied {
                continue;
            }

            let reason_code = if providers.is_some() {
                "incompatible_dependency_version"
            } else {
                "missing_dependency"
            };
            issues.insert(
                project_id.clone(),
                FabricValidationIssue {
                    reason_code,
                    owner_project_id: project_id.clone(),
                    mod_id: metadata.mod_id.clone(),
                    dependency_id: Some(dependency_id.clone()),
                    detail: if providers.is_some() {
                        format!(
                            "embedded metadata requires '{}' with a compatible version, but only incompatible versions are present",
                            dependency_id
                        )
                    } else {
                        format!("embedded metadata requires '{}', which is missing", dependency_id)
                    },
                },
            );
            break;
        }

        if issues.contains_key(project_id) {
            continue;
        }

        for (dependency_id, predicates) in &metadata.breaks {
            let Some(providers) = providers_by_id.get(dependency_id) else {
                continue;
            };
            let Some(conflicting_provider) = providers.iter().find(|provider| {
                fabric_dependency_predicates_match(predicates, &provider.metadata.version)
            }) else {
                continue;
            };

            issues.insert(
                project_id.clone(),
                FabricValidationIssue {
                    reason_code: "breaks_conflict",
                    owner_project_id: project_id.clone(),
                    mod_id: metadata.mod_id.clone(),
                    dependency_id: Some(dependency_id.clone()),
                    detail: format!(
                        "embedded metadata breaks '{}' version {}",
                        dependency_id, conflicting_provider.metadata.version
                    ),
                },
            );
            break;
        }
    }

    issues
}

async fn resolve_embedded_dependency_version(
    app_handle: &tauri::AppHandle,
    launcher_paths: &LauncherPaths,
    http_client: &reqwest::Client,
    client: &ModrinthClient,
    target: &ResolutionTarget,
    logical_dependency_id: &str,
    existing_project_ids: &HashSet<&str>,
) -> Result<Option<ModrinthVersion>> {
    for candidate_project_id in embedded_dependency_project_candidates(logical_dependency_id) {
        if existing_project_ids.contains(candidate_project_id.as_str()) {
            return Ok(None);
        }

        if let Some(version) = select_latest_launch_compatible_version(
            app_handle,
            launcher_paths,
            http_client,
            client,
            &candidate_project_id,
            target,
        )
        .await?
        {
            return Ok(Some(version));
        }
    }

    Ok(None)
}

fn embedded_dependency_project_candidates(logical_dependency_id: &str) -> Vec<String> {
    match logical_dependency_id {
        "fabric" | "fabric-api" => vec!["fabric-api".to_string()],
        other => vec![other.to_string()],
    }
}

fn embedded_dependency_is_builtin(dependency_id: &str) -> bool {
    matches!(
        dependency_id.trim().to_ascii_lowercase().as_str(),
        "minecraft" | "java" | "fabricloader" | "fabric-loader" | "quilt_loader" | "quiltloader"
    )
}

fn validate_selected_parent_dependencies(
    parent_versions: &[ModrinthVersion],
    selected_parent_versions: &HashMap<String, ModrinthVersion>,
    selected_project_ids: &HashSet<String>,
) -> HashSet<String> {
    let mut excluded_parents = HashSet::new();

    for parent_version in parent_versions {
        for dependency in &parent_version.dependencies {
            if dependency.dependency_type != DependencyType::Required {
                continue;
            }

            let Some(project_id) = dependency
                .project_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };

            if !selected_project_ids.contains(project_id) {
                continue;
            }

            let Some(selected_version) = selected_parent_versions.get(project_id) else {
                continue;
            };

            let exact_version_matches = dependency
                .version_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none_or(|version_id| version_id == selected_version.id);

            if !exact_version_matches {
                excluded_parents.insert(parent_version.project_id.clone());
                break;
            }
        }
    }

    excluded_parents
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

fn load_cached_mod_record_for_target(
    launcher_paths: &LauncherPaths,
    project_id: &str,
    target: &ResolutionTarget,
) -> Result<Option<ModCacheRecord>> {
    let connection = Connection::open(launcher_paths.database_path()).with_context(|| {
        format!(
            "failed to open launcher database at {}",
            launcher_paths.database_path().display()
        )
    })?;
    let repository = SqliteModCacheRepository::new(&connection, launcher_paths.mods_cache_dir());
    repository.find_compatible_by_project(project_id, target)
}

fn load_cached_mod_record_by_version(
    launcher_paths: &LauncherPaths,
    version_id: &str,
) -> Result<Option<ModCacheRecord>> {
    let connection = Connection::open(launcher_paths.database_path()).with_context(|| {
        format!(
            "failed to open launcher database at {}",
            launcher_paths.database_path().display()
        )
    })?;
    let repository = SqliteModCacheRepository::new(&connection, launcher_paths.mods_cache_dir());
    repository.find_by_version_id(version_id)
}

fn load_cached_dependency_requests(
    launcher_paths: &LauncherPaths,
    parent_mod_ids: &[String],
) -> Result<Vec<DependencyRequest>> {
    if parent_mod_ids.is_empty() {
        return Ok(Vec::new());
    }

    let connection = Connection::open(launcher_paths.database_path()).with_context(|| {
        format!(
            "failed to open launcher database at {}",
            launcher_paths.database_path().display()
        )
    })?;

    let mut requests = Vec::new();
    let mut statement = connection.prepare(
        r#"
        SELECT dependency_id, specific_version
        FROM dependencies
        WHERE mod_parent_id = ?1
          AND dep_type = 'required'
        ORDER BY dependency_id ASC
        "#,
    )?;

    for parent_mod_id in parent_mod_ids {
        let rows = statement.query_map([parent_mod_id.as_str()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;

        for row in rows {
            let (dependency_id, specific_version) = row?;
            let selector = match specific_version {
                Some(version_id) if !version_id.trim().is_empty() => {
                    DependencySelector::VersionId { version_id }
                }
                _ => DependencySelector::ProjectId {
                    project_id: dependency_id.clone(),
                },
            };

            requests.push(DependencyRequest {
                parent_mod_id: parent_mod_id.clone(),
                selector,
            });
        }
    }

    Ok(requests)
}

fn resolved_dependency_from_cache_record(record: &ModCacheRecord) -> ResolvedDependency {
    ResolvedDependency {
        dependency_id: record.modrinth_project_id.clone(),
        version_id: record.modrinth_version_id.clone(),
        jar_filename: record.jar_filename.clone(),
        download_url: record.download_url.clone().unwrap_or_default(),
        file_hash: record.file_hash.clone(),
        date_published: String::new(),
    }
}

fn finalize_dependency_candidates(
    candidates: Vec<DependencyResolutionCandidate>,
    mut excluded_parents: HashSet<String>,
) -> Result<(DependencyResolution, Vec<RemoteArtifact>)> {
    loop {
        let valid_candidates = candidates
            .iter()
            .filter(|candidate| !excluded_parents.contains(&candidate.parent_mod_id))
            .cloned()
            .collect::<Vec<_>>();

        let (selected_by_dependency, newly_excluded_parents) =
            select_cached_dependency_candidates(&valid_candidates);

        if newly_excluded_parents.is_empty() {
            return build_cached_dependency_resolution(
                valid_candidates,
                selected_by_dependency,
                excluded_parents,
            );
        }

        excluded_parents.extend(newly_excluded_parents);
    }
}

fn select_cached_dependency_candidates(
    candidates: &[DependencyResolutionCandidate],
) -> (
    HashMap<String, DependencyResolutionCandidate>,
    HashSet<String>,
) {
    let mut groups: HashMap<String, Vec<&DependencyResolutionCandidate>> = HashMap::new();
    for candidate in candidates {
        groups
            .entry(candidate.resolved_dependency.dependency_id.clone())
            .or_default()
            .push(candidate);
    }

    let mut selected_by_dependency = HashMap::new();
    let mut excluded_parents = HashSet::new();

    for (dependency_id, group) in groups {
        let exact_candidates = group
            .iter()
            .copied()
            .filter(|candidate| matches!(candidate.selector, DependencySelector::VersionId { .. }))
            .collect::<Vec<_>>();

        let selected = if exact_candidates.is_empty() {
            group.into_iter().max_by(|left, right| {
                left.resolved_dependency
                    .date_published
                    .cmp(&right.resolved_dependency.date_published)
            })
        } else {
            let distinct_exact_versions = exact_candidates
                .iter()
                .map(|candidate| candidate.resolved_dependency.version_id.as_str())
                .collect::<HashSet<_>>();

            if distinct_exact_versions.len() > 1 {
                for candidate in exact_candidates {
                    excluded_parents.insert(candidate.parent_mod_id.clone());
                }
                None
            } else {
                exact_candidates.into_iter().max_by(|left, right| {
                    left.resolved_dependency
                        .date_published
                        .cmp(&right.resolved_dependency.date_published)
                })
            }
        };

        if let Some(selected) = selected {
            selected_by_dependency.insert(dependency_id, selected.clone());
        }
    }

    for candidate in candidates {
        let Some(selected) =
            selected_by_dependency.get(&candidate.resolved_dependency.dependency_id)
        else {
            continue;
        };

        if matches!(candidate.selector, DependencySelector::VersionId { .. })
            && selected.resolved_dependency.version_id != candidate.resolved_dependency.version_id
        {
            excluded_parents.insert(candidate.parent_mod_id.clone());
        }
    }

    (selected_by_dependency, excluded_parents)
}

fn build_cached_dependency_resolution(
    candidates: Vec<DependencyResolutionCandidate>,
    selected_by_dependency: HashMap<String, DependencyResolutionCandidate>,
    excluded_parents: HashSet<String>,
) -> Result<(DependencyResolution, Vec<RemoteArtifact>)> {
    let mut resolved_dependencies = selected_by_dependency
        .values()
        .map(|candidate| candidate.resolved_dependency.clone())
        .collect::<Vec<_>>();
    resolved_dependencies.sort_by(|left, right| left.dependency_id.cmp(&right.dependency_id));

    let mut deduplicated_links = HashMap::new();
    for candidate in candidates {
        let Some(selected_candidate) =
            selected_by_dependency.get(&candidate.resolved_dependency.dependency_id)
        else {
            continue;
        };

        if matches!(candidate.selector, DependencySelector::VersionId { .. })
            && selected_candidate.resolved_dependency.version_id
                != candidate.resolved_dependency.version_id
        {
            continue;
        }

        let selected_dependency = &selected_candidate.resolved_dependency;
        let specific_version = match candidate.selector {
            DependencySelector::ProjectId { .. } => None,
            DependencySelector::VersionId { .. } => Some(selected_dependency.version_id.clone()),
        };

        deduplicated_links.insert(
            (
                candidate.parent_mod_id.clone(),
                selected_dependency.dependency_id.clone(),
            ),
            DependencyLink {
                parent_mod_id: candidate.parent_mod_id,
                dependency_id: selected_dependency.dependency_id.clone(),
                specific_version,
                jar_filename: selected_dependency.jar_filename.clone(),
            },
        );
    }

    let mut links = deduplicated_links.into_values().collect::<Vec<_>>();
    links.sort_by(|left, right| {
        left.parent_mod_id
            .cmp(&right.parent_mod_id)
            .then(left.dependency_id.cmp(&right.dependency_id))
    });

    let artifacts = resolved_dependencies
        .iter()
        .filter_map(|dependency| {
            selected_by_dependency
                .get(&dependency.dependency_id)
                .map(|candidate| candidate.artifact.clone())
        })
        .collect::<Vec<_>>();

    Ok((
        DependencyResolution {
            resolved_dependencies,
            links,
            excluded_parents,
        },
        artifacts,
    ))
}

async fn resolve_dependency_requests_with_cache_fallback(
    launcher_paths: &LauncherPaths,
    requests: &[DependencyRequest],
    selected_mod_ids: &HashSet<String>,
    target: &ResolutionTarget,
) -> Result<(DependencyResolution, Vec<RemoteArtifact>)> {
    let mut candidates = Vec::with_capacity(requests.len());
    let mut excluded_parents = HashSet::new();

    for request in requests {
        if let DependencySelector::ProjectId { project_id } = &request.selector {
            if selected_mod_ids.contains(project_id) {
                continue;
            }
        }

        if excluded_parents.contains(&request.parent_mod_id) {
            continue;
        }

        let candidate = match &request.selector {
            DependencySelector::ProjectId { project_id } => {
                if let Some(record) =
                    load_cached_mod_record_for_target(launcher_paths, project_id, target)?
                {
                    DependencyResolutionCandidate {
                        parent_mod_id: request.parent_mod_id.clone(),
                        selector: request.selector.clone(),
                        resolved_dependency: resolved_dependency_from_cache_record(&record),
                        artifact: RemoteArtifact::Cached(record),
                    }
                } else {
                    excluded_parents.insert(request.parent_mod_id.clone());
                    continue;
                }
            }
            DependencySelector::VersionId { version_id } => {
                if let Some(record) = load_cached_mod_record_by_version(launcher_paths, version_id)?
                {
                    DependencyResolutionCandidate {
                        parent_mod_id: request.parent_mod_id.clone(),
                        selector: request.selector.clone(),
                        resolved_dependency: resolved_dependency_from_cache_record(&record),
                        artifact: RemoteArtifact::Cached(record),
                    }
                } else {
                    excluded_parents.insert(request.parent_mod_id.clone());
                    continue;
                }
            }
        };

        candidates.push(candidate);
    }

    finalize_dependency_candidates(candidates, excluded_parents)
}

fn build_remote_acquisition_plan_from_artifacts(
    launcher_paths: &LauncherPaths,
    live_versions: &[ModrinthVersion],
    cached_records: &[ModCacheRecord],
    target: &ResolutionTarget,
) -> Result<ModAcquisitionPlan> {
    let connection = Connection::open(launcher_paths.database_path()).with_context(|| {
        format!(
            "failed to open launcher database at {}",
            launcher_paths.database_path().display()
        )
    })?;
    let repository = SqliteModCacheRepository::new(&connection, launcher_paths.mods_cache_dir());

    let mut seen_version_ids = HashSet::new();
    let mut cached = Vec::new();
    let mut to_download = Vec::new();

    for record in cached_records {
        if seen_version_ids.insert(record.modrinth_version_id.clone()) {
            cached.push(record.clone());
        }
    }

    for version in live_versions {
        if !seen_version_ids.insert(version.id.clone()) {
            continue;
        }

        match repository.find_by_version_id(&version.id)? {
            Some(record) => cached.push(record),
            None => to_download.push(pending_download_from_version(version, target)?),
        }
    }

    Ok(ModAcquisitionPlan {
        cached,
        to_download,
    })
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
    let total = artifacts.len();
    if total == 0 {
        return Ok(());
    }

    let semaphore = Arc::new(tokio::sync::Semaphore::new(10));
    let mut tasks: tokio::task::JoinSet<Result<DownloadArtifact>> = tokio::task::JoinSet::new();

    for artifact in artifacts {
        let artifact = artifact.clone();
        let http_client = http_client.clone();
        let permit_source = semaphore.clone();
        tasks.spawn(async move {
            // Permit is held through the await so concurrency stays bounded.
            let _permit = permit_source
                .acquire_owned()
                .await
                .map_err(|error| anyhow::anyhow!("failed to acquire download permit: {error}"))?;
            match &artifact.file_hash {
                Some(hash) => crate::minecraft_downloader::download_file_verified(
                    &http_client,
                    &artifact.url,
                    &artifact.destination_path,
                    hash,
                )
                .await
                .with_context(|| {
                    format!(
                        "failed to download '{}' to {}",
                        artifact.url,
                        artifact.destination_path.display()
                    )
                })?,
                None => download_file(&http_client, &artifact.url, &artifact.destination_path)
                    .await
                    .with_context(|| {
                        format!(
                            "failed to download '{}' to {}",
                            artifact.url,
                            artifact.destination_path.display()
                        )
                    })?,
            }
            Ok(artifact)
        });
    }

    let mut completed: usize = 0;
    while let Some(join_result) = tasks.join_next().await {
        let artifact =
            join_result.map_err(|error| anyhow::anyhow!("download task panicked: {error}"))??;
        completed += 1;

        let progress = 42u8 + ((16usize * completed) / total) as u8;
        emit_progress(
            app_handle,
            "resolving",
            progress,
            "Downloading Mods",
            &format!("Downloaded {completed} of {total} mods."),
        )?;

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
        let version_match =
            rule.mc_versions.is_empty() || rule.mc_versions.iter().any(|v| v == mc_version);
        let loader_match = rule.loader == "any" || rule.loader.eq_ignore_ascii_case(loader);
        match rule.kind {
            VersionRuleKind::Exclude => {
                if version_match && loader_match {
                    return false;
                }
            }
            VersionRuleKind::Only => {
                if !(version_match && loader_match) {
                    return false;
                }
            }
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
    std::fs::create_dir_all(cache_dir).with_context(|| {
        format!(
            "failed to create content packs cache at {}",
            cache_dir.display()
        )
    })?;

    let mc_version = &target.minecraft_version;
    let loader_str = target.mod_loader.as_modrinth_loader();

    for (content_type, instance_subdir) in
        [("resourcepack", "resourcepacks"), ("shader", "shaderpacks")]
    {
        let list = load_content_list(&modlist_dir, content_type).unwrap_or_else(|_| {
            crate::content_packs::ContentList {
                content_type: content_type.to_string(),
                entries: vec![],
                groups: vec![],
            }
        });

        // Filter active entries
        let active_entries: Vec<&ContentEntry> = list
            .entries
            .iter()
            .filter(|e| is_content_entry_active(e, mc_version, loader_str))
            .collect();

        let instance_dir = instance_root.join(instance_subdir);
        if instance_dir.exists() {
            // Clear existing content in instance dir (even if no active entries remain)
            crate::instance_mods::clear_instance_mods_directory(&instance_dir)?;
        }

        if active_entries.is_empty() {
            continue;
        }

        std::fs::create_dir_all(&instance_dir)
            .with_context(|| format!("failed to create {}", instance_dir.display()))?;

        for entry in &active_entries {
            if entry.source == "modrinth" {
                // Fetch latest compatible version from Modrinth
                match modrinth_client
                    .fetch_content_pack_versions(&entry.id, mc_version)
                    .await
                {
                    Ok(versions) => {
                        // Pick the latest by date
                        let best = versions
                            .into_iter()
                            .max_by(|a, b| a.date_published.cmp(&b.date_published));
                        if let Some(version) = best {
                            if let Some(file) = version.primary_file() {
                                let cached_path = cache_dir.join(&file.filename);
                                let was_cached = cached_path.exists();
                                if !was_cached {
                                    emit_log(
                                        app_handle,
                                        ProcessLogStream::Stdout,
                                        format!(
                                            "[Content] Downloading {} ({})",
                                            entry.id, file.filename
                                        ),
                                    )?;
                                    download_file(http_client, &file.url, &cached_path)
                                        .await
                                        .with_context(|| {
                                            format!(
                                                "failed to download content pack '{}'",
                                                entry.id
                                            )
                                        })?;
                                }
                                let target_path = instance_dir.join(&file.filename);
                                crate::instance_mods::create_file_link(&cached_path, &target_path)
                                    .with_context(|| {
                                        format!(
                                            "failed to link content pack '{}' into instance",
                                            entry.id
                                        )
                                    })?;
                                if was_cached {
                                    emit_log(
                                        app_handle,
                                        ProcessLogStream::Stdout,
                                        format!(
                                            "[Content] {} → {} (cached)",
                                            entry.id, instance_subdir
                                        ),
                                    )?;
                                } else {
                                    emit_log(
                                        app_handle,
                                        ProcessLogStream::Stdout,
                                        format!(
                                            "[Content] Downloaded {} → {}",
                                            entry.id, instance_subdir
                                        ),
                                    )?;
                                }
                            }
                        } else {
                            emit_log(
                                app_handle,
                                ProcessLogStream::Stdout,
                                format!(
                                    "[Content] No compatible version found for '{}' on {}",
                                    entry.id, mc_version
                                ),
                            )?;
                        }
                    }
                    Err(e) => {
                        emit_log(
                            app_handle,
                            ProcessLogStream::Stdout,
                            format!(
                                "[Content] Failed to fetch versions for '{}': {}",
                                entry.id, e
                            ),
                        )?;
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
        let active_entries: Vec<&ContentEntry> = list
            .entries
            .iter()
            .filter(|e| is_content_entry_active(e, mc_version, loader_str))
            .collect();

        let instance_dir = instance_root.join("datapacks");
        if instance_dir.exists() {
            // Clear existing content (even if no active entries remain)
            crate::instance_mods::clear_instance_mods_directory(&instance_dir)?;
        }

        if !active_entries.is_empty() {
            std::fs::create_dir_all(&instance_dir)
                .with_context(|| format!("failed to create {}", instance_dir.display()))?;

            for entry in &active_entries {
                if entry.source == "modrinth" {
                    match modrinth_client
                        .fetch_content_pack_versions(&entry.id, mc_version)
                        .await
                    {
                        Ok(versions) => {
                            let best = versions
                                .into_iter()
                                .max_by(|a, b| a.date_published.cmp(&b.date_published));
                            if let Some(version) = best {
                                if let Some(file) = version.primary_file() {
                                    let cached_path = cache_dir.join(&file.filename);
                                    let was_cached = cached_path.exists();
                                    if !was_cached {
                                        emit_log(
                                            app_handle,
                                            ProcessLogStream::Stdout,
                                            format!(
                                                "[Content] Downloading {} ({})",
                                                entry.id, file.filename
                                            ),
                                        )?;
                                        download_file(http_client, &file.url, &cached_path)
                                            .await
                                            .with_context(|| {
                                            format!("failed to download data pack '{}'", entry.id)
                                        })?;
                                    }
                                    let target_path = instance_dir.join(&file.filename);
                                    crate::instance_mods::create_file_link(
                                        &cached_path,
                                        &target_path,
                                    )
                                    .with_context(|| {
                                        format!(
                                            "failed to link data pack '{}' into instance",
                                            entry.id
                                        )
                                    })?;
                                    if was_cached {
                                        emit_log(
                                            app_handle,
                                            ProcessLogStream::Stdout,
                                            format!("[Content] {} → datapacks (cached)", entry.id),
                                        )?;
                                    } else {
                                        emit_log(
                                            app_handle,
                                            ProcessLogStream::Stdout,
                                            format!(
                                                "[Content] Downloaded {} → datapacks",
                                                entry.id
                                            ),
                                        )?;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            emit_log(
                                app_handle,
                                ProcessLogStream::Stdout,
                                format!(
                                    "[Content] Failed to fetch versions for '{}': {}",
                                    entry.id, e
                                ),
                            )?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn build_cached_mod_jars(
    app_handle: &tauri::AppHandle,
    selected_mods: &[SelectedMod],
    versions: &[ModrinthVersion],
    cached_records: &[ModCacheRecord],
    target: &ResolutionTarget,
    launcher_paths: &LauncherPaths,
    modlist_name: &str,
) -> Result<Vec<CachedModJar>> {
    let mut jars = Vec::new();
    let mut seen = HashSet::new();
    let local_jars_dir = launcher_paths
        .modlists_dir()
        .join(modlist_name)
        .join("local-jars");
    let mod_loader = target.mod_loader.as_modrinth_loader();

    // Local mods: JAR lives at local-jars/{mod_id}.jar — copy to cache/mods/
    for selected in selected_mods {
        if !matches!(selected.source, ModSource::Local) {
            continue;
        }

        let file_name = format!("{}.jar", selected.mod_id);
        if seen.insert(file_name.clone()) {
            let source = local_jars_dir.join(&file_name);
            let dest =
                cached_local_artifact_path(launcher_paths.mods_cache_dir(), mod_loader, &file_name);
            if source.exists() && !jar_metadata_allows_target(&source, target)? {
                emit_log(
                    app_handle,
                    ProcessLogStream::Stdout,
                    format!(
                        "[Launch] skipping local mod '{}': embedded metadata is incompatible with {} / {}",
                        selected.mod_id,
                        target.minecraft_version,
                        target.mod_loader.as_modrinth_loader()
                    ),
                )?;
                continue;
            }
            if source.exists() && !dest.exists() {
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                std::fs::copy(&source, &dest).with_context(|| {
                    format!("failed to copy local JAR '{}' to mod cache", file_name)
                })?;
            }
            jars.push(CachedModJar {
                jar_filename: file_name,
                cache_path: dest,
            });
        }
    }

    for record in cached_records {
        if seen.insert(record.jar_filename.clone()) {
            jars.push(CachedModJar {
                jar_filename: record.jar_filename.clone(),
                cache_path: cached_artifact_path_for_record(
                    launcher_paths.mods_cache_dir(),
                    record,
                ),
            });
        }
    }

    for version in versions {
        let record = cache_record_from_version(version, target)?;
        let jar_filename = record.jar_filename.clone();
        if seen.insert(jar_filename.clone()) {
            jars.push(CachedModJar {
                jar_filename,
                cache_path: cached_artifact_path_for_record(
                    launcher_paths.mods_cache_dir(),
                    &record,
                ),
            });
        }
    }

    Ok(jars)
}

fn required_java_for_cached_mod_jars(jars: &[CachedModJar]) -> Result<u32> {
    let mut required_java = 0;

    for jar in jars {
        let requirements = read_embedded_fabric_requirements(&jar.cache_path)?;
        if let Some(min_java) = embedded_min_java_requirement(&requirements) {
            required_java = required_java.max(min_java);
        }
    }

    Ok(required_java)
}

fn jar_metadata_allows_target(jar_path: &Path, target: &ResolutionTarget) -> Result<bool> {
    Ok(embedded_minecraft_requirements_match(
        &read_embedded_fabric_requirements(jar_path)?,
        target,
    ))
}

fn read_embedded_fabric_requirements(jar_path: &Path) -> Result<EmbeddedFabricRequirements> {
    let file = std::fs::File::open(jar_path)
        .with_context(|| format!("failed to open JAR metadata from {}", jar_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("failed to read JAR metadata from {}", jar_path.display()))?;

    read_embedded_fabric_requirements_from_archive(&mut archive, &jar_path.display().to_string())
}

fn read_embedded_fabric_mod_metadata(jar_path: &Path) -> Result<Vec<EmbeddedFabricModMetadata>> {
    let file = std::fs::File::open(jar_path)
        .with_context(|| format!("failed to open JAR metadata from {}", jar_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("failed to read JAR metadata from {}", jar_path.display()))?;

    read_embedded_fabric_mod_metadata_from_archive(&mut archive, &jar_path.display().to_string())
}

fn read_root_fabric_mod_metadata(jar_path: &Path) -> Result<Option<EmbeddedFabricModMetadata>> {
    let file = std::fs::File::open(jar_path)
        .with_context(|| format!("failed to open JAR metadata from {}", jar_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("failed to read JAR metadata from {}", jar_path.display()))?;
    let metadata = match read_embedded_fabric_metadata(&mut archive, &jar_path.display().to_string())?
    {
        Some(metadata) => metadata,
        None => return Ok(None),
    };

    Ok(fabric_mod_metadata_from_json(&metadata))
}

fn read_root_fabric_provided_ids(jar_path: &Path) -> Result<HashSet<String>> {
    let file = std::fs::File::open(jar_path)
        .with_context(|| format!("failed to open JAR metadata from {}", jar_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("failed to read JAR metadata from {}", jar_path.display()))?;
    let metadata = match read_embedded_fabric_metadata(&mut archive, &jar_path.display().to_string())? {
        Some(metadata) => metadata,
        None => return Ok(HashSet::new()),
    };

    Ok(fabric_mod_metadata_from_json(&metadata)
        .map(|entry| provided_ids_for_metadata(&entry))
        .unwrap_or_default())
}

fn read_bundled_fabric_provided_ids(jar_path: &Path) -> Result<HashSet<String>> {
    let file = std::fs::File::open(jar_path)
        .with_context(|| format!("failed to open JAR metadata from {}", jar_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("failed to read JAR metadata from {}", jar_path.display()))?;
    let metadata = match read_embedded_fabric_metadata(&mut archive, &jar_path.display().to_string())? {
        Some(metadata) => metadata,
        None => return Ok(HashSet::new()),
    };

    let mut provided_ids = HashSet::new();
    for nested_path in nested_fabric_jar_paths(&metadata) {
        let mut nested_bytes = Vec::new();
        let read_nested = {
            let mut nested_file = match archive.by_name(&nested_path) {
                Ok(file) => file,
                Err(_) => continue,
            };
            nested_file.read_to_end(&mut nested_bytes).is_ok()
        };
        if !read_nested {
            continue;
        }

        let mut nested_archive = match zip::ZipArchive::new(Cursor::new(nested_bytes)) {
            Ok(archive) => archive,
            Err(_) => continue,
        };
        let nested_entries = read_embedded_fabric_mod_metadata_from_archive(
            &mut nested_archive,
            &format!("{}!{nested_path}", jar_path.display()),
        )?;
        for entry in nested_entries {
            provided_ids.extend(provided_ids_for_metadata(&entry));
        }
    }

    Ok(provided_ids)
}

fn read_embedded_fabric_requirements_from_archive<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    source: &str,
) -> Result<EmbeddedFabricRequirements> {
    let metadata = match read_embedded_fabric_metadata(archive, source)? {
        Some(metadata) => metadata,
        None => return Ok(EmbeddedFabricRequirements::default()),
    };

    let mut requirements = EmbeddedFabricRequirements::default();
    let root_requirements = requirement_set_from_metadata(&metadata);
    requirements.root_entry = Some(root_requirements.clone());
    requirements.entries.push(root_requirements);

    for nested_path in nested_fabric_jar_paths(&metadata) {
        let mut nested_bytes = Vec::new();
        let read_nested = {
            let mut nested_file = match archive.by_name(&nested_path) {
                Ok(file) => file,
                Err(_) => continue,
            };
            nested_file.read_to_end(&mut nested_bytes).is_ok()
        };
        if !read_nested {
            continue;
        }

        let mut nested_archive = match zip::ZipArchive::new(Cursor::new(nested_bytes)) {
            Ok(archive) => archive,
            Err(_) => continue,
        };
        let nested_requirements = read_embedded_fabric_requirements_from_archive(
            &mut nested_archive,
            &format!("{source}!{nested_path}"),
        )?;
        requirements.entries.extend(nested_requirements.entries);
    }

    Ok(requirements)
}

fn read_embedded_fabric_mod_metadata_from_archive<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    source: &str,
) -> Result<Vec<EmbeddedFabricModMetadata>> {
    let metadata = match read_embedded_fabric_metadata(archive, source)? {
        Some(metadata) => metadata,
        None => return Ok(Vec::new()),
    };

    let mut entries = fabric_mod_metadata_from_json(&metadata).into_iter().collect::<Vec<_>>();

    for nested_path in nested_fabric_jar_paths(&metadata) {
        let mut nested_bytes = Vec::new();
        let read_nested = {
            let mut nested_file = match archive.by_name(&nested_path) {
                Ok(file) => file,
                Err(_) => continue,
            };
            nested_file.read_to_end(&mut nested_bytes).is_ok()
        };
        if !read_nested {
            continue;
        }

        let mut nested_archive = match zip::ZipArchive::new(Cursor::new(nested_bytes)) {
            Ok(archive) => archive,
            Err(_) => continue,
        };
        let nested_entries = read_embedded_fabric_mod_metadata_from_archive(
            &mut nested_archive,
            &format!("{source}!{nested_path}"),
        )?;
        entries.extend(nested_entries);
    }

    Ok(entries)
}

fn read_embedded_fabric_metadata<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    source: &str,
) -> Result<Option<serde_json::Value>> {
    let mut metadata_file = match archive.by_name("fabric.mod.json") {
        Ok(file) => file,
        Err(_) => return Ok(None),
    };

    let mut metadata = String::new();
    metadata_file
        .read_to_string(&mut metadata)
        .with_context(|| format!("failed to read fabric.mod.json from {source}"))?;

    Ok(serde_json::from_str(&metadata).ok())
}

fn fabric_mod_metadata_from_json(metadata: &serde_json::Value) -> Option<EmbeddedFabricModMetadata> {
    let mod_id = metadata
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let version = metadata
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("*")
        .to_string();

    let mut provides = metadata
        .get("provides")
        .map(json_predicates_to_strings)
        .unwrap_or_default();
    provides.retain(|value| !value.trim().is_empty() && value != &mod_id);
    provides.sort();
    provides.dedup();

    Some(EmbeddedFabricModMetadata {
        mod_id,
        version,
        provides,
        depends: fabric_dependency_map_from_json(metadata, "depends"),
        breaks: fabric_dependency_map_from_json(metadata, "breaks"),
    })
}

fn provided_ids_for_metadata(metadata: &EmbeddedFabricModMetadata) -> HashSet<String> {
    let mut provided_ids = HashSet::new();
    if !metadata.mod_id.trim().is_empty() {
        provided_ids.insert(metadata.mod_id.clone());
    }
    provided_ids.extend(metadata.provides.iter().cloned());
    provided_ids
}

fn requirement_set_from_metadata(metadata: &serde_json::Value) -> EmbeddedFabricRequirementSet {
    let depends = metadata
        .get("depends")
        .and_then(serde_json::Value::as_object);

    EmbeddedFabricRequirementSet {
        minecraft_predicates: depends
            .and_then(|depends| depends.get("minecraft"))
            .map(json_predicates_to_strings)
            .unwrap_or_default(),
        java_predicates: depends
            .and_then(|depends| depends.get("java"))
            .map(json_predicates_to_strings)
            .unwrap_or_default(),
    }
}

fn fabric_dependency_map_from_json(
    metadata: &serde_json::Value,
    key: &str,
) -> HashMap<String, Vec<String>> {
    let mut dependency_map = metadata
        .get(key)
        .and_then(serde_json::Value::as_object)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|(dependency_id, predicates)| {
                    let dependency_id = dependency_id.trim();
                    if dependency_id.is_empty() {
                        return None;
                    }

                    Some((
                        dependency_id.to_string(),
                        json_predicates_to_strings(predicates),
                    ))
                })
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();

    dependency_map.retain(|dependency_id, _| !dependency_id.trim().is_empty());
    dependency_map
}

fn nested_fabric_jar_paths(metadata: &serde_json::Value) -> Vec<String> {
    metadata
        .get("jars")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("file"))
        .filter_map(serde_json::Value::as_str)
        .map(ToString::to_string)
        .collect()
}

fn embedded_minecraft_requirements_match(
    requirements: &EmbeddedFabricRequirements,
    target: &ResolutionTarget,
) -> bool {
    if target.mod_loader != ModLoader::Fabric {
        return true;
    }

    let Some(entry) = requirements.root_entry.as_ref() else {
        return true;
    };

    entry.minecraft_predicates.is_empty()
        || entry.minecraft_predicates.iter().any(|predicate| {
            minecraft_version_predicate_matches(predicate, &target.minecraft_version)
        })
}

fn embedded_min_java_requirement(requirements: &EmbeddedFabricRequirements) -> Option<u32> {
    requirements
        .entries
        .iter()
        .filter_map(|entry| {
            entry
                .java_predicates
                .iter()
                .filter_map(|predicate| minimum_java_version_for_predicate(predicate))
                .min()
        })
        .max()
}

fn json_predicates_to_strings(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(predicate) => vec![predicate.to_string()],
        serde_json::Value::Array(predicates) => predicates
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(ToString::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn minecraft_version_predicate_matches(predicate: &str, concrete: &str) -> bool {
    let predicate = predicate.trim();
    if predicate.is_empty() || predicate == "*" {
        return true;
    }

    predicate
        .split("||")
        .map(str::trim)
        .filter(|branch| !branch.is_empty())
        .any(|branch| {
            branch
                .replace(',', " ")
                .split_whitespace()
                .all(|token| minecraft_version_token_matches(token, concrete))
        })
}

fn minecraft_version_token_matches(token: &str, concrete: &str) -> bool {
    let token = token.trim();
    if token.is_empty() || token == "*" {
        return true;
    }

    if let Some(expected) = token.strip_prefix('~') {
        return tilde_minecraft_version_matches(expected.trim(), concrete);
    }

    for (prefix, ordering) in [
        (">=", Some(std::cmp::Ordering::Greater)),
        ("<=", Some(std::cmp::Ordering::Less)),
        (">", Some(std::cmp::Ordering::Greater)),
        ("<", Some(std::cmp::Ordering::Less)),
        ("=", None),
    ] {
        if let Some(expected) = token.strip_prefix(prefix) {
            let Some(actual_ordering) = compare_minecraft_versions(concrete, expected.trim())
            else {
                return true;
            };
            return match (prefix, ordering) {
                (">=", Some(std::cmp::Ordering::Greater)) => {
                    actual_ordering == std::cmp::Ordering::Greater
                        || actual_ordering == std::cmp::Ordering::Equal
                }
                ("<=", Some(std::cmp::Ordering::Less)) => {
                    actual_ordering == std::cmp::Ordering::Less
                        || actual_ordering == std::cmp::Ordering::Equal
                }
                (">", Some(std::cmp::Ordering::Greater)) => {
                    actual_ordering == std::cmp::Ordering::Greater
                }
                ("<", Some(std::cmp::Ordering::Less)) => {
                    actual_ordering == std::cmp::Ordering::Less
                }
                ("=", None) => actual_ordering == std::cmp::Ordering::Equal,
                _ => true,
            };
        }
    }

    crate::modrinth::mc_version_matches(token, concrete)
        || compare_minecraft_versions(concrete, token)
            .is_some_and(|ordering| ordering == std::cmp::Ordering::Equal)
}

fn tilde_minecraft_version_matches(expected: &str, concrete: &str) -> bool {
    let Some(actual_ordering) = compare_minecraft_versions(concrete, expected) else {
        return true;
    };
    if actual_ordering == std::cmp::Ordering::Less {
        return false;
    }

    let Some(mut upper_parts) = parse_minecraft_version_parts(expected) else {
        return true;
    };
    if upper_parts.len() >= 2 {
        upper_parts[1] += 1;
        if upper_parts.len() > 2 {
            upper_parts[2..].fill(0);
        } else {
            upper_parts.push(0);
        }
    } else if let Some(last) = upper_parts.last_mut() {
        *last += 1;
    } else {
        return true;
    }

    let upper_bound = upper_parts
        .into_iter()
        .map(|part| part.to_string())
        .collect::<Vec<_>>()
        .join(".");

    compare_minecraft_versions(concrete, &upper_bound)
        .is_some_and(|ordering| ordering == std::cmp::Ordering::Less)
}

fn compare_minecraft_versions(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    let mut left_parts = parse_minecraft_version_parts(left)?;
    let mut right_parts = parse_minecraft_version_parts(right)?;
    let max_len = left_parts.len().max(right_parts.len());
    left_parts.resize(max_len, 0);
    right_parts.resize(max_len, 0);
    Some(left_parts.cmp(&right_parts))
}

fn parse_minecraft_version_parts(value: &str) -> Option<Vec<u64>> {
    value
        .split('.')
        .map(|segment| {
            let numeric = segment
                .trim()
                .split(|ch: char| !ch.is_ascii_digit())
                .next()
                .unwrap_or("");
            if numeric.is_empty() {
                None
            } else {
                numeric.parse::<u64>().ok()
            }
        })
        .collect()
}

fn minimum_java_version_for_predicate(predicate: &str) -> Option<u32> {
    predicate
        .split("||")
        .map(str::trim)
        .filter(|branch| !branch.is_empty())
        .filter_map(minimum_java_version_for_branch)
        .min()
}

fn fabric_dependency_predicates_match(predicates: &[String], concrete: &str) -> bool {
    predicates.is_empty()
        || predicates
            .iter()
            .any(|predicate| fabric_dependency_predicate_matches(predicate, concrete))
}

fn fabric_dependency_predicate_matches(predicate: &str, concrete: &str) -> bool {
    let predicate = predicate.trim();
    if predicate.is_empty() || predicate == "*" {
        return true;
    }

    predicate
        .split("||")
        .map(str::trim)
        .filter(|branch| !branch.is_empty())
        .any(|branch| {
            branch
                .replace(',', " ")
                .split_whitespace()
                .all(|token| fabric_dependency_token_matches(token, concrete))
        })
}

fn fabric_dependency_token_matches(token: &str, concrete: &str) -> bool {
    let token = token.trim();
    if token.is_empty() || token == "*" {
        return true;
    }

    for (prefix, ordering) in [
        (">=", Some(std::cmp::Ordering::Greater)),
        ("<=", Some(std::cmp::Ordering::Less)),
        (">", Some(std::cmp::Ordering::Greater)),
        ("<", Some(std::cmp::Ordering::Less)),
        ("=", None),
    ] {
        if let Some(expected) = token.strip_prefix(prefix) {
            let expected = expected.trim();
            let Some(actual_ordering) = compare_fabric_dependency_versions(concrete, expected) else {
                return concrete.eq_ignore_ascii_case(expected);
            };
            return match (prefix, ordering) {
                (">=", Some(std::cmp::Ordering::Greater)) => {
                    actual_ordering == std::cmp::Ordering::Greater
                        || actual_ordering == std::cmp::Ordering::Equal
                }
                ("<=", Some(std::cmp::Ordering::Less)) => {
                    actual_ordering == std::cmp::Ordering::Less
                        || actual_ordering == std::cmp::Ordering::Equal
                }
                (">", Some(std::cmp::Ordering::Greater)) => {
                    actual_ordering == std::cmp::Ordering::Greater
                }
                ("<", Some(std::cmp::Ordering::Less)) => {
                    actual_ordering == std::cmp::Ordering::Less
                }
                ("=", None) => actual_ordering == std::cmp::Ordering::Equal,
                _ => true,
            };
        }
    }

    compare_fabric_dependency_versions(concrete, token)
        .is_some_and(|ordering| ordering == std::cmp::Ordering::Equal)
        || concrete.eq_ignore_ascii_case(token)
}

fn compare_fabric_dependency_versions(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    if let Some(ordering) = compare_semver_like_versions(left, right) {
        return Some(ordering);
    }

    let left = parse_fabric_dependency_version(left)?;
    let right = parse_fabric_dependency_version(right)?;
    let max_len = left.core.len().max(right.core.len());

    for index in 0..max_len {
        let left_part = left.core.get(index).copied().unwrap_or(0);
        let right_part = right.core.get(index).copied().unwrap_or(0);
        match left_part.cmp(&right_part) {
            std::cmp::Ordering::Equal => {}
            ordering => return Some(ordering),
        }
    }

    match (&left.prerelease, &right.prerelease) {
        (None, None) => Some(std::cmp::Ordering::Equal),
        (Some(_), None) => Some(std::cmp::Ordering::Less),
        (None, Some(_)) => Some(std::cmp::Ordering::Greater),
        (Some(left_pre), Some(right_pre)) => Some(compare_prerelease_identifiers(left_pre, right_pre)),
    }
}

fn compare_semver_like_versions(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    let left = normalize_semver_like_version(left)?;
    let right = normalize_semver_like_version(right)?;
    Some(left.cmp(&right))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedFabricDependencyVersion {
    core: Vec<u64>,
    prerelease: Option<Vec<String>>,
}

fn parse_fabric_dependency_version(value: &str) -> Option<ParsedFabricDependencyVersion> {
    let mut main_and_build = value.trim().splitn(2, '+');
    let main = main_and_build.next()?.trim();
    if main.is_empty() {
        return None;
    }

    let mut core_and_pre = main.splitn(2, '-');
    let core = core_and_pre
        .next()?
        .split('.')
        .map(|segment| segment.trim().parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    if core.is_empty() {
        return None;
    }

    let prerelease = core_and_pre.next().map(|value| {
        value
            .split('.')
            .map(|segment| segment.trim().to_ascii_lowercase())
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>()
    });

    Some(ParsedFabricDependencyVersion { core, prerelease })
}

fn normalize_semver_like_version(value: &str) -> Option<semver::Version> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let (core_and_pre, build) = match value.split_once('+') {
        Some((left, right)) => (left.trim(), Some(right.trim())),
        None => (value, None),
    };
    let (core, prerelease) = match core_and_pre.split_once('-') {
        Some((left, right)) => (left.trim(), Some(right.trim())),
        None => (core_and_pre, None),
    };

    let mut core_parts = core
        .split('.')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if core_parts.is_empty() || core_parts.len() > 3 {
        return None;
    }
    if core_parts.iter().any(|segment| segment.parse::<u64>().is_err()) {
        return None;
    }
    while core_parts.len() < 3 {
        core_parts.push("0");
    }

    let mut normalized = core_parts.join(".");
    if let Some(prerelease) = prerelease {
        if prerelease.is_empty() {
            return None;
        }
        normalized.push('-');
        normalized.push_str(prerelease);
    }
    if let Some(build) = build {
        if !build.is_empty() {
            normalized.push('+');
            normalized.push_str(build);
        }
    }

    semver::Version::parse(&normalized).ok()
}

fn compare_prerelease_identifiers(left: &[String], right: &[String]) -> std::cmp::Ordering {
    for index in 0..left.len().max(right.len()) {
        let Some(left_part) = left.get(index) else {
            return std::cmp::Ordering::Less;
        };
        let Some(right_part) = right.get(index) else {
            return std::cmp::Ordering::Greater;
        };

        let left_numeric = left_part.parse::<u64>().ok();
        let right_numeric = right_part.parse::<u64>().ok();
        let ordering = match (left_numeric, right_numeric) {
            (Some(left_numeric), Some(right_numeric)) => left_numeric.cmp(&right_numeric),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => left_part.cmp(right_part),
        };
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }

    std::cmp::Ordering::Equal
}

fn minimum_java_version_for_branch(branch: &str) -> Option<u32> {
    let mut minimum = 0u32;

    for token in branch.replace(',', " ").split_whitespace() {
        let token = token.trim();
        if token.is_empty() || token == "*" {
            continue;
        }

        if let Some(value) = token.strip_prefix(">=") {
            minimum = minimum.max(value.trim().parse::<u32>().ok()?);
            continue;
        }
        if let Some(value) = token.strip_prefix('>') {
            minimum = minimum.max(value.trim().parse::<u32>().ok()?.saturating_add(1));
            continue;
        }
        if let Some(value) = token.strip_prefix('=') {
            minimum = minimum.max(value.trim().parse::<u32>().ok()?);
            continue;
        }
        if token.starts_with('<') {
            continue;
        }

        minimum = minimum.max(token.parse::<u32>().ok()?);
    }

    Some(minimum)
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
                format!(
                    "{base}/{}",
                    relative_path.to_string_lossy().replace('\\', "/")
                )
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
        RawRepo::new(&connection)
            .load_active_account()
            .ok()
            .flatten()
    };
    let db_path = launcher_paths.database_path().to_path_buf();
    drop(connection); // Release connection before async work.

    if let Some(raw) = raw_account {
        let profile = raw
            .profile_data
            .as_deref()
            .and_then(|pd| serde_json::from_str::<serde_json::Value>(pd).ok());

        let username = profile
            .as_ref()
            .and_then(|v| v.get("username").and_then(|u| u.as_str()).map(String::from))
            .or(raw.xbox_gamertag.clone())
            .unwrap_or_else(|| "CubicPlayer".to_string());

        let uuid = format_uuid_with_dashes(
            &raw.minecraft_uuid
                .unwrap_or_else(|| deterministic_offline_uuid(&username).to_string()),
        );

        let microsoft_id = raw.microsoft_id.clone();

        // Try to refresh the Minecraft access token using the stored MS refresh token.
        let ms_refresh = profile.as_ref().and_then(|v| {
            v.get("ms_refresh_token")
                .and_then(|t| t.as_str())
                .map(String::from)
        });

        if let Some(refresh_token) = ms_refresh {
            if !refresh_token.is_empty() {
                eprintln!(
                    "[Auth] Has refresh token (len={}), attempting refresh...",
                    refresh_token.len()
                );

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

                match oauth_client
                    .refresh_access_token(&config, &refresh_token)
                    .await
                {
                    Ok(ms_tokens) => {
                        eprintln!("[Auth] MS token refresh OK, authenticating with Xbox/MC...");
                        let chain = crate::microsoft_auth::MinecraftAuthChain::new();
                        match chain
                            .authenticate(
                                &ms_tokens.access_token,
                                ms_tokens.refresh_token.as_deref(),
                                ms_tokens.user_id.as_deref(),
                            )
                            .await
                        {
                            Ok(login) => {
                                eprintln!(
                                    "[Auth] Full auth chain OK: username={}",
                                    login.minecraft_username
                                );
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
                                    let _ = RawRepo::new(&conn).update_profile_data(
                                        &microsoft_id,
                                        &new_profile.to_string(),
                                    );
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
    required_version: u32,
) -> Result<PathBuf> {
    if let Some(path) = &settings.java_path_override {
        let inspector = CommandJavaBinaryInspector;
        let probe = inspector
            .inspect(path)
            .with_context(|| format!("failed to inspect Java override at {}", path.display()))?
            .with_context(|| format!("Java override '{}' is not runnable", path.display()))?;
        if probe.version < required_version {
            bail!(
                "Java override '{}' reports version {}, but launch requires Java {} or higher for Minecraft {}",
                path.display(),
                probe.version,
                required_version,
                target.minecraft_version
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

    if let Some(installation) = select_java_for_requirement(&installations, required_version) {
        return Ok(installation.path);
    }

    // No suitable Java found — auto-download via Adoptium.
    emit_log(
        app_handle,
        ProcessLogStream::Stdout,
        format!(
            "[Java] No Java {} found, downloading from Adoptium...",
            required_version
        ),
    )?;
    emit_progress(
        app_handle,
        "resolving",
        85,
        "Downloading Java",
        &format!("Fetching Java {} runtime from Adoptium.", required_version),
    )?;

    let adoptium = AdoptiumClient::new();
    let os = host_adoptium_os();
    let arch = normalize_adoptium_architecture(std::env::consts::ARCH);

    let package = adoptium
        .fetch_latest_jre_package(required_version, os, arch)
        .await?
        .with_context(|| {
            format!(
                "Adoptium has no JRE {} for {}/{}",
                required_version, os, arch
            )
        })?;

    let plan = plan_runtime_download(
        launcher_paths.java_runtimes_dir(),
        required_version,
        package,
        os,
        arch,
    );

    // Download the archive.
    adoptium
        .download_package(&plan.package, &plan.archive_path)
        .await?;

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
        archive.extract(&install_dir).with_context(|| {
            format!(
                "failed to extract Java archive to {}",
                install_dir.display()
            )
        })?;
        // Clean up archive file.
        let _ = std::fs::remove_file(&archive_path);
        Ok(())
    })
    .await
    .context("Java extraction task panicked")??;

    // Re-scan and select.
    let installations = discover_java_installations(launcher_paths.java_runtimes_dir())?;
    persist_java_installations(&connection, &installations)?;

    select_java_for_requirement(&installations, required_version)
        .map(|installation| installation.path)
        .with_context(|| {
            format!(
                "Java {} was downloaded but could not be found after extraction. Check {}",
                required_version,
                launcher_paths.java_runtimes_dir().display()
            )
        })
}

fn format_uuid_with_dashes(uuid: &str) -> String {
    let clean = uuid.replace('-', "");
    if clean.len() == 32 {
        format!(
            "{}-{}-{}-{}-{}",
            &clean[0..8],
            &clean[8..12],
            &clean[12..16],
            &clean[16..20],
            &clean[20..32]
        )
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

fn emit_log(app_handle: &tauri::AppHandle, stream: ProcessLogStream, line: String) -> Result<()> {
    if let Some(session) = current_launch_log_session() {
        let _ = session.append_launcher_line(stream, &line);
    }
    app_handle
        .emit(MINECRAFT_LOG_EVENT, ProcessLogEvent { stream, line })
        .map_err(|error| anyhow::anyhow!(error.to_string()))
}

fn emit_launcher_issue(
    app_handle: &tauri::AppHandle,
    title: &str,
    message: &str,
    detail: &str,
    severity: &str,
    scope: &str,
) -> Result<()> {
    if let Some(session) = current_launch_log_session() {
        let _ = session.append_line(
            "issues.log",
            &format!("[{severity}] {title} | {message} | {detail}"),
        );
    }
    app_handle
        .emit(
            LAUNCHER_ERROR_EVENT,
            LauncherErrorEvent {
                id: unique_error_id(),
                title: title.to_string(),
                message: message.to_string(),
                detail: detail.to_string(),
                severity: severity.to_string(),
                scope: scope.to_string(),
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
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;

    use crate::app_shell::{ShellGlobalSettings, ShellModListOverrides};
    use crate::modrinth::{DependencyType, ModrinthDependency, ModrinthVersion};
    use crate::resolver::{ModLoader, ResolutionTarget};

    use super::{
        build_instance_root, build_top_level_owner_map, collect_selected_project_ids,
        embedded_min_java_requirement, fabric_dependency_predicates_match,
        maven_artifact_relative_path, minecraft_version_predicate_matches,
        minimum_java_version_for_predicate, parse_mod_loader, substitute_known_placeholders,
        validate_final_fabric_runtime, validate_selected_parent_dependencies,
        DependencyLink, EffectiveLaunchSettings, EmbeddedFabricModMetadata,
        EmbeddedFabricRequirementSet, EmbeddedFabricRequirements, LaunchPlaceholders,
        OwnedEmbeddedFabricModMetadata, PlayerIdentity,
    };

    fn global_settings() -> ShellGlobalSettings {
        ShellGlobalSettings {
            min_ram_mb: 2048,
            max_ram_mb: 4096,
            custom_jvm_args: "-Dglobal=true".into(),
            profiler_enabled: false,
            cache_only_mode: true,
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
        assert!(settings.cache_only_mode);
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

    fn sample_version(project_id: &str, version_id: &str) -> ModrinthVersion {
        ModrinthVersion {
            id: version_id.into(),
            project_id: project_id.into(),
            version_number: "1.0.0".into(),
            name: format!("{project_id} {version_id}"),
            game_versions: vec!["1.21.5".into()],
            loaders: vec!["fabric".into()],
            dependencies: Vec::new(),
            files: Vec::new(),
            date_published: "2024-08-15T10:00:00.000Z".into(),
        }
    }

    fn metadata_entry(
        owner_project_id: &str,
        mod_id: &str,
        version: &str,
    ) -> OwnedEmbeddedFabricModMetadata {
        OwnedEmbeddedFabricModMetadata {
            owner_project_id: owner_project_id.into(),
            metadata: EmbeddedFabricModMetadata {
                mod_id: mod_id.into(),
                version: version.into(),
                provides: Vec::new(),
                depends: HashMap::new(),
                breaks: HashMap::new(),
            },
        }
    }

    #[test]
    fn java_predicates_extract_minimum_requirement() {
        assert_eq!(minimum_java_version_for_predicate(">=22"), Some(22));
        assert_eq!(minimum_java_version_for_predicate(">21"), Some(22));
        assert_eq!(minimum_java_version_for_predicate("21"), Some(21));
        assert_eq!(
            embedded_min_java_requirement(&EmbeddedFabricRequirements {
                root_entry: None,
                entries: vec![
                    EmbeddedFabricRequirementSet {
                        minecraft_predicates: Vec::new(),
                        java_predicates: vec![">=21".into()],
                    },
                    EmbeddedFabricRequirementSet {
                        minecraft_predicates: Vec::new(),
                        java_predicates: vec![">=22".into(), ">=23".into()],
                    },
                ],
            }),
            Some(22)
        );
    }

    #[test]
    fn tilde_minecraft_predicates_match_target_patch_line() {
        assert!(minecraft_version_predicate_matches("~1.21.6", "1.21.6"));
        assert!(minecraft_version_predicate_matches("~1.21.6", "1.21.9"));
        assert!(!minecraft_version_predicate_matches("~1.21.6", "1.22.0"));
    }

    #[test]
    fn fabric_dependency_predicates_match_semver_ranges() {
        assert!(fabric_dependency_predicates_match(&["<7.0.0".into()], "6.2.9"));
        assert!(fabric_dependency_predicates_match(&[">=17.0.6".into()], "17.0.6"));
        assert!(!fabric_dependency_predicates_match(&["<3.0.0".into()], "3.0.0"));
        assert!(fabric_dependency_predicates_match(
            &["<1.8.0".into()],
            "1.8.0-beta.4+mc1.21.1"
        ));
        assert!(!fabric_dependency_predicates_match(
            &[">=1.8.0".into()],
            "1.8.0-beta.4+mc1.21.1"
        ));
    }

    #[test]
    fn exact_parent_dependency_check_uses_project_ids() {
        let mut iris = sample_version("YL57xq9U", "iris-1");
        iris.dependencies.push(ModrinthDependency {
            version_id: Some("sodium-0.6.12".into()),
            project_id: Some("AANobbMI".into()),
            dependency_type: DependencyType::Required,
            file_name: None,
        });
        let sodium = ModrinthVersion {
            id: "sodium-0.6.13".into(),
            ..sample_version("AANobbMI", "sodium-0.6.13")
        };
        let parent_versions = vec![iris.clone(), sodium.clone()];
        let selected_parent_versions = HashMap::from([
            (iris.project_id.clone(), iris),
            (sodium.project_id.clone(), sodium),
        ]);
        let selected_project_ids = collect_selected_project_ids(&parent_versions);

        let excluded = validate_selected_parent_dependencies(
            &parent_versions,
            &selected_parent_versions,
            &selected_project_ids,
        );

        assert_eq!(
            excluded,
            std::collections::HashSet::from(["YL57xq9U".to_string()])
        );
    }

    #[test]
    fn owner_map_propagates_transitive_dependency_owners() {
        let parent_versions = vec![sample_version("top-level", "top-level-1")];
        let owner_map = build_top_level_owner_map(
            &parent_versions,
            &[
                DependencyLink {
                    parent_mod_id: "top-level".into(),
                    dependency_id: "mid".into(),
                    specific_version: None,
                    jar_filename: "mid.jar".into(),
                },
                DependencyLink {
                    parent_mod_id: "mid".into(),
                    dependency_id: "leaf".into(),
                    specific_version: None,
                    jar_filename: "leaf.jar".into(),
                },
            ],
        );

        assert_eq!(
            owner_map.get("leaf"),
            Some(&HashSet::from(["top-level".to_string()]))
        );
    }

    #[test]
    fn final_fabric_validation_excludes_top_level_on_breaks_conflict() {
        let owner_map = HashMap::from([(
            "puzzle-project".to_string(),
            HashSet::from(["puzzle-project".to_string()]),
        )]);
        let mut puzzle = metadata_entry("puzzle-project", "puzzle", "2.3.0");
        puzzle
            .metadata
            .breaks
            .insert("entity_model_features".into(), vec!["<3.0.0".into()]);
        let emf = metadata_entry("emf-project", "entity_model_features", "2.4.1");

        let issues = validate_final_fabric_runtime(&[puzzle, emf], &owner_map);

        assert_eq!(
            issues.get("puzzle-project").map(|issue| issue.reason_code),
            Some("breaks_conflict")
        );
    }

    #[test]
    fn final_fabric_validation_excludes_top_level_on_prerelease_breaks_conflict() {
        let owner_map = HashMap::from([
            (
                "sodium-project".to_string(),
                HashSet::from(["sodium-project".to_string()]),
            ),
            (
                "reeses-project".to_string(),
                HashSet::from(["sodiumoptionsapi-project".to_string()]),
            ),
        ]);
        let mut sodium = metadata_entry("sodium-project", "sodium", "0.6.13+mc1.21.1");
        sodium
            .metadata
            .breaks
            .insert("reeses-sodium-options".into(), vec!["<1.8.0".into()]);
        let reeses = metadata_entry(
            "reeses-project",
            "reeses-sodium-options",
            "1.8.0-beta.4+mc1.21.1",
        );

        let issues = validate_final_fabric_runtime(&[sodium, reeses], &owner_map);

        assert_eq!(
            issues.get("sodium-project").map(|issue| issue.reason_code),
            Some("breaks_conflict")
        );
    }

    #[test]
    fn final_fabric_validation_excludes_top_level_on_missing_dependency() {
        let owner_map = HashMap::from([
            (
                "sodiumoptionsapi-project".to_string(),
                HashSet::from(["sodiumoptionsapi-project".to_string()]),
            ),
            (
                "embedded-helper-project".to_string(),
                HashSet::from(["sodiumoptionsapi-project".to_string()]),
            ),
        ]);
        let mut sodium_options_api =
            metadata_entry("sodiumoptionsapi-project", "sodiumoptionsapi", "1.0.10");
        sodium_options_api
            .metadata
            .depends
            .insert("reeses-sodium-options".into(), vec!["*".into()]);

        let issues = validate_final_fabric_runtime(&[sodium_options_api], &owner_map);

        assert_eq!(
            issues
                .get("sodiumoptionsapi-project")
                .and_then(|issue| issue.dependency_id.as_deref()),
            Some("reeses-sodium-options")
        );
        assert_eq!(
            issues
                .get("sodiumoptionsapi-project")
                .map(|issue| issue.reason_code),
            Some("missing_dependency")
        );
    }
}
