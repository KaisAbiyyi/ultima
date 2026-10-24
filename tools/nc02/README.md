# NC-02: baseline process-tree benchmark

This small Rust binary measures process-tree resident memory (RSS), CPU, process count,
launch latency and stdout line-observation timestamps. It can launch a headless
Codex CLI/App Server and a future Ultima desktop binary as separate variants using
the same scenario. The sampler never reads or saves the full stdout/stderr payload,
Codex credentials or child environment variables. No telemetry/network calls.

**State:** reproducible harness, with independent native product qualification
**NOT_RUN** until Ultima has an executable and the owner runs a matched workload
on real Windows hardware. Synthetic CI smoke validates the harness only.

## Build and test

Install Rust stable. At repository root:

```powershell
cargo test --locked --manifest-path tools/nc02/Cargo.toml
cargo run --locked --release --manifest-path tools/nc02/Cargo.toml -- --plan tools/nc02/synthetic-smoke.json --out artifacts/nc02/synthetic.json
```

Run once per distinct scenario; each plan enforces at least three repetitions.
Reports are local under ignored `artifacts/nc02`. The CI workflow runs synthetic
smoke on Ubuntu and Windows; its figures cannot be cited as Ultima's RAM/latency.

## Native Windows S0 baseline

Copy `windows-native-template.json` to a private untracked plan outside the repo
and edit `PATH_TO_ULTIMA_EXE` to a trusted, absolute executable path **after NC-06
provides the desktop application**. Choose the matching Codex CLI/App Server
version, auth state, repository, power plan and idle workload for both variants.
For `codex app-server`, an open but unwritten stdin keeps the server available;
for interactive workflows, use dedicated scripted drivers with explicit request
and approval handling before claiming measurements for S1/S2.

```powershell
cargo run --locked --release --manifest-path tools/nc02/Cargo.toml -- --plan <absolute-private-plan.json> --out artifacts/nc02/windows-s0.json
```

The JSON plan fields are:

| Field | Meaning |
|---|---|
| `schema_version` | Must be `1`. |
| `scenario` | Scenario ID, e.g. `S0_NATIVE_WINDOWS_IDLE_60S`. |
| `duration_ms` | Measured duration, 200 ms through 60 min; S0 requires 60 s. |
| `sample_interval_ms` | Period 100–5000 ms; minimum two samples. |
| `repetitions` | 3–20 repetitions; each variant tested sequentially. |
| Total sample limit | Up to 240,000 samples across all variants and repetitions; adjust interval or repetitions for long S4 runs. |
| `variants[].id` | Unique CLI/client label. |
| `variants[].argv` | Command as argument array, launched without a shell. `<SELF>` runs an internal synthetic fixture only. |
| `variants[].cwd` | Working directory, resolved from plan file directory. |
| `variants[].allow_early_exit` | Optional: only for explicit short-lived workloads; default false. |

### Reading results

- `tree_rss_bytes` is the sum of RSS for the launched PID and descendants at each
  sampling tick; shared pages may be double-counted. Compare identical accounting
  on the same device. `process_count` helps explain spikes.
- `tree_cpu_percent_one_core` is a one-core-normalized process-tree sum:
  100% = one fully occupied logical core. The first sample is a warmup.
  The summary reports median and interpolated p95 across valid repetitions.
- `spawn_latency_ms` measures `Command.spawn`, **not** a responsive UI. Stdout
  timestamps are pipe observation proxies, **not** rendered-frame latency.
  A GUI with no stdout will correctly report null. UI paint/input latencies
  require NC-06/NC-09 native instrumentation.
- `stdout_line_timestamps_ms` retains at most 10,000 observed newline timings
  per run; `stdout_complete_lines` tracks the total. Very long lines are never
  buffered in memory; the count of over-1MiB lines is exposed. Child stderr is
  drained but only byte-counted.
- Early exits fail unless explicitly configured and still sampled. A failed
  comparison reports `INCOMPLETE`; the runner returns a nonzero exit code.
  Every result includes `native_windows_gate: NOT_RUN` because executing the
  harness does not prove acceptance of an Ultima desktop artifact.

### Evidence to attach to the NC-02 issue

Record the exact Ultima and Codex binary SHAs, versions, operating system/build,
CPU/RAM/GPU, power plan, graphics backend, source SHA, scenario JSON hash,
login state, project fixture, sample period, cold versus warm process state, and
all repetition reports. Compare matched CLI vs client on S0 and S1; add p50/p95
process-tree RAM, CPU and latency plus observed variance. Before native acceptance,
explicitly classify every native-specific check **NOT_RUN**. Never infer native
performance from GitHub-hosted runner numbers.

## Limitations / operational safety

The harness launches each supplied command without a shell and drains output
without retaining secrets. Use trusted command paths and private plans. It is
non-interactive: do not exercise live tools or approvals through it. When the
duration expires it terminates the launched process and currently attached
children (never unrelated processes). Detached/reparented sidecars may not be
visible to the sampler; validate those scenarios with process-tree trace tools.
OS schedulers, sampling overhead, shared-memory RSS accounting, antivirus and
cache state can affect short intervals. Full S4 endurance remains a separate
60-minute owner test.
