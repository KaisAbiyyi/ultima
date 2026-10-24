# NC-06 — Interactive Codex desktop shell

## Included
- Tauri 2 host window loads `/?desktop=1`; NC-03 prototype is preserved at `/` for regression comparison.
- Svelte UI: explicit project/executable path inputs, local recent-project/thread-ID metadata, server thread/start and thread/resume, model/list, text composer, assistant delta streaming, real turn/interrupt, errors, connection status and bounded message rendering.
- Native Rust command allowlist uses the NC-04 supervisor and NC-05 broker. The host does not invoke a shell, fetch credentials, expose TCP ports or auto-retry a tool.
- Approval requests are **declined**, not accepted, until NC-07 implements an explicit confirmation workflow. Errors are deliberately redacted to avoid exposing sensitive upstream message payloads.
- Entire UI can be previewed in a browser without an engine, but sending/connecting is intentionally disabled there.

## Run
1. Install Node 22+, Rust toolchain, Tauri 2 native prerequisites for your platform. Install dependencies: `npm install`.
2. Run `npm run dev:desktop` from the repo root. Add a known absolute project folder in the sidebar.
3. Supply the trusted absolute path to your already-installed Codex executable (native `codex.exe` on Windows) and choose **Connect**. The supervisor canonicalizes executable and project; it never searches PATH or shells out.
4. Use the engine-reported model list, new threads and saved thread IDs. An authenticated Codex CLI install is a prerequisite; Ultima does not handle passwords or copy tokens.

## Known limits and remaining acceptance
- No chooser dialog/interactive tool approval or rich command/diff flow yet (NC-07). In particular the current workflow cannot execute commands requiring tool approval.
- LocalStorage holds recent project paths and thread IDs **only**; no user message content or credentials are persisted by Ultima. Codex owns durable thread content and auth.
- Recovered thread render depends on App Server `thread/resume` reply returning `thread.turns[].input/items` for the pinned interface; missing history is shown empty instead of fabricating messages.
- Synthetic browser/Rust CI does not demonstrate authenticated live Codex execution, native owner Windows WebView2 behavior, process-tree RAM, p95 input/stream latency or release readiness. Those remain NOT_RUN pending proper evidence.
- Backend Tauri build is separately validated in NC-06 hosted cross-platform CI. Never treat hosted runner data as owner's machine.

## NC-06 disconnect/reconnect acceptance
- The native process-tree gate moves atomically from Running to Stopping; a new connect cannot spawn until `EngineSupervisor::stop()` confirms successful shutdown. Teardown errors poison the slot, blocking new starts until the application is restarted and old child state is diagnosed.
- Disconnect uses a detached completion watcher so a cancelled IPC request does not release the native slot early. Stale Rust teardown callbacks cannot clear a newer generation.
- Every host event carries a session generation. The UI accepts only the acknowledged generation, buffers up to 64 pre-ack events, ignores stale replies/statuses/deltas, and invalidates pending request callbacks on disconnect.
- Hosted Windows CI runs native lifecycle unit tests (rapid 20-cycle stop/reconnect, simultaneous start, failed stop, stale completion) in addition to Tauri `cargo check`. Vitest covers late UI events and buffer bounds.
