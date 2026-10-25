# NC-08 — Bounded Rendering & Event Lifecycle (Cloud)

## Scope and contracts

Ultima retains Codex as the authoritative store for thread history and authorization. The desktop renders a **window** of messages via fixed 88 px rows and 6-row overscan; message content remains escaped plain text (no HTML injection or expensive Markdown highlighter). Long messages have a lazy source reader in 4096-character pages, so Markdown/code remain escaped and readable without loading a costly parser or highlighter. This does not claim a complete Markdown renderer or a native performance measurement.

- Maximum in-memory UI history is `MAX_RENDER_HISTORY=5000` rows **and** `MAX_RETAINED_TEXT_CHARS=8 Mi chars`. Oldest snapshots are evicted, not durable Codex history. Historical individual preview is capped at 64 Ki chars with a visible truncation marker. Live reply limit remains 2 Mi chars.
- Stream delta updates are grouped into one animation-frame commit, preserving the accepted delta ordering; pending deltas are capped at 128 Ki chars. Overflow causes an explicit disconnect rather than a silent truncated answer.
- On disconnect, connect, project/thread switch, and unmount, pending render frames are cancelled and their memory discarded. Turn completion drains the frame once before clearing active turn identity.
- User scroll is not forced to the bottom while reading older rows. Follow-tail resumes when within 128 px of the end or a new turn begins.
- NC-07 safety boundaries stay unchanged: permission previews must match the exact active thread and turn; tool evidence is already bounded at 16 entries with 32 Ki output / 64 Ki diff and remains lazy-rendered. Any not-yet-reviewed approval remains decline-only.
- Additional 32-unanswered-request cap prevents unbounded UI `pending` growth. Existing generation fence and 16 early replies remain.

## Cloud acceptance evidence

`npm run check && npm run test && npm run build && npm run test:e2e` are CI gates. Unit stress exercises 10k tiny rapid deltas before one paint, historical 20k turns, size caps, cleanup and overflow. On Linux hosted runners this demonstrates bounded JS UI logic, not WebView2 Windows process-tree RSS. The existing NC-07 Linux/Windows Rust security suites must continue to pass on the exact NC-08 HEAD.

**NOT_RUN until NC-10/owner:** Windows native 10k history wheel/input p95, event-to-painted p95, process-tree memory delta and 60-minute real agent/WebView2 leak profile. Cloud tests are not a substitute.
