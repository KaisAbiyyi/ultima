use sha2::{Digest, Sha256};
use std::{fs, path::Path};
use tempfile::TempDir;
use ultima_runtime_manager::{Approval, Choice, Manager};

const VERSION: &str = "0.164.0";
const RELEASE: &str = "https://github.com/openai/codex/releases/tag/rust-v0.164.0";
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn fixture(dir: &TempDir, bytes: &[u8]) -> (std::path::PathBuf, Approval) {
    let path = dir.path().join("candidate.bin");
    fs::write(&path, bytes).unwrap();
    (
        path,
        Approval::new(VERSION, RELEASE, &digest(bytes)).unwrap(),
    )
}
fn new_manager(dir: &TempDir, approval: Approval) -> Manager {
    Manager::open(&dir.path().join("ultima-managed"), vec![approval]).unwrap()
}
#[test]
fn system_is_default_even_when_candidate_prepared() {
    let dir = TempDir::new().unwrap();
    let (artifact, approved) = fixture(&dir, b"test fixture bytes");
    let manager = new_manager(&dir, approved);
    assert_eq!(manager.active(), Choice::System);
    manager.stage(&artifact, VERSION).unwrap();
    assert_eq!(manager.active(), Choice::System);
}
#[test]
fn activate_reopen_rollback_and_explicit_system() {
    let dir = TempDir::new().unwrap();
    let (artifact, approved) = fixture(&dir, b"valid candidate");
    let manager = new_manager(&dir, approved.clone());
    manager.stage(&artifact, VERSION).unwrap();
    manager.activate_managed(VERSION).unwrap();
    assert_eq!(manager.active(), Choice::Managed(VERSION.into()));
    drop(manager);
    let manager = new_manager(&dir, approved);
    assert_eq!(manager.active(), Choice::Managed(VERSION.into()));
    assert_eq!(manager.rollback().unwrap(), Choice::System);
    assert_eq!(manager.active(), Choice::System);
    manager.activate_managed(VERSION).unwrap();
    manager.activate_system().unwrap();
    assert_eq!(manager.active(), Choice::System);
}
#[test]
fn altered_installed_binary_is_never_executed() {
    let dir = TempDir::new().unwrap();
    let (artifact, approved) = fixture(&dir, b"known good");
    let manager = new_manager(&dir, approved);
    manager.stage(&artifact, VERSION).unwrap();
    manager.activate_managed(VERSION).unwrap();
    let binary = dir
        .path()
        .join("ultima-managed/versions")
        .join(VERSION)
        .join(if cfg!(windows) { "codex.exe" } else { "codex" });
    fs::remove_file(&binary).unwrap();
    fs::write(&binary, b"tamper").unwrap();
    assert_eq!(manager.active(), Choice::System);
    assert!(manager.activate_managed(VERSION).is_err());
    assert_eq!(manager.rollback().unwrap(), Choice::System);
}
#[test]
fn no_forged_release_or_unknown_version() {
    let dir = TempDir::new().unwrap();
    let (artifact, _) = fixture(&dir, b"known");
    assert!(Approval::new(
        "0.163.0",
        "https://github.com/openai/codex/releases/tag/rust-v0.163.0",
        &digest(b"known")
    )
    .is_err());
    assert!(Approval::new(
        "0.164.1",
        "https://github.com/openai/codex/releases/tag/rust-v0.164.1",
        &digest(b"known")
    )
    .is_err());
    assert!(Approval::new(VERSION, "https://example.net/fake", &digest(b"known")).is_err());
    let bad = Approval::new(VERSION, RELEASE, &digest(b"not the artifact")).unwrap();
    let manager = new_manager(&dir, bad);
    assert!(manager.stage(&artifact, VERSION).is_err());
    assert_eq!(manager.active(), Choice::System);
}
#[test]
fn never_overwrite_existing_version_or_user_data() {
    let dir = TempDir::new().unwrap();
    let unrelated = dir.path().join("unrelated");
    fs::create_dir(&unrelated).unwrap();
    fs::write(unrelated.join("user.txt"), b"do not touch").unwrap();
    assert!(Manager::open(&unrelated, vec![]).is_err());
    assert_eq!(
        fs::read(unrelated.join("user.txt")).unwrap(),
        b"do not touch"
    );
    let (artifact, approved) = fixture(&dir, b"one");
    let manager = new_manager(&dir, approved);
    manager.stage(&artifact, VERSION).unwrap();
    assert!(manager.stage(&artifact, VERSION).is_err());
}
#[test]
fn corrupt_journal_falls_back_without_running_managed() {
    let dir = TempDir::new().unwrap();
    let (artifact, approved) = fixture(&dir, b"v");
    let manager = new_manager(&dir, approved);
    manager.stage(&artifact, VERSION).unwrap();
    manager.activate_managed(VERSION).unwrap();
    let entry = dir
        .path()
        .join("ultima-managed/journal/00000000000000000001.json");
    fs::write(entry, b"corrupt").unwrap();
    assert_eq!(manager.active(), Choice::System);
    assert!(manager.activate_managed(VERSION).is_err());
}
#[test]
fn selection_uses_nc04_trusted_system_binary_and_project_root() {
    let dir = TempDir::new().unwrap();
    let (_, approved) = fixture(&dir, b"good");
    let manager = new_manager(&dir, approved);
    let system = dir
        .path()
        .join(if cfg!(windows) { "codex.exe" } else { "codex" });
    fs::write(&system, b"system test fixture").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&system, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let project = dir.path().join("project");
    fs::create_dir(&project).unwrap();
    assert!(manager
        .engine_config(&system, dir.path(), &project, dir.path())
        .is_ok());
    assert!(manager
        .engine_config(&system, dir.path(), Path::new(".."), dir.path())
        .is_err());
}
#[cfg(unix)]
#[test]
fn symlink_source_rejected() {
    use std::os::unix::fs::symlink;
    let dir = TempDir::new().unwrap();
    let (artifact, approved) = fixture(&dir, b"original");
    let link = dir.path().join("malicious-link");
    symlink(&artifact, &link).unwrap();
    let manager = new_manager(&dir, approved);
    assert!(manager.stage(&link, VERSION).is_err());
}
