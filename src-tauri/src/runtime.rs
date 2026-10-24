//! Host-owned runtime update bridge. No implicit downloads or global mutations.
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager as TauriManager};
use ultima_runtime_manager::{Approval, Choice, Manager as RuntimeManager};

fn sanitize(_: impl std::fmt::Display) -> String {
    // Never expose filesystem internals or upstream stderr to WebView.
    "Managed Codex runtime operation failed".into()
}
pub fn store_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let parent = app.path().app_data_dir().map_err(sanitize)?;
    fs::create_dir_all(&parent).map_err(sanitize)?;
    Ok(parent.join("managed-codex"))
}
pub fn manager(app: &AppHandle) -> Result<RuntimeManager, String> {
    let root = store_dir(app)?;
    // Other operating systems retain system Codex only until reviewed
    // platform-specific official raw executable digest pins are added.
    let approvals = Approval::official().ok().into_iter().collect();
    RuntimeManager::open(&root, approvals).map_err(sanitize)
}
pub fn status(app: &AppHandle) -> Result<serde_json::Value, String> {
    let manager = manager(app)?;
    let choice = manager.selected();
    let supported = Approval::official().is_ok();
    Ok(serde_json::json!({
        "mode":match &choice { Choice::System => "system", Choice::Managed(_) => "managed" },
        "version":match choice { Choice::System => None, Choice::Managed(v) => Some(v) },
        "managedSupported": supported,
        "pinnedVersion": if supported {Some("0.164.0")} else {None},
        "assetUrl": Approval::official().ok().map(|a| a.asset_url()),
    }))
}
pub fn stage(app: &AppHandle, source: &str) -> Result<(), String> {
    if source.len() > 4096 || !Path::new(source).is_absolute() {
        return Err("Choose an absolute downloaded official Codex executable path".into());
    }
    // The source file is user-selected; it can never choose the expected hash.
    let approved = Approval::official().map_err(sanitize)?;
    manager(app)?
        .stage(Path::new(source), approved.version())
        .map_err(sanitize)
}
pub async fn activate(app: &AppHandle, project: &str) -> Result<(), String> {
    if project.len() > 4096 || !Path::new(project).is_absolute() {
        return Err("Select an absolute trusted project directory".into());
    }
    let approved = Approval::official().map_err(sanitize)?;
    manager(app)?
        .activate_checked(approved.version(), Path::new(project), Path::new(project))
        .await
        .map_err(sanitize)
}
pub fn restore_system(app: &AppHandle) -> Result<(), String> {
    manager(app)?.activate_system().map_err(sanitize)
}
pub fn rollback(app: &AppHandle) -> Result<(), String> {
    manager(app)?.rollback().map(|_| ()).map_err(sanitize)
}

/// A failed managed launch is recovered only when the manager really moved
/// away from the failed version and its persisted effective selection agrees.
pub(crate) fn is_safe_recovery(
    failed_version: &str,
    recovered: &Choice,
    effective: &Choice,
) -> bool {
    recovered == effective
        && match recovered {
            Choice::System => true,
            Choice::Managed(version) => version != failed_version,
        }
}

pub fn rollback_after_failure(app: &AppHandle, failed_version: &str) -> Result<(), String> {
    let manager = manager(app)?;
    let recovered = manager.rollback().map_err(sanitize)?;
    if !is_safe_recovery(failed_version, &recovered, &manager.selected()) {
        return Err(sanitize("same or unverified selection"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_safe_recovery;
    use ultima_runtime_manager::Choice;

    #[test]
    fn failure_recovery_requires_distinct_and_verified_effective_choice() {
        let failed = "0.164.0";
        assert!(!is_safe_recovery(
            failed,
            &Choice::Managed(failed.into()),
            &Choice::Managed(failed.into()),
        ));
        assert!(!is_safe_recovery(
            failed,
            &Choice::Managed("0.163.0".into()),
            &Choice::System,
        ));
        assert!(is_safe_recovery(failed, &Choice::System, &Choice::System));
        assert!(is_safe_recovery(
            failed,
            &Choice::Managed("0.163.0".into()),
            &Choice::Managed("0.163.0".into()),
        ));
    }
}
