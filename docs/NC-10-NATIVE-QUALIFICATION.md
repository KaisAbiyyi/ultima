# NC-10 native Windows qualification

Owner: `Codex-local-20261025-NC10`; lock: `native-benchmark`.
Base: `524432c776d08f5c12e71e2a41509b20bd71b93e`, pulled from main before claim.
Branch: `feat/nc-10-native-qualification`. Scope and budgets come from
[the performance contract](PERFORMANCE-CONTRACT.md), S0–S5.

## Implementation ledger

1. Claim task and lock in Notion and GitHub before implementation. COMPLETE.
2. Add bounded, opt-in UI timing observations and regression tests. COMPLETE.
3. Add native runner, artifact/device identity, matched CLI baseline and strict
   evidence evaluation. Test incomplete, wrong-platform and failed evidence. COMPLETE.
4. Build and exercise the integrated Windows candidate; record raw samples,
   timing percentiles, long-history and endurance outcomes.
5. Review the complete diff, run relevant checks, and synchronize exact local
   implementation status and outstanding acceptance gates.

Native build repair: main's unconstrained Rust `tauri = "2"` resolved 2.12.5,
but the pinned frontend API is 2.8.0. `tauri build --no-bundle` refused the
mismatch. Pin Rust Tauri to 2.8.0 and retain dependency lockfiles; qualify this
integrated version rather than bypassing the CLI version check.

Native build also exposed a local frontend test blocker: Vite's automatic HTML
entry scan included `src-tauri/target/.../tauri-codegen-assets/*.html`. The dev
server accepted connections but returned no document, reproducing all browser
navigation timeouts even with one worker. Restrict dependency scan to
`index.html` and exclude native build/evidence folders from the watcher. Keep
the original 30 s UI test timeout.

Qualification is evidence work. A working collector does not make the product
pass. Browser fixtures, synthetic App Server responses and hosted Windows CI
cannot satisfy the physical Windows / official-engine workload gates. Owner IME,
screen reader, visual acceptance and signed/packaged release receipts remain
separate. Budgets must not be weakened to make a report green.

## Boundaries

The normal desktop does not collect timings. A local qualification operator may
activate a bounded numeric-only probe through the debugger. It stores no text,
paths, prompts, tool output, credentials or environment values. Paint timings
use a Svelte flush followed by two animation frames: a conservative scheduling
proxy, not proof of compositor presentation or physical keyboard latency.
The report retains this distinction and cannot promote that proxy to a paint PASS.

Private process traces and machine-specific outputs belong in ignored
`artifacts/nc10/`. Source/artifact hashes, dirty status, tool versions, sampling
boundaries and any missing evidence must survive in the qualification receipt.

## Reproduce locally

```powershell
npm ci --no-audit --no-fund
npm run test:native-evidence
npm test
powershell -NoProfile -File scripts/nc-10-build.ps1
node scripts/nc-10-native.mjs <absolute-private-plan.json> <new-output-directory>
```

The build script requires a clean local commit and binds the executable digest
to that source SHA and both dependency locks in an adjacent build receipt.
Unstamped/stale/modified binaries remain diagnostic-only. The runner verifies
the selected official Codex binary against the approved upstream digest in
`protocol/managed-assets.json`.

Private plan example (replace paths with trusted native executables and three
dedicated fixture folders containing only a bounded `fixture.txt`; never pass a
shell command or credential). The fixture content is hashed, not the plan path.
Provide an independently reviewed host device JSON receipt with `reviewedBy`,
`manufacturer`, and `model`; retain hypervisor state and hardware evidence there.
Owner acceptance remains separate.

```json
{
  "exe": "D:\\projects\\ultima\\src-tauri\\target\\release\\ultima-desktop.exe",
  "codex": "C:\\trusted\\codex.exe",
  "projects": ["D:\\fixtures\\a", "D:\\fixtures\\b", "D:\\fixtures\\c"],
  "physicalWindows": true,
  "deviceReceiptFile": "D:\\private\\reviewed-device.json",
  "mode": "smoke",
  "allowModelTurns": false
}
```

`smoke` connects the actual desktop to the official engine and exercises input.
Setting `allowModelTurns: true` additionally sends a minimal no-tools model turn
through existing official authentication. `idle` runs three sequential CLI and
connected desktop pairs, each with 60 s settlement plus 60 s observation (about
12 minutes total). `stress` requires explicit model-turn opt-in and runs for an
hour with at most 64 no-tools turns, three distinct projects, verified project
selection changes and reconnects. It explicitly tests clean session pause/resume
and durable thread recovery after terminating only its owned engine child;
this is not an OS suspend/resume or whole-desktop crash receipt. Optional
`exerciseRecovery: true` does the same in a live smoke. An interrupted collector retains its exclusive local
lock and streamed samples for inspection; verify its PID and owned descendants
have stopped before explicitly removing that stale lock. Outputs are never
overwritten. A partial/failed report is a valid diagnostic, not task acceptance.

The driver uses Playwright CDP because the Browser plugin is not available for
attaching to the native WebView2. Each run uses an isolated WebView profile and
an ephemeral loopback debugger port; it does not change normal user profiles,
global Codex configuration, authentication, runtime selection or tool approvals.

## Independent review and open qualification gates

Read-only whole-diff review found no Critical issues. Important findings fixed
with regressions: false project-switch counts, absent source/artifact and device
provenance, first-run parent directory failure, and overwriting raw evidence.
The sampler test now also verifies a real descendant as well as exclusion of an
unrelated sibling. Controlled-crash tests refuse a mismatched executable and
verify that the root and unrelated process survive.

Native cold launch to a **responsive composer**, compositor presentation latency
and the actual desktop's 10k-history workload remain OPEN. The composer requires
an explicit connect action; the shell mark alone cannot satisfy that gate.
The browser prototype's 10k fixture is not a substitute for native Desktop
history acceptance. Do not mark these ledger/acceptance items complete from the
collector, a scheduled-frame proxy, or a passing browser screenshot test.

Other separate receipts: real approval/cancel correctness, whole-app crash
durability, WebView2 lifetime, Windows DPI/IME/screen reader, signed/packaged
candidate, exact-head cloud CI, and owner acceptance. This implementation cannot
self-approve those gates. Native qualification may therefore be PARTIAL or FAIL
even when all collector tests pass. No thresholds or acceptance were weakened.

## First measurement and bounded runner repair

First frozen-candidate run (`e8c2a8c`): all three idle pairs completed;
instrumented idle RAM gate FAIL (desktop aggregate p95 490.4 MiB, CLI 88.0 MiB,
extra 402.4 MiB against 150 MiB); sampled settled CPU PASS. Its endurance stopped
at the first project change because the runner clicked during asynchronous
disconnect. The original raw files remain intact under `idle-e8c2a8c` /
`stress-e8c2a8c`. Never promote that aborted run to a 60-minute result.

`project-smoke` is a diagnostic integration test: it connects to all three
fixture projects in a cycle, optionally makes a no-tools turn before each
switch, and records three verified changes. It never creates an endurance
receipt. This native smoke reproduced the race (RED); waiting for the real
`disconnected` acknowledgement verified 3/3 real changes (GREEN). Project
buttons remain visible during teardown and the application correctly refuses
a premature selection. Collector errors now classify the receipt FAIL rather
than PARTIAL. Freeze a new source/artifact receipt and repeat both idle and
full-duration endurance; no target or workload gate is weakened.

A second runner race was reproduced at cleanup: the OS child had exited but its
Node exit callback was queued behind synchronous `taskkill`. Cleanup now awaits
the original child handle's exit event, including an already-completed native
exit; a still-live/unconfirmed child fails after a bounded 5 s wait. Two real
Windows process regressions failed before this repair and passed after it.
Three short native CLI/desktop diagnostic pairs then completed cleanup; they
are explicitly not accepted 120 s baseline or 60-minute endurance evidence.
The dedicated suite is 16/16. The turn driver requires a new non-placeholder
assistant row and completion acknowledgement, not a stale prior `OK` row.

## Reclaim audit — 25 October 2026

Reclaimed after the prior native collector exited; historical evidence
remains under ignored `artifacts/nc10/`. Candidate `38c785e` completed
three nominal 120 s CLI/desktop pairs. Its original evaluator reported
383 MiB idle overhead (FAIL), but desktop repetition three had **no Codex
process** during its entire settled 60 s observation. It is not a valid
same-engine comparison. The evaluator now requires an attributable official
engine in every settled sample; a replacement matched baseline is necessary.

The official model stress completed 33 turns and 11 project switches before
an additional turn timed out around minute 36. The full-hour endurance gate
is NOT_RUN, collection FAIL. Another 12-switch diagnostic failed at the fifth
switch; its screenshot showed a textless assistant row and completed tool
item. Those are observations, not proof that the official engine lost text.

The reclaim patch reconciles bounded canonical `agentMessage` final events
for the exact active turn, ignores reasoning/assistant items in the tool
panel, and labels completed textless turns explicitly. The runner distinguishes
completed turns lacking text (`NATIVE_TURN_NO_TEXT`) from transport timeouts,
and retains sanitized boolean/numeric failure observations. The existing
uncommitted diagnostic changes were preserved.

Settled WebView2 processes account for approximately 350 MiB RSS, while the
Ultima Rust parent contributes approximately 26 MiB, subject to CDP and
shared-page counting limitations. The original 150 MiB budget is unchanged.
Native replacement baseline, full-hour run, native history and presentation,
owner verification, packaging and hosted CI remain unaccepted.

## Root-cause follow-up — upstream turn failure and WebView base RSS

An isolated, real official Codex App Server 0.164.0 JSONL diagnostic performed
`initialize → thread/start → turn/start` using the same no-tools fixture.
The engine emitted `turn/completed` with `status=failed` and error-message
category **usage limit**, with no completed assistant text item. The failure
occurred without Ultima UI. Treat the resulting native user-facing blank turn
as an upstream quota failure; bypassing an account limit is outside the app.
The production UI now classifies the authoritative final turn status using a
fixed allowlist and renders safe, actionable error text. Arbitrary upstream
error messages and account details are never echoed. The NC-10 model driver
identifies `NATIVE_UPSTREAM_USAGE_LIMIT` separately from a genuinely
successful but textless completion and must abort impossible model stress
until Codex upstream access is available.

Chromium `userMessage` notifications were also incorrectly retained as
generic tool items, explaining an unrelated misleading `Tool · complete`
row. The tool list now excludes assistant, reasoning and user message items.

On the same native binary `39b2d7c7…1df280`, the role-aware Windows sampler
measured both RSS and private committed memory; the RSS acceptance gate was
**not** changed. A disposable connected-CDP project-switch smoke showed
WebView2 browser, GPU, renderer, utilities and crash-handler processes rather
than six independent WebViews. The WebView2 GPU process alone showed roughly
115 MiB RSS / 101 MiB private commit in that sample. Readings:

- Disconnected Ultima shell, **without CDP**: WebView2 RSS average 329.0 MiB,
  private commit 150.8 MiB across 12 settled samples.
- Disposable **about:blank page with CDP**, no engine: WebView2 RSS average
  329.7 MiB, private commit 150.1 MiB across 11 settled samples.
- Previous matched CLI/desktop exact-source idle: **444.3 MiB incremental
  RSS p95**, 150 MiB budget FAIL; CPU idle PASS at sampler resolution.

The nearly identical blank-page WebView2 RSS suggests most active-view
working-set overhead belongs to Chromium/WebView2 itself, rather than the
Ultima Svelte application. These ~30 s diagnostics are **not** replacements
for matched 3×120 s acceptance, and private commit is **not** private working
set nor a substitute for the agreed RSS metric. Microsoft recommends keeping
hardware acceleration enabled; production browser flags that compromise
security/isolation or GPU performance have not been introduced. An alternate
native rendering shell needs an architecture/performance qualification before
it can replace the WebView UI while keeping Codex functionality, accessibility,
approval boundaries and upstream compatibility. The 150 MiB limit stays open.

## Independent native Rust renderer feasibility

See [NC-10 native renderer spike](NC-10-NATIVE-RENDERER-SPIKE.md). The isolated
egui/AccessKit UI with the existing Rust official-Codex supervisor/broker
measured **92.72 MiB incremental RSS p95** above the official initialized
App Server CLI baseline, in two groups of three 120-second matched-workload
repetitions (60 s settled observation each). Both exact-source reports had
official engine presence throughout; sampled CPU was 0% in both. Native Rust
therefore has materially better **idle feasibility** than WebView2 with
roughly 57 MiB headroom against the original 150 MiB ceiling.

This does **not** close the NC-10 product RAM gate. The prototype is read-only,
with no active model turns, permission parity, real Codex history, full
accessibility acceptance or packaged release. Existing Tauri/WebView2 retains
its measured FAIL; the experimental renderer has no product acceptance yet.
The rest of NC-10 remains blocked by the unfinished parity and workload gates.

## Follow-up — October 26, 2026

Reclaimed the same `feat/nc-10-native-qualification` branch, with the previous
agent's `bdaf77a` already in its ancestry. The native egui candidate now
supports a bounded, opt-in official Codex read-only-sandbox turn/stream/stop
path with exact IDs and fail-closed approval declines. This is an experimental
extension, with standalone streaming and cancellation unit coverage only.
The native UI also retains the latest HTML-inspired visual refinements.

Local checks: 16/16 NC-10 evidence, 50/50 frontend unit, 14/14 Playwright E2E
(stable Windows browser concurrency and independent screenshot scenarios),
7/7 renderer Rust, 7/7 SessionGate Rust, frontend build and Rust formatter /
strict Clippy PASS. Svelte reports zero errors and four existing accessibility
warnings. The original 30-second Playwright timeout is retained.

**Still not complete**: measured active native renderer memory/paint/input
latency on an exact release, a full authenticated 60-minute stress run,
production host actor/approval/durable-history parity, physical Windows owner
IME and screen reader acceptance, signed release, and exact-head hosted CI.
The prior WebView2 product RAM FAIL is not closed by unit tests or the old
read-only egui idle feasibility PASS.

## Reclaim follow-up — October 26, 2026 (pending turn races)

The native egui renderer now allows Stop while an accepted prompt still awaits
its official turn ID. A cancellation during initial `thread/start` prevents any
model `turn/start`, returns an explicit cancelled-before-start UI event and
allows a later prompt on the verified read-only thread. Cancellation after
`turn/start` still waits for the real ID and official terminal acknowledgement.

The same official turn can complete before its `turn/start` response arrives.
The protocol broker confirms its identity before emitting a terminal; the UI
now accepts that verified outcome while awaiting start, presents the terminal
status and unlocks the composer. Diagnostic errors follow the authoritative
terminal event without erasing its status. Unsolicited idle completions are
ignored. Four targeted Rust regressions cover these cases, including a
broker-to-display-queue-to-UI envelope test, in addition to the existing
broker identity and approval tests.

Local Rust renderer suite: **23/23 PASS**, formatter **PASS**, strict Clippy
`-D warnings` **PASS**. This is synthetic regression evidence, not a live
native streaming or interruption acceptance receipt. The full NC-10 product
gates, existing Tauri RAM failure and owner sign-off stay open.
