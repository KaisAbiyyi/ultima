# NC-04 — Rust Codex App Server process supervisor

Scope: standalone Rust transport core. Integration into the Tauri desktop follows in NC-06. NC-05 owns JSON-RPC request IDs, protocol negotiation, typed events, approvals and cancellation. This crate launches the user-approved native executable with a fixed app-server argument; it never invokes a shell or a WebSocket endpoint.

## Host integration contract

1. Select an official system Codex native binary under an explicitly trusted installation root. The directory discovery helper probes caller-approved directories only. It does not trust PATH or execute Windows .cmd shims.
2. Select a project path within a separately trusted project root. Both roots, executable and project cwd are canonicalized. Symlink/path traversal escape is rejected.
3. Instantiate a single EngineSupervisor, call start, and use the session to send JSON values and receive newline-delimited frames. Follow the NC-01 initialize/initialized handshake before all other JSON-RPC calls. This crate must not decide approvals.
4. stop kills and reaps the owned direct child, aborts readers and discards queued frames. restart is explicit, never an automatic unlimited crash loop. A new generation requires clearing outstanding RPC/approval state, renegotiating initialize, and resuming threads via upstream protocol rather than silently replaying tool actions.
5. JSONL input/output maximum is 1 MiB per frame. Stdout read chunks are 8 KiB and the bounded event channel defaults to 64, hard maximum 128. Invalid UTF-8, output overflow, read failures and EOF are categorical events, not raw log lines. On terminal transport events the caller must stop or restart.
6. Writes are serialized as one JSON frame followed by newline. A write timeout/failure destroys the session because a partially written frame cannot safely be continued.
7. Raw engine stderr is drained into a sink (not logged, no secrets leaked). Any future structured diagnostics/redaction require independent security review. Engine itself may log to its own managed destinations; those are outside this crate.

## Cloud acceptance

GitHub Actions Linux and hosted Windows run Rust format, Clippy and synthetic child-process tests for output framing, crash/restart, JSONL limits, trusted executable/workspace paths, and sanitized diagnostics. These are not native product RAM/latency/WebView2/packaging evidence; such gates are NOT_RUN until measured.

## Deferred risks and responsibilities

- Tauri integration and single instance coordination across UI windows: NC-06.
- JSON-RPC event routing, bounded request queues and permission enforcement: NC-05.
- Windows Job Object descendant process-tree containment is not implemented here. stop/kill_on_drop cover the direct child, not a grandchild tree; a native tree cleanup gate is REQUIRED before claiming that ability or shipping tool subprocess workloads.
- Upstream binary installer management, version pinning, updates and rollback: NC-12.
