# NC-10G cloud integration — exact-source checkpoint (2026-10-29)

This integration is a **Draft source candidate**, not authenticated native production acceptance.

## Reviewed sources

- Parent NC-10 Draft PR #26: `033d10f314db231db6c5039b4d3fd3025f756c98`.
- NC-10E Draft PR #40: `d2eec7a8d58078f8617fd29f796fb6e2cc8876d3`; independent scoped security source-review PASS, upstream exact-head CI 20/20.
- NC-10F Draft PR #41: `1002ef74d34df4bde21599af4f6d3b0a072bfa1c`; independent scoped correctness source-review PASS, upstream exact-head CI 8/8.
- NC-12: runtime-manager foundation already merged to main; do **not** introduce a separate updater or login database.
- Original branches and historical review failures are preserved.

## Conflict resolutions

- `crates/protocol-bridge/src/lib.rs`: nonoverlapping approved request-descriptor and official history RPC changes both retained.
- `experiments/native-renderer/src/engine_probe.rs`: separate E/F event, command, lifecycle and test additions retained; safe decline/approval dispatch and failed-resume `resumed_unverified` gate must both remain. SelectThread in explicit coding mode is rejected because F resume validates **read-only**, not writable permissions.
- `experiments/native-renderer/src/main.rs`: combined approval and durable history state. Kept E's explicit coding flag and four-argument engine start, retained F's authenticated history/reconnect UI, and cleared all stale consent/evidence/prompt state on explicit reconnect. F's three-argument startup and divergent renderer variable were deliberately not copied.

## Cloud acceptance — gates

All integration-head test/CI outcomes must be recorded against the **new** exact SHA. Upstream checks do not qualify this merged tree.

| Gate | At initial candidate |
|---|---|
| Rust protocol-bridge and native renderer tests | NOT_RUN |
| E approval + F resume failure, zero `turn/start` regressions | NOT_RUN |
| NC-12 selection/rollback and Svelte/Tauri security tests | NOT_RUN |
| Windows hosted native build | NOT_RUN |
| Integrated NC-12 executable selection from native launch | **NOT_VERIFIED**; task cannot be called DONE until proven |
| Live authenticated Codex App Server acceptance | NOT_RUN |
| Owner physical Windows acceptance | NOT_RUN |
| Independent security/integration review | PENDING |

## Required local Windows handoff (later phase, never infer from hosted CI)

1. Check out exact independently-approved NC-10G SHA and build the native binary on the owner's real Windows system. Record source SHA, executable SHA-256 and official Codex executable identity without secrets.
2. Test real official signed-in thread start/stream/terminal; explicit selected existing completed thread and active-turn resume with zero duplicate execution.
3. Exercise request-id/command/cwd/scope-bound Allow once, Decline and complete file change diff, including a negative same-id/different-command case.
4. Stop during streaming and pending approval; crash owned engine and reopen; check no auto replay or stale approval.
5. Verify official NC-12 system runtime default, optional verified managed runtime and rollback, plus process-tree ownership and profile isolation.
6. Save sanitized timestamps, commands redacted from private file content, SHA-pinned logs, screenshots and terminal statuses. Report PASS / FAIL / NOT_RUN separately.

**No merge** of Draft PR #26, #40, #41 or NC-10G before independent checker and downstream acceptance.

## NC-12 native runtime selection adapter (additional candidate checkpoint)

Native renderer `--runtime-store=<absolute existing NC-12 managed-codex store>` opts into **the same owned NC-12 runtime journal** used by the production Tauri host. The renderer never stages or activates a managed version; NC-12 `Manager::engine_config` verifies the requested binary and the existing shared supervisor owns the child. The system Codex path remains explicit `--codex-exe` and is prevalidated as the fallback. Default without `--runtime-store` remains System. A failed managed session can roll back only **after** confirmed supervisor tree stop and session gate teardown, and there is no automatic engine/prompt replay. This is code/CI qualification, NOT authenticated real runtime acceptance; the actual owner store and switching remain NOT_RUN. The NC-12 local store path must be supplied by the owner (never logged).

After any future correction, update this section with final exact HEAD CI and independent review evidence. The earlier `NOT_VERIFIED` entry above represents the initial merge checkpoint, not the corrected source.
