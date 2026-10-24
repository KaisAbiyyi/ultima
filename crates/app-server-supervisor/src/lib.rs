//! Bounded Rust stdio supervisor for the official Codex App Server.
//! JSON-RPC routing and tool-approval policy belong to NC-05.

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStdin, Command},
    sync::mpsc,
    task::JoinHandle,
    time::timeout,
};

pub const MAX_JSONL_BYTES: usize = 1024 * 1024;
pub const DEFAULT_QUEUE_CAPACITY: usize = 64;
const MAX_QUEUE_CAPACITY: usize = 128;
const STOP_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug)]
pub enum SupervisorError {
    InvalidConfig(&'static str),
    Io(io::Error),
    Json(serde_json::Error),
    OversizedInput,
    WriteTimeout,
    NotRunning,
    AlreadyRunning,
}

impl fmt::Display for SupervisorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // No payloads, paths, environment, or stderr in diagnostics.
        let message = match self {
            Self::InvalidConfig(_) => "invalid supervisor configuration",
            Self::Io(_) => "engine process I/O failure",
            Self::Json(_) => "invalid outbound JSON",
            Self::OversizedInput => "outbound JSONL exceeds configured limit",
            Self::WriteTimeout => "engine stdin write timed out",
            Self::NotRunning => "engine is not running",
            Self::AlreadyRunning => "engine is already running",
        };
        f.write_str(message)
    }
}
impl std::error::Error for SupervisorError {}
impl From<io::Error> for SupervisorError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<serde_json::Error> for SupervisorError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Clone, Debug)]
pub struct EngineConfig {
    executable: PathBuf,
    cwd: PathBuf,
    max_line_bytes: usize,
    queue_capacity: usize,
    write_timeout: Duration,
}

impl EngineConfig {
    /// Both trust roots must be explicitly supplied by the host. Never use a shell.
    pub fn new(
        executable: impl AsRef<Path>,
        trusted_executable_root: impl AsRef<Path>,
        project_directory: impl AsRef<Path>,
        trusted_project_root: impl AsRef<Path>,
    ) -> Result<Self, SupervisorError> {
        let root = fs::canonicalize(trusted_executable_root)?;
        let project_root = fs::canonicalize(trusted_project_root)?;
        let executable = fs::canonicalize(executable)?;
        let cwd = fs::canonicalize(project_directory)?;
        if !root.is_dir() || !project_root.is_dir() {
            return Err(SupervisorError::InvalidConfig("root"));
        }
        if !executable.is_file() || !executable.starts_with(&root) {
            return Err(SupervisorError::InvalidConfig("executable"));
        }
        #[cfg(windows)]
        if !executable
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
        {
            return Err(SupervisorError::InvalidConfig("native exe"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if fs::metadata(&executable)?.permissions().mode() & 0o111 == 0 {
                return Err(SupervisorError::InvalidConfig("permissions"));
            }
        }
        if !cwd.is_dir() || !cwd.starts_with(&project_root) {
            return Err(SupervisorError::InvalidConfig("cwd"));
        }
        Ok(Self {
            executable,
            cwd,
            max_line_bytes: MAX_JSONL_BYTES,
            queue_capacity: DEFAULT_QUEUE_CAPACITY,
            write_timeout: Duration::from_secs(5),
        })
    }

    /// Search only caller-approved directories, never arbitrary PATH entries.
    pub fn discover_in_trusted_directories(directories: &[PathBuf]) -> Option<PathBuf> {
        #[cfg(windows)]
        const NAME: &str = "codex.exe";
        #[cfg(not(windows))]
        const NAME: &str = "codex";
        directories.iter().find_map(|dir| {
            let root = fs::canonicalize(dir).ok()?;
            let executable = fs::canonicalize(root.join(NAME)).ok()?;
            (executable.is_file() && executable.starts_with(&root)).then_some(executable)
        })
    }

    pub fn with_limits(
        mut self,
        max_line_bytes: usize,
        queue_capacity: usize,
        write_timeout: Duration,
    ) -> Result<Self, SupervisorError> {
        if !(1..=MAX_JSONL_BYTES).contains(&max_line_bytes)
            || !(1..=MAX_QUEUE_CAPACITY).contains(&queue_capacity)
            || write_timeout.is_zero()
        {
            return Err(SupervisorError::InvalidConfig("bounds"));
        }
        self.max_line_bytes = max_line_bytes;
        self.queue_capacity = queue_capacity;
        self.write_timeout = write_timeout;
        Ok(self)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum OutputEvent {
    Line(String),
    Closed,
    LineTooLong,
    InvalidUtf8,
    ReadFailed,
}

async fn read_stdout<R: AsyncRead + Unpin>(
    mut stdout: R,
    max_line_bytes: usize,
    tx: mpsc::Sender<OutputEvent>,
) {
    let mut chunk = [0u8; 8192];
    let mut line = Vec::with_capacity(8192);
    loop {
        match stdout.read(&mut chunk).await {
            Ok(0) => {
                let _ = tx.send(OutputEvent::Closed).await;
                return;
            }
            Ok(n) => {
                for byte in &chunk[..n] {
                    if *byte == b'\n' {
                        if line.last() == Some(&b'\r') {
                            line.pop();
                        }
                        let event = match String::from_utf8(std::mem::take(&mut line)) {
                            Ok(value) => OutputEvent::Line(value),
                            Err(_) => OutputEvent::InvalidUtf8,
                        };
                        let invalid = matches!(event, OutputEvent::InvalidUtf8);
                        if tx.send(event).await.is_err() || invalid {
                            return;
                        }
                    } else {
                        if line.len() == max_line_bytes {
                            let _ = tx.send(OutputEvent::LineTooLong).await;
                            return;
                        }
                        line.push(*byte);
                    }
                }
            }
            Err(_) => {
                let _ = tx.send(OutputEvent::ReadFailed).await;
                return;
            }
        }
    }
}

pub struct EngineSession {
    child: Child,
    stdin: Option<ChildStdin>,
    incoming: mpsc::Receiver<OutputEvent>,
    stdout_task: JoinHandle<()>,
    stderr_task: JoinHandle<()>,
    max_line_bytes: usize,
    write_timeout: Duration,
}

impl EngineSession {
    fn spawn(config: &EngineConfig) -> Result<Self, SupervisorError> {
        let mut child = Command::new(&config.executable)
            .arg("app-server")
            .current_dir(&config.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let stdin = child.stdin.take().ok_or(SupervisorError::NotRunning)?;
        let stdout = child.stdout.take().ok_or(SupervisorError::NotRunning)?;
        let mut stderr = child.stderr.take().ok_or(SupervisorError::NotRunning)?;
        let (tx, incoming) = mpsc::channel(config.queue_capacity);
        let max_line_bytes = config.max_line_bytes;
        let stdout_task = tokio::spawn(read_stdout(stdout, max_line_bytes, tx));
        // Sensitive upstream stderr must never enter application logs.
        let stderr_task = tokio::spawn(async move {
            let _ = tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await;
        });
        Ok(Self {
            child,
            stdin: Some(stdin),
            incoming,
            stdout_task,
            stderr_task,
            max_line_bytes,
            write_timeout: config.write_timeout,
        })
    }

    /// Structured single JSONL frame, never arbitrary newline injection.
    /// Partial writes and timeouts force session teardown.
    pub async fn send_json(&mut self, value: &serde_json::Value) -> Result<(), SupervisorError> {
        let mut frame = serde_json::to_vec(value)?;
        if frame.len() > self.max_line_bytes {
            return Err(SupervisorError::OversizedInput);
        }
        frame.push(b'\n');
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(SupervisorError::NotRunning);
        };
        match timeout(self.write_timeout, stdin.write_all(&frame)).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => {
                let _ = self.stop().await;
                Err(SupervisorError::Io(error))
            }
            Err(_) => {
                let _ = self.stop().await;
                Err(SupervisorError::WriteTimeout)
            }
        }
    }

    pub async fn next_event(&mut self) -> Option<OutputEvent> {
        self.incoming.recv().await
    }

    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>, SupervisorError> {
        Ok(self.child.try_wait()?)
    }

    pub async fn stop(&mut self) -> Result<(), SupervisorError> {
        self.stdin.take();
        self.stdout_task.abort();
        self.stderr_task.abort();
        let _ = self.child.start_kill();
        match timeout(STOP_TIMEOUT, self.child.wait()).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(error)) if error.kind() == io::ErrorKind::InvalidInput => Ok(()),
            Ok(Err(error)) => Err(SupervisorError::Io(error)),
            Err(_) => Err(SupervisorError::Io(io::Error::new(
                io::ErrorKind::TimedOut,
                "engine termination timed out",
            ))),
        }
    }
}

/// Exactly one owned engine instance. Restart is explicit, never an infinite loop.
pub struct EngineSupervisor {
    config: EngineConfig,
    session: Option<EngineSession>,
    generation: u64,
}
impl EngineSupervisor {
    pub fn new(config: EngineConfig) -> Self {
        Self {
            config,
            session: None,
            generation: 0,
        }
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn session_mut(&mut self) -> Option<&mut EngineSession> {
        self.session.as_mut()
    }
    pub fn start(&mut self) -> Result<(), SupervisorError> {
        if self.session.is_some() {
            return Err(SupervisorError::AlreadyRunning);
        }
        self.session = Some(EngineSession::spawn(&self.config)?);
        self.generation += 1;
        Ok(())
    }
    pub async fn stop(&mut self) -> Result<(), SupervisorError> {
        if let Some(mut session) = self.session.take() {
            session.stop().await?;
        }
        Ok(())
    }
    pub async fn restart(&mut self) -> Result<(), SupervisorError> {
        self.stop().await?;
        self.start()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_limits() {
        let cfg = EngineConfig {
            executable: PathBuf::new(),
            cwd: PathBuf::new(),
            max_line_bytes: MAX_JSONL_BYTES,
            queue_capacity: 64,
            write_timeout: Duration::from_secs(5),
        };
        assert!(cfg
            .clone()
            .with_limits(MAX_JSONL_BYTES + 1, 8, Duration::from_secs(1))
            .is_err());
        assert!(cfg
            .clone()
            .with_limits(128, 0, Duration::from_secs(1))
            .is_err());
        assert!(cfg
            .clone()
            .with_limits(128, 129, Duration::from_secs(1))
            .is_err());
        assert!(cfg.with_limits(128, 8, Duration::ZERO).is_err());
    }
    #[test]
    fn diagnostics_redact_sensitive_details() {
        let error = SupervisorError::Io(io::Error::other("secret-token"));
        assert!(!error.to_string().contains("secret-token"));
        let error = SupervisorError::InvalidConfig("secret-path");
        assert!(!error.to_string().contains("secret-path"));
    }
}
