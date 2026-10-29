//! NC-12: opt-in, checksum-verified Codex selection and crash-safe rollback.
//! No network/download, global PATH changes, shell, or executable overwrite.
pub mod health;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
const MAX_BINARY: u64 = 512 * 1024 * 1024;
const MAX_HISTORY: usize = 128;
const UPSTREAM: &str = "https://github.com/openai/codex/releases/tag/rust-v";
const MARKER: &str = ".ultima-managed-runtime-v1";
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Invalid(&'static str),
    Checksum,
    Locked,
    JournalFull,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Io(_) => "runtime I/O failure",
            Self::Invalid(_) => "runtime policy failure",
            Self::Checksum => "runtime checksum mismatch",
            Self::Locked => "runtime update is locked",
            Self::JournalFull => "runtime journal capacity reached",
        })
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
#[derive(Clone, Debug)]
pub struct Approval {
    version: String,
    release_url: String,
    sha256: String,
}
fn qualified(version: &str) -> bool {
    let Ok(matrix): Result<serde_json::Value, _> =
        serde_json::from_str(include_str!("../../../protocol/compat-matrix.json"))
    else {
        return false;
    };
    matrix["source"] == "https://github.com/openai/codex"
        && matrix["transport"] == "stdio-jsonl"
        && matrix["pinnedVersion"] == version
        && matrix["versions"].as_array().is_some_and(|v| {
            v.iter().any(|entry| {
                entry["version"] == version
                    && entry["tag"] == format!("rust-v{version}")
                    && entry["state"] == "supported"
                    && entry["release"] == format!("{UPSTREAM}{version}")
            })
        })
}
impl Approval {
    /// Production trust source: a reviewed, immutable official RELEASE ASSET
    /// with an exact digest, size, download URL, OS and architecture.
    /// No caller-supplied matching digest can become its own trust anchor.
    pub fn official() -> Result<Self, Error> {
        let pins: serde_json::Value =
            serde_json::from_str(include_str!("../../../protocol/managed-assets.json"))
                .map_err(|_| Error::Invalid("assets manifest"))?;
        let target = if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            "x86_64-pc-windows-msvc"
        } else {
            return Err(Error::Invalid("unsupported platform"));
        };
        if pins["schemaVersion"] != 1
            || pins["version"] != "0.164.0"
            || pins["source"]
                != "https://api.github.com/repos/openai/codex/releases/tags/rust-v0.164.0"
            || pins["release"] != "https://github.com/openai/codex/releases/tag/rust-v0.164.0"
        {
            return Err(Error::Invalid("untrusted manifest"));
        }
        let assets = pins["assets"].as_array().ok_or(Error::Invalid("assets"))?;
        if assets.len() != 1 {
            return Err(Error::Invalid("asset count"));
        }
        let item = assets
            .iter()
            .find(|i| i["target"] == target)
            .ok_or(Error::Invalid("architecture"))?;
        let version = pins["version"].as_str().ok_or(Error::Invalid("version"))?;
        let name = format!("codex-{target}.exe");
        let url =
            format!("https://github.com/openai/codex/releases/download/rust-v{version}/{name}");
        let hash = item["sha256"].as_str().ok_or(Error::Invalid("digest"))?;
        if !qualified(version)
            || item["name"] != name
            || item["url"] != url
            || item["size"] != 354558768u64
            || hash.len() != 64
            || !hash
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err(Error::Invalid("official asset"));
        }
        Self::new(
            version,
            pins["release"].as_str().ok_or(Error::Invalid("release"))?,
            hash,
        )
    }

    /// Retained API shape but now only accepts the exact official, pinned
    /// digest for the OS/architecture; arbitrary hash + tag cannot qualify.
    pub fn new(version: &str, url: &str, hash: &str) -> Result<Self, Error> {
        let supported_target = cfg!(all(target_os = "windows", target_arch = "x86_64"));
        let official_hash = "6f267830b029ee25d9339864aa867cd45de9f8fd4c5ea007d2e63c13a0c66d36";
        if !supported_target
            || !qualified(version)
            || version != "0.164.0"
            || url != format!("{UPSTREAM}{version}")
            || hash != official_hash
        {
            return Err(Error::Invalid("trusted official release digest"));
        }
        Ok(Self {
            version: version.into(),
            release_url: url.into(),
            sha256: hash.into(),
        })
    }

    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn digest(&self) -> &str {
        &self.sha256
    }
    pub fn asset_url(&self) -> &'static str {
        "https://github.com/openai/codex/releases/download/rust-v0.164.0/codex-x86_64-pc-windows-msvc.exe"
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(
    tag = "kind",
    content = "version",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Choice {
    System,
    Managed(String),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    sequence: u64,
    choice: Choice,
}
pub struct Manager {
    root: PathBuf,
    approvals: BTreeMap<String, Approval>,
}
/// A non-cloneable store-level launch lease. Its update lock covers both
/// selecting a binary and the entire owned process lifetime; supported host
/// activation and rollback cannot overtake the native launch.
pub struct LaunchSnapshot {
    pub config: ultima_app_server_supervisor::EngineConfig,
    pub choice: Choice,
    pub digest: Option<String>,
    generation: u64,
    owner: PathBuf,
    _guard: Lock,
}

struct Lock(PathBuf);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn hash_file(path: &Path) -> Result<String, Error> {
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut chunk = [0; 65536];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        if size > MAX_BINARY {
            return Err(Error::Invalid("too large"));
        }
        hasher.update(&chunk[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
impl Manager {
    /// An existing nonempty unrelated folder is never adopted.
    pub fn open(root: &Path, approvals: Vec<Approval>) -> Result<Self, Error> {
        if root.exists() {
            let m = fs::symlink_metadata(root)?;
            if !m.is_dir() || m.file_type().is_symlink() {
                return Err(Error::Invalid("root"));
            }
        } else {
            fs::create_dir(root)?;
        }
        let root = fs::canonicalize(root)?;
        let marker = root.join(MARKER);
        if !marker.exists() {
            if fs::read_dir(&root)?.next().is_some() {
                return Err(Error::Invalid("unowned store"));
            }
            let mut f = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(marker)?;
            f.write_all(b"ultima-managed-v1\n")?;
            f.sync_all()?;
        } else if fs::read(&marker)? != b"ultima-managed-v1\n" {
            return Err(Error::Invalid("marker"));
        }
        for name in ["versions", "journal"] {
            let folder = root.join(name);
            if !folder.exists() {
                fs::create_dir(&folder)?;
            }
            let m = fs::symlink_metadata(&folder)?;
            if !m.is_dir() || m.file_type().is_symlink() {
                return Err(Error::Invalid("folder"));
            }
        }
        let mut allowed = BTreeMap::new();
        for a in approvals {
            let verified = Approval::new(&a.version, &a.release_url, &a.sha256)?;
            if allowed.insert(verified.version.clone(), verified).is_some() {
                return Err(Error::Invalid("dup"));
            }
        }
        Ok(Self {
            root,
            approvals: allowed,
        })
    }
    /// Read-only attachment to a store initialized by the production host.
    /// Unlike open(), a missing root, marker or layout is never adopted or repaired.
    /// Callers must additionally bind the root to their trusted host location.
    pub fn open_existing(root: &Path, approvals: Vec<Approval>) -> Result<Self, Error> {
        if !root.is_absolute() {
            return Err(Error::Invalid("existing store path"));
        }
        let meta = fs::symlink_metadata(root)?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(Error::Invalid("existing store root"));
        }
        let root = fs::canonicalize(root)?;
        let marker = root.join(MARKER);
        let marker_meta = fs::symlink_metadata(&marker)?;
        if !marker_meta.is_file() || marker_meta.file_type().is_symlink()
            || marker_meta.len() != b"ultima-managed-v1\n".len() as u64
            || fs::read(&marker)? != b"ultima-managed-v1\n"
        {
            return Err(Error::Invalid("existing store marker"));
        }
        for name in ["versions", "journal"] {
            let meta = fs::symlink_metadata(root.join(name))?;
            if !meta.is_dir() || meta.file_type().is_symlink() {
                return Err(Error::Invalid("existing store layout"));
            }
        }
        let mut allowed = BTreeMap::new();
        for a in approvals {
            let verified = Approval::new(&a.version, &a.release_url, &a.sha256)?;
            if allowed.insert(verified.version.clone(), verified).is_some() {
                return Err(Error::Invalid("dup"));
            }
        }
        // Validate the existing journal before returning an attachable manager.
        let manager = Self { root, approvals: allowed };
        manager.history()?;
        Ok(manager)
    }

    fn lock(&self) -> Result<Lock, Error> {
        let path = self.root.join(".update-lock");
        match OpenOptions::new().create_new(true).write(true).open(&path) {
            Ok(f) => {
                f.sync_all()?;
                Ok(Lock(path))
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(Error::Locked),
            Err(e) => Err(e.into()),
        }
    }
    fn approval(&self, version: &str) -> Result<&Approval, Error> {
        self.approvals
            .get(version)
            .ok_or(Error::Invalid("unapproved"))
    }
    fn binary(&self, version: &str) -> PathBuf {
        self.root
            .join("versions")
            .join(version)
            .join(if cfg!(windows) { "codex.exe" } else { "codex" })
    }
    fn verified(&self, version: &str) -> Result<PathBuf, Error> {
        let approval = self.approval(version)?;
        let folder = self.root.join("versions").join(version);
        let m = fs::symlink_metadata(folder)?;
        if !m.is_dir() || m.file_type().is_symlink() {
            return Err(Error::Invalid("version dir"));
        }
        let path = self.binary(version);
        let m = fs::symlink_metadata(&path)?;
        if !m.is_file() || m.file_type().is_symlink() || m.len() > MAX_BINARY {
            return Err(Error::Invalid("managed executable"));
        }
        if hash_file(&path)? != approval.sha256 {
            return Err(Error::Checksum);
        }
        Ok(path)
    }
    /// Stage only a raw binary, never extract arbitrary archive members.
    pub fn stage(&self, source: &Path, version: &str) -> Result<(), Error> {
        let _guard = self.lock()?;
        let approval = self.approval(version)?;
        let m = fs::symlink_metadata(source)?;
        if !m.is_file() || m.file_type().is_symlink() || m.len() > MAX_BINARY {
            return Err(Error::Invalid("source"));
        }
        if hash_file(source)? != approval.sha256 {
            return Err(Error::Checksum);
        }
        let folder = self.root.join("versions").join(version);
        if folder.exists() {
            return Err(Error::Invalid("immutable version"));
        }
        if fs::read_dir(self.root.join("versions"))?.count() >= 8 {
            return Err(Error::Invalid("version capacity"));
        }
        fs::create_dir(&folder)?;
        let result = (|| -> Result<(), Error> {
            let source = File::open(source)?;
            let dest = self.binary(version);
            let mut target = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&dest)?;
            io::copy(&mut source.take(MAX_BINARY + 1), &mut target)?;
            target.sync_all()?;
            if hash_file(&dest)? != approval.sha256 {
                return Err(Error::Checksum);
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(dest, fs::Permissions::from_mode(0o500))?;
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(self.binary(version));
            let _ = fs::remove_dir(&folder);
        }
        result
    }
    fn history(&self) -> Result<Vec<Record>, Error> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(self.root.join("journal"))? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            let num = name
                .strip_suffix(".json")
                .ok_or(Error::Invalid("journal name"))?;
            if num.len() != 20 || !num.bytes().all(|c| c.is_ascii_digit()) {
                return Err(Error::Invalid("journal name"));
            }
            let seq: u64 = num.parse().map_err(|_| Error::Invalid("sequence"))?;
            let m = fs::symlink_metadata(entry.path())?;
            if !m.is_file() || m.file_type().is_symlink() || m.len() > 2048 {
                return Err(Error::Invalid("journal entry"));
            }
            let record: Record = serde_json::from_slice(&fs::read(entry.path())?)
                .map_err(|_| Error::Invalid("record"))?;
            if record.sequence != seq {
                return Err(Error::Invalid("journal sequence"));
            }
            entries.push(record);
            if entries.len() > MAX_HISTORY {
                return Err(Error::JournalFull);
            }
        }
        entries.sort_by_key(|r| r.sequence);
        if entries.windows(2).any(|v| v[0].sequence == v[1].sequence) {
            return Err(Error::Invalid("duplicate sequence"));
        }
        Ok(entries)
    }
    /// On any corruption, deletion, or unqualified binary, use SYSTEM only.
    pub fn active(&self) -> Choice {
        let Ok(history) = self.history() else {
            return Choice::System;
        };
        let choice = history.last().map_or(Choice::System, |r| r.choice.clone());
        if let Choice::Managed(version) = &choice {
            if self.verified(version).is_err() {
                return Choice::System;
            }
        }
        choice
    }
    fn record(&self, choice: Choice) -> Result<(), Error> {
        let history = self.history()?;
        if history.len() >= MAX_HISTORY {
            return Err(Error::JournalFull);
        }
        let next = history.last().map_or(1, |r| r.sequence.saturating_add(1));
        if next == u64::MAX {
            return Err(Error::JournalFull);
        }
        let dir = self.root.join("journal");
        let target = dir.join(format!("{next:020}.json"));
        if target.exists() {
            return Err(Error::Invalid("journal collision"));
        }
        let temp = dir.join(format!(".{}-{next}.tmp", std::process::id()));
        let mut out = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        let result = (|| -> Result<(), Error> {
            let bytes = serde_json::to_vec(&Record {
                sequence: next,
                choice,
            })
            .map_err(|_| Error::Invalid("encoding"))?;
            out.write_all(&bytes)?;
            out.sync_all()?;
            fs::rename(&temp, target)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }
    /// The ONLY production managed activation path: read-only bounded live
    /// initialize/initialized/model-list smoke and confirmed process teardown
    /// BEFORE journaling the new selection.
    pub async fn activate_checked(
        &self,
        version: &str,
        project: &Path,
        project_root: &Path,
    ) -> Result<(), Error> {
        let _guard = self.lock()?;
        let path = self.verified(version)?;
        let trusted = self.root.join("versions").join(version);
        let config =
            ultima_app_server_supervisor::EngineConfig::new(&path, &trusted, project, project_root)
                .map_err(|_| Error::Invalid("preflight config"))?;
        health::probe(config).await?;
        // Detect any concurrent post-smoke binary change.
        self.verified(version)?;
        let choice = Choice::Managed(version.to_owned());
        // A re-check of the same approved version is a journal no-op.
        // Historic duplicate entries still remain safe for rollback().
        if self
            .history()?
            .last()
            .is_some_and(|entry| entry.choice == choice)
        {
            return Ok(());
        }
        self.record(choice)
    }
    pub fn activate_system(&self) -> Result<(), Error> {
        let _guard = self.lock()?;
        // System is the safe default even before the first journal entry.
        // Repeated clicks must not exhaust the bounded journal.
        if self
            .history()?
            .last()
            .is_none_or(|entry| entry.choice == Choice::System)
        {
            return Ok(());
        }
        self.record(Choice::System)
    }
    /// Roll back to the newest DISTINCT, currently verified previous runtime,
    /// or System. Repeated journal entries for the failing version must never
    /// select that same version again. A System selection is a safe terminal
    /// fallback: repeated rollback is idempotent, not a managed reactivation.
    pub fn rollback(&self) -> Result<Choice, Error> {
        let _guard = self.lock()?;
        self.rollback_locked()
    }

    fn rollback_locked(&self) -> Result<Choice, Error> {
        let entries = self.history()?;
        let current = entries.last().map(|entry| &entry.choice);
        let fallback = match current {
            Some(Choice::Managed(failing)) => entries
                .iter()
                .rev()
                .skip(1)
                .find_map(|entry| match &entry.choice {
                    Choice::System => Some(Choice::System),
                    Choice::Managed(version)
                        if version != failing && self.verified(version).is_ok() =>
                    {
                        Some(Choice::Managed(version.clone()))
                    }
                    // Never restore the same version, an unapproved binary,
                    // or a candidate whose digest/integrity no longer verifies.
                    _ => None,
                })
                .unwrap_or(Choice::System),
            _ => Choice::System,
        };
        // Avoid writing redundant System records (including an empty journal).
        // This also prevents a second rollback from re-enabling a failed build.
        if current != Some(&fallback) {
            self.record(fallback.clone())?;
        }
        Ok(fallback)
    }
    /// One lock-protected read binds generation, selection, verified executable
    /// and official digest to the exact engine config used for launch. Keep this
    /// returned lease alive until the child tree is confirmed reaped.
    pub fn launch_snapshot(
        &self,
        system_exe: &Path,
        system_root: &Path,
        project: &Path,
        project_root: &Path,
    ) -> Result<LaunchSnapshot, Error> {
        let guard = self.lock()?;
        let entries = self.history()?;
        let generation = entries.last().map_or(0, |entry| entry.sequence);
        let choice = entries.last().map_or(Choice::System, |entry| entry.choice.clone());
        let (binary, root, digest) = match &choice {
            Choice::System => (system_exe.to_path_buf(), system_root.to_path_buf(), None),
            Choice::Managed(version) => {
                let binary = self.verified(version)?;
                let digest = self.approval(version)?.digest().to_owned();
                (binary, self.root.join("versions").join(version), Some(digest))
            }
        };
        let config = ultima_app_server_supervisor::EngineConfig::new(
            binary, root, project, project_root,
        ).map_err(|_| Error::Invalid("launch snapshot config"))?;
        Ok(LaunchSnapshot {
            config,
            choice,
            digest,
            generation,
            owner: self.root.clone(),
            _guard: guard,
        })
    }

    /// Conditional rollback runs under the original launch lease, not a
    /// separate precheck and an unconditional journal update. A stale or
    /// foreign lease never mutates another actor's selected runtime.
    pub fn rollback_launch(&self, lease: &LaunchSnapshot) -> Result<Choice, Error> {
        if lease.owner != self.root || !matches!(lease.choice, Choice::Managed(_)) {
            return Err(Error::Invalid("foreign or system launch lease"));
        }
        let entries = self.history()?;
        let generation = entries.last().map_or(0, |entry| entry.sequence);
        let current = entries.last().map_or(Choice::System, |entry| entry.choice.clone());
        if generation != lease.generation || current != lease.choice {
            return Err(Error::Invalid("journal advanced after launch"));
        }
        self.rollback_locked()
    }

    pub fn selected(&self) -> Choice {
        self.active()
    }

    pub fn engine_config(
        &self,
        system_exe: &Path,
        system_root: &Path,
        project: &Path,
        project_root: &Path,
    ) -> Result<ultima_app_server_supervisor::EngineConfig, Error> {
        let (bin, root) = match self.active() {
            Choice::System => (system_exe.to_path_buf(), system_root.to_path_buf()),
            Choice::Managed(v) => (self.verified(&v)?, self.root.join("versions").join(v)),
        };
        ultima_app_server_supervisor::EngineConfig::new(bin, root, project, project_root)
            .map_err(|_| Error::Invalid("supervisor config"))
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;
    use tempfile::TempDir;
    fn fixture() -> (TempDir, Manager, PathBuf, String) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("owned");
        let mut manager = Manager::open(&root, vec![]).unwrap();
        let file = tmp.path().join(if cfg!(windows) { "codex.exe" } else { "codex" });
        fs::write(&file, b"synthetic test binary").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&file, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let hash = hash_file(&file).unwrap();
        // Private test fixture: production cannot construct ApprovedArtifact
        // except the exact reviewed upstream platform pin.
        manager.approvals.insert(
            "0.164.0".into(),
            Approval {
                version: "0.164.0".into(),
                release_url: format!("{UPSTREAM}0.164.0"),
                sha256: hash.clone(),
            },
        );
        (tmp, manager, file, hash)
    }
    #[test]
    fn stage_is_opt_in_and_fallback_after_tamper() {
        let (_tmp, manager, file, _hash) = fixture();
        assert_eq!(manager.active(), Choice::System);
        manager.stage(&file, "0.164.0").unwrap();
        assert_eq!(manager.active(), Choice::System);
        manager.record(Choice::Managed("0.164.0".into())).unwrap();
        assert_eq!(manager.active(), Choice::Managed("0.164.0".into()));
        let dest = manager.binary("0.164.0");
        fs::remove_file(&dest).unwrap();
        fs::write(&dest, b"tampered").unwrap();
        assert_eq!(manager.active(), Choice::System);
        assert_eq!(manager.rollback().unwrap(), Choice::System);
    }
    #[test]
    fn explicit_rollback_returns_preceding_selection() {
        let (_tmp, manager, file, _) = fixture();
        manager.stage(&file, "0.164.0").unwrap();
        manager.record(Choice::System).unwrap();
        manager.record(Choice::Managed("0.164.0".into())).unwrap();
        assert_eq!(manager.active(), Choice::Managed("0.164.0".into()));
        assert_eq!(manager.rollback().unwrap(), Choice::System);
        assert_eq!(manager.active(), Choice::System);
    }
    // These private synthetic approvals exercise multi-version selection logic
    // without weakening the single, officially pinned production release.
    fn add_test_runtime(manager: &mut Manager, root: &Path, version: &str) -> PathBuf {
        let source = root.join(format!("fixture-{version}.bin"));
        fs::write(&source, format!("synthetic-runtime-{version}")).unwrap();
        let digest = hash_file(&source).unwrap();
        manager.approvals.insert(
            version.to_owned(),
            Approval {
                version: version.to_owned(),
                release_url: format!("{UPSTREAM}{version}"),
                sha256: digest,
            },
        );
        manager.stage(&source, version).unwrap();
        source
    }

    #[test]
    fn rollback_skips_duplicate_records_for_the_same_runtime() {
        let (_tmp, manager, file, _) = fixture();
        manager.stage(&file, "0.164.0").unwrap();
        for _ in 0..3 {
            manager.record(Choice::Managed("0.164.0".into())).unwrap();
        }
        assert_eq!(manager.rollback().unwrap(), Choice::System);
        assert_eq!(manager.active(), Choice::System);
        let history_len = manager.history().unwrap().len();
        // A second rollback from System cannot resurrect the failed runtime.
        assert_eq!(manager.rollback().unwrap(), Choice::System);
        assert_eq!(manager.history().unwrap().len(), history_len);
    }

    #[test]
    fn rollback_prefers_newest_distinct_verified_managed_runtime() {
        let (tmp, mut manager, file, _) = fixture();
        manager.stage(&file, "0.164.0").unwrap();
        add_test_runtime(&mut manager, tmp.path(), "0.163.0");
        manager.record(Choice::Managed("0.163.0".into())).unwrap();
        manager.record(Choice::Managed("0.164.0".into())).unwrap();
        manager.record(Choice::Managed("0.164.0".into())).unwrap();
        assert_eq!(
            manager.rollback().unwrap(),
            Choice::Managed("0.163.0".into())
        );
        assert_eq!(manager.active(), Choice::Managed("0.163.0".into()));
    }

    #[test]
    fn rollback_skips_tampered_and_unapproved_candidates() {
        let (tmp, mut manager, file, _) = fixture();
        manager.stage(&file, "0.164.0").unwrap();
        add_test_runtime(&mut manager, tmp.path(), "0.163.0");
        add_test_runtime(&mut manager, tmp.path(), "0.162.0");
        manager.record(Choice::Managed("0.162.0".into())).unwrap();
        manager.record(Choice::Managed("0.163.0".into())).unwrap();
        manager.record(Choice::Managed("0.999.0".into())).unwrap();
        manager.record(Choice::Managed("0.164.0".into())).unwrap();
        manager.record(Choice::Managed("0.164.0".into())).unwrap();
        let bad = manager.binary("0.163.0");
        fs::remove_file(&bad).unwrap();
        fs::write(&bad, b"tampered").unwrap();
        // Scan past both an unapproved entry and an integrity failure.
        assert_eq!(
            manager.rollback().unwrap(),
            Choice::Managed("0.162.0".into())
        );
    }

    #[test]
    fn rollback_respects_prior_system_selection_and_is_idempotent() {
        let (tmp, mut manager, file, _) = fixture();
        manager.stage(&file, "0.164.0").unwrap();
        add_test_runtime(&mut manager, tmp.path(), "0.163.0");
        manager.record(Choice::Managed("0.163.0".into())).unwrap();
        manager.record(Choice::System).unwrap();
        manager.record(Choice::Managed("0.164.0".into())).unwrap();
        assert_eq!(manager.rollback().unwrap(), Choice::System);
        let count = manager.history().unwrap().len();
        assert_eq!(manager.rollback().unwrap(), Choice::System);
        assert_eq!(manager.history().unwrap().len(), count);
    }

    #[test]
    fn activating_system_again_is_a_no_op_for_the_bounded_journal() {
        let (_tmp, manager, file, _) = fixture();
        assert_eq!(manager.history().unwrap().len(), 0);
        manager.activate_system().unwrap();
        manager.activate_system().unwrap();
        assert_eq!(manager.history().unwrap().len(), 0);
        manager.stage(&file, "0.164.0").unwrap();
        manager.record(Choice::Managed("0.164.0".into())).unwrap();
        manager.activate_system().unwrap();
        let count = manager.history().unwrap().len();
        manager.activate_system().unwrap();
        manager.activate_system().unwrap();
        assert_eq!(manager.history().unwrap().len(), count);
        assert_eq!(manager.active(), Choice::System);
    }

    #[test]
    fn existing_store_attach_never_creates_or_adopts() {
        let temp = TempDir::new().unwrap();
        let missing = temp.path().join("missing");
        assert!(Manager::open_existing(&missing, vec![]).is_err());
        assert!(!missing.exists());
        let foreign = temp.path().join("foreign");
        fs::create_dir(&foreign).unwrap();
        fs::write(foreign.join(".ultima-managed-runtime-v1"), b"forged\n").unwrap();
        assert!(Manager::open_existing(&foreign, vec![]).is_err());
        assert_eq!(fs::read(foreign.join(".ultima-managed-runtime-v1")).unwrap(), b"forged\n");
        let owner = temp.path().join("production");
        Manager::open(&owner, vec![]).unwrap();
        assert!(Manager::open_existing(&owner, vec![]).is_ok());
    }

    #[test]
    fn launch_lease_excludes_concurrent_host_selection_and_rollback() {
        let (tmp, manager, file, _) = fixture();
        manager.stage(&file, "0.164.0").unwrap();
        manager.record(Choice::Managed("0.164.0".into())).unwrap();
        let snapshot = manager.launch_snapshot(&file, tmp.path(), tmp.path(), tmp.path()).unwrap();
        assert_eq!(snapshot.choice, Choice::Managed("0.164.0".into()));
        assert!(snapshot.digest.is_some());
        assert!(matches!(manager.activate_system(), Err(Error::Locked)));
        let other = Manager::open_existing(&manager.root, vec![]).unwrap();
        assert!(matches!(other.rollback(), Err(Error::Locked)));
        assert_eq!(manager.history().unwrap().len(), 1);
        assert_eq!(manager.rollback_launch(&snapshot).unwrap(), Choice::System);
        assert_eq!(manager.active(), Choice::System);
        let recovered_len = manager.history().unwrap().len();
        // A second native attempt for the same failed runtime cannot replay it.
        assert!(manager.rollback_launch(&snapshot).is_err());
        assert_eq!(manager.history().unwrap().len(), recovered_len);
        drop(snapshot);
        assert!(manager.activate_system().is_ok());
        let reconnect = manager.launch_snapshot(&file, tmp.path(), tmp.path(), tmp.path()).unwrap();
        assert_eq!(reconnect.choice, Choice::System);
    }

    #[test]
    fn stale_launch_generation_does_not_rollback_external_journal_change() {
        let (tmp, manager, file, _) = fixture();
        manager.stage(&file, "0.164.0").unwrap();
        manager.record(Choice::Managed("0.164.0".into())).unwrap();
        let snapshot = manager.launch_snapshot(&file, tmp.path(), tmp.path(), tmp.path()).unwrap();
        // Simulate a malicious/out-of-contract external writer bypassing
        // the lease; even then the rollback refuses the newer generation.
        manager.record(Choice::System).unwrap();
        let before = manager.history().unwrap().len();
        assert!(manager.rollback_launch(&snapshot).is_err());
        assert_eq!(manager.history().unwrap().len(), before);
        assert_eq!(manager.selected(), Choice::System);
    }

    #[test]
    fn snapshot_blocks_system_to_managed_until_release() {
        let (tmp, manager, file, _) = fixture();
        manager.stage(&file, "0.164.0").unwrap();
        let system = manager.launch_snapshot(&file, tmp.path(), tmp.path(), tmp.path()).unwrap();
        assert_eq!(system.choice, Choice::System);
        assert!(matches!(manager.activate_system(), Err(Error::Locked)));
        assert!(manager.rollback_launch(&system).is_err());
        drop(system);
        manager.record(Choice::Managed("0.164.0".into())).unwrap();
        let managed = manager.launch_snapshot(&file, tmp.path(), tmp.path(), tmp.path()).unwrap();
        assert_eq!(managed.choice, Choice::Managed("0.164.0".into()));
        assert!(manager.rollback_launch(&managed).is_ok());
    }

    #[test]
    fn journal_corruption_fails_closed_and_forged_folders_rejected() {
        let (tmp, manager, _, _) = fixture();
        manager.record(Choice::Managed("0.164.0".into())).unwrap();
        fs::write(
            manager.root.join("journal/00000000000000000001.json"),
            b"corrupt",
        )
        .unwrap();
        assert_eq!(manager.active(), Choice::System);
        assert!(manager.activate_system().is_err());
        let foreign = tmp.path().join("foreign");
        fs::create_dir(&foreign).unwrap();
        fs::write(foreign.join("keep"), b"keep").unwrap();
        assert!(Manager::open(&foreign, vec![]).is_err());
        assert_eq!(fs::read(foreign.join("keep")).unwrap(), b"keep");
    }
}
