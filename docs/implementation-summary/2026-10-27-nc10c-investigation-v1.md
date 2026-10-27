# NC-10C investigation and handoff — 2026-10-27

Status: RECLAIM / investigation only. No product implementation or acceptance.

## Identity and claim

- Owner: Codex / ultima-nc10c-20261027-local-01; environment: local Windows.
- Base: `033d10f314db231db6c5039b4d3fd3025f756c98`, PR #26 draft head at claim.
- Branch: `feature/nc-10c-history-20261027`; task lock: `native-history`.
- NC-08 and NC-09 were Done in Notion and closed on GitHub. NC-10G is open and owns native production host integration.

## Findings

- `experiments/native-renderer/src/main.rs` creates at most 10,000 synthetic `ChatRow`s containing short single-line fixture text. Its `show_rows` fixture path has a fixed 49-point row height. It does not exercise long code, diffs, variable heights, insertion, focus or selection.
- The live engine path instead iterates all `live_rows` in a normal scroll area. It retains at most 512 rows and evicts the oldest. It therefore does not establish accessible 10K live history, and its layout work is not viewport-bounded. Do not change eviction semantics without an explicit history/storage contract.
- The engine UI mailbox is independently bounded to 128 events and 512 KiB, with bounded commands. This does not establish a bounded durable history or viewport.
- The renderer README identifies the native application as an experiment without durable history or production integration. NC-10G covers integration; a synthetic implementation can be developed independently, but final production evidence must be requalified after integration. The old pilot cannot satisfy this task's Windows acceptance.
- NC-10B paint callbacks precede buffer swap and compositor presentation; they cannot alone establish the required displayed-pixel latency.

## Verification and interference

- PR #26 at claim was open, draft and mergeable; its exact-head CI still had a native tooling job in progress. No NC-10C CI run exists.
- Started `cargo test --locked --manifest-path experiments/native-renderer/Cargo.toml` from a clean isolated checkout, then interrupted during compilation; no test result. This compilation may have overlapped another session holding NC-10D's `native-benchmark` lock. Any concurrent physical measurements should be invalidated or rerun after settling; actual overlap was not established.
- Windows 10K, wheel/input latency, process-tree memory, correctness tests and independent review: NOT_RUN. No PASS claimed.

## Next exact steps

1. Check the current PR #26 head, task status and `native-history` lock; reclaim before work. Coordinate physical sampling with the holder of `native-benchmark` and do not sample concurrently.
2. Design a viewport-bounded variable-height history using stable row identity, explicit insertion/eviction semantics and bounded measurement caches. Do not substitute fixed-height clipped long text for readable long code/diffs. Exercise synthetic 10K messages without private data.
3. Add meaningful automated checks for navigation, insertion-preserved scroll anchor, selection and composer focus, plus bounded layout/cache and queue behavior. Implement and test against the intended live/native path; preserve separate fixture and production claims.
4. On the exact Windows release SHA, collect independently observable wheel/input p50/p95 and process-tree memory with the complete synthetic workload; record environment and raw evidence. Requalify the integrated production build after NC-10G. Mark Review only after all scoped checks and independent review, never Done from hosted CI.
