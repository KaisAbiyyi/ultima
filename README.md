# Ultima

**Ultima** is a minimal, fast, beautiful and modular **AI development workstation**. One lightweight desktop, independent coding agents and (eventually) optional continuity services. **Performance first**; security, permission correctness and durable work are mandatory.

**Default download experience:** Ultima Core with **Codex preselected as the default agent** and a simple official Codex setup/discovery flow. Codex is the only active implementation focus. **All other blocks are optional user downloads**: Gemini/Antigravity (if supported), CoSoS (once stable), Mind Place (once stable), **Custom API Agent (BYOK)**, and **Local Agent (Ollama/LM Studio or compatible engines)**. Claude remains a potential authorized SDK/API integration, not an open-source dependency. No future block is a current release prerequisite.

**Initial stack:** Tauri 2 + Svelte/TypeScript + Rust; Windows x64 first. The system-installed Codex binary remains independently updated and independently licensed. No agent-core fork and no always-on bundled browser.

## Direction and documentation

- [Ultima product vision and staged integration](docs/PRODUCT-VISION.md)
- [Codex-default installation and downloadable block architecture](docs/MODULE-SYSTEM.md)
- [Codex Foundation four-phase roadmap (13 existing NC tasks)](docs/ROADMAP.md)
- [Canonical dependency and parallel execution board](docs/EXECUTION-BOARD.md)
- [Performance + UX qualification budgets](docs/PERFORMANCE-CONTRACT.md)
- [NC-02 reproducible CPU/RAM/latency benchmark harness](tools/nc02/README.md)
- [NC-10 local Windows qualification and evidence boundaries](docs/NC-10-NATIVE-QUALIFICATION.md)
- [Architecture and upstream update policy](docs/ARCHITECTURE.md)
- [Licensing / provider integration review](docs/LICENSING-AND-PROVIDERS.md)
- [Notion Project Hub](https://app.notion.com/p/403c4a6aaf738104addaf3358440d7aa)
- [GitHub Issues](https://github.com/KaisAbiyyi/ultima/issues)

## Initial execution snapshot (24 Oct 2026)

**0/13 tasks Done; 0%.** `NC-01` [#1](https://github.com/KaisAbiyyi/ultima/issues/1), `NC-02` [#2](https://github.com/KaisAbiyyi/ultima/issues/2) and `NC-03` [#3](https://github.com/KaisAbiyyi/ultima/issues/3) are Ready in Now (unclaimed). `NC-04` and `NC-05` are blocked direct successors in Up Next. **NC prefix is intentionally preserved** for the Codex Foundation tasks; do not renumber the existing board after branding. Live status comes from Notion task data and exact GitHub PR/CI evidence, not this snapshot.

## Non-negotiable release criteria

- Measure total process-tree RAM/CPU and UI event/input latency on *real Windows hardware*, compare equivalent CLI work, and qualify long-history/multi-project sessions.
- **Installed ≠ Loaded ≠ Running**: inactive downloadable blocks must not start processes, model servers, background workers or eager UI trees. Measure **both default-only and all-modules-installed** idle baselines, as well as a multi-block active stress workload. User-managed local servers must not be killed by Ultima.
- Minimize process count, virtualize histories, isolate provider secrets and use bounded caches.
- Do not bypass provider approval/auth rules or silently copy/rewrite credentials.
- Claim before coding, review exact SHA, pass relevant CI and native/owner gates separately.

## License and relationship to upstreams

**Ultima's original first-party source: [Apache License 2.0](LICENSE).** Other coding agents, SDKs, models, binaries and APIs retain their own licenses and terms. Ultima is independent and is not affiliated with or endorsed by OpenAI, Anthropic or Google. See the [provider legal policy](docs/LICENSING-AND-PROVIDERS.md).
