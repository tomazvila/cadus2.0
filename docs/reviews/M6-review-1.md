# M6 review, rounds 1 and 2 (2026-08-30)

Workflow `wf_5615dfd6-c52` (resumed once after a process exit): six lenses (authoring gates, the C6 admin path, SPA session correctness, SPA traps/security/a11y, packaging + CI, test quality), one refuter per finding, major and blocker only. Round 1: 21 raised, 17 confirmed. Round 2: 19 raised, 9 confirmed. Total 40 raised, 26 confirmed, 20 distinct defects. Reviewed tree: `main` at `d5b6def` (22 units, 1,754 Rust tests, web check green).

## Rulings and fix units

### FIX-M6-A1 (F1, F6, F26)

- **F1** [blocker] `crates/worker/src/authoring/job.rs:595` (T3, C6, L4, L5) — A teach or hint_ladder body shared by two knowledge points collides on the content_store primary key: the second knowledge point stores nothing, the batch reports it stored, and every later pass pays again

- **F6** [major] `crates/worker/src/authoring/job.rs:508` (C6, T3) — A rejected digest is re-authored on every pass, an identical re-author is silently swallowed, and the pass reports it as stored

- **F26** [major] `crates/store/src/content.rs:302` (C6) — Reject's digest binding is unpinned: a reject that rejects every row in content_store passes the whole M6 Rust suite


**Ruling:** Fix (blocker). The content_store key is `(kp_id, kind, body digest)`: the stored `digest` column becomes `sha256(kp_id || kind || body)` computed in ONE function that every writer and every reader uses, so two knowledge points that earn one body store two rows. A duplicate insert (same kp, kind, body) is reported as `Outcome::Duplicate` and counted in a new `duplicate` field of the batch report, never in `stored`. A knowledge point whose only rows are `rejected` is re-authored, but a re-author that reproduces a rejected digest is `Outcome::Rejected { same_body: true }`, counted as `declined`, with the reviewer reason in the decline record. `content::reject` gets a test that seeds three rows and pins that exactly one row changes status; mutation: widen the WHERE, confirm red.

### FIX-M6-B (F2, F15, F25)

- **F2** [major] `crates/core/src/instruction.rs:306` (C1, L5) — The hint-ladder gate reads exemplar answers only, so a rung that names the answer of a served template instance is stored and served verbatim

- **F15** [blocker] `crates/core/src/instruction.rs:307` (C1, L5, L4) — The L5 hint gate and the L4 teach gate never read a rendered template instance, so C1 is unenforced for every knowledge point that serves a template

- **F25** [major] `crates/core/src/instruction.rs:308` (C1, R6) — The L5 give-away gate is switched off for 51 shipped knowledge points, and the test that pins the exemption uses a fixture where the exemption is coincidental


**Ruling:** Fix (blocker). The hint and teach gates read the answers of the served material: `InstructionSpec` gains `instance_answers: Vec<String>` filled by the worker from the approved templates of the knowledge point (render every approved template through `cadus_core::template` with the M4 fixed seeds, at least 8 instances per template) plus every exemplar answer. `check_no_answer` compares each rung against every answer in that set with no exemption: the "problem text contains the answer" skip is removed. A rung that names any served answer is rejected with the literal message naming the rung and the answer. Tests: a ladder that names a template instance answer is rejected; the 51 exempted knowledge points now gate (pin the count of shipped knowledge points whose exemplar text contains its answer and assert each rejects a give-away rung).

### FIX-M6-C (F18, F19)

- **F18** [blocker] `crates/worker/src/bin/cadus-worker.rs:162` (T5, T6, T3) — The authoring pass sends the diagnosis output ceiling, and the reasoning cap eats all of it

- **F19** [major] `crates/worker/src/authoring/job.rs:659` (T3) — The zero-call guard for an undecidable answer kind skips the diagnosis kind, which enforces the same rule


**Ruling:** Fix (blocker). Authoring takes its own output budget: `AUTHORING_OUTPUT_TOKENS` (default 4000) and `AUTHORING_REASONING_MAX_TOKENS` (default 2000), never the diagnosis values; the request body to the fake server pins `max_tokens` 4000 and `reasoning.max_tokens` 2000 for an authoring call and 600/600 for a diagnosis call. The zero-call guard for a non-templatable answer kind covers every kind whose gate refuses that kind (`template` and `diagnosis`): a `proof` topic makes zero calls for both and one decline record each.

### FIX-M6-D1 (F7, F10)

- **F7** [blocker] `web/src/views/session/Session.tsx:326` (DD-3/P1, trap-T5, O3) — A drill's leftover countdown blank-submits the DD-3/P1 re-solve and rewrites the correct assisted pass into a permanent miss

- **F10** [major] `web/src/hooks/useCall.ts:98` (F-37-1c, F-36-1, F-36-1b, T12, O3) — A stale useCall Retry re-posts a graded problem_id past the phase gate, and the refusal toast re-arms itself forever


**Ruling:** Fix (blocker). The rework branch stops and clears the per-problem countdown before it returns the view to `ready`; the auto-submit effect never fires for a problem in the re-solve state, and a blank auto-submit never runs after a graded reply. The useCall Retry re-enters through the phase gate (`gate.tryEnter`): a Retry after the problem was graded is refused with a non-actionable toast, and a refusal toast is never actionable. Tests with fake timers pin both: a drill with 3 s left, an assisted-correct reply, then 5 s of time: no second POST.

### FIX-M6-D2 (F8, F21, F22)

- **F8** [major] `web/src/app/Root.tsx:143` (W-C3, O3) — The topbar's Home and Map controls are inert on /ops and /review, so both operator screens are dead ends

- **F21** [major] `web/src/app/Root.tsx:106` (O3, S13 router (no dead end)) — A mid-session 401 keeps the previous account's screen, and the next sign-in writes to its task

- **F22** [major] `web/src/main.tsx:144` (O3) — A signed-in learner who opens a password-reset link lands on the dashboard, and the token is stripped


**Ruling:** Fix. `Root` derives the screen from ONE source: the history location, updated with `history.pushState` on every navigation and read on `popstate`; Home and Map on /ops and /review navigate. A 401 (`onUnauthorized`) resets `view` to the boot default and drops every task of the previous account, so the next sign-in lands on the dashboard. A reset link opened by a signed-in learner shows the reset card first (the token is read before the session resolves and is not stripped until the card consumes it).

### FIX-M6-E (F5, F9, F16, F20)

- **F5** [major] `web/src/views/admin/Review.tsx:364` (C6) — Approve and Reject stay live when the document pane failed to load, so an irreversible C6 decision is taken on a body that never rendered

- **F9** [major] `web/src/views/Dashboard.tsx:122` (O3, spec-4.5-a11y, spec-4.2-tokens) — The Switch-course dialog is not a dialog: no role, no aria-modal, and no modal surface

- **F16** [major] `web/test/admin.test.tsx:410` (C6) — No test binds a review decision to the document in the pane; every approve and reject test decides on the default selection

- **F20** [major] `web/src/views/admin/Ops.tsx:52` (T3, A6) — The T3 cost table on /ops sums at most 200 documents and prints no truncation notice


**Ruling:** Fix. Approve and Reject are disabled until the document pane rendered the body for the SELECTED digest, and both post that digest (a test selects the second row, lets the pane fail, asserts both buttons disabled; another selects the second row, lets it load, approves, and asserts the posted digest is the second). The course picker is a `Modal` (role dialog, aria-modal, focus trap, Esc). The /ops bill reads every page of `listContent` (follow the cursor or page parameter of the R5 route; if the route has none, add a `page` query in crates/web/src/admin.rs — the ONLY crates/** edit this unit may make) and shows a literal "n documents" count; a test with 250 rows pins the total.

### FIX-M6-F (F11/F12/F24, F13/F23, F14)

- **F11** [blocker] `scripts/deploy.sh:149` (D9, O3) — scripts/deploy.sh prints DEPLOY OK while the only ingress crash-loops

- **F12** [major] `scripts/deploy.sh:137` (D9, O3) — An upgrade never applies a Caddyfile-only change: compose leaves the edge container in place

- **F24** [major] `scripts/deploy.sh:131` (O3) — scripts/deploy.sh --no-caddy rebuilds the SPA image and never restarts the edge, so an upgrade serves the previous commit's bundle against the new API and prints DEPLOY OK

- **F13** [major] `scripts/check_ops.sh:748` (D9, O3) — check_ops.sh check (j) never asks Caddy whether the Caddyfile adapts

- **F23** [major] `scripts/check_ops.sh:809` (O3) — check_ops (j) reads the Caddyfile's text but binds nothing about the edge that actually runs: two one-line deletions pass all 14 checks and take the site down or expose /api/ready

- **F14** [major] `.env.example:20` (W9, O3) — The shipped .env.example serves a Secure __Host- cookie over a plain-http site, so no session ever persists


**Ruling:** Fix (blocker). deploy.sh: the start check covers every started service (caddy included; a restart loop fails the deploy with the container name); a changed Caddyfile is applied (`docker compose up -d --force-recreate caddy` when the Caddyfile digest changed, or `caddy reload` through exec); `--no-caddy` is removed or redefined so the SPA bundle is always recreated with the API (state which in the runbook). check_ops (j): runs `caddy validate` in the caddy image against deploy/Caddyfile; asserts docker-compose.yml mounts that file into the edge service; asserts the @ops block guards /api/ready and /metrics. .env.example: SITE_ADDRESS and CADUS_WEB_INSECURE_COOKIE are documented as a pair with an http example that sets INSECURE_COOKIE=1 and an https example that does not; check_ops asserts the pair rule on .env.example. Mutate each guarded line and confirm red.

### FIX-M6-G (F17)

- **F17** [major] `web/test/api-contract.test.ts:21` (S2, F-F6-1) — The S2 route-table contract test cannot see a route added to the service, and its docstring states the opposite


**Ruling:** Fix. The route table has an oracle: a Rust test dumps every route of `create_app` (method + path) to a committed JSON fixture; `cargo test` fails when the fixture differs from the router (regenerate with an env flag); the TypeScript contract test reads that fixture and asserts every route has a typed method and no method names a route the fixture lacks. Mutation: add a route to create_app without the fixture, confirm the Rust test red; remove a method, confirm the TS test red.

### FIX-M6-A2 (F3, F4)

- **F3** [major] `crates/worker/src/authoring/job.rs:479` (C1, C6) — Trap T1 (repair_latex_escapes) is not ported: a JSON-eaten backslash reaches content_store on every kind

- **F4** [major] `crates/worker/src/authoring/prompt.rs:845` (C6) — The prompt digest is computed and pinned but never stored, so the C6 re-authoring workflow the spec names has no input


**Ruling:** Fix (phase 2, after A1, B, C, G merge). Port trap T1: `repair_latex_escapes` runs on every text field of the decoded tool arguments before any gate (statement, answer, hints, concept, steps, distractor answers and notes); a control character that remains after the repair rejects the body with a literal message. The prompt digest is stored on the row (`content_store.prompt_digest`), written by the insert and read by the CLI: `cadus-worker author --stale` lists approved rows whose prompt_digest differs from the current digest of their kind, and the batch loop re-authors those first; a test edits the prompt, runs the pass, and pins one re-authored row and the old row still approved (spec 2.2: a prompt edit does not unapprove).


## Not fixed

None. Every confirmed finding maps to a fix unit.

## Flagged items with no finding

Of the eleven items the unit agents flagged, the reviewers confirmed (1) the T1 repair, (2) the hint gate, (4) the admin gate block indirectly through F5, (5) the digest collision, and (9) `.env.example`; the rest raised no major finding.
