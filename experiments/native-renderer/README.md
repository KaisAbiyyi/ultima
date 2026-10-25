# NC-10 Native Renderer Feasibility Spike

This is a stand-alone Rust/egui Windows window for measuring active idle memory **without WebView2**. Its default mode has no Codex connection; an explicitly opted-in probe can initialize the official App Server. It cannot send model turns, store real history, execute tools or approve operations. The project rail, chat viewport, keyboard editor and disabled approval mock are real native widgets; fixtures are local.

## Native minimalist UI reference

The UI reproduces the visual hierarchy of the supplied `minimalist_codex_wrapper_ui.html`: inset rounded obsidian sidebar, expandable project/chat groups, narrow session breadcrumb, minimal execution stream and pinned prompt composer with context, permissions and model/effort positions. Launch the executable without fixture flags for a **reference-content preview**. Use `--fixture-empty` to render the identical UI shell without illustrative conversation items, or `--fixture-history=10000` to stress virtualized history.

Performance and security choices:

- Native egui with local OS/default fonts; **no HTML, WebView2, Tailwind CDN, Google Fonts, icon font, external assets or remote CSS**. Tiny icons use native vector lines and circles.
- No perpetual animation, timer, polling loop or background render. Only UI interaction and bounded engine status change request repaint.
- **10,000 maximum** synthetic history rows, only visible rows painted; **8,192-character** maximum draft.
- Fake models, token rates, preset API keys and artificial “patch applied” behavior from the reference HTML are **not** imported. Model/access popovers are informational, Send, Apply, mounting a new project and Dictation are explicitly disabled until authenticated functionality exists.
- The preview sample conversation is clearly marked illustrative. No real Codex history is accessed by the visual layer.

The uploaded file is a visual reference, **not** a production implementation or upstream feature contract. UI matching must not weaken approval policy, memory budget or application boundaries.

The renderer uses `eframe = 0.33.3` with only glow, default fonts and AccessKit. It retains the repo's Apache-2.0 license compatibility. No production runtime code or permission boundary is replaced.

When provided *both* `--codex-exe=<absolute trusted executable>` and `--project=<absolute directory>`, the spike starts a read-only live handshake with the official Codex App Server, reusing Ultima's Rust `EngineSupervisor`, `Broker` and shared generation-fenced `SessionGate`. It exposes no model-turn, tool execution, or approval acceptance controls; any unexpected server approval is denied. The engine is owned by a Windows Job and shuts down on the UI's exit. The connection state is displayed in the top bar. This is a transport **probe**, not NC-07 implementation parity or a production runtime selector.

Build from repository root:

```powershell
cargo build --release --manifest-path experiments/native-renderer/Cargo.toml
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

The strict diagnostic guard below rejects source/binary mismatches, missing
Codex samples, partial runs and idle RSS overhead above **150 MiB**. It does
not claim active-turn or production NC-10 acceptance:

```powershell
node experiments/native-renderer/check-idle-budget.mjs <cli-report.json> <connected-report.json>
node --test experiments/native-renderer/tests/check-idle-budget.test.mjs
```

The connected mode reads only the user's existing ignored `artifacts/nc10/project-smoke-plan.json` and verifies that the official Codex child remains present in every settled sample. It does not ask Codex for an answer and cannot bypass upstream usage limits. Results are machine-local under ignored `experiments/native-renderer/results/` and carry the binary SHA256. Any uncommitted source is flagged `sourceDirty`; results must never be presented as a clean exact-commit qualification if that flag is true.

The `cli` mode launches the same exact official Codex executable and performs only the official initialize/initialized handshake through its stdio JSONL protocol. The `--full-idle` option uses the actual NC-10 idle observation window (60 seconds settlement, 60 seconds measurement). Compare three native and CLI repetitions with the same version, machine, power settings and fixture. This still establishes only **system-engine connected idle parity**, not streaming/approval/recovery parity.

Memory readings from the disconnected mode are renderer-only diagnostics. Connected mode includes the official initialized Codex process, but remains a read-only spike and cannot establish NC-10 full product acceptance. Before choosing it for Ultima, test the complete process tree during real model turns, with continuous event streams, exact turn/user/approval identities, native 10k-history event-to-frame timings, Windows IME and screen-reader behavior, and cleanup/rollback.
