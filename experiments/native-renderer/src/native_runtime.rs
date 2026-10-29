//! NC-10G: native renderer consumes the existing NC-12 runtime journal.
//! There is no native updater, runtime download, or alternate credential store.
//! The system executable remains the fallback; managed is checked by NC-12.
use std::path::Path;
use ultima_app_server_supervisor::EngineConfig;
use ultima_runtime_manager::{Approval, Choice, LaunchSnapshot, Manager};

pub(crate) struct Selection {
    pub config: EngineConfig,
    manager: Option<Manager>,
    lease: Option<LaunchSnapshot>,
    selected: Choice,
}

pub(crate) fn resolve(
    system_exe: &Path,
    project: &Path,
    store: Option<&Path>,
) -> Result<Selection, &'static str> {
    // The store is anchored in Tauri's app_data_dir() + managed-codex.
    // A CLI path is merely a locator, never a trust root of its own.
    let trusted = if store.is_some() {
        Some(production_host_store()?)
    } else {
        None
    };
    resolve_against_host(system_exe, project, store, trusted.as_deref())
}

#[cfg(windows)]
fn production_host_store() -> Result<std::path::PathBuf, &'static str> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_RoamingAppData, SHGetKnownFolderPath};

    // Read the OS-known folder instead of a caller-controlled APPDATA env var.
    // Tauri app_data_dir is RoamingAppData / the configured app identifier.
    let mut raw: *mut u16 = std::ptr::null_mut();
    let status = unsafe {
        SHGetKnownFolderPath(&FOLDERID_RoamingAppData, 0, std::ptr::null_mut(), &mut raw)
    };
    if status != 0 || raw.is_null() {
        unsafe { CoTaskMemFree(raw.cast()) };
        return Err("Production host app-data directory unavailable.");
    }
    let mut len = 0usize;
    while len < 32768 && unsafe { *raw.add(len) } != 0 {
        len += 1;
    }
    if len == 32768 {
        unsafe { CoTaskMemFree(raw.cast()) };
        return Err("Production host app-data path exceeds safe bounds.");
    }
    let roaming = std::ffi::OsString::from_wide(unsafe {
        std::slice::from_raw_parts(raw, len)
    });
    unsafe { CoTaskMemFree(raw.cast()) };
    Ok(std::path::PathBuf::from(roaming)
        .join("com.kaisabiyyi.ultima")
        .join("managed-codex"))
}

#[cfg(not(windows))]
fn production_host_store() -> Result<std::path::PathBuf, &'static str> {
    Err("Managed Codex attachment is Windows-host only.")
}

fn resolve_against_host(
    system_exe: &Path,
    project: &Path,
    store: Option<&Path>,
    trusted: Option<&Path>,
) -> Result<Selection, &'static str> {
    if !system_exe.is_absolute() || !project.is_absolute() {
        return Err("Codex executable and project require explicit absolute paths.");
    }
    let system_root = system_exe
        .parent()
        .ok_or("Invalid Codex executable path.")?;
    // Require a usable system fallback BEFORE accepting an optional managed
    // selection. A staged binary must never be the only recovery route.
    let system = EngineConfig::new(system_exe, system_root, project, project)
        .map_err(|_| "System Codex executable or project failed path validation.")?;
    let Some(store) = store else {
        return Ok(Selection {
            config: system,
            manager: None,
            lease: None,
            selected: Choice::System,
        });
    };
    if !store.is_absolute() {
        return Err("NC-12 managed store must be an absolute trusted path.");
    }
    let trusted = trusted.ok_or("Trusted production host store is unavailable.")?;
    // Reject a missing, redirected or foreign path without creating a store.
    // Both the supplied locator and trusted host root must be real directories,
    // not filesystem symlinks/reparse-point aliases.
    for path in [store, trusted] {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|_| "NC-12 host store does not already exist.")?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("NC-12 host store is redirected or not a directory.");
        }
    }
    if std::fs::canonicalize(store).ok() != std::fs::canonicalize(trusted).ok() {
        return Err("NC-12 store is not the configured production host store.");
    }
    // No native updater, store creation or ownership marker generation.
    let manager = Manager::open_existing(store, Approval::official().ok().into_iter().collect())
        .map_err(|_| "Existing NC-12 host store failed ownership verification.")?;
    // Hold the cross-process NC-12 journal update lock until after the
    // supervisor has fully stopped the selected child/process tree.
    let lease = manager
        .launch_snapshot(system_exe, system_root, project, project)
        .map_err(|_| "NC-12 coherent runtime launch snapshot unavailable.")?;
    let selected = lease.choice.clone();
    let config = lease.config.clone();
    Ok(Selection {
        config,
        manager: Some(manager),
        lease: Some(lease),
        selected,
    })
}

impl Selection {
    pub(crate) fn is_managed(&self) -> bool {
        matches!(self.selected, Choice::Managed(_))
    }

    /// Call only AFTER the supervisor has reaped its owned process tree and
    /// the session gate is finalized. Never automatically retry a new engine.
    pub(crate) fn rollback_after_confirmed_failure(&self) -> Result<(), &'static str> {
        let (Some(manager), Some(lease), Choice::Managed(failed)) =
            (&self.manager, &self.lease, &self.selected)
        else {
            return Ok(());
        };
        let recovered = manager
            .rollback_launch(lease)
            .map_err(|_| "NC-12 conditional rollback refused; trusted-host recovery required.")?;
        if recovered == Choice::Managed(failed.clone()) || manager.selected() != recovered {
            return Err("NC-12 did not verify a distinct runtime fallback.");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nc12_rejects_relative_or_untrusted_store_paths() {
        assert!(resolve(Path::new("codex.exe"), Path::new("project"), None).is_err());
        assert!(resolve(
            Path::new("codex.exe"),
            Path::new("project"),
            Some(Path::new("store")),
        )
        .is_err());
    }

    #[test]
    fn nc12_absent_and_foreign_store_are_denied_without_mutation() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("ultima-nc10g-{}-{nonce}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let executable = root.join(if cfg!(windows) { "codex.exe" } else { "codex" });
        fs::write(&executable, b"synthetic only: never executed").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let trusted = root.join("production-app-data").join("managed-codex");
        let foreign = root.join("foreign").join("managed-codex");
        assert!(resolve_against_host(&executable, &root, Some(&trusted), Some(&trusted)).is_err());
        assert!(!trusted.exists());
        fs::create_dir_all(foreign.parent().unwrap()).unwrap();
        Manager::open(&foreign, vec![]).unwrap();
        assert!(resolve_against_host(&executable, &root, Some(&foreign), Some(&trusted)).is_err());
        assert!(!trusted.exists());
        fs::create_dir_all(trusted.parent().unwrap()).unwrap();
        Manager::open(&trusted, vec![]).unwrap();
        let choice =
            resolve_against_host(&executable, &root, Some(&trusted), Some(&trusted)).unwrap();
        assert_eq!(choice.selected, Choice::System);
        choice.rollback_after_confirmed_failure().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn nc12_symlink_alias_to_host_store_is_denied() {
        use std::fs;
        use std::os::unix::fs::{symlink, PermissionsExt};
        let temp = std::env::temp_dir().join(format!(
            "ultima-nc10g-symlink-{}", std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir(&temp).unwrap();
        let exe = temp.join("codex");
        fs::write(&exe, b"synthetic test binary").unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
        let trusted = temp.join("managed-codex");
        Manager::open(&trusted, vec![]).unwrap();
        let alias = temp.join("alias");
        symlink(&trusted, &alias).unwrap();
        assert!(resolve_against_host(&exe, &temp, Some(&alias), Some(&trusted)).is_err());
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn nc12_system_default_does_not_need_host_store() {
        // Absence of opt-in must never access or create any NC-12 journal.
        assert!(
            resolve_against_host(Path::new("codex.exe"), Path::new("project"), None, None).is_err()
        );
    }
}
