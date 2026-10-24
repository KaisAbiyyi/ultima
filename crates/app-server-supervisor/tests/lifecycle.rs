use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::time::timeout;
use ultima_app_server_supervisor::{
    EngineConfig, EngineSupervisor, OutputEvent, SupervisorError, MAX_JSONL_BYTES,
};

static SEQ: AtomicU64 = AtomicU64::new(0);
struct TestPaths {
    root: PathBuf,
    project: PathBuf,
}
impl TestPaths {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ultima-nc04-{}-{nanos}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let project = root.join("workspace");
        fs::create_dir_all(&project).unwrap();
        Self { root, project }
    }
    fn config(&self) -> EngineConfig {
        let executable = PathBuf::from(env!("CARGO_BIN_EXE_fake_codex"));
        EngineConfig::new(
            &executable,
            executable.parent().unwrap(),
            &self.project,
            &self.root,
        )
        .unwrap()
    }
}
impl Drop for TestPaths {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn echo_one_frame_and_stop_without_real_codex() {
    let paths = TestPaths::new();
    let mut supervisor = EngineSupervisor::new(paths.config());
    supervisor.start().unwrap();
    assert_eq!(supervisor.generation(), 1);
    assert!(matches!(
        supervisor.start(),
        Err(SupervisorError::AlreadyRunning)
    ));
    let request = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"text":"one\nline"}});
    let session = supervisor.session_mut().unwrap();
    session.send_json(&request).await.unwrap();
    let event = timeout(Duration::from_secs(5), session.next_event())
        .await
        .unwrap()
        .unwrap();
    let OutputEvent::Line(raw) = event else {
        panic!("expected frame")
    };
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&raw).unwrap(),
        request
    );
    supervisor.stop().await.unwrap();
    assert!(supervisor.session_mut().is_none());
}

#[tokio::test]
async fn restart_after_abnormal_exit_discards_old_transport() {
    let paths = TestPaths::new();
    let mut supervisor = EngineSupervisor::new(paths.config());
    supervisor.start().unwrap();
    let session = supervisor.session_mut().unwrap();
    session
        .send_json(&serde_json::json!({"method":"fixture/crash"}))
        .await
        .unwrap();
    let event = timeout(Duration::from_secs(5), session.next_event())
        .await
        .unwrap();
    assert!(matches!(event, Some(OutputEvent::Closed)));
    supervisor.restart().await.unwrap();
    assert_eq!(supervisor.generation(), 2);
    let session = supervisor.session_mut().unwrap();
    session
        .send_json(&serde_json::json!({"id":2}))
        .await
        .unwrap();
    assert!(matches!(
        timeout(Duration::from_secs(5), session.next_event())
            .await
            .unwrap(),
        Some(OutputEvent::Line(_))
    ));
    supervisor.stop().await.unwrap();
}

#[tokio::test]
async fn malformed_oversized_output_fails_closed() {
    let paths = TestPaths::new();
    let config = paths
        .config()
        .with_limits(128, 2, Duration::from_secs(2))
        .unwrap();
    let mut supervisor = EngineSupervisor::new(config);
    supervisor.start().unwrap();
    let session = supervisor.session_mut().unwrap();
    session
        .send_json(&serde_json::json!({"method":"fixture/oversize"}))
        .await
        .unwrap();
    assert_eq!(
        timeout(Duration::from_secs(5), session.next_event())
            .await
            .unwrap(),
        Some(OutputEvent::LineTooLong)
    );
    supervisor.stop().await.unwrap();
}

#[tokio::test]
async fn outgoing_oversize_rejected_without_spawning_more_processes() {
    let paths = TestPaths::new();
    let mut supervisor = EngineSupervisor::new(paths.config());
    supervisor.start().unwrap();
    let large = serde_json::json!({"payload":"A".repeat(MAX_JSONL_BYTES)});
    assert!(matches!(
        supervisor.session_mut().unwrap().send_json(&large).await,
        Err(SupervisorError::OversizedInput)
    ));
    assert_eq!(supervisor.generation(), 1);
    supervisor.stop().await.unwrap();
}

#[test]
fn canonical_paths_enforce_both_trusted_roots() {
    let paths = TestPaths::new();
    let executable = PathBuf::from(env!("CARGO_BIN_EXE_fake_codex"));
    let bin_root = executable.parent().unwrap();
    assert!(EngineConfig::new(&executable, &paths.root, &paths.project, &paths.root).is_err());
    assert!(EngineConfig::new(&executable, bin_root, bin_root, &paths.root).is_err());
    assert!(EngineConfig::new(&executable, bin_root, &paths.project, &paths.root).is_ok());
}

#[cfg(windows)]
#[tokio::test]
async fn windows_job_contains_and_kills_grandchild() {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, WaitForSingleObject, SYNCHRONIZE},
    };
    let paths = TestPaths::new();
    let mut supervisor = EngineSupervisor::new(paths.config());
    supervisor.start().unwrap();
    let session = supervisor.session_mut().unwrap();
    session
        .send_json(&serde_json::json!({"method":"fixture/spawnChild"}))
        .await
        .unwrap();
    let event = timeout(Duration::from_secs(5), session.next_event())
        .await
        .unwrap()
        .unwrap();
    let OutputEvent::Line(line) = event else {
        panic!("expected child ID")
    };
    let pid = serde_json::from_str::<serde_json::Value>(&line).unwrap()["childPid"]
        .as_u64()
        .unwrap() as u32;
    let child = unsafe { OpenProcess(SYNCHRONIZE, 0, pid) };
    assert!(!child.is_null(), "child process must exist before teardown");
    supervisor.stop().await.unwrap();
    let status = unsafe { WaitForSingleObject(child, 5000) };
    unsafe { CloseHandle(child) };
    assert_eq!(
        status, WAIT_OBJECT_0,
        "job close/terminate must stop grandchild"
    );
}
