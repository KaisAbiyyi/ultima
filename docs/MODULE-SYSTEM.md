# Ultima — Downloadable Block Architecture (design contract, NOT implemented)

**Owner decision, 24 Oct 2026:** Ultima ships a lightweight **Codex-default experience**. Every other block is user-selected, independently downloadable, removable and updateable. Product performance remains the highest optimization priority with *all* modules installed, not just a blank baseline. Permissions, correctness, durability and security are non-negotiable.

## Core versus optional blocks

| Block | Installation state | Activation | Future gate |
|---|---|---|---|
| Ultima Core + Codex integration | Included as **default user experience** | Default selected agent; use official Codex App Server | Current active NC-01–NC-13 roadmap |
| Gemini / Antigravity | Optional user download | Lazy on demand; vendor-authorized SDK/CLI | Official integration/terms + perf tests |
| CoSoS | Optional user download **only after independent stability** | Lazy process adapter, isolated config | Separate CI/native/security/owner acceptance |
| Mind Place | Optional user download **only after independent stability** | Scoped context connector, read-only by default | Privacy, revocation, storage integrity, real Windows acceptance |
| Custom API Agent | Optional user download | Bring-your-own API credentials/base URL/models; opt-in agent tools | Provider tests, token security, permission gate |
| Local Agent | Optional user download | Connect to user-managed Ollama/LM Studio or supported local inference engine; opt-in agent tools | Model capability + resource measurements |
| Claude (possible future) | Exploratory, not committed | Documented Claude Agent SDK/CLI or API if licensed and permitted | Provider policy, auth, runtime and benchmark qualification |

**Included Codex integration is NOT a promise to package third-party binaries by default.** Installer should offer a frictionless Codex onboarding flow: detect a supported system installation, provide user-consented managed installation **only after** provenance/license/release checks are proven, and otherwise show clear prerequisites. Never silently reuse or rewrite external apps' auth/config.

## External project releases are the source of truth (future integration contract)

**CoSoS and Mind Place remain independent products and repositories.** Ultima must NEVER copy their source into its own tree or couple their Git histories. The default integration is **release-based consumption**, using user-chosen published tags/assets, not `git pull main` or building from an arbitrary unreviewed commit.

| Block | Upstream repository | Artifact expected from each qualifying release |
|---|---|---|
| CoSoS | [KaisAbiyyi/chat-on-steroids-on-steroids](https://github.com/KaisAbiyyi/chat-on-steroids-on-steroids) | Versioned Windows runtime/portable sidecar and a documented IPC/agent protocol (not merely the desktop installer) |
| Mind Place | [KaisAbiyyi/mind-place](https://github.com/KaisAbiyyi/mind-place) | Versioned service/connector or approved installer artifact, documented MCP/API contract, explicit storage schema version and migration policy |

**These are desired future release contracts, not statements that today's releases already contain those assets.** Each upstream team independently decides when a version is stable and published.

**Version picker / update policy**
1. Discover explicitly published stable release tags and platform assets; expose available versions and tested compatibility. Allow **Install version...**, **Pin version**, **Check updates**, **Update after verification**, **Rollback** and **Uninstall adapter**.
2. Resolve each selected tag to a **specific artifact digest and source commit**. Verify HTTPS origin, checksums and available signatures/provenance; treat mutable tags/replaced assets as new candidates requiring fresh verification. Never run unverified release binaries.
3. Maintain an Ultima-side compatibility matrix (`ultima_version`, `module_id`, `module_version`, `platform`, `protocol_version`, `asset_sha256`, `qualification`). Reject unknown incompatible combinations without modifying installed working versions.
4. Install to a per-module/per-version directory with an atomic activation pointer; preserve a known-good binary for rollback. Do not change an existing user-installed CoSoS/Mind Place executable or overwrite its configuration.
5. **Connect to existing standalone service when explicitly chosen**, or launch an Ultima-owned on-demand sidecar as a distinct lifecycle. Detect port/profile conflicts; avoid duplicate background runtimes; disconnect/stop only processes Ultima owns.
6. **Mind Place migration warning:** binary rollback is not automatically data rollback. Preserve separate data ownership; snapshot/backup and test schema compatibility before upgrades. A destructive/irreversible migration requires explicit user approval and verified recovery; never automatically downgrade live archives or change their ownership.
7. Release update must not automatically update Ultima, CoSoS, and Mind Place together. Their version lifecycles, documentation, CI and legal notices remain independent.

**Concrete future user flow:** Ultima → Modules → Mind Place → Versions → choose a compatible **published release** → verify → download/install (or connect to an existing installation) → launch only when activated. Same pattern for CoSoS. A module can stay installed/pinned indefinitely, subject to documented security or compatibility warnings.

## Invariant — Installed != Loaded != Running

- **Installed:** validated manifest, version and disk files; no agent process, model load, persistent background worker, network polling or reactive UI tree instantiated.
- **Loaded:** only lightweight capability metadata necessary for on-screen selection; no agent runtime until explicit activation. Module list may load manifest metadata without booting modules.
- **Running:** demand-activated isolated runtime; its own lifecycle, reconnect, heartbeat, health, bounded queues, lazy UI panels, measured resource accounting.
- **Disabled/uninstalled:** unregister and release Ultima-owned resources; cleanly terminate only processes Ultima created. Never kill a user-owned Ollama/LM Studio server or clear a user's persisted model/cache without explicit consent.
- **After last session closes:** stop Ultima-owned sidecars and drop UI caches; configurable grace period only with recorded RAM trade-off. Crash/restart must not leave stale children.

**No resident per-provider background service for inactive blocks**, no global eager imports, no always-active model inference, and no permanent plugin polling. Integrity manifests and compatibility checks can be run on explicit update actions or low-cost on-demand discovery; avoid launch-time network.

## Two distinct kinds of blocks — do not conflate an API with a coding agent

1. **Agent adapter:** calls an existing agent runtime (Codex, CoSoS, authorized third-party SDK) that owns its agent loop and tools.
2. **Model/API connector:** calls inference via vendor API/BYOK or local endpoints (e.g. OpenAI-compatible /v1). An API key or model is *not* automatically a coding agent. A **separately permissioned agent runtime** must provide plan → tool invocation → workspace edits → approvals → retries → cancellation if Ultima offers coding-agent abilities. First version may support plain chat only; do not claim parity with Codex.
3. **Context/memory connector:** Mind Place is not an agent or LLM; it provides revocable, provenance-aware retrieval via documented APIs/MCP with scoped permission and no silent writes.

Example user-managed local endpoints (never required or launched at Ultima startup):
- Ollama `http://127.0.0.1:11434/v1` (OpenAI-compat subset, capabilities negotiated).
- LM Studio `http://127.0.0.1:1234/v1` (documented OpenAI-compatible endpoints).
- Custom HTTPS model provider base URL with explicitly configured credentials and provider-specific API model IDs.

## Provisional module manifest (not frozen schema)

```json
{
  "schema_version": 1,
  "id": "ultima.local-model",
  "kind": "model-connector",
  "version": "0.1.0",
  "required_ultima_range": ">=0.2 <1.0",
  "capabilities": ["models", "stream"],
  "permissions": ["loopback-network"],
  "runtime": {
    "activation": "on-demand",
    "owned_processes": false
  },
  "artifacts": {
    "source": "verified-registry",
    "sha256": "required-at-distribution-time"
  }
}
```

This is an architectural sketch, **not working schema or an issued artifact hash**. Manifest and binaries must have verifiable integrity, source attribution and release authorization. A future module manager should support explicit install, update, disable, uninstall, health, version pinning and rollback.

## Permission & isolation contract

- Limit each module to declared capabilities and only user-granted file/workspace/network scopes. Signed/verified release artifacts do not by themselves guarantee harmless behavior: least privilege and user review still matter.
- Never share provider OAuth tokens, API keys or Mind Place archive data across blocks by default. Use OS-protected secrets where available, do not print them into logs.
- Network connect rules: local loopback default for local providers; non-loopback endpoints require deliberate consent and TLS where applicable. Avoid SSRF/path traversal and untrusted plug-in command execution.
- Per-tool approval must be scoped to the current active agent and exact action; switching agent cannot inherit broad permissions or auto-approve.
- Versioned capability negotiation: no assumption of shared history/session formats, tool protocols or feature parity.

## Total-system performance gate

Test **three separate conditions** on exact release SHA and native Windows:

A. **Bare default:** Ultima + Codex selected, no optional blocks installed.
B. **Everything installed, inactive:** all optional packages present on disk and configured (where authorized), but none running. Compare cold start, idle process count, process-tree RAM/CPU, UI input p95 with A. Fail on unmeasured regressions; proposed installed-only delta target: effectively negligible within repeatable measurement noise; revise after baseline evidence.
C. **Multiple blocks active:** Codex plus several agent/model/context connectors with typical workflows. Measure attributable subprocess CPU/RAM/GPU and per-provider startup/TTFT, p95 input and streaming latency, crash recovery, stale worker cleanup and isolation. External user-managed model server cost must be separately reported, not falsely counted as zero.

Under memory pressure prefer optional unload, bounded queues, incremental transcripts, viewport virtualization, lazy syntax highlighting and deliberate user choice over aggressive always-on caching. Never trade away correct tool authorization to meet benchmarks.

## Timeline & task governance

Current **Codex Foundation only**: preserve the 4 phases and stable NC-01–NC-13 task IDs. This document records post-foundation acceptance requirements, **not additional claimed/Ready/Now tasks**. When Codex is stable, issue new small dependency-gated units; prioritize reusable module lifecycle/performance before individual extra providers. Existing CoSoS and Mind Place remain separate projects.

References:
- [Vision](PRODUCT-VISION.md)
- [Architecture](ARCHITECTURE.md)
- [Performance contract](PERFORMANCE-CONTRACT.md)
- [Provider license policy](LICENSING-AND-PROVIDERS.md)
- https://github.com/ollama/ollama/blob/main/docs/api/openai-compatibility.mdx
- https://lmstudio.ai/docs/developer/openai-compat
- https://platform.claude.com/docs/en/cli-sdks-libraries/overview
