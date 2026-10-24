# NC-11 · Codex App Server upstream compatibility

Ultima reads only official stable releases from https://github.com/openai/codex. The versioned policy is in protocol/compat-matrix.json. Pinned production contract: **0.164.0** (https://github.com/openai/codex/releases/tag/rust-v0.164.0). Historical regression candidate: **0.163.0** (https://github.com/openai/codex/releases/tag/rust-v0.163.0). Candidate is not automatically approved even if smoke passes. Unknown/alpha/future versions fail closed.

## CI & watch

.github/workflows/nc-11-upstream-compat.yml runs deterministic matrix tests plus two official Codex versions on Linux and hosted Windows (four contract jobs). For each: exact official npm install, asserted binary version, export of official JSON Schema and TypeScript types, NC-01 required-definition checker, actual *unauthenticated* initialize/initialized/model-list contract smoke. Generated schemas are uploaded as per-version evidence.

Daily 04:17 UTC (11:17 WIB) and manual workflow_dispatch checks latest verified stable release via the official GitHub API. New stable version newer than pinned or network/parse error **fails** the monitor; no auto-updating system Codex or owner files. No release is promoted merely because its version number is close to pinned.

## Promotion and evidence boundaries

To promote: diff generated official schemas, review typed NC-05 bridge and approval handling, run replay/security checks and exact-head GitHub CI, then explicitly update the pinned protocol and compatibility matrix in a reviewed PR. No silent approval escalation, credential handling, runtime install, rollback or system configuration changes in NC-11. NC-12 owns safe optional runtime updates.

The CI tests here cover only limited cloud contract compatibility. Native owner Windows RAM/latency, full authenticated model turns, real approvals, Tauri integration, signed package provenance and end-user acceptance remain **NOT_RUN** under NC-06/07/10/12/13.
