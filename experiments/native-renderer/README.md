# NC-10 Native Renderer Feasibility Spike

This is a stand-alone Rust/egui Windows window for measuring active idle memory **without WebView2**. It deliberately does not connect to Codex, send requests, store real history, or approve tools. The project rail, chat viewport, keyboard editor and disabled approval mock are real native widgets; fixtures are local.

The renderer uses `eframe = 0.33.3` with only glow, default fonts and AccessKit. It retains the repo's Apache-2.0 license compatibility. No production runtime code or permission boundary is replaced.

When provided *both* `--codex-exe=<absolute trusted executable>` and `--project=<absolute directory>`, the spike starts a read-only live handshake with the official Codex App Server, reusing Ultima's Rust `EngineSupervisor` and `Broker`. It exposes no model-turn, tool execution, or approval acceptance controls; any unexpected server approval is denied. The engine is owned by a Windows Job and shuts down on the UI's exit. The connection state is displayed in the top bar. This is a transport **probe**, not NC-07 implementation parity or a production runtime selector.

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
```

The connected mode reads only the user's existing ignored `artifacts/nc10/project-smoke-plan.json` and verifies that the official Codex child remains present in every settled sample. It does not ask Codex for an answer and cannot bypass upstream usage limits. Results are machine-local under ignored `experiments/native-renderer/results/` and carry the binary SHA256. Any uncommitted source is flagged `sourceDirty`; results must never be presented as a clean exact-commit qualification if that flag is true.

Memory readings from this program are **renderer-only diagnostics**, not the NC-10 matched official-Codex idle gate. Before choosing it for Ultima, reproduce the complete process-tree benchmark with the real official Codex App Server, continuous event stream, exact turn/user/approval identities, 10k-history event-to-frame timing, Windows IME and screen-reader behavior, and cleanup/rollback.
