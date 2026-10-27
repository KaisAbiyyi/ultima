# NC-10 Native Renderer Feasibility Spike

This is a stand-alone Rust/egui Windows window for measuring active idle memory **without WebView2**. Its default mode has no Codex connection; an explicitly opted-in probe can initialize the official App Server and now submit real model turns in a **read-only sandbox**. It has no durable real thread history, recovery, permission-acceptance UI or production integration. Codex may perform read-only sandboxed tool activity; every explicit tool approval is declined. The project rail, chat viewport and keyboard editor are native widgets; offline fixtures remain local.

## Native minimalist UI reference

The UI reproduces the visual hierarchy of the supplied `minimalist_codex_wrapper_ui.html`: inset rounded obsidian sidebar, expandable project/chat groups, narrow session breadcrumb, minimal execution stream and pinned prompt composer with context, permissions and model/effort positions. Launch the executable without fixture flags for a **reference-content preview**. Use `--fixture-empty` to render the identical UI shell without illustrative conversation items, or `--fixture-history=10000` to stress virtualized history.

Performance and security choices:

- Native egui with local OS/default fonts; **no HTML, WebView2, Tailwind CDN, Google Fonts, icon font, external assets or remote CSS**. Tiny icons use native vector lines and circles.
- No perpetual animation, timer, polling loop or background render. Only UI interaction and bounded engine status change request repaint.
- **10,000 maximum** synthetic history rows, including long synthetic code and diffs. A bounded variable-height index lays out the viewport plus neighboring rows for fixture and live conversation; live history still retains at most 512 rows. **8,192-character** maximum draft.
- Fake models, token rates, preset API keys and artificial “patch applied” behavior from the reference HTML are **not** imported. Model/access popovers are informational; Apply, mounting a new project and Dictation remain disabled. Send is enabled only after the explicit official Codex connection reaches Ready, and Stop uses the actual turn ID.
- The preview sample conversation is clearly marked illustrative. No real Codex history is accessed by the visual layer.

The uploaded file is a visual reference, **not** a production implementation or upstream feature contract. UI matching must not weaken approval policy, memory budget or application boundaries.

### Closer source-fidelity pass (October 25, 2026)

The native mock now uses the actual reference TypeScript JSONL example instead
of the previous Rust placeholder, with relaxed 18.5px code line height, user
message left rule, centered max-768px conversation, the 768px composer parent
and its own 16px horizontal inset (736px input box), narrower ghost rows,
native vector disclosure arrows, profile avatar and two-level model/effort
menu. We follow the original obsidian palette and 262px inset sidebar.
Windows installs supply Segoe UI and Cascadia Mono *at runtime* when present;
neither font is embedded, uploaded or redistributed; egui bundled defaults
remain available elsewhere.

Two differences are intentionally retained: the OS's native titlebar and the
read-only safety state in place of the reference's mock Full Access permission.
This is a **native source-inspired recreation**, not a pixel-perfect browser
render. Browser CSS line metrics and interactive voice/model/patch services
cannot be exactly reproduced without changing engines or adding backend
features. No model execution, patch apply or permissions were enabled.

The renderer uses `eframe = 0.33.3` with only glow, default fonts and AccessKit. It retains the repo's Apache-2.0 license compatibility. No production runtime code or permission boundary is replaced.

When provided *both* `--codex-exe=<absolute trusted executable>` and `--project=<absolute directory>`, the spike connects to official Codex App Server using Ultima's Rust `EngineSupervisor`, `Broker` and shared generation-fenced `SessionGate`. A user-entered prompt starts a single read-only-sandbox thread/turn; bounded assistant deltas and canonical final text are displayed with checked thread/turn IDs. Stop requests an official `turn/interrupt` for that exact ID. Tool approval requests are always declined. Engine ownership uses a Windows Job and shuts down on window exit. The connection state appears in the title bar. **Do not treat this experimental path as approved NC-07 permission parity, durable resume or the production runtime.**

Build a clean, commit-bound Windows executable from repository root before qualification:

```powershell
node experiments/native-renderer/build-exact.mjs
```

The release exe lives in `experiments/native-renderer/target/release/ultima-native-renderer-spike.exe`. To test the virtualized 10k row UI:

```powershell
experiments/native-renderer/target/release/ultima-native-renderer-spike.exe --fixture-history=10000
```

Use the same NC-10 sampler for a 3-repeat renderer pilot (45 seconds per repetition; settle for 15 seconds and observe 30 seconds):

```powershell
node experiments/native-renderer/benchmark.mjs empty
node experiments/native-renderer/benchmark.mjs history
node experiments/native-renderer/benchmark.mjs connected
node experiments/native-renderer/benchmark.mjs cli --full-idle
node experiments/native-renderer/benchmark.mjs connected --full-idle
```

The build helper refuses dirty sources, compiles with `--locked`, and records an
ignored receipt binding HEAD, the executable SHA256 and Cargo.lock SHA256. The
sampler requires that receipt before any run. Rebuild after every source commit.
The strict diagnostic guard below rejects source/binary mismatches, missing
Codex samples, partial runs and idle RSS overhead above **150 MiB**. It does
not claim active-turn or production NC-10 acceptance:

```powershell
node experiments/native-renderer/check-idle-budget.mjs <cli-report.json> <connected-report.json>
node --test experiments/native-renderer/tests/check-idle-budget.test.mjs
```

The `connected` benchmark mode reads only the user's existing ignored `artifacts/nc10/project-smoke-plan.json` and verifies that the official Codex child remains present in every settled sample. **The automated idle benchmark does not send a model turn**, even though the interactive window can. It cannot bypass upstream usage limits. Results are machine-local under ignored `experiments/native-renderer/results/` and carry the binary SHA256. Any uncommitted source is flagged `sourceDirty`; results must never be presented as a clean exact-commit qualification if that flag is true.

The `cli` mode launches the same exact official Codex executable and performs only the official initialize/initialized handshake through its stdio JSONL protocol. The `--full-idle` option uses the actual NC-10 idle observation window (60 seconds settlement, 60 seconds measurement). Compare three native and CLI repetitions with the same version, machine, power settings and fixture. This still establishes only **system-engine connected idle parity**, not streaming/approval/recovery parity.

Memory readings from disconnected mode are renderer-only diagnostics. Connected mode includes official initialized Codex, but cannot establish NC-10 full product acceptance. The 25 October idle figures predate the live turn implementation. Before choosing this renderer for Ultima, test the complete process tree during real model turns and tool activity, actual cancellation and streamed text, permission review/approval UI, durable history/resume, native 10k-history event-to-frame timings, Windows IME and screen reader, 60-minute endurance and cleanup/rollback.

## NC-10B native latency diagnostic

The native renderer timestamps each official assistant delta/final event at
admission to the bounded UI queue, preserving the oldest timestamp when deltas
coalesce. Raw keyboard timestamps count only when the composer actually
changes. A native OpenGL paint callback records the corresponding elapsed
times. A separate `rawKeyDuringActiveTurnToPaintCallback` series only counts
composer edits while an official turn is running, preventing idle-keyboard
samples from satisfying a streaming workload. Startup has one sample
measured from entry into `main`. Samples are
content-free: prompt text, project paths and message IDs are never written to
the metric report. Each metric series is capped at 4,096 samples; any overflow
invalidates a qualification run. No periodic repaint loop is added.

**Measurement limit:** the callback runs **before buffer swap and compositor
presentation**. Its event-to-paint p95 is a diagnostic proxy; it cannot prove
physical keyboard-to-visible-pixel performance, or full OS cold start. Native
Windows capture/UIA still must establish the independently visible p95 gates.
Even a healthy diagnostic reports `NOT_RUN` for presented pixels.

Use three separate, normal streaming sessions with an **exact committed**
Windows release and synthetic project. Each session must use a unique ignored
receipt filename and exit normally so the report is written:

```powershell
node experiments/native-renderer/build-exact.mjs
experiments/native-renderer/target/release/ultima-native-renderer-spike.exe --fixture-empty --codex-exe=<absolute-codex.exe> --project=<absolute-synthetic-project> --latency-receipt=<ignored-receipt-1.json>
# Run a genuine read-only Codex streaming turn; type 30+ keys in the composer.
# Repeat in fresh processes with ignored-receipt-2.json and ignored-receipt-3.json.
node experiments/native-renderer/verify-latency.mjs <ignored-receipt-1.json> <ignored-receipt-2.json> <ignored-receipt-3.json>
node --test experiments/native-renderer/tests/verify-latency.test.mjs
```

The verifier requires a source-clean SHA-verified executable/lockfile receipt,
three valid native measurement files, no overflow and valid percentile data.
The exact-build script also embeds the Git HEAD SHA into the release executable;
the verifier rejects receipts from any other SHA, including ordinary `cargo`
builds stamped `UNBOUND`, so earlier sessions cannot be reused after a source
change.
Every process generates an opaque 128-bit session token; the verifier rejects
duplicate tokens, so one receipt cannot count as three independent sessions.
The token contains no user or prompt data and is not an integrity signature.
It reports per-run sample counts, p50/p95, executable SHA256, Cargo.lock SHA256,
source SHA and the available official Codex CLI version. Each run has exactly
one `main`-to-paint startup sample. Normal streaming loads and real displayed
pixels must be independently qualified; running this script alone cannot
mark NC-10B accepted.
