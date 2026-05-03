use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use tauri::State;

use crate::launcher_paths::LauncherPaths;

const DEBUG_TRACE_FILENAME: &str = "debug-trace.txt";

#[tauri::command]
pub fn append_debug_trace_command(
    launcher_paths: State<'_, LauncherPaths>,
    entry: String,
) -> Result<(), String> {
    append_debug_trace_to_root(launcher_paths.root_dir(), &entry).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn clear_debug_trace_command(
    launcher_paths: State<'_, LauncherPaths>,
) -> Result<String, String> {
    clear_debug_trace_at_root(launcher_paths.root_dir())
        .map(|path| path.display().to_string())
        .map_err(|error| error.to_string())
}

pub fn debug_trace_path(root_dir: &Path) -> PathBuf {
    root_dir.join(DEBUG_TRACE_FILENAME)
}

pub fn append_debug_trace_to_root(root_dir: &Path, entry: &str) -> Result<()> {
    fs::create_dir_all(root_dir)
        .with_context(|| format!("failed to create {}", root_dir.display()))?;

    let trace_path = debug_trace_path(root_dir);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&trace_path)
        .with_context(|| format!("failed to open {}", trace_path.display()))?;

    writeln!(file, "{} {}", trace_timestamp(), entry)
        .with_context(|| format!("failed to append {}", trace_path.display()))?;

    Ok(())
}

pub fn clear_debug_trace_at_root(root_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(root_dir)
        .with_context(|| format!("failed to create {}", root_dir.display()))?;

    let trace_path = debug_trace_path(root_dir);
    fs::write(
        &trace_path,
        format!("{} [debug] trace cleared\n", trace_timestamp()),
    )
    .with_context(|| format!("failed to reset {}", trace_path.display()))?;

    Ok(trace_path)
}

fn trace_timestamp() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("[{}.{:03}]", duration.as_secs(), duration.subsec_millis())
}
