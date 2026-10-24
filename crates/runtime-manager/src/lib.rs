//! NC-12: opt-in, checksum-verified Codex selection and crash-safe rollback.
//! No network/download, global PATH changes, shell, or executable overwrite.
mod health;
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
        let pins: serde_json::Value = serde_json::from_str(include_str!("../../../protocol/managed-assets.json"))
            .map_err(|_| Error::Invalid("assets manifest"))?;
        let target = if cfg!(all(target_os="windows", target_arch="x86_64")) {
            "x86_64-pc-windows-msvc"
        } else {
            return Err(Error::Invalid("unsupported platform"));
        };
        if pins["schemaVersion"] != 1 || pins["version"] != "0.164.0" ||
            pins["source"] != "https://api.github.com/repos/openai/codex/releases/tags/rust-v0.164.0" ||
            pins["release"] != "https://github.com/openai/codex/releases/tag/rust-v0.164.0" {
            return Err(Error::Invalid("untrusted manifest"));
        }
        let assets = pins["assets"].as_array().ok_or(Error::Invalid("assets"))?;
        if assets.len()!=1 { return Err(Error::Invalid("asset count")); }
        let item = assets.iter().find(|i| i["target"]==target).ok_or(Error::Invalid("architecture"))?;
        let version = pins["version"].as_str().ok_or(Error::Invalid("version"))?;
        let name = format!("codex-{target}.exe");
        let url = format!("https://github.com/openai/codex/releases/download/rust-v{version}/{name}");
        let hash = item["sha256"].as_str().ok_or(Error::Invalid("digest"))?;
        if !qualified(version) || item["name"]!=name || item["url"]!=url ||
            item["size"]!=354558768u64 || hash.len()!=64 ||
            !hash.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
            return Err(Error::Invalid("official asset"));
        }
        Self::new(version, pins["release"].as_str().ok_or(Error::Invalid("release"))?, hash)
    }

    /// Retained API shape but now only accepts the exact official, pinned
    /// digest for the OS/architecture; arbitrary hash + tag cannot qualify.
    pub fn new(version: &str, url: &str, hash: &str) -> Result<Self, Error> {
        let supported_target = cfg!(all(target_os="windows", target_arch="x86_64"));
        let official_hash = "6f267830b029ee25d9339864aa867cd45de9f8fd4c5ea007d2e63c13a0c66d36";
        if !supported_target || !qualified(version) || version != "0.164.0" ||
            url != format!("{UPSTREAM}{version}") || hash != official_hash {
            return Err(Error::Invalid("trusted official release digest"));
        }
        Ok(Self { version: version.into(), release_url: url.into(), sha256: hash.into() })
    }

    pub fn version(&self) -> &str { &self.version }
    pub fn digest(&self) -> &str { &self.sha256 }
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
        &self, version: &str, project: &Path, project_root: &Path,
    ) -> Result<(), Error> {
        let _guard = self.lock()?;
        let path = self.verified(version)?;
        let trusted = self.root.join("versions").join(version);
        let config = ultima_app_server_supervisor::EngineConfig::new(
            &path, &trusted, project, project_root,
        ).map_err(|_| Error::Invalid("preflight config"))?;
        health::probe(config).await?;
        // Detect any concurrent post-smoke binary change.
        self.verified(version)?;
        self.record(Choice::Managed(version.to_owned()))
    }
    pub fn activate_system(&self) -> Result<(), Error> {
        let _guard = self.lock()?;
        self.record(Choice::System)
    }
    pub fn rollback(&self) -> Result<Choice, Error> {
        let _guard = self.lock()?;
        let entries = self.history()?;
        let previous = if entries.len() < 2 {
            Choice::System
        } else {
            entries[entries.len() - 2].choice.clone()
        };
        let choice = match previous {
            Choice::Managed(ref v) if self.verified(v).is_err() => Choice::System,
            _ => previous,
        };
        self.record(choice.clone())?;
        Ok(choice)
    }
    pub fn selected(&self) -> Choice { self.active() }

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
