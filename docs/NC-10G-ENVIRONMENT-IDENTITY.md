# NC-10G command / FileChange environment authorization

Strict cloud-only FileChange parity correction from
`2db4c86dfdc2dec4b058bfdd66c3806758642a2b`, on the existing Draft PR #42 / issue #33.
The successful explicit-local command correction is preserved.
No physical Windows or authenticated acceptance is claimed.

## Contract

Only an original command approval descriptor with **explicit
`environmentId: "local"`** is eligible for Allow once. Missing/null identity is
unidentified execution, never an implicit local grant. No compatibility fallback
or version gate is introduced.

FileChange approvals require the `environmentId` field to be **absent**. Any
present field is invalid, including null and the otherwise valid command identity
`"local"`. A field with JS value `undefined` is also rejected by presence in Svelte;
no descriptor is normalized to remove it.

| Original field | Command Allow once | FileChange Allow once |
| --- | --- | --- |
| Exact string `"local"` | Eligible after all existing checks | Rejected |
| Missing | Rejected | Eligible after complete diff review and all existing checks |
| Null | Rejected | Rejected |
| Remote/custom/empty/malformed/non-string | Rejected | Rejected |

Inspection and safe Decline remain available within existing descriptor bounds.
The public `ApprovalKind::environment_allows_approval` predicate expresses both
contracts and is shared by the broker and native UI/action handler. It is only a
necessary environment constraint, never a grant or replacement for exact matching.
Native/Tauri validators retain their independent FileChange field rejection.

The shared Rust classification preserves `Absent`, `Null`, `Local` and
`Unsupported` for diagnostics. Its explicit `allows_approval()` predicate is true
only for `Local`. Classification never rewrites the original descriptor or grants
permission. Command authorization calls that predicate; FileChange uses field absence.
Svelte applies the same rules, including strict property presence.

The broker validates the **authoritative pending descriptor** before serializing
Accept, including through its direct `reply_approval` API. FileChange accepts only
field absence; both direct and exact APIs reject every present value without
enqueueing Accept or consuming the pending request. Rejection does not
consume the request or enqueue a response; safe decline remains available.
Protocol ingestion and notifications are not globally rejected for unsupported
identity. Native/Tauri independently reject unsupported original requests before
calling broker acceptance. Complete file-diff and path/scope validation is unchanged.

The native card and its action handler share an eligibility predicate. A reviewed
unsupported request cannot enable Allow once or enter the engine decision queue.
Both UIs explain unidentified/unsupported execution and keep Decline available.
Native review still stores the complete Approval; Svelte retains a cloned
ApprovalRequest and rechecks it at click time.

## Descriptor and permission invariants

The complete raw request, including whether environment identity is absent, null
or a string, remains immutable and must match the exact pending broker entry.
A changed environment never inherits consent, even with identical command/cwd.
There is no manufactured environment echo from preview/history items, which do
not carry that authority.

All existing command/canonical-cwd, session/thread/turn/item/timestamp,
permission/evidence, cancellation, reconnect, stale-replay and one-shot checks
remain required. Unknown request fields, non-null escalation fields, unsupported
callback/stdin approvals, malformed/oversized proposals and unavailable one-shot
accept remain rejected. Complete file-change validation is unchanged.

Allow once still sends only `ApprovalDecision::Accept` → `"accept"`.
`proposedExecpolicyAmendment` remains bounded read-only proposal metadata;
advertised persistent/session/network-policy choices are never dispatched or
applied. Identity itself never confers permission.

## Protocol evidence and fixture provenance

Official baseline: Codex `rust-v0.166.0-alpha.5.1`, peeled commit
`f79b26ccbb3116b052544666a181012fb45dc49b`:

- [Approval params](https://github.com/openai/codex/blob/f79b26ccbb3116b052544666a181012fb45dc49b/codex-rs/app-server-protocol/src/protocol/v2/item.rs#L1654)
  permit optional nullable environment metadata; protocol representability does
  not establish authorization eligibility for this local-only client.
- [Environment manager](https://github.com/openai/codex/blob/f79b26ccbb3116b052544666a181012fb45dc49b/codex-rs/exec-server/src/environment.rs#L123)
  reserves `LOCAL_ENVIRONMENT_ID = "local"`; other environments exist.
- [Server forwarding](https://github.com/openai/codex/blob/f79b26ccbb3116b052544666a181012fb45dc49b/codex-rs/app-server/src/bespoke_event_handling.rs#L674)
  puts the environment in the authoritative approval request.
- [Response mapping](https://github.com/openai/codex/blob/f79b26ccbb3116b052544666a181012fb45dc49b/codex-rs/app-server/src/bespoke_event_handling.rs#L2025)
  distinguishes one-shot Accept from session/persistent amendments.

The [physical failure receipt](https://github.com/KaisAbiyyi/ultima/issues/33#issuecomment-6521059805)
reports explicit local identity, proposal metadata and available accept. Shared
fixtures are explicitly synthetic, built from that reported combination and the
official schema. Raw owner-local evidence was not accessed. The old assumption
that absent/null metadata may authorize local execution was not justified by an
explicit version-gated contract; this correction removes it.

## Regression and parity gates

Before implementation, new tests reproduced original missing identity being
accepted by Svelte and the broker, and the actual native card enabling Allow once.
Headless native tests render the real egui card and read its accessibility nodes:
16 command and 16 FileChange cases verify Allow once/Decline enabled states, the
unsupported explanation, and direct action-handler rejection. New pre-fix tests
observed FileChange null metadata enabling the card and succeeding at broker
Accept; both are now rejected.

`tests/approval-parity` is a test-only executable importing the **actual** native
ToolLedger and Tauri EvidenceLedger source. It constructs each original request
with valid command/cwd/evidence, exercises native→broker, host→broker, direct and exact broker decisions for both
approval kinds (32 original-descriptor cases), and preserves unsupported
requests in the native and broker pending ledgers for safe decline. Its actual
wire receipts are compared against Svelte's production parser/validator in Vitest.
This joint test is required on Windows and Ubuntu in the NC-10E workflow; it is
not merely a collection of isolated expected-value unit tests.

Example cloud commands (redirect receipt outside the checkout):

```sh
cargo run --locked --manifest-path tests/approval-parity/Cargo.toml > /tmp/nc10g-parity.json
NC10G_PARITY_RECEIPT=/tmp/nc10g-parity.json npx vitest run --config tests/approval-parity/vitest.config.ts
```

Existing exact descriptor, stale/cross-session/reconnect/cancel, proposal bounds,
unknown/escalation field, timestamp and complete file-change regressions remain
required. Positive fixtures for those unrelated checks now identify explicit local
execution so environment rejection cannot mask their intended validation.
Browser tests cover explicit one-shot command dispatch, original unsupported
identities with safe decline, local→remote/null/absent substitution after review,
and FileChange full-diff review with absent/null/local environment metadata.

## Handoff

Publish starting/final SHA, changed paths, exact counts and exact-new-HEAD workflow
links in PR #42, issue #33 and canonical Notion. Old CI does not qualify this change.
Request fresh independent GPT-6 security/source review of the final SHA. No
self-approval or merge; keep PR Draft and NC-10G in Review, not Done.

Separate owner-authorized physical acceptance must rebuild the independently
reviewed exact SHA and repeat real signed-in Allow once/Decline, a second
independent approval, file-change diff, Stop/reconnect, history/lifecycle, actual
process cleanup, managed runtime selection/rollback and policy/journal audit.
Hosted Windows CI and synthetic fixtures do not establish physical acceptance.
