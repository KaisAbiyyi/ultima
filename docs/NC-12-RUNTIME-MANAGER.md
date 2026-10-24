# NC-12 — Verified optional managed Codex + rollback

**Default is system Codex.** The standalone Rust manager does not download, replace, modify, or invoke a user's system-installed Codex binary; it does not change PATH, npm global installs, Codex login/config, or workspace files. It only supplies a verified executable selection to NC-04 for the next launch.

## Trust and staging
- Compatible versions are fail-closed against NC-11's compile-time official matrix. Only the currently **supported pinned version** is eligible; unknown versions and candidate releases are rejected until NC-11 promotion + reviewed rebuild.
- Host must supply an **independently trusted, reviewed SHA-256 digest of the official executable asset** and the exact official release URL. Merely obtaining a hash next to an untrusted binary is *not* proof of provenance. This library checks digest, release URL, and version but **does not verify artifact signatures or fetch upstream attestations**; those are distribution/owner responsibilities.
- Stage a raw native binary (no archives, scripts or network). Size <=512 MiB. Byte-for-byte SHA-256 check before and after private-store copy. Destination stays inside a dedicated Ultima-owned store at versions/<version>/codex[.exe]. Never overwrite a version. Unrelated nonempty directories cannot be adopted as a managed store. Up to eight versions, explicit update lock; no implicit garbage collection.
- Reject symlinked source, store root, store subdirectories, staged version folder and binary as inspected. Protect store permissions using the OS: simultaneous hostile file mutation by a process with equal write privileges is outside this library's guarantee.

## State transitions and failure safety
- Stage does **not** activate. activate_managed, activate_system and rollback only write immutable sequence-numbered JSON journal entries via temp file, fsync and same-directory rename. Bounded at 128 entries; corruption, missing binary or hash mismatch falls back to system. A crash leftover update lock fails closed until manually examined/recovered.
- Offline operation is supported for previously approved+staged bytes: no network needed at launch. New updates require the prior independently trusted digest; no online-only silent auto-fetch.
- Rollback selects previous verified runtime or system if previous is unusable. A caller **must stop/reap** its existing NC-04 EngineSupervisor and invalidate outstanding protocol/approval state before adopting a new selection. No automatic process hot-swap, restart loop or secret logging.

## Scope / evidence
CI covers Linux and hosted Windows Rust static checks, synthetic integrity, version forgery, default system, persistent activation, rollback, journal corruption, immutable install, path/permission, offline re-selection and symlink rejection. CI does NOT prove real official binary provenance, signed update packaging, live healthcheck/automatic rollback integration, owner native Windows RAM/latency, authenticated turns, or Tauri UI integration. These are **NOT_RUN** pending NC-06/NC-10/NC-13 or follow-up release qualification. Do not market as production updater yet.
