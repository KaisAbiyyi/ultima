# NC-10: Native Rust Renderer Feasibility, Windows

**Status: experimental candidate, not a product acceptance or migration decision.**

## Problem

The existing Ultima Tauri/WebView2 Svelte product scored **444.3 MiB incremental
RSS p95** over its matched official Codex CLI baseline (150 MiB upper budget:
FAIL), even after validating the engine was present in all three settled runs.
Independent diagnostics measured approximately **329–330 MiB WebView2 RSS**
for an empty native window, with and without CDP. The browser/renderer/GPU/
utility process group was present without Codex. Optimizing Svelte alone has
limited room to remove hundreds of MiB of Chromium baseline.

## Chosen feasibility experiment

Use an isolated **egui/eframe 0.33.3** binary with OpenGL glow, native AccessKit
adapter, and the repo's Apache-2.0-compatible MIT/Apache dependencies. Rust
1.91.1 can compile this pinned version; newer eframe releases require newer
Rust. The prototype shares *production Rust* `app-server-supervisor` and
`protocol-bridge` dependencies for an optional **read-only official Codex**
handshake. The upstream executable and workspace must be supplied explicitly
as absolute trusted paths. Windows Job ownership and teardown come from the
existing supervisor. No model turns, commands, changes or approvals can be
initiated from the spike UI; unexpected approval requests are declined.

The prototype implements a fixed project rail, mock threads, a keyboard
composer, disabled send/allow controls and **10k virtualized local history
rows**. Its synthetic messages are explicitly fixture data and never present
as real Codex history. Production remains Tauri/Svelte.

## Renderer-only pilot, 3 x 45 s each, settled last 30 s

Same HP Victus Windows host and original NC-10 PowerShell process-tree sampler;
metrics are per-process summed resident working sets (RSS), which may
double-count shared pages.

| Mode | Cross-repetition p95 RSS | Cross-repetition p95 private commit | Sampled CPU |
|---|---:|---:|---|
| Empty egui UI without engine | 101.9 MiB | 53.6 MiB | 0% in runs 2/3; one transient run-1 burst |
| 10,000 virtual rows, without engine | 93.6 MiB | 56.5 MiB | 0% in all three runs |
| Native UI plus owned real Codex App Server (read-only handshake) | 215.0 MiB | 139.3 MiB | 0% in all three runs |

The connected pilot verified `APP SERVER READY` in the native Windows
window and **Codex remained present in all 25 settled samples per repetition**.
These pilots used a 15 s warm-up and 30 s observation, so **none may be
promoted to the official 3 x 120 s idle gate**. The first two renderer-only
pilot runs were collected before freezing their source; binary hashes and
ignored raw files are retained. The connected three-run pilot is backed by
`904bf3c` and binary hashes under `experiments/native-renderer/results/`.

For context, the prior Svelte/WebView2 idle connected product used
approximately 515.5 MiB total desktop RSS p95 and 71.1 MiB CLI RSS p95.
Subtracting a *historical* 71.1 MiB from the native connected pilot
215.0 MiB gives ~144 MiB, **but this is not a matched current baseline** and
cannot be called PASS. The official test must control warm-up, history, engine
status, power plan, repeated process samples and source/binary identity.

## Full-window clean-source idle comparison — 25 October 2026

The headless official App Server and same official App Server launched as an
egui child were tested on the same Windows host and fixture. Each mode ran
**three independent 120-second repetitions** (60 s warm-up, 60 s settled
samples). Both performed `initialize/initialized`; the native egui host
reported `APP SERVER READY` through its actual Windows title. Codex was
present in **every settled sample** for both modes.

| Full idle process-tree result | Codex CLI | Native egui + Codex |
|---|---:|---:|
| Repetition 1 RSS p95 | 70.97 MiB | 163.17 MiB |
| Repetition 2 RSS p95 | 68.31 MiB | 163.55 MiB |
| Repetition 3 RSS p95 | 69.11 MiB | 161.66 MiB |
| p95 of repetition p95s | **70.79 MiB** | **163.51 MiB** |
| Private commit p95 of repetition p95s | 31.71 MiB | 83.83 MiB |
| Mean sampled CPU | 0% | 0% |

**Incremental RSS p95 = 163.51 - 70.79 = 92.72 MiB**, within the 150 MiB
budget by approximately **57.28 MiB** for this *read-only prototype
workload*. This is a successful **candidate idle feasibility measurement**.
It is explicitly **not NC-10 final RAM acceptance**: the native client does
not yet support active Codex turns, streaming, tool approval, real durable
history or packaged release qualification. Tests were sequential by mode,
not interleaved per pair, so host-load differences may still matter.

Evidence provenance:

- Git source `f3388fb70947580ff2e7c0db8f63d1f53635dd82`, clean experiment source in both reports.
- Native renderer binary SHA256:
  `7e753380ca9d4817476a253941dc7c6b6da6ff8ab1fc3942890aa0bc9f93d697`.
- Official Codex executable SHA256:
  `6f267830b029ee25d9339864aa867cd45de9f8fd4c5ea007d2e63c13a0c66d36`.
- Ignored local reports:
  `experiments/native-renderer/results/cli-f3388fb7-1792910053781/report.json`,
  `experiments/native-renderer/results/connected-f3388fb7-1792910430344/report.json`.
  Full numeric process-tree samples are in each report's `run-*` subdirectories.

## Blocking parity work

1. **Shared SessionGate extracted** into
   `crates/native-session` with its seven original generation fencing,
   shutdown confirmation and poisoning regression tests preserved; both
   Tauri and egui reference this crate. Remaining: extract the Tauri-specific
   full actor (disconnect/reconnect event multiplexing, exact approval
   identity, backpressure and managed-runtime rollback) into a shared Rust
   host implementation. The experiment presently supports only one
   read-only system Codex lifecycle.
2. Port validated atomic command/file-diff previews, exact thread/turn/tool
   evidence, revocable approve-once, fail-closed decline, and cancellation
   semantics from `src/lib/tool-events.ts` and `src/Desktop.svelte` before
   an actionable egui Allow control exists.
3. Implement bounded real Codex event/turn rendering and durable thread
   resume via the shared Rust broker; currently 10k history is synthetic.
4. Native keyboard/IME at 100–200% display scale, screen reader and AccessKit
   labels, focus management, event-to-paint latency, heavy streaming, memory
   leak, high-frequency project changes, package/signing and owner review
   remain NOT_RUN. No change to the accepted performance budget is authorized.
5. Codex model-turn endurance is blocked by an upstream usage-limit error
   reproduced directly against the official Codex App Server. The shell
   cannot bypass that limit.

## Shared lifecycle extraction verification

Commit `14c0c40cb37e59f236c861eb902fdac474708b67` moved the
unchanged `SessionGate` implementation from the Tauri binary into the
host-neutral `crates/native-session` crate. Tauri imports it directly;
the egui probe uses the same generation-fenced start/confirmed-stop policy.

The original seven SessionGate Rust tests passed at their new location;
four Tauri runtime/approval tests passed; 50/50 frontend unit tests and
16/16 native evidence/Windows tests passed. The production Tauri Windows
release built from the clean new commit has binary SHA256
`e4e25659738e33e416ebf9be0052b0b1bd9c9d1ffd81f8bbe75475922b64e7ff`;
its no-model smoke verified 3/3 project switches (PARTIAL receipt by design).

The egui read-only 3×45s post-extraction diagnostic had
**214.97 MiB total RSS p95** and sampled CPU 0% with official Codex
present in all settled observations. This is consistent with the prior
215.03 MiB short pilot. It does not replace the successful 3×120s
feasibility measurement.

The shared crate remains *process-local*: cross-app single-instance
coordination, full actor extraction, approval evidence, model turns,
managed rollback and production renderer migration remain open.

## Minimalist Codex wrapper UI reference — local design follow-up

The owner's `minimalist_codex_wrapper_ui.html` was reimplemented as **native
egui widgets**, not embedded HTML or WebView2. Source `afc5c7a494559aea01a24848e25a501011cdc931`
contains an inset obsidian sidebar with expandable project/chat fixture tree,
sidebar collapse, keyboard search, breadcrumb header, reference conversation/
patch-preview, anchored composer, read-only permissions and informational
model/effort selector. Icons are native vector primitives; no remote fonts,
Tailwind runtime, CDN requests, scripted animations or retained browser
renderer exist. Demo buttons never claim to execute a patch, authorize a
tool or change Codex permission. The only live Codex integration is the
same optional read-only official App Server handshake.

Visual QA used a real Windows native 1216×859 screenshot kept locally at
`experiments/native-renderer/results/minimal-ui-afc5c7a.png` (ignored,
owner machine only). This shows the supplied design's hierarchy and does
not imply feature parity.

**Native idle benchmark — updated UI, clean-source source/binary verified**

- Windows source: `afc5c7a494559aea01a24848e25a501011cdc931`
- Native executable SHA256:
  `609c934b831b8ad6f1e24e3c2e1507b8d5dac4f22120c308bff9a29163a8606f`
- 10,000 locally generated rows, three 45-second renderer-only runs:
  **93.03 MiB RSS p95** of run p95s, sampled idle CPU **0%**.
  Previous simpler egui layout was 93.57 MiB RSS p95 in the same pilot
  scenario: ~0.54 MiB difference, too small to characterize as a large
  optimization but not a regression.
- Three full 120-second CLI repetitions, each 60 s warm-up and 60 s
  observation, official Codex present in every settled sample:
  **113.93 MiB RSS p95**, sampled idle CPU **0%**. The first CLI run had
  a transient engine RSS spike; its median was ~67.43 MiB.
- Three full 120-second native UI + official Codex repetitions under the
  same fixture/version/machine, all child Codex samples present:
  **207.78 MiB RSS p95**, sampled idle CPU **0%**.
- **Read-only candidate incremental RSS = 207.78 - 113.93 =
  93.85 MiB**, with **56.15 MiB** remaining below the unchanged
  150 MiB limit. Its `check-idle-budget.mjs` fail-closed budget guard
  reports **PASS** for the *read-only candidate only*, with four node
  regression tests PASS.

The second connected repetition had a temporary total process-tree RSS
increase to ~214 MiB, while the egui host remained ~92.07 MiB; the official
Codex child increased from ~57.81 to ~112.24 MiB. This demonstrates engine
fluctuation rather than a visual-renderer leak within those samples.
Measurements are sequential across modes, not simultaneous/interleaved.
Full real-turn streaming, patch preview, approvals, active performance and
accessibility remain NOT_RUN. The earlier Tauri/WebView2 product RAM FAIL
is not retroactively closed.

Local ignored reports: `experiments/native-renderer/results/history-afc5c7a4-1792913001376/report.json`,
`experiments/native-renderer/results/connected-afc5c7a4-1792913171999/report.json`,
`experiments/native-renderer/results/cli-afc5c7a4-1792913567347/report.json`.

## Reclaim: bounded native Codex turns (October 26, 2026)

The isolated egui implementation now offers a **read-only-sandbox official Codex
turn path**, built on the production protocol broker/supervisor and shared
`SessionGate`. The `thread/start` request explicitly uses `sandbox: read-only`
and `approvalPolicy: never`; unexpected tool approvals are declined by the
broker. A single bounded turn can start, stream assistant deltas, reconcile a
canonical final message and request `turn/interrupt` using its actual
thread/turn identifiers. The renderer attaches exact IDs to every assistant
event, bounds prompt and display buffers, and renders true error states.

The Windows Playwright regression harness was made less resource-intensive
(one browser worker and three independent DPI/theme screenshot tests). It now
passes **14/14** tests, preserving the 30-second individual timeout and all
100%/200% DPI, desktop/mobile and dark/light screenshot scenarios. Frontend
unit and native evidence tests pass **50/50** and **16/16** respectively.
Native renderer and shared lifecycle Rust tests pass **7/7** each, with
`cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` passing.
The simulated protocol tests confirm read-only policy, exact event identity,
fail-closed approvals, bounded queues and interrupt ordering; they are
**synthetic tests**, not verified real Codex turn acceptance.

The older **92.72 MiB** candidate idle feasibility PASS and **444.3 MiB**
Tauri product RAM FAIL are unchanged historical evidence. The new active-turn
native build has **no matched active workload RAM/latency or 60-minute
endurance PASS**. Full host actor/managed-runtime parity, owner physical
keyboard/IME/accessibility, real approval review, durable history and native
signed packaging remain OPEN. Upstream authenticated turns previously failed
with an official usage-limit response. No renderer migration or NC-10 merge
is authorized solely by these regression tests.

## Decision gate

### Native UI reference alignment (October 25, 2026)

The supplied `minimalist_codex_wrapper_ui.html` was re-audited for closer
visual alignment. The Rust-only renderer now centers the conversation at
768px, renders the actual reference TypeScript patch with 18.5px lines,
uses the source's 768px wrapper / 16px composer padding / 262px sidebar,
draws chevrons natively, and provides a two-layer read-only model/effort
popover and ghost navigation. Segoe UI and Cascadia Mono are loaded from the
**host's already installed Windows fonts** when present, without bundling
those font assets. The mock remains offline and read-only. The Windows
reference QA screenshot is retained under ignored
`experiments/native-renderer/results/minimal-ui-fidelity-final.png`.
No claim of browser-exact pixel fidelity, functional model choice or NC-10
product acceptance is made.

The candidate meets the first **read-only matched idle feasibility**
threshold. Continue the shared Rust session and permission implementation,
then retest CPU, latency and full-memory headroom under active real-product
workloads. If it fails, investigate another native renderer. This document
does not authorize a production renderer switch or NC-10 merge.

References: [performance contract](PERFORMANCE-CONTRACT.md),
[existing NC-10 evidence](NC-10-NATIVE-QUALIFICATION.md),
[native renderer source](../experiments/native-renderer/README.md),
[egui Accessibility](https://github.com/emilk/egui/blob/main/docs/accessibility.md).
