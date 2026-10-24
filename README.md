# Necessary Codex

A minimalist, beautiful, **performance-first** Windows desktop client for the **official OpenAI Codex App Server**.

**Goal:** ultra-low UI latency, bounded process-tree RAM/CPU, fast startup and keyboard-first developer workflows—without forking the Codex agent engine. Independent from CoSoS and Mind Place.

**Planned stack:** Tauri 2 + Svelte/TypeScript + Rust, Windows x64 first; system-installed `codex app-server` over stdio/JSONL. This is a roadmap-stage project, **not a finished app**.

## Project planning

- [Project Hub and execution board in Notion](https://app.notion.com/p/403c4a6aaf738104addaf3358440d7aa)
- [Four-phase roadmap (13 tasks)](docs/ROADMAP.md)
- [Dependency and parallel execution board](docs/EXECUTION-BOARD.md)
- [Performance and UX acceptance contract](docs/PERFORMANCE-CONTRACT.md)
- [Architecture and upstream update policy](docs/ARCHITECTURE.md)
- [GitHub execution issues](https://github.com/KaisAbiyyi/necessary-codex/issues)

## Current state

**Roadmap initialized; 0/13 tasks Done (0%).** Initial Now (Ready, unclaimed): [NC-01](https://github.com/KaisAbiyyi/necessary-codex/issues/1), [NC-02](https://github.com/KaisAbiyyi/necessary-codex/issues/2), [NC-03](https://github.com/KaisAbiyyi/necessary-codex/issues/3). Initial Up Next: NC-04, NC-05, both blocked. The **Notion execution task database** is the live status authority; this README is an initialization snapshot.

## Development governance

Claim before coding, respect AND dependencies and resource locks, use reviewable GitHub PRs/CI, and separately record **real Windows** native measurements and owner gates; never infer actual performance from roadmap targets.

## Legal

Planned first-party license: Apache-2.0. OpenAI Codex remains a separately maintained upstream project with its own license and notices. Neither affiliated with nor endorsed by OpenAI.
