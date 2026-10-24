# Ultima — Licensing and Provider Integration Policy

**Product first-party source:** **Apache License 2.0** ([root LICENSE](../LICENSE)); not a license grant over third-party executables, model weights, APIs, SDKs or trademarks.

## Why Apache-2.0

Permissive, allows redistribution/modification/commercial use, includes an express contributor patent license and notice preservation. Apache-2.0 works with MIT-licensed first-party contributions, subject to preserving original copyright/license notices. The root license applies to Ultima's own code only.

## 2026-10-24 provider matrix

| Surface | Open-source status | Proposed future route | Gate |
|---|---|---|---|
| OpenAI Codex CLI and App Server | Codex repository Apache-2.0 | Official installed `codex app-server` via stdio, current active phase | Versioned protocol tests; explicit approvals and Windows native performance |
| CoSoS | Separate project with its own license/policy | Optional local adapter after its independent stabilization | Own CI/native correctness and security, no mandatory shared build |
| Mind Place | Separate, independently maintained project; rights as its own repository currently declares | Optional read-only-by-default MCP/API continuity connector after stable release | Revocation, credentials, storage integrity, privacy, owner consent |
| Anthropic Claude Code | **Proprietary; all rights reserved** despite public GitHub issue/examples repo | Assess official Claude Agent SDK, documented CLI and permitted API integrations | Review current terms, account/subscription authorization and SDK license; do not wrap unauthorized consumer OAuth |
| Google Gemini CLI | Apache-2.0 source, but 2026 consumer access shifted to Antigravity CLI | Research-only possible adapter, not default consumer recommendation | Provider service terms and supported auth method |
| Google Antigravity (desktop/CLI agent harness) | Core proprietary; official Python SDK is Apache-2.0, bundled agent runtime is compiled | Potential later SDK/API adapter | Auth and API terms, distributability of runtime, Python sidecar process/RAM, capability tests |

**Sources:** https://github.com/openai/codex ; https://github.com/anthropics/claude-code/blob/main/LICENSE.md ; https://github.com/google-gemini/gemini-cli ; https://developers.googleblog.com/an-important-update-transitioning-gemini-cli-to-antigravity-cli/ ; https://antigravity.google/blog/introducing-google-antigravity-sdk ; https://antigravity.google/docs/sdk/overview .

## New opt-in provider blocks (owner decision 24 October 2026)

| Block | Legal/product approach |
|---|---|
| **Codex default** | Ultima ships with Codex preselected as its core **experience**; detect existing official binary or use an explicitly consented, provenance-verified installation path. The Apache-2.0 project license does **not** automatically certify how every third-party binary may be bundled. |
| **Custom API Agent / BYOK** | User brings own provider API credentials and config. Respect selected provider's API/SDK terms, billing, model licensing and secrets; no bundled third-party account entitlement. Agent actions require explicit permissioned runtime. |
| **Local Agent** | Connect to user-managed localhost Ollama/LM Studio (or supported equivalent). Preserve server/model/runtime licenses, quantization/model weight license, and installed version attribution; Ultima may not own or kill external model servers. |
| **Gemini / Antigravity, CoSoS, Mind Place, Claude** | Explicit per-user downloaded/activated blocks only after each appropriate provider stability, rights, security, auth and measured performance gate; no redistribution of proprietary vendor assets without authorization. |

## Distribution rules

1. Keep third-party source and runtime licenses independent. Preserve copyright notices, LICENSE/NOTICE and modification notices when redistribution requires them; build an SBOM when packaging.
2. Prefer system-installed, user-managed official engines in the early releases. Never copy credentials, subscription tokens, proprietary bundles or undocumented protocol endpoints to Ultima.
3. Public APIs/SDKs may still require API billing, quotas, permitted OAuth scopes and terms-of-service compliance; a visible GitHub repository does not equal open-source rights.
4. **GPL caveat:** Apache-2.0 code may in some circumstances be combined into GPLv3 programs, but a distributable *combined* work may have to meet GPLv3 requirements. Mere separation into different processes is not a universal legal exemption. Audit before creating combined binary/distribution, especially for unrelated GPL projects.
5. Ultima is a distinct project. Do not imply affiliation with OpenAI, Anthropic, or Google; do not treat provider trademarks as Ultima's own.
6. Before first commercial distribution, run an actual third-party rights, trademark, SDK and packaging legal audit. This document is project policy, not legal advice.

## Current decision

**Keep root Apache-2.0 unchanged.** No CLA or contributor-license change is required merely to rebrand from Necessary Codex to Ultima. Adding support for a proprietary engine later does not automatically force Ultima to be proprietary, but every integration must independently comply with applicable terms and licenses.
