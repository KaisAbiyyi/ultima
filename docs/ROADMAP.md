# Necessary Codex — Performance-First Roadmap v0.1

**Updated:** 2026-10-24 (Asia/Jakarta) · **Product:** independent, lean desktop frontend for official OpenAI Codex App Server · **License decision:** Apache-2.0 (add LICENSE before shipping).

## North star

The fastest *usable* Codex desktop: minimal yet polished UI, keyboard-first, low input/stream latency, low total process-tree RAM and CPU, reliable permissions, durable sessions, and zero unnecessary browser/runtime duplication. **Performance is a release gate, not a late optimization.**

**Hard boundaries:** Remain independent from CoSoS and Mind Place. Use upstream `codex app-server` through stdio JSONL/JSON-RPC; do not fork Codex Core, duplicate login/session stores, build a second agent or bundle a full Chromium browser. Official App Server's WebSocket transport is experimental; do not make it the local MVP dependency. User's system-installed Codex binary is the initial engine; managed runtime is a later option, not a baseline requirement.

**Scope:** Windows x64 first. Tauri 2 + Svelte/TypeScript UI + Rust runtime host. No embedded browser or rich feature parity mandate. Accessibility, safe tool approvals, and text/file correctness are non-negotiable.

## Four phases (13 bounded tasks)

| Phase | Outcome | Exit |
|---|---|---|
| **01 · Foundation** | Contract, measurable baseline, visual system | Versioned protocol smoke and reproducible Windows test plan, responsive mock UI |
| **02 · Core Desktop** | Functional daily coding workflow | Sign-in handoff, threads/turns, streaming, approvals, command/file diffs, crash recovery |
| **03 · Performance & Polish** | Polished low-bloat real product | Native workload gate on cold/warm start, process-tree RAM/CPU, latency, long-history UI, accessibility |
| **04 · Release & Upstream** | Safe upstream-following distribution | Version matrix, verified update/rollback, Windows signed/reproducible packaging decision and owner acceptance |

### Phase 01 — Foundation

- **NC-01 — App Server contract spike** (P0, CLOUD, no predecessors, lock `protocol`): initialize/initialized, model/list, thread start/resume, turn start/interrupt, incremental notifications and approval round-trip; generate schemas pinned to an exact Codex version. Record failures/capability negotiation, never assume all versions support all methods.
- **NC-02 — Performance baseline + harness** (P0, CLOUD+LOCAL, no predecessors, lock `benchmark`): automate scenario runner, process-tree memory/CPU and event timestamps with CLI as baseline, add required owner Windows native measurement gate. Do not invent benchmark results or use cloud CI as native evidence.
- **NC-03 — Design system + Svelte UI prototype** (P0, CLOUD, no predecessors, lock `ui-tokens`): minimalist layout, elegant typography/spacing, light/dark themes, keyboard shortcuts, visible focus, mock streamed 10k-message fixture, screenshots.

### Phase 02 — Core Desktop

- **NC-04 — Rust process/session supervisor** (P0, CLOUD, depends NC-01, lock `runtime`): detect system Codex, bounded stdio subprocess, restart/disconnect handling, trusted `cwd`, logs on stderr, no secrets logged, single controlled engine instance when feasible.
- **NC-05 — Typed protocol bridge** (P0, CLOUD, depends NC-01, lock `protocol-adapter`): schemas, routing, request IDs, bounded queues/backpressure, cancellation, unknown-event tolerance, test replay and security error handling.
- **NC-06 — Minimal interactive desktop** (P0, CLOUD, depends NC-03/04/05, lock `ui-shell`): project chooser, sidebar, thread list/resume, composer, streamed messages, model selector, loading/stop/error states; no permanent browser webview beyond Tauri shell.
- **NC-07 — Coding-critical workflow** (P0, CLOUD, depends NC-06, lock `approval-diff`): explicit permission controls, safe tool execution display, lazy code diff, command history, retry/interrupt and restoration after crash. No automatic approval escalation.

### Phase 03 — Performance & Polish

- **NC-08 — Bounded UI & event lifecycle** (P0, CLOUD, depends NC-02/06/07, lock `perf-ui`): virtualization, lazy heavy panes, incremental Markdown, stream batching, bounded log/event/cache buffers, teardown, memory-leak tests.
- **NC-09 — Keyboard-first UX and visual QA** (P1, CLOUD, depends NC-06/07, lock `ux`): responsive polished density, accessible controls, shortcuts, focus/IME correctness, diff UX, light/dark and DPI snapshots, zero cosmetic animation bottlenecks.
- **NC-10 — Native Windows performance qualification** (P0, LOCAL, depends NC-08/09, lock `native-benchmark`): exact artifact SHA, representative long sessions and multi-project workload; compare CLI and optional ChatGPT Desktop where measurable, record all PASS/FAIL/NOT_RUN with process tree and p50/p95 latency.

### Phase 04 — Release & Upstream

- **NC-11 — Upstream release compatibility matrix** (P0, CLOUD, depends NC-05, lock `upstream-compat`): discover official stable Codex releases, verify source, regenerate schemas, negotiate capabilities, run multi-version contract smoke. It may run in parallel with Phase 02/03 when predecessors complete.
- **NC-12 — Safe runtime manager/rollback** (P0, CLOUD, depends NC-04/11, lock `runtime-update`): System Codex default, optional managed version with verifiable checksums/provenance, offline behavior, version pinning, staged rollout, rollback. Never silently replace user's system binary.
- **NC-13 — Release candidate & owner acceptance** (P0, CLOUD+LOCAL+OWNER, depends NC-10/12, lock `release`): security + accessibility + functional + performance gates, packaging/update smoke on real Windows, third-party licenses and clear attribution, documented upstream support matrix; user acceptance before v1 release.

## Critical path and parallelism

- Initial **Now (unclaimed, Ready)**: NC-01, NC-02, NC-03.
- Initial **Up Next (blocked direct successors of Now)**: NC-04, NC-05, NC-11 (from NC-01). NC-06 is *not* Up Next yet because its other predecessors NC-04/05 are neither Done nor Now.
- NC-04 and NC-05 can execute in parallel after NC-01; NC-11 can also run independently then. NC-03 and NC-02 can run alongside NC-01.
- No arbitrary phase barrier: readiness follows AND dependencies and resource locks, not phase number.

## Definition of Done

Every unit has an explicit claimed owner/environment, base/head SHA, isolated branch/resource lock, acceptance tests, self-review diff/security/performance notes, exact-head CI evidence, and a linked PR/issue. `Review` is not `Done`. Native Windows and owner checks cannot be claimed PASS from hosted CI.

Canonical status: `Ready → Claimed → Review → Done`; `Reclaim` only for an actually blocked previous implementor. `Last Done` ≤5 accepted units, `Now` unlimited actionable and review units, `Up Next` unlimited **direct successors** blocked by Now only. Sort Now by priority; later tasks remain DAG-linear. Recompute all lanes after claims, merges, blockers and handoffs; no fabricated status.

## Non-goals (until measured need)

Internal browser, custom agent logic, telemetry-by-default, broad plugins/gallery, arbitrary settings pages, fancy animations, bundled duplicate Chromium, automatic modification of user Codex configuration, cross-project super-app coupling. Codex Cloud integration/worktrees are follow-up candidates, not prerequisites to shipping a fast editor.

## Source

- [Official App Server docs](https://learn.chatgpt.com/docs/app-server)
- [OpenAI Codex source](https://github.com/openai/codex)
- [Execution board](EXECUTION-BOARD.md) · [Performance contract](PERFORMANCE-CONTRACT.md) · [Architecture](ARCHITECTURE.md)
