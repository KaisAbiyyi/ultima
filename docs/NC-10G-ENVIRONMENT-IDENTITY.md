# NC-10G environment identity correction

Strict cloud-only correction from `907a8f870b213d79d1d61bf7be77d51f21cf3472`.
Draft PR #42; issue #33. No physical or authenticated acceptance is claimed.

## Repeated read-only investigation

The separate accepted Phase 1 report was unavailable in the accessible Notion
checkpoint, issue receipts and repository. Before editing source we repeated
its necessary investigation against the official Codex tag
`rust-v0.166.0-alpha.5.1`, peeled commit
`f79b26ccbb3116b052544666a181012fb45dc49b` (also the previous correction's
protocol baseline):

- [Approval params](https://github.com/openai/codex/blob/f79b26ccbb3116b052544666a181012fb45dc49b/codex-rs/app-server-protocol/src/protocol/v2/item.rs#L1654)
  define optional nullable `environmentId` as the environment in which the
  command will run. `kind` defaults to command. Regular shell/unified-exec
  callbacks have null/absent `approvalId`; callback-specific and stdin approvals
  remain unsupported here.
- [Environment manager](https://github.com/openai/codex/blob/f79b26ccbb3116b052544666a181012fb45dc49b/codex-rs/exec-server/src/environment.rs#L123)
  defines `LOCAL_ENVIRONMENT_ID = "local"`; registration reserves that identity
  for EnvironmentManager. Other environment IDs exist and are not evidence of
  local execution. Generic non-null acceptance is unsafe.
- [Server approval forwarding](https://github.com/openai/codex/blob/f79b26ccbb3116b052544666a181012fb45dc49b/codex-rs/app-server/src/bespoke_event_handling.rs#L674)
  forwards the event environment into the authoritative approval params. The
  exact command is retained. CommandExecution preview/history items have no
  environment field; do not synthesize identity from cwd, display text or a
  frontend-supplied environment echo.
- [Response mapping](https://github.com/openai/codex/blob/f79b26ccbb3116b052544666a181012fb45dc49b/codex-rs/app-server/src/bespoke_event_handling.rs#L2025)
  maps `accept` to `ReviewDecision::Approved`. Persistent/session and amendment
  decisions are separate variants. A proposal is metadata, never a grant.

The physical failure's [cloud receipt](https://github.com/KaisAbiyyi/ultima/issues/33#issuecomment-6521059805)
reports local identity, proposal metadata, available accept and fail-closed
rejection before dispatch. Raw owner-local evidence was not accessed. Shared
fixtures explicitly identify themselves as synthetic, not captured evidence.

## Invariant and implementation

This client authorizes only local command scope: exact `"local"`, or legacy
absent/null identity. Every other value fails closed, including valid remote
IDs, wrong types, empty strings, whitespace/case variants, lookalikes and large
values. Rust shares the typed `LocalCommandEnvironment` parser between native
and Tauri; Svelte implements the same fixed scope set and shared fixture matrix.

Scope classification never grants consent. The **complete original descriptor**
(including raw environment representation) must match the pending broker entry
at dispatch. Absent, null and explicit local are never rewritten or treated as
interchangeable for consent equality. Native review now stores the entire
Approval; Svelte stores a cloned ApprovalRequest and compares it at click time.
Host-side independent evidence/scope validation and broker exact equality both
remain necessary. A remote descriptor with identical command/cwd cannot inherit
local consent. Environment changes after preview fail closed.

All existing command/canonical cwd, session, thread, turn, item, timestamp,
permission/evidence, cancellation, reconnect and one-shot checks remain. Unknown
request fields, non-null permission/network grants, unsupported callback/stdin
approvals, malformed proposals, and unavailable one-shot accept remain rejected.
File-change approval scope is unchanged (command environment fields remain
unsupported). Allow once still sends only `ApprovalDecision::Accept` → `"accept"`;
no advertised persistent choice is dispatched and no proposal is applied.

## Validation and independent-review handoff

The new Vitest reproduction was observed failing at the exact local fixture
before the implementation and passing afterward. Shared local/adversarial
fixtures exercise all three renderer paths. Native lifecycle tests check the
whole route through evidence, descriptor, pending ledger and broker to the exact
`{"id":"local-once","result":{"decision":"accept"}}` response. Broker tests
reject environment substitution including legacy null/absent, stale reuse and
new-session reuse. Browser tests check explicit review and shutdown on a changed
environment. Existing cancellation, reconnect, permission, proposal and complete
file-diff regressions remain required.

Publish exact final SHA, changed paths, counts and workflow links in PR #42 and
issue #33; historical CI at the starting SHA does not qualify this correction.
Request fresh independent GPT-6 source/security review against that final SHA,
including both environment scope and full descriptor equality at review/click/
host/broker boundaries. No self-approval. Keep PR Draft/unmerged and NC-10G in
Review pending independent review and separately authorized physical Windows
acceptance. Live Allow once/Decline, a second independently pending command,
file-change diff, Stop/reconnect, actual process cleanup, runtime activation/
rollback and policy/journal audit remain owner-local acceptance requirements.
