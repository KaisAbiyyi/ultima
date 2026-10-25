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
