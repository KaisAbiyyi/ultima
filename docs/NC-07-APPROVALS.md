# NC-07 — Coding approvals, command output and file diffs

## Security contract

NC-07 adds a single-action-per-request permission card to the NC-06 Svelte/Tauri desktop. Default is **decline**, never accept. No standing grant, automatic retry, persistence, or background permission escalation exists.

1. The pinned official App Server `item/commandExecution/requestApproval` and `item/fileChange/requestApproval` arrive as server-side JSON-RPC requests through the bounded NC-05 broker.
2. The host emits an `approvalPending` event with its session generation and complete immutable request identity (RPC ID, kind, thread ID, turn ID, item ID, timestamp). A malformed or unknown server request is denied by the bridge, not inferred.
3. A user must inspect the command, or open the provided file-change diff, and explicitly check **I reviewed this specific action** before clicking **Allow once**. If a command/diff is missing or truncated, accept stays disabled. Preview is text-only, never injected as HTML, and never executes code.
4. An `ultima_approval` IPC with the exact generation and request identity is checked again inside Rust, against the still-pending upstream request; stale, duplicate or already-resolved approvals cannot be replayed. The broker only writes the one decision. A missing acknowledgement is treated as uncertain and **never automatically retried**.
5. On interrupt, all outstanding approval requests for the interrupted turn are declined first, then the upstream `turn/interrupt` RPC is sent. A failed decline/flush terminates the owned session. On disconnect, the NC-06 session gate keeps the engine exclusive until process-tree teardown completes. On reconnect, old-generation approval and tool events are discarded.
6. An unexpected thread or full UI permission queue prompts an explicit decline, never an allow. No permission content is written to LocalStorage. Server and UI queues have independent hard limits.

## Performance bounds and user-visible limits

- Broker JSONL line maximum: 1 MiB; pending approvals <=32; event and outbound byte/count caps retained from NC-05.
- UI holds at most **16** recent tool entries. Per entry command preview is 4,096 characters, aggregated command output 32 Ki characters and diff text 64 Ki characters, with visible truncation notice.
- Diff and output panels render on demand when expanded. Preview is bounded and read from the server event; **no filesystem scan or untrusted path read** is done just for a preview.
- **All-or-nothing file coverage:** every declared file change must have a nonempty valid path and nonempty full diff. Malformed, missing, empty, duplicated, explicitly partial/truncated, count-mismatched, >32-file, overlong-path or aggregate >64 Ki-character previews are flagged incomplete and **cannot** enable Allow once. One valid file diff must never mask another missing diff. Any later incomplete snapshot revokes previously complete evidence.
- **Correct-turn provenance:** evidence is keyed by the exact `threadId + turnId + itemId`. Item and output notifications without all three fields never establish reviewable proof. Reusing an item ID in another turn cannot reuse an old command/diff or the approval checkbox. The active turn and selected thread must match the approval request before allowing it.
- If upstream sends a file-change item without a full diff, file approval remains **decline-only** until a compatible full preview exists; never fabricate evidence or infer it from filenames.
- The synthetic/browser preview has no active bridge; it cannot run tools or grant permissions.

## Tests and acceptance

- `cargo test --manifest-path crates/protocol-bridge/Cargo.toml`: explicit accept, stale identity, deny-on-interrupt, duplicate/no replay, bounded output notification.
- `npm run check && npm test && npm run build && npm run test:e2e`: typed IPC, complete positive two-file diff, negative second-file missing/empty/malformed diff, count mismatch, per-file/aggregate overflow, changed evidence snapshots, reused item IDs across threads/turns, bounded item retention, browser fail-closed and NC-03/06 regressions.
- Dedicated `.github/workflows/nc-07-approvals.yml` runs Linux Rust and frontend, hosted Windows Rust/Tauri. Existing NC-01, NC-03, NC-05, NC-06 workflows also guard regressions.

## Acceptance boundary

Hosted CI can validate synthetic approval and lifecycle behavior. Authenticated real Codex tool requests, actual Windows WebView2 UI/security review, exact machine RAM/p95, and owner sign-off are **NOT_RUN** until their own evidence exists. Do not conflate hosted Windows with the owner's Windows device. NC-07 remains **Review**, not Done, until independent checker approval. No merge is authorized by implementation alone.
