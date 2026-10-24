use std::{fs, path::PathBuf};
use tempfile::TempDir;
use ultima_app_server_supervisor::EngineConfig;
use ultima_runtime_manager::{health::probe, Approval, Choice, Manager};

const VERSION: &str = "0.164.0";
const RELEASE: &str = "https://github.com/openai/codex/releases/tag/rust-v0.164.0";
const OFFICIAL_DIGEST: &str = "6f267830b029ee25d9339864aa867cd45de9f8fd4c5ea007d2e63c13a0c66d36";

#[test]
fn arbitrary_matching_hash_cannot_claim_official_provenance() {
    let digest_of_attacker_file =
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    assert!(Approval::new(VERSION, RELEASE, digest_of_attacker_file).is_err());
    assert!(Approval::new(
        "0.163.0",
        "https://github.com/openai/codex/releases/tag/rust-v0.163.0",
        OFFICIAL_DIGEST
    )
    .is_err());
    assert!(Approval::new(
        "0.164.1",
        "https://github.com/openai/codex/releases/tag/rust-v0.164.1",
        OFFICIAL_DIGEST
    )
    .is_err());
    assert!(Approval::new(
        VERSION,
        "https://malicious.example/tag/rust-v0.164.0",
        OFFICIAL_DIGEST
    )
    .is_err());
    assert!(Approval::new(
        VERSION,
        RELEASE,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
    )
    .is_err());
}
#[test]
fn pinned_official_asset_bound_to_target_platform() {
    let approval = Approval::official();
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        let approval = approval.unwrap();
        assert_eq!(approval.version(), VERSION);
        assert_eq!(approval.digest(), OFFICIAL_DIGEST);
        assert_eq!(approval.asset_url(),
            "https://github.com/openai/codex/releases/download/rust-v0.164.0/codex-x86_64-pc-windows-msvc.exe");
    } else {
        assert!(
            approval.is_err(),
            "no wrong-architecture managed asset may be accepted"
        );
    }
}
#[test]
fn runtime_manager_does_not_change_default_selection() {
    let tmp = TempDir::new().unwrap();
    let manager = Manager::open(&tmp.path().join("owned-runtime"), vec![]).unwrap();
    assert_eq!(manager.active(), Choice::System);
    assert!(manager.activate_system().is_ok());
    assert_eq!(manager.active(), Choice::System);
    // No managed executable is available without the exact approved release asset.
}

fn preflight_setup() -> (TempDir, EngineConfig) {
    let tmp = TempDir::new().unwrap();
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_fake_runtime"));
    let cfg = EngineConfig::new(&bin, bin.parent().unwrap(), tmp.path(), tmp.path()).unwrap();
    (tmp, cfg)
}
#[tokio::test]
async fn healthy_synthetic_server_requires_successful_init_and_model_list() {
    let (_tmp, cfg) = preflight_setup();
    probe(cfg).await.unwrap();
}
#[tokio::test]
async fn rejected_initialize_aborts_without_activation() {
    let (tmp, cfg) = preflight_setup();
    fs::write(tmp.path().join("probe-fail-init"), b"1").unwrap();
    assert!(probe(cfg).await.is_err());
}
#[tokio::test]
async fn rejected_model_list_aborts_without_activation() {
    let (tmp, cfg) = preflight_setup();
    fs::write(tmp.path().join("probe-fail-model"), b"1").unwrap();
    assert!(probe(cfg).await.is_err());
}
