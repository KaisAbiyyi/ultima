//! Sidecar process manager
//!
//! Manages llama-server sidecar process.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex as TokioMutex;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

use crate::config::{
    health_check_url, DEFAULT_CONTEXT_WINDOW, LLAMA_SERVER_BINARY, LOCAL_SERVER_HOST,
    POST_HEALTH_DELAY_SECS, SERVER_HEALTH_INTERVAL_MS, SERVER_HEALTH_TIMEOUT_SECS,
};
use crate::error::{AppError, AppResult};
use crate::events::{InferenceEvent, SidecarStatusPayload};

/// Sidecar handle for Tauri state
pub type SidecarHandle = Arc<TokioMutex<SidecarManager>>;

/// Get sidecar from state
pub fn get_sidecar(app_handle: &AppHandle) -> AppResult<SidecarHandle> {
    app_handle
        .try_state::<SidecarHandle>()
        .map(|s| s.inner().clone())
        .ok_or_else(|| AppError::Internal("Sidecar not initialized".to_string()))
}

/// Sidecar process manager
pub struct SidecarManager {
    /// Process ID
    pid: Option<u32>,
    /// Child process handle
    child: Option<Child>,
    /// Model path being served
    model_path: Option<String>,
    /// Multimodal projector path (for vision models)
    mmproj_path: Option<String>,
    /// Server port
    port: u16,
    /// Context window size
    context_window: u32,
    /// Request ID for status updates
    _request_id: String,
}

impl SidecarManager {
    /// Create new sidecar manager
    pub fn new(port: u16) -> Self {
        Self {
            pid: None,
            child: None,
            model_path: None,
            mmproj_path: None,
            port,
            context_window: DEFAULT_CONTEXT_WINDOW,
            _request_id: Uuid::new_v4().to_string(),
        }
    }

    /// Start sidecar process
    pub async fn start(
        &mut self,
        app_handle: &AppHandle,
        model_path: &str,
        port: Option<u16>,
        context_window: Option<u32>,
        mmproj_path: Option<&str>,
    ) -> AppResult<()> {
        let port = port.unwrap_or(self.port);
        let ctx = context_window.unwrap_or(DEFAULT_CONTEXT_WINDOW);

        // If already running with same model, port, context size, and mmproj, do nothing
        if self.is_running()
            && self.model_path.as_deref() == Some(model_path)
            && self.mmproj_path.as_deref() == mmproj_path
            && self.port == port
            && self.context_window == ctx
        {
            return Ok(());
        }

        // If running but different, stop first
        if self.is_running() {
            self.stop(app_handle).await?;
        }

        // Emit starting status
        let status_payload = SidecarStatusPayload::starting(port);
        app_handle.emit(InferenceEvent::SIDECAR_STATUS, &status_payload)?;

        // Construct path to binary
        let cwd = std::env::current_dir()
            .map_err(|e| AppError::Inference(format!("Failed to get current directory: {}", e)))?;

        // Target: src-tauri/binaries/llama-server-x86_64-pc-windows-msvc.exe
        // Running from root: cwd/src-tauri/binaries/...
        // Running from src-tauri: cwd/binaries/...

        let binary_name = LLAMA_SERVER_BINARY;
        let mut binary_path = cwd.join("binaries").join(binary_name);

        if !binary_path.exists() {
            binary_path = cwd.join("src-tauri").join("binaries").join(binary_name);
        }

        let work_dir = binary_path.parent().unwrap();

        println!("[Sidecar] Binary path: {:?}", binary_path);
        println!("[Sidecar] Working directory: {:?}", work_dir);

        // Verify file exists
        if !binary_path.exists() {
            return Err(AppError::Inference(format!(
                "Sidecar binary not found at: {:?}",
                binary_path
            )));
        }

        let mut cmd = Command::new(&binary_path);
        cmd.current_dir(work_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let port_str = port.to_string();
        let ctx_str = ctx.to_string();

        // Build base args
        let mut args = vec![
            "-m".to_string(),
            model_path.to_string(),
            "--port".to_string(),
            port_str.clone(),
            "--ctx-size".to_string(),
            ctx_str.clone(),
            "--host".to_string(),
            LOCAL_SERVER_HOST.to_string(),
        ];

        // Add mmproj if provided (for vision models)
        if let Some(mmproj) = mmproj_path {
            args.push("--mmproj".to_string());
            args.push(mmproj.to_string());
        }

        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        println!("[Sidecar] Executing with args: {:?}", args);

        let mut child = cmd.args(&args).spawn().map_err(|e| {
            AppError::Inference(format!("Failed to start llama-server sidecar: {}", e))
        })?;

        let pid = child.id();

        // Handle stdout
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    println!("[LLAMA stdout] {}", line);
                }
            });
        }

        // Handle stderr
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    eprintln!("[LLAMA stderr] {}", line);
                }
            });
        }

        self.port = port;
        self.context_window = ctx;
        self.model_path = Some(model_path.to_string());
        self.mmproj_path = mmproj_path.map(String::from);
        self.pid = pid;
        self.child = Some(child);

        // Emit running status
        let status_payload =
            SidecarStatusPayload::running(model_path.to_string(), pid.unwrap_or(0), port);
        app_handle.emit(InferenceEvent::SIDECAR_STATUS, &status_payload)?;

        // Wait for server to be ready
        let client = reqwest::Client::new();
        let health_url = health_check_url(port);

        let start = Instant::now();
        loop {
            // Check if server responds with 200 OK (not just any response)
            if let Ok(response) = client.get(&health_url).send().await {
                if response.status().is_success() {
                    // Server is healthy, now wait a bit more for model to fully load
                    sleep(Duration::from_secs(POST_HEALTH_DELAY_SECS)).await;
                    break;
                }
            }

            if start.elapsed().as_secs() > SERVER_HEALTH_TIMEOUT_SECS {
                // Check if process is still alive using kill(0) approach or try_wait
                if let Some(child) = &mut self.child {
                    if let Ok(Some(status)) = child.try_wait() {
                        return Err(AppError::Inference(format!(
                            "Llama server exited unexpectedly with status: {}",
                            status
                        )));
                    }
                }

                // Clean up process if failed
                self.stop(app_handle).await?;
                return Err(AppError::Inference(format!(
                    "Llama server failed to start within {}s",
                    SERVER_HEALTH_TIMEOUT_SECS
                )));
            }

            sleep(Duration::from_millis(SERVER_HEALTH_INTERVAL_MS)).await;
        }

        Ok(())
    }

    /// Stop sidecar process
    pub async fn stop(&mut self, app_handle: &AppHandle) -> AppResult<()> {
        if let Some(mut child) = self.child.take() {
            child
                .kill()
                .await
                .map_err(|e| AppError::Inference(format!("Failed to kill process: {}", e)))?;
        }

        // Emit stopped status
        let status_payload = SidecarStatusPayload::stopped(self.port);
        app_handle.emit(InferenceEvent::SIDECAR_STATUS, &status_payload)?;

        self.pid = None;
        self.model_path = None;
        self.mmproj_path = None;

        Ok(())
    }

    /// Ensure sidecar is running with specific model and context
    pub async fn ensure_running(
        &mut self,
        app_handle: &AppHandle,
        model_path: &str,
        context_window: Option<u32>,
        mmproj_path: Option<&str>,
    ) -> AppResult<()> {
        let ctx = context_window.unwrap_or(DEFAULT_CONTEXT_WINDOW);
        if self.is_running()
            && self.model_path.as_deref() == Some(model_path)
            && self.mmproj_path.as_deref() == mmproj_path
            && self.context_window == ctx
        {
            return Ok(());
        }
        self.start(app_handle, model_path, None, context_window, mmproj_path)
            .await
    }

    /// Check if sidecar is running
    pub fn is_running(&self) -> bool {
        self.pid.is_some()
    }

    /// Get current model path
    pub fn model_path(&self) -> Option<&str> {
        self.model_path.as_deref()
    }

    /// Get current port
    pub fn port(&self) -> u16 {
        self.port
    }
}

/// Initialize sidecar manager
pub fn init_sidecar(port: u16) -> SidecarHandle {
    Arc::new(TokioMutex::new(SidecarManager::new(port)))
}
