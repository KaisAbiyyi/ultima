# NC-03 · Minimal Svelte UI prototype — acceptance

**Scope:** design system + presentation mock only. No live Codex App Server, filesystem, approvals, authentication or native Windows performance claims.

## Run locally (optional owner preview)

```bash
npm install
npm run dev
npm run check
npm test
npm run build
npx playwright install chromium
npm run test:e2e
```

Cloud acceptance is GitHub Actions `NC-03 Svelte UI prototype` on the exact PR HEAD.

## Demonstrated

- Responsive single-workspace shell with project rail, composer and virtualized synthetic history.
- Dark/light token palette, typographic hierarchy, keyboard focus and reduced-motion handling.
- Keyboard: Ctrl/Cmd+K project search, Ctrl/Cmd+N blank thread, Ctrl/Cmd+L composer, Enter send, Shift+Enter newline, Escape dismiss mobile navigation.
- Deterministic 10,000-message fixture; a bounded DOM window and safe fixture limits. Reset button reloads fixture.
- Local simulated incremental response (no real agent call), cancel/stop and timer teardown.
- Automated fixture, virtualization and shortcut unit tests, plus Chromium desktop/mobile screenshots and interaction tests.

## Explicit limitations / risks

- The simulated conversation content is synthetic and does not persist. Project list is mock-only.
- Fixed-height rows clamp long text to two lines. Real Markdown and variable-height virtualized messages belong to NC-06/NC-08.
- Screenshot artifacts are generated in GitHub CI; screenshots are not proof of Windows/WebView2 usability or process-tree memory.
- No real streaming, tool approval, App Server auth or filesystem integration. No claims are made about native Windows RAM, p95 frame latency, IME or screen-reader acceptance.
- Only system font fallbacks are used; no font CDN, tracking, network request, or icon library is required.

## Verification checklist

- [ ] `npm run check` passes on exact PR HEAD.
- [ ] `npm test` passes on exact PR HEAD.
- [ ] `npm run build` passes on exact PR HEAD.
- [ ] Playwright keyboard, mobile, theme, 10k DOM bound tests pass on exact PR HEAD.
- [ ] Screenshot artifacts attached to same CI run.
- [ ] Exact diff reviewed for XSS, resource bounds, timers/listeners and permission boundaries.
- [ ] Windows native acceptance: **NOT_RUN** (not this cloud task's gate).
- [ ] Owner product acceptance: **NOT_RUN**.
