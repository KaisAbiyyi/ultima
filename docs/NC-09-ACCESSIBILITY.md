# NC-09 — Keyboard-first Accessibility and Visual QA

**Owner/session:** ChatGPT GPT-6 / `ultima-nc09-20261024-2346-wib-cloud-01`  
**Environment:** GitHub Cloud only · **Lock:** `ux`  
**Base:** `348837a5f0d1af2ef2d16db1a6b5d0a6a9ab8c73` (NC-06 and NC-07 merged).

## Acceptance surface

- Global `Ctrl/⌘+K`, `Ctrl/⌘+L`, `Ctrl/⌘+N`, and Escape follow keyboard focus semantics. Escape closes navigation and restores focus to the opener.
- The mobile off-canvas sidebar is `inert` (not keyboard-/screen-reader-reachable) while closed. The toggle announces `aria-expanded` and controls the actual sidebar.
- IME is never sent as a prompt: `isComposing`, `keyCode=229`, Process/Dead key events, and the immediate composition-end commit interval are ignored for Enter-to-send and project-add.
- Selected project/thread expose `aria-current`; theme toggle exposes `aria-pressed`; expandable tool output/diff toggles announce `aria-expanded`. User-visible diffs render *only* on demand, with horizontal and vertical keyboard-scrollable regions; no diff content transformation or authorization-path changes.
- CSS preserves visible focus in forced-colors mode and error/warning contrast in both themes. No animations, browser process, new dependencies, telemetry or unbounded cache.
- Playwright generates six screenshots for desktop 100%/200% device pixel ratio and mobile 200%, light/dark. These are **inspection artifacts**, not a native Windows/WebView2 screenshot or screenshot-diff pixel baseline.

## Cloud checks

`npm run check`, `npm run test`, `npm run build`, and Playwright Chromium tests on Ubuntu; hosted Windows Svelte check, tests and build. PR runs must all PASS on the **exact** latest head before independent checker handoff. Existing NC-06 and NC-07 CI must remain green.

## Manual Windows/owner acceptance — NOT_RUN until measured

1. On physical Windows with a real Japanese/Chinese/Korean IME, type into the composer; Enter accepts IME candidate, never sends. Enter after composing sends once; Shift+Enter inserts newline.
2. Keyboard-only flow: navigation, search, resume, theme, connect, model, approvals, review/decline/allow once. Confirm permission stays fail-closed with missing, incomplete, stale, or cross-turn diff evidence (NC-07).
3. Windows WebView2 at display scaling 100%, 125%, 150%, 175%, and 200% in light/dark; inspect clipping, focus visibility, readable diff and touch targets.
4. NVDA or equivalent screen-reader basics: labels, selected states, expansion states, focus return and nonmodal approval semantics.
5. Measure p50/p95 latency and total process-tree RAM in NC-10. Screenshots/hosted CI do **not** establish native performance or owner UI acceptance.

## Security/performance self-review

No Rust supervisor, protocol adapter, permission decisions or file operations are changed. Permission accept remains NC-07's current thread/turn/preview/acknowledgement gate. `inert` affects hidden navigation only; tool evidence remains bounded and lazy. The test matrix opens only synthetic browser preview (`?desktop=1`); it cannot connect to Codex, read credentials, or approve tools. Dynamic screenshots are finite (three viewport-DPI profiles × two themes) and uploaded only as CI artifacts. Owner-native IME/DPI and screen reader performance are explicitly **NOT_RUN**.
