use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::launch_command::PreparedLaunchCommand;

pub const MINECRAFT_LOG_EVENT: &str = "minecraft-log";
pub const MINECRAFT_EXIT_EVENT: &str = "minecraft-exit";

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProcessLogStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessLogEvent {
    pub stream: ProcessLogStream,
    pub line: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessExitEvent {
    pub success: bool,
    pub exit_code: Option<i32>,
}

pub trait ProcessEventSink: Send + Sync {
    fn emit_log(&self, event: ProcessLogEvent) -> Result<()>;
    fn emit_exit(&self, event: ProcessExitEvent) -> Result<()>;
}

#[derive(Clone)]
pub struct TauriProcessEventSink {
    app_handle: tauri::AppHandle,
}

impl TauriProcessEventSink {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self { app_handle }
    }
}

impl ProcessEventSink for TauriProcessEventSink {
    fn emit_log(&self, event: ProcessLogEvent) -> Result<()> {
        self.app_handle
            .emit(MINECRAFT_LOG_EVENT, event)
            .map_err(|error| anyhow!(error.to_string()))
    }

    fn emit_exit(&self, event: ProcessExitEvent) -> Result<()> {
        self.app_handle
            .emit(MINECRAFT_EXIT_EVENT, event)
            .map_err(|error| anyhow!(error.to_string()))
    }
}

pub struct ManagedProcess {
    pub pid: u32,
    monitor_thread: JoinHandle<Result<ProcessExitEvent>>,
}

impl ManagedProcess {
    pub fn wait(self) -> Result<ProcessExitEvent> {
        self.monitor_thread
            .join()
            .map_err(|_| anyhow!("process monitor thread panicked"))?
    }
}

pub fn spawn_and_stream_process(
    command: PreparedLaunchCommand,
    event_sink: Arc<dyn ProcessEventSink>,
) -> Result<ManagedProcess> {
    let mut cmd = Command::new(&command.program);
    cmd.args(&command.args)
        .current_dir(&command.current_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to spawn process '{}'", command.program.display()))?;

    let pid = child.id();
    let stdout = child
        .stdout
        .take()
        .context("spawned process is missing piped stdout")?;
    let stderr = child
        .stderr
        .take()
        .context("spawned process is missing piped stderr")?;

    let stdout_thread =
        spawn_reader_thread(stdout, ProcessLogStream::Stdout, Arc::clone(&event_sink));
    let stderr_thread =
        spawn_reader_thread(stderr, ProcessLogStream::Stderr, Arc::clone(&event_sink));

    let monitor_thread = thread::spawn(move || -> Result<ProcessExitEvent> {
        let status = child
            .wait()
            .context("failed while waiting for spawned process")?;

        join_reader_thread(stdout_thread, "stdout")?;
        join_reader_thread(stderr_thread, "stderr")?;

        let exit_event = ProcessExitEvent {
            success: status.success(),
            exit_code: status.code(),
        };
        event_sink.emit_exit(exit_event.clone())?;

        Ok(exit_event)
    });

    Ok(ManagedProcess {
        pid,
        monitor_thread,
    })
}

fn spawn_reader_thread<R: Read + Send + 'static>(
    stream: R,
    log_stream: ProcessLogStream,
    event_sink: Arc<dyn ProcessEventSink>,
) -> JoinHandle<Result<()>> {
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            let line = line.context("failed to read process output line")?;
            event_sink.emit_log(ProcessLogEvent {
                stream: log_stream,
                line,
            })?;
        }

        Ok(())
    })
}

fn join_reader_thread(handle: JoinHandle<Result<()>>, name: &str) -> Result<()> {
    handle
        .join()
        .map_err(|_| anyhow!("{} reader thread panicked", name))?
        .with_context(|| format!("{} reader thread failed", name))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use crate::launch_command::PreparedLaunchCommand;

    use super::{
        spawn_and_stream_process, ProcessEventSink, ProcessExitEvent, ProcessLogEvent,
        ProcessLogStream,
    };

    #[derive(Default)]
    struct RecordingProcessEventSink {
        logs: Mutex<Vec<ProcessLogEvent>>,
        exits: Mutex<Vec<ProcessExitEvent>>,
    }

    impl RecordingProcessEventSink {
        fn logs(&self) -> Vec<ProcessLogEvent> {
            self.logs.lock().expect("logs mutex poisoned").clone()
        }

        fn exits(&self) -> Vec<ProcessExitEvent> {
            self.exits.lock().expect("exits mutex poisoned").clone()
        }
    }

    impl ProcessEventSink for RecordingProcessEventSink {
        fn emit_log(&self, event: ProcessLogEvent) -> anyhow::Result<()> {
            self.logs.lock().expect("logs mutex poisoned").push(event);
            Ok(())
        }

        fn emit_exit(&self, event: ProcessExitEvent) -> anyhow::Result<()> {
            self.exits.lock().expect("exits mutex poisoned").push(event);
            Ok(())
        }
    }

    #[cfg(target_os = "windows")]
    fn shell_command(script: &str) -> PreparedLaunchCommand {
        PreparedLaunchCommand {
            program: PathBuf::from("powershell"),
            args: vec!["-NoProfile".into(), "-Command".into(), script.into()],
            current_dir: std::env::temp_dir(),
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn shell_command(script: &str) -> PreparedLaunchCommand {
        PreparedLaunchCommand {
            program: PathBuf::from("sh"),
            args: vec!["-c".into(), script.into()],
            current_dir: std::env::temp_dir(),
        }
    }

    #[cfg(target_os = "windows")]
    const STREAM_SCRIPT: &str =
        "Write-Output 'stdout-line'; [Console]::Error.WriteLine('stderr-line'); exit 7";

    #[cfg(not(target_os = "windows"))]
    const STREAM_SCRIPT: &str = "printf 'stdout-line\n'; printf 'stderr-line\n' 1>&2; exit 7";

    #[test]
    fn streams_stdout_stderr_and_exit_events() {
        let sink = std::sync::Arc::new(RecordingProcessEventSink::default());
        let process = spawn_and_stream_process(shell_command(STREAM_SCRIPT), sink.clone())
            .expect("process should spawn");

        let exit_event = process.wait().expect("process should complete");
        let logs = sink.logs();
        let exits = sink.exits();

        assert!(!exit_event.success);
        assert_eq!(exit_event.exit_code, Some(7));
        assert_eq!(exits, vec![exit_event.clone()]);
        assert!(logs.contains(&ProcessLogEvent {
            stream: ProcessLogStream::Stdout,
            line: "stdout-line".into(),
        }));
        assert!(logs.contains(&ProcessLogEvent {
            stream: ProcessLogStream::Stderr,
            line: "stderr-line".into(),
        }));
    }
}
