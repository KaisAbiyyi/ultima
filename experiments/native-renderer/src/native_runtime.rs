//! NC-10G: native renderer consumes the existing NC-12 runtime journal.
//! There is no native updater, runtime download, or alternate credential store.
//! The system executable remains the fallback; managed is checked by NC-12.
use std::path::Path;
use ultima_app_server_supervisor::EngineConfig;
use ultima_runtime_manager::{Approval, Choice, Manager};

pub(crate) struct Selection {
    pub config: EngineConfig,
    manager: Option<Manager>,
    selected: Choice,
}

pub(crate) fn resolve(
    system_exe: &Path,
    project: &Path,
    store: Option<&Path>,
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
            selected: Choice::System,
        });
    };
    if !store.is_absolute() {
        return Err("NC-12 managed store must be an absolute trusted path.");
    }
    // Reuse the shared existing NC-12 owned store. Never stage or activate
    // an executable from this renderer; verified activation is host-owned.
    let manager = Manager::open(store, Approval::official().ok().into_iter().collect())
        .map_err(|_| "NC-12 runtime store failed ownership verification.")?;
    let selected = manager.selected();
    let config = manager
        .engine_config(system_exe, system_root, project, project)
        .map_err(|_| "NC-12 selected executable failed verification.")?;
    Ok(Selection {
        config,
        manager: Some(manager),
        selected,
    })
}

impl Selection {
    /// Call only AFTER the supervisor has reaped its owned process tree and
    /// the session gate is finalized. Never automatically retry a new engine.
    pub(crate) fn rollback_after_confirmed_failure(&self) -> Result<(), &'static str> {
        let (Some(manager), Choice::Managed(failed)) = (&self.manager, &self.selected) else {
            return Ok(());
        };
        let recovered = manager
            .rollback()
            .map_err(|_| "NC-12 managed rollback failed; reconnect blocked.")?;
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
    fn nc12_system_default_never_creates_separate_runtime_selection() {
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
        let store = root.join("managed-codex");
        let selection = resolve(&executable, &root, Some(&store)).unwrap();
        assert!(selection.manager.is_some());
        assert_eq!(selection.selected, Choice::System);
        selection.rollback_after_confirmed_failure().unwrap();
        assert_eq!(
            selection.manager.as_ref().unwrap().selected(),
            Choice::System
        );
        fs::remove_dir_all(&root).unwrap();
    }
}
