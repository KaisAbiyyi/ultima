# NC-05 — typed, bounded App Server bridge

This crate is a **library** only. It owns no subprocess, filesystem, credentials, network listener, UI or permission grants. NC-04 owns the process and both stdio streams; NC-06/07 own presentation and consent. Upstream protocol pinned at Codex `0.164.0` by NC-01.

## Host contract

1. Construct `Broker::new(Limits::default())` and `LineDecoder::new(limits.line_bytes)` **per App Server process**.
2. Call `begin_initialize()`, drain `take_outbound()` to **stdin**, pass stdout chunks into `LineDecoder::feed(chunk, |line| broker.ingest_line(line))`. Send `acknowledge_initialized()` only after successful initialization reply, then start normal `call` operations.
3. Poll `take_event()` to deliver replies, streamed notifications and explicit approval prompts to the UI. Unknown future notifications are intentionally dropped; unknown server requests get JSON-RPC method-not-found errors (never auto-accepted).
4. `cancel_local(id)` only removes queued commands or detaches their response. For an already sent generation, call `interrupt_turn(thread_id, turn_id)` to actually signal Codex and wait for its terminal event.
5. Approval events **never authorize execution**. `reply_approval(id, Accept, true)` requires an affirmative host-side user decision; `decline_approval` is the safe default. Reject session-scoped/unknown decisions.
6. **Any** decoder, parser or backpressure error must terminate this owned child session, stop both stdio pumps, and discard all buffered events/approvals. Do not continue consuming input and do not interpret a dropped approval as consent. After the child has been joined, `reset_for_new_process()` permits a new handshake with monotonic IDs.

### Bounds

Default caps: 1 MiB UTF-8 JSONL line, 128 pending outbound requests, 32 incoming approvals, 128 queued events / 4 MiB payload, 128 outbound frames / 4 MiB. Calls return explicit `Backpressure`; no silently dropping a **known** event or queued approval. Decoder streams one line per callback even when fed a massive stdout chunk.

The typed adapter intentionally recognizes only the NC-01 stable request subset and known notifications. Schema changes must be validated against official generated App Server schemas in NC-11. This library does **not** claim full schema interoperability with untested upstream versions, native latency budgets or a live authenticated turn.

## Tests

`cargo test --manifest-path crates/protocol-bridge/Cargo.toml --all-targets` on hosted Linux and Windows. Tests replay synthetic JSONL for handshake, ID routing, interruption/cancellation, consent denial, schema evolution, framing, and memory caps. Real tool approvals and native product benchmarks: **NOT_RUN**.
