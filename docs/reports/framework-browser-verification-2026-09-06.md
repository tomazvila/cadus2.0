# Framework browser verification — 2026-09-06
## Result
The Playwright acceptance gate now includes a dedicated 2.0 journey in real Chromium:
1. approved instruction renders before application;
2. one integrated scenario renders as a whole task;
3. a hint stays answer-free and the response shows field-level feedback;
4. a distinct unseen seven-day assessment starts without repeated instruction;
5. the session closes and the retention report shows the independent result.

The walk also checks the built KaTeX output. Each integrated scenario must contain a MathML
tree for assistive technology and a visual HTML layer marked `aria-hidden="true"`.

## Commands and evidence
Run from `web/` in the isolated `framework/browser-e2e` worktree at base `3a3ecd3`:
- `node e2e/run.mjs journey --port=4183`: passed 7 steps with no console errors, page
  errors, failed requests, or HTTP failures; four MathML trees were observed on the first
  integrated task.
- `node e2e/run.mjs acceptance --port=4184`: passed the blank-page, raw-LaTeX, and
  wrong-problem negative controls, the clean compatibility walk, and the 2.0 journey.
- `npm run types`: passed.
- `npm run lint`: passed.
- `npm test`: 62 files and 922 tests passed.

The browser runs in `mcr.microsoft.com/playwright:v1.62.1-noble`; the runner selected host
networking after its bridge probe. Each fixture server was stopped by the runner.

## Evidence boundary
The 2.0 walk uses a deterministic in-browser API fixture so browser rendering and UI
sequencing are repeatable. It does not claim database persistence, scheduler-time passage,
or production deployment. The Rust integrated-journey and recovery suites own those
boundaries.
