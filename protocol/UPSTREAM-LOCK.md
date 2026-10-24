# NC-01 — pinned official Codex App Server contract

Upstream: [rust-v0.164.0](https://github.com/openai/codex/releases/tag/rust-v0.164.0), published 2026-10-23.
Pinned commit: [6aaac694803004a63c6f393d153c96d22521a6b9](https://github.com/openai/codex/commit/6aaac694803004a63c6f393d153c96d22521a6b9).

The JS probe is deliberately NOT the complete upstream schema. CI installs the exact Codex release, generates actual JSON Schema and TypeScript artifacts, checks required definitions, and uploads them. NC-11 handles later upstream compatibility.

## Contract matrix

| Exchange | Expected payload | Failure behavior |
|---|---|---|
| initialize then initialized | clientInfo.name/version; response userAgent/platformFamily/platformOs | no requests before handshake; no duplicate initialize |
| model/list | cursor, limit, includeHidden; result.data/nextCursor | no invented model IDs |
| thread/start then thread/resume | thread.id; resume threadId | preserve identity and permission boundary |
| turn/start then turn/interrupt | threadId and text input; interrupt threadId/turnId | distinguish completed/failed/interrupted |
| item/commandExecution/requestApproval | server request with id/threadId/turnId/itemId/startedAtMs | decline default; explicit accept only |
| item/fileChange/requestApproval | equivalent fields and result.decision | never silently allow write |
| unknown notification | method without id | tolerate/ignore rather than crash or accumulate queue |
| unknown server request | method with id | respond -32601 and fail closed |

Stable API only; do not enable experimentalApi, use WebSocket, copy secrets, grant session permissions, or run real model/tool actions in unattended CI. JSONL payload cap: 1 MiB per line. NC-04 owns process supervision, NC-05 owns typed bridge/backpressure.

## Reproduction

- node --test tests/nc-01-contract.test.mjs
- npm install --global @openai/codex@0.164.0
- codex app-server generate-json-schema --out generated/json
- codex app-server generate-ts --out generated/ts
- node scripts/nc-01-export-check.mjs generated/json generated/ts
- node scripts/nc-01-smoke.mjs

Fixture-based approvals, turn lifecycle and unknown events are synthetic. Live CI smoke covers unauthenticated initialize and model/list only. Authenticated real turns/approvals/interrupt and owner Windows native performance remain **NOT_RUN**, not PASS.
