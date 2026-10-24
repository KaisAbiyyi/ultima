# Ultima — Product Vision & Staged Integration

**Decision:** 24 October 2026 · **Owner intent:** one performant, beautiful, minimal AI development workstation, assembled from independently useful LEGO modules, **not** a mandatory merged runtime.

## Identity

**Ultima** is the product/desktop shell and orchestration surface. **Codex** is its *first* agent integration, not its permanent identity. The original Necessary Codex work is now **Ultima: Codex Foundation**. Existing `NC-01` to `NC-13` task IDs, claims, dependencies and acceptance remain unchanged; avoid churn and false completion. Version 0.x must still deliver a strong standalone Codex client.

## Integration ladder — deliberately not scheduled as current tasks

1. **Now — Codex Foundation:** Deliver current 4-phase, 13-unit plan: official `codex app-server`, Tauri/Svelte/Rust, low RAM/CPU/startup/streaming latency, native Windows tests, upstream version compatibility. This is the sole active implementation scope.
2. **Later — CoSoS Adapter:** When CoSoS is demonstrably stable and meets its own CI, safety, Windows and endurance gates, add an **opt-in, lazily launched** adapter. Keep CoSoS's repository, releases, licenses and sessions independently usable. No mandatory code merge; benchmark incremental process tree cost and permissions before adopting.
3. **Later — Mind Place Connector:** After Mind Place's storage/retrieval/privacy/lock/revocation and real-device gates pass, integrate using **scoped, explicit, read-only-by-default APIs/MCP**. Mind Place owns its data, authorization and historical truth; Ultima doesn't mirror or mutate archives or bypass consent. Disable by default, revocable.
4. **Exploration only — Other agent adapters:** Claude Code via official SDK/CLI surface *only where permitted by applicable terms*, Google Antigravity through its official SDK/API, or compatible open-source runtimes. Technical feasibility, legal authorization, performance and update stability are independent gates; no guarantee of subscription reuse or universal feature parity.

No upstream project is blocked on Ultima, and no Ultima release is blocked on future integration. Integration plans remain on **Hold / Exploratory** until the owner explicitly promotes them after stability evidence.

## Adapter contract (future design target, NOT implemented)

A capability-negotiated `AgentProvider` interface, with:
- Detect/install guidance and exact upstream version, no unauthorized binary redistribution.
- Start/stop/reconnect, login/account boundary, session list/resume and streaming events.
- Explicit tool approval and cancel/interrupt; never automatically escalate authority across providers.
- Optional files, patch, diff, terminal, background tasks, usage metrics exposed only by actual provider capability.
- Uniform UI events but **no assumption that all engines share histories, tokens, models, prompt structure or permission semantics**.
- Per-provider isolated configuration, credentials, working directories, cache and subprocess lifetime.
- Lazy load: disconnected integrations consume effectively no application-owned running agent processes; measure.
- Secure failure closed, version-aware schema/contract tests, no scraping or reverse-engineering restricted services.

The shell should act as a *host*, not secretly rebuild Codex, CoSoS, Claude Code or Antigravity internals.

## Performance doctrine

A new integration fails admission if idle RAM/processes, p95 input latency, cold startup, tool correctness or resilience regress beyond approved budgets. Measure **whole process tree + attributable external subprocesses**, disaggregated by active provider. Idle providers stay unloaded. Measure activation/deactivation, reconnection, stale worker cleanup, three parallel agents and multi-hour workloads. Display user-visible resource cost.

## Legal boundaries

The Ultima first-party code is Apache-2.0. Third-party dependencies/binaries/SDKs keep their own license and terms; third-party access and authentication are separately governed. Proprietary Claude Code or Antigravity runtime is **not** made open source by an Ultima adapter. Do not redistribute, decompile or reuse leaked source; integrate only documented authorized surfaces. If a GPL component is ever incorporated into one combined distributed derivative, conduct license review before shipping; separate IPC/process integration does not automatically settle every copyleft issue.

See [Licensing](LICENSING-AND-PROVIDERS.md) for a provider-by-provider matrix.

## Beyond the first puzzle piece

The product eventually looks like: **one UI → independently versioned agent adapters (Codex, optional CoSoS, maybe Claude/Antigravity) → optional Mind Place continuity service**. User consent, boundaries and performance always outrank a higher number of checkboxes.
