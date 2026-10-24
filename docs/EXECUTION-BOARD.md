# Necessary Codex — Task execution board

**Scope:** Canonical task DAG and governance. Live acceptance state must come from verifiable GitHub PRs/CI and Notion task properties, not a static snapshot in this file. All tasks created **Ready** or **Blocked**, none claimed/done at roadmap creation.

## Rules borrowed from Mind Place / CoSoS

- Claim first (task ID, unique agent/session owner, CLOUD/LOCAL/OWNER environment, starting SHA, branch, resource lock). Never double-claim; GitHub comment is not an atomic lock.
- `Now`: all prerequisite-eligible Ready tasks plus legitimately Claimed/Review/local/owner action items, unlimited count, sorted by **Priority** high to low. `Up Next`: only direct successors of a Now task whose other prerequisites are Done or also Now. `Last Done`: max five latest accepted.
- Cloud can implement and test on CI. Real Windows latency, RAM, IME, WebView2 lifecycle, usability and packaging require native evidence; owner approval is distinct.
- Self-review every exact commit/SHA for diff, security, races, failure modes, tests, license obligations; merge only when all required current task gates pass. Recompute focus lanes and Notion+GitHub status immediately.

| ID | P | Phase | Env | AND dependencies | Exclusive lock | Initial status | Outcome |
|---|---|---|---|---|---|---|---|
| NC-01 | P0 | 01 | Cloud | — | protocol | Ready · Now | Upstream RPC handshake and schema |
| NC-02 | P0 | 01 | Cloud/Local | — | benchmark | Ready · Now | Repeatable CPU/RAM/latency harness |
| NC-03 | P0 | 01 | Cloud | — | ui-tokens | Ready · Now | Responsive minimal UI prototype |
| NC-04 | P0 | 02 | Cloud | NC-01 | runtime | Blocked · Up Next | Rust App Server supervisor |
| NC-05 | P0 | 02 | Cloud | NC-01 | protocol-adapter | Blocked · Up Next | Typed bounded event bridge |
| NC-06 | P0 | 02 | Cloud | NC-03, NC-04, NC-05 | ui-shell | Blocked · no lane | Functional threads/chat UI |
| NC-07 | P0 | 02 | Cloud | NC-06 | approval-diff | Blocked · no lane | Approval/tools/diff |
| NC-08 | P0 | 03 | Cloud | NC-02, NC-06, NC-07 | perf-ui | Blocked · no lane | Virtualized bounded history |
| NC-09 | P1 | 03 | Cloud | NC-06, NC-07 | ux | Blocked · no lane | Polished accessible keyboard UX |
| NC-10 | P0 | 03 | Local | NC-08, NC-09 | native-benchmark | Blocked · no lane | Exact-SHA native performance gate |
| NC-11 | P0 | 04 | Cloud | NC-05 | upstream-compat | Blocked · no lane | Upstream release compatibility |
| NC-12 | P0 | 04 | Cloud | NC-04, NC-11 | runtime-update | Blocked · no lane | Safe updates and rollback |
| NC-13 | P0 | 04 | Cloud/Local/Owner | NC-10, NC-12 | release | Blocked · no lane | Release readiness and approval |

**Projection note:** Initial Up Next should show NC-04 and NC-05 only; NC-11 depends on NC-05 and therefore is not a direct successor of initial Now. Source DAG above is authoritative over prose snapshots.

## Completion receipt template

`TASK:` / `OWNER:` / `ENVIRONMENT:` / `BASE_SHA:` / `HEAD_SHA:` / `BRANCH/PR:` / `RESOURCE_LOCK:` / `CLOUD_CI:` / `WINDOWS_NATIVE:` / `OWNER_ACCEPTANCE:` / `DIFF/SECURITY SELF-REVIEW:` / `PASS|FAIL|PARTIAL|BLOCKED|NOT_RUN:` / `FOLLOW-UP:`

Do not print PASS without exact evidence. Never mutate Windows D: workspace from cloud units.
