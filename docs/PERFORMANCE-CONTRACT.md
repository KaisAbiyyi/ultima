# Ultima: Codex Foundation — Performance & UX Acceptance Contract

**Performance first, but correctness/security/authorization are non-negotiable.** This is a proposed v0.1 benchmark contract, **NOT evidence of achieved performance**.

## Measurement principle

Measure **total product process tree**: Ultima executable + WebView2 children + Codex App Server + any attributable subprocesses; also report UI-only and engine-only deltas separately. Compare like-for-like on same machine, same Codex engine version, login state, project size, history length, tool policy, context and workload. No browser in one variant and browser in another. Record hardware, OS, power mode, binary SHAs, warm/cold cache, graphics backend and 3+ repetitions. Report median, p95 and variance. Never attribute shared OS/browser usage to one product without evidence.

## Provisional budgets (validate against NC-02 baseline; revise via reviewed ADR, not silent weakening)

| Metric | Initial goal / gate |
|---|---|
| UI cold start to responsive composer (native Windows) | p95 ≤ 1.5 s, measured; target pending baseline feasibility |
| Idle CPU for entire product, settled 60 s | ≤ 1% average on reference machine, sampled |
| UI incremental event receipt → painted feedback | p95 ≤ 50 ms at normal streaming load |
| Keyboard input → visible feedback | p95 ≤ 50 ms; no perceptible missed inputs |
| Extra idle RAM over same-engine headless CLI | target ≤ 150 MiB total incremental desktop overhead; evaluate against platform baseline |
| 10,000 message history | bounded/virtualized DOM, no lockups; p95 wheel/input latency ≤ 50 ms |
| 60-minute stress | no monotonic unexplained process-tree RAM leak; investigate > 20 MiB/hour trend |
| Tool approval and cancel | no double execution, no state corruption, no hidden implicit auto-approval |
| Crash/restart | durable threads resumable; no lost permission boundary, no log of secrets |

These figures are *targets*, not verified facts or promises. If native feasibility disproves a budget, document the competing measurements and require owner-reviewed revision. CLI baseline and total-workload memory reduction matter more than arbitrary package size.

## Bench scenarios

1. **S0:** clean process; cold start, login resolved; idle 60 s; measure RAM/CPU/start time.
2. **S1:** small repository; one streamed question and model answer; event latency and input latency.
3. **S2:** 100+ tool events, approvals, cancellation, errors and retries; correctness and backpressure.
4. **S3:** 10k message synthetic history + long code blocks and diffs; virtualization, scrolling, memory.
5. **S4:** 60-min repeated turns, 3 project switches, pause/resume, engine restart and recovery.
6. **S5:** native Windows DPI, light/dark, keyboard-only, screen reader smoke, composition/IME and WebView2 subprocess lifecycle.

## Continuous gates

- Cloud CI: contract tests, fuzz malformed/out-of-order JSONL, authorization tests, rendering fixtures, synthetic performance regression signals. Cloud numbers are trend data, **not native Windows product PASS**.
- Windows native: exact-sha signed/packaged candidate, same-workload comparison with CLI (and optional official desktop), screenshots/traces/process tree, PASS/FAIL/NOT_RUN.
- Release: all required gates green, no unbounded queues, no hidden tool approval, validated updater rollback.

## UI budget

Single primary workspace, narrow left history rail, focused composer, minimal command palette, progressive disclosure for tool/log/diff panels. Virtualize long lists, batch stream updates to render cadence, lazy-load highlighters and diff editor, no heavy motion or permanent embedded browser.
