# Necessary Codex — Architecture & Upstream Policy

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

- Independent public repo; no required code/runtime dependency on Mind Place or CoSoS.
- **System Codex first:** locate supported installed executable; never overwrite another application's Codex or token/config files.
- **Protocol:** official `initialize`→`initialized`, `thread/start`/`thread/resume`, `turn/start`/`turn/interrupt`; event routing with monotonically matched request IDs. Generate schemas from each supported engine version. Treat unknown methods/events and protocol drift as capability negotiation/error states, not crashes.
- **Security:** no network-exposed App Server, default stdio; explicit approvals cannot be bypassed. Strong `cwd` scoping, exec path verification, bounded buffers, no credential capture or external telemetry by default.
- **UI:** Svelte/Tauri rather than rebuilding Codex core. WebView2 can still cost RAM: measure full process tree, use laziness/virtualization.
- **Managed engine (optional in Phase 04):** independent version channel with official release verification, pinned known-good version, staged promote after contract+native smoke, rollback. Engine and app versions are separate; never assume newer upstream can be adopted safely or all new UI features appear automatically.

## Version tracking

Maintain `compatibility.json` (future artifact): app version, tested Codex version ranges, protocol hashes, supported capability flags, upstream artifact SHA, contract test status, native acceptance status, rollback path. Watch stable GitHub releases; open a compatibility PR instead of silently auto-merging all upstream changes.

## Reference

- https://learn.chatgpt.com/docs/app-server
- https://github.com/openai/codex
- [Roadmap](ROADMAP.md) · [Task board](EXECUTION-BOARD.md) · [Performance budget](PERFORMANCE-CONTRACT.md)
