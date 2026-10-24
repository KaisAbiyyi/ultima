# Ultima: Codex Foundation — Architecture & Upstream Policy

## Modular LEGO boundaries

```text
Tauri + Svelte minimalist UI
    | Tauri IPC (typed, low-copy)
Rust host: runtime manager / JSONL event broker / bounded buffers
    | process stdio / official JSON-RPC protocol
Official system-installed `codex app-server`
    | upstream-managed auth / agent / tools / MCP / durable threads
OpenAI Codex Core
```

- **Independent first milestone:** no mandatory code/runtime dependency on Mind Place or CoSoS. The Ultima product may integrate stable projects later via explicitly optional adapters; no automatic merge or shared-state assumption.
- **System Codex first:** locate supported installed executable; never overwrite another application's Codex or token/config files.
- **Protocol:** official `initialize`→`initialized`, `thread/start`/`thread/resume`, `turn/start`/`turn/interrupt`; event routing with monotonically matched request IDs. Generate schemas from each supported engine version. Treat unknown methods/events and protocol drift as capability negotiation/error states, not crashes.
- **Security:** no network-exposed App Server, default stdio; explicit approvals cannot be bypassed. Strong `cwd` scoping, exec path verification, bounded buffers, no credential capture or external telemetry by default.
- **UI:** Svelte/Tauri rather than rebuilding Codex core. WebView2 can still cost RAM: measure full process tree, use laziness/virtualization.
- **Managed engine (optional in Phase 04):** independent version channel with official release verification, pinned known-good version, staged promote after contract+native smoke, rollback. Engine and app versions are separate; never assume newer upstream can be adopted safely or all new UI features appear automatically.

## Downloadable modules — explicit product contract (not implemented)

Ultima should ship with **Codex preselected as the default agent integration**. The user alone decides whether to **download and activate** Gemini/Antigravity, CoSoS, Mind Place, BYOK Custom API Agent, Local Agent or future authorized providers. A preselected Codex experience does not authorize silently bundling provider binaries or extracting existing app credentials.

**Installed ≠ Loaded ≠ Running.** Only validate small module manifest metadata at startup; do not import provider UI, spawn subprocesses, allocate model runtime, start network polling or initialize background services for inactive blocks. Activate on user demand. Destroy owned process trees, streams and cache buffers at session end (never terminate user-owned local inference servers). Verify update provenance, per-module compatibility and permissions, with rollback. The same release must pass native performance qualification **with all optional blocks installed but idle**, plus multiple-active-block tests.

Distinguish **agent adapter** (preexisting agent loop), **model connector** (BYOK/local inference endpoint), and **context/memory connector** (Mind Place). A model endpoint/API key does not supply autonomous coding features; those need an explicit tool/approval/sandbox layer with observable agent capabilities.

See [Downloadable Block Architecture](MODULE-SYSTEM.md) and [Product Vision](PRODUCT-VISION.md).

## Future provider boundary (NOT implemented)

Ultima's eventual host can expose capability-negotiated, lazily activated provider adapters: Codex (first), possible CoSoS (once stable), possible Mind Place read-only-by-default continuity (once stable), and legally authorized third-party agent SDKs. Keep configuration and credentials isolated; never proxy approvals or bypass auth. No cross-provider process starts on idle. See [staged Product Vision](PRODUCT-VISION.md).

## Version tracking

Maintain `compatibility.json` (future artifact): app version, tested Codex version ranges, protocol hashes, supported capability flags, upstream artifact SHA, contract test status, native acceptance status, rollback path. Watch stable GitHub releases; open a compatibility PR instead of silently auto-merging all upstream changes.

## Reference

- https://learn.chatgpt.com/docs/app-server
- https://github.com/openai/codex
- [Roadmap](ROADMAP.md) · [Task board](EXECUTION-BOARD.md) · [Performance budget](PERFORMANCE-CONTRACT.md)
