# NC-10E — Native approval, command and diff security boundary

Cloud-only implementation checkpoint. **Not a Windows owner-acceptance or release receipt.** NC-10 and its baseline PR #26 remain Draft until independent final gate NC-10J.

## Mode / product boundary

- The existing native egui renderer defaults to the original official `read-only`, `never` approval policy. `--enable-native-coding` is an **explicit launch-only opt-in** and requires both `--codex-exe` and `--project`.
- Coding mode requests `workspace-write` and `on-request` with network access denied. The broker validates typed RPC envelopes and the engine verifies the **returned effective** approval policy, sandbox type, network condition, canonical project directory and thread cwd before submitting a model turn. Any downgrade/mismatch aborts the session.
- This is still an experimental native client. Production-host integration, physical Windows UI checks, endurance, distribution and final parity are separate NC-10G/NC-10H/NC-10I/NC-10J gates.

## Authority / consent threat model

The official Codex broker owns RPC identities and is the authority for pending approval requests. Neither egui text nor tool-preview notifications grant permission. The `ToolLedger` stores bounded, keyed `threadId+turnId+itemId` evidence and exact immutable approval descriptors; the native worker re-checks broker state immediately before replying.

A request can be **accepted once only** when all of the following hold: selected official turn is active; the request descriptor and broker pending entry match; a complete, untruncated command or every proposed file diff was observed; the user explicitly opened the corresponding native review, then clicked “Allow once”; the command queue did not fail; the request has not been canceled or resolved. There is **no automatic accept**.

Unknown, foreign-thread/turn, repeated, stale, over-quota or evidence-incomplete requests are declined by the broker. If a decline cannot be sent or an approval decision becomes ambiguous, the owned Codex process is stopped. A decline is always permitted against a matching pending request even if the previously reviewed preview became invalid; an accept is not. A completed turn and Stop revoke pending approval controls; a canceled/finished turn cannot grant a late approval. The UI never retries a decision when ACK is lost.

## Render limits and path constraints

- At most 16 tool items / 16 pending UI requests.
- Command: 4 KiB; command output: 32 KiB; aggregate proposed diff: 64 KiB; at most 32 change paths, each at most 512 bytes.
- File-change approvals reject partial/absent diff evidence, contradictory counts, repeated/absolute/traversal/encoded paths, existing escaping links and dangling symlinks.
- Tool evidence and command output are displayed as text, not executed as code or interpreted as markup; output/diff panes expand lazily in bounded native scroll areas.
- **Residual:** in-app path checks cannot defeat a malicious process that changes filesystem links after inspection; the operating-system sandbox and verified project root remain the enforcement boundary. Never treat visual diff evidence as proof of immutable filesystem content.

## Deterministic evidence

`.github/workflows/nc-10e-native-approval.yml` executes `cargo fmt --check`, `cargo test --locked`, and strict `cargo clippy -D warnings` on Ubuntu and Windows. Native regression also runs in the parent NC-10 qualification workflow.

The new test cases cover: downgrade/project validation; one-shot broker-approved request; forged identity and duplicate decision; server request resolution race; invalid paths and partial diffs; bounded output; refusal after preview changes; late/duplicate handling. Check exact commit results; previously passing runs on older commits **are not transferable** to a later head.

## Merge / acceptance

1. Exact-head Ubuntu and Windows synthetic runs pass.
2. Independently audit the complete PR diff, especially click-to-consent binding and protocol event races; resolve all P0/P1.
3. Merge NC-10E only after independent review, then land integration into the parent branch without regressing read-only defaults.
4. Leave physical Windows acceptance **NOT_RUN** until measured by owner under separate downstream gates. Do not merge or mark parent NC-10 Done based on this PR.
