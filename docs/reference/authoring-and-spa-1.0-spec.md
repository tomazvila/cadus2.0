# Cadus 2.0 — M6 specification (authoring pipeline, review tooling, SPA)

Requirement IDs: A2, C6, O3, and the supports C1, C5, L4, L5, T2, T3, T5, T6.
Reference implementation: `/home/deploy/dev/cadus` (1.0, Python). Every claim below cites
`file:line` in that tree or in this one.

Owner decisions that bind M6 (`docs/DECISIONS.md:7,9`): **O3** — the frontend is a React +
TypeScript rewrite, and the 1.0 SPA is a design reference only (Tokyo Night). **O2** — DeepSeek
V4 through OpenRouter now (`deepseek/deepseek-v4-pro`), a local Qwen 3.6 later; T4 caps default
to `0` = unlimited; T5 stays on, because it bounds latency.

---

## 1. Where things live

### 1.1 The 1.0 authoring path

| Item | Location |
|---|---|
| Authoring system prompt | `cadus_web/prompts.py:441` (`TEMPLATE_SYSTEM`) |
| Tool schema | `cadus_web/prompts.py:314` (`TEMPLATE_SCHEMA`), contract at `:386` |
| User message + retry feedback block | `cadus_web/prompts.py:734` (`template_user`) |
| Prompt digest (retires stored templates) | `cadus_web/prompts.py:779` (`template_prompt_digest`) |
| Model call | `cadus_web/openai_engine.py:507-527` (`author_template`) |
| Gate | `cadus_web/problem_templates.py:839` (`gate_template`) |
| Authoring loop | `cadus_web/problem_templates.py:1384` (`_author_one`) |
| Serve path with the bank | `cadus_web/problem_templates.py:1298` (`generate`) |
| Stored document + digest + approval | `cadus_web/problem_templates.py:1071,1040,1056` |
| Cache key | `cadus_web/problem_templates.py:1129` (`template_key`) |
| Store (`tutor_cache`, `op='template'`) | `cadus_web/problem_templates.py:1188` |
| Review CLI | `scripts/review_templates.py:1-237` |
| Runbook | `docs/WEB_SERVICE.md:393-441` |
| Metrics label set | `cadus_web/metrics.py:117-148` |
| Knobs | `cadus_web/config.py:79-107` |
| Tests | `tests/test_problem_templates.py` (1,553 lines) |

Teach pages and hint ladders have **no** authoring path in 1.0: both are live model calls on
the request path, and the grade cache excludes them on purpose
(`cadus_web/grade_cache.py:451-467`). M6 creates that path.

### 1.2 The 1.0 frontend

| Item | Location |
|---|---|
| Shipping SPA (vanilla, served) | `static/` — `app.js` 186 lines, `api.js` 155, `ui.js` 278, `views/*.js` 2,298 (`docs/FRONTEND_ARCHITECTURE.md:36-37`) |
| Design tokens | `static/app.css:5-57` |
| Design context (Tokyo Night) | `CLAUDE.md:84-124` |
| React + TypeScript port (built, not cut over) | `web/src` — 6,126 lines across 33 files; `web/src/main.tsx:32-38` states it |
| Port plan and invariant table | `docs/FRONTEND_ARCHITECTURE.md:51-90` |
| Invariant gate | `web/test/invariants.json`, `web/scripts/check-invariants.mjs` |
| Browser click-through | `web/e2e/{demo,authed,flows}.mjs`, `web/e2e/README.md` |
| Graph view record | `docs/GRAPH_VISUALIZATION_PLAN.md` |

**The largest finding of this survey.** 1.0 already holds a complete React 19 + TypeScript SPA
under `web/`, with Vitest, MSW, `vitest-axe`, an invariant gate, and Playwright click-through
scripts, and its `package.json` names the exact stack O3 asks for (`web/package.json:26-45`).
M6 therefore starts from `web/` as the design and idiom reference — not from `static/` — and
rewrites the API layer against the 2.0 contract.

### 1.3 What 2.0 already holds

Template document types, constraint language, renderer, and the 28-check gate live in
`crates/core/src/template/` (`constraint.rs`, `document.rs`, `domain.rs`, `draw.rs`, `eval.rs`,
`gate.rs`, `render.rs`), specified at `docs/reference/serving-1.0-spec.md:115-201` and
`docs/plans/M4.md:50-51`. `content_store` carries C6 approval by digest and the T3 columns
(`migrations/0005_content.sql:11-24`; grants at `docs/SCHEMA.md:51,62`). The model client, the
T5 defaults, and the T6 `model_call_log` come from M5 (`docs/plans/M5.md:20-33`,
`migrations/0005_content.sql:51-68`). The HTTP contract the SPA consumes is
`docs/reference/web-service-1.0-spec.md:34-118`.

---

## 2. The authoring pipeline

### 2.1 What 1.0 ran

**The prompt.** `TEMPLATE_SYSTEM` (`prompts.py:441-498`) asks for the reusable structure, not
one problem. Its nine operative rules, in order: mirror the exemplars in type, step count, and
phrasing and turn their values into `{placeholders}` (`:449-453`); make every instance a good
problem, with no division by zero, no negative length, and no answer of 0 or 1 that gives the
method away (`:454-459`); keep `answer_expr` exact over the whole domain in SymPy syntax, never
a decimal approximation (`:460-465`); work the samples **by hand**, because the server
re-derives them and discards the template on one disagreement (`:466-470`); cover both ends of
every int parameter, every value of every choice parameter, and one crossed corner per pair of
int parameters (`:471-482`); produce at least a dozen distinct problems (`:483-486`); answer to
a real number in every instance (`:487-491`); double every literal brace (`:492-494`); and keep
the answer and the method out of the statement — Hard Rule 1 (`:495-498`).

Rules 5 and 9 carry two verbatim incident reports inside the prompt, which is why the prompt
reads as instruction rather than as policy: `b**a` authored as `a**b` graded a correct learner
wrong on 18 of 30 problems (`:472-476`), and `{a} {op} {b}` with `answer_expr = 'a + b'` served
a wrong answer to half of every learner's problems (`:477-482`).

**The tool schema.** `TEMPLATE_SCHEMA` (`prompts.py:314-383`) is a plain JSON-Schema dict:
`additionalProperties: false`, required `["text", "params", "answer_expr", "solution_expr",
"samples"]`; each sample is `{params: object, expected: string}`. `samples` never reaches
storage — it is the verification oracle only (`problem_templates.py:929-931`).

**The user message.** `template_user` (`prompts.py:734-777`) renders topic, answer kind, KP id,
difficulty target, constraints, and the exemplars. On a retry it appends the literal block:

```
YOUR PREVIOUS ATTEMPT WAS REFUSED. The server's exact reason was:
    <the TemplateRejected message>
Fix that specifically. Do not restate the same template — change the
domains, the samples, or the expression so the reason no longer applies.
```

(`prompts.py:766-774`.)

**The loop.** `_author_one` (`problem_templates.py:1384-1428`):

Call `author_template(spec, feedback)` with `feedback = None`; run `gate_template`; on success
`put` the document with `approved: false`. On `TemplateRejected`, count
`PROBLEM_TEMPLATE{result="rejected"}`, log topic/KP/attempt/reason, set `feedback = str(exc)`,
and retry. `_AUTHORING_ATTEMPTS = 2` (`:178`) — one call plus **one** re-prompt. After the
second refusal, a decline marker occupies the slot (`:1030`), so no later serve pays for the
same doomed knowledge point.

**The bank.** `BANK_TARGET = 3` slots per key (`:156`). A serve with a full approved bank picks
one template at random and instantiates locally — zero model calls. A serve that finds the bank
short authors exactly **one** template, never a burst (`:1298-1326`). A pending or declined
slot counts as occupied, so nothing re-authors it (`:1352-1374`).

**The gate.** `gate_template` (`:839-940`) refuses in this order, with these literal messages:

| Check | Message (excerpt) | Line |
|---|---|---|
| kind | `answer kind {kind} is not symbolically decidable` | `:852` |
| empty fields | `template text is missing or empty` / `answer_expr is missing or empty` | `:857,859` |
| no params | `a template needs at least one parameter` | `:863` |
| bad name | `parameter name {n!r} is not an identifier` | `:867` |
| shadows a function | `... collides with a SymPy function the answer expression may call` | `:872` |
| shadows the unknown | `... collides with the unknown an expression answer is written in` | `:882` |
| domain shape | `an int domain needs integer 'low' and 'high'`, `int domain {l}..{h} is empty`, `... exceeds MAX_DOMAIN_SIZE`, `a choice domain needs a non-empty 'values' list`, `... exceeds MAX_CHOICES ({24})`, `choice values must be strings or integers`, `unknown domain kind {k!r}` | `:491-510` |
| placeholders | `text uses undeclared parameters {...}` | `:517` |
| braces | `text has an unescaped brace at index {i} (...) — literal LaTeX braces must be doubled` | `:520` |
| dead params | `parameters {...} are declared but never used` | `:906` |
| expression names | `answer_expr references unknown names {...}` | `:572` |
| space size | `the declared domains produce only {n} distinct problem(s); at least {12} are needed ... (Hard Rule 4)` | `:914` |
| sample shape | `a template needs worked samples to verify it`, `a sample was not an object`, `a sample needs 'params' and a scalar 'expected'`, `sample binds {...}, template declares {...}` | `:972-985` |
| sample agreement | `answer_expr gives {c!r} for {b} but the sample claims {e!r} — the expression does not compute the stated answer` | `:991` |
| sample inside domain | `sample {i} binds {n}={v!r}, which its own domain cannot produce — a sample outside the domain verifies nothing` | `:640` |
| choice coverage | `no worked sample uses {n}={...!r} — every choice must appear in a sample, or the expression is unverified for it` | `:649` |
| edge coverage | `no worked sample uses the {low\|high} end of {n} ({v}) — the edges are where an expression stops being right` | `:657` |
| crossed corner | `no worked sample crosses {l} and {r} — one of them at its low end WITH the other at its high end (...). Matching corners are exactly where a swapped-operand expression looks right` | `:698` |
| non-answer | `instance {b} answers {a!r}, which is not a number ({tokens})` | `:720` |
| free symbol | `numeric answer {a!r} for {b} still contains {...} — a parameter is undeclared` | `:946` |
| exemplar envelope | `... but every authored answer for this knowledge point is a whole number` / `... is non-negative — narrow the domains so no instance goes below zero` | `:778,784,790` |
| render | `a rendered problem still contains a placeholder`, `instantiation for {b} produced no answer` | `:830,832` |

Bounds: `MAX_DOMAIN_SIZE = 10_000` (`:160`), `MIN_SPACE_SIZE = 12` (`:174`, equal to
`state.SERVED_TEXT_MEMORY` at `state.py:133`), `GATE_SAMPLES = 200` (`:183`),
`MAX_CHOICES = 24` (`:188`), `EXHAUSTIVE_SPACE_LIMIT = 4_096` (`:217`),
`TEMPLATABLE_KINDS = (numeric, expression)` (`:221`).

**The limit of the gate, proven not asserted.** For any finite set S of forced sample points,
`truth + (boundary factors)·Π_{s∈S}((a−s_a)² + (b−s_b)²)` agrees on all of S and is wrong nearly
everywhere else; seven forced points were honored while 97 of 144 instances stayed wrong
(`:620-632`). That proof is the reason C6 exists: `REVIEW_BEFORE_SERVE = True` (`:1021`)
addresses the consequence, not the probability.

**Measured yields, as 1.0 recorded them.**

Against `deepseek/deepseek-v4-pro` over three knowledge points, **one** yielded a template the
gate accepts (`problem_templates.py:68-72`); that measurement is why
`CADUS_WEB_PROBLEM_TEMPLATES` defaults to **off** (`config.py:96`). Three of four knowledge
points were refused on the first attempt, for reasons that read as instructions — an undeclared
placeholder, samples missing a domain edge, a domain wide enough to answer negative on a
borrowing topic (`:1386-1393`). The saving when a template lands is 200 problems in 86 ms with
zero API calls (`config.py:85`, `REQUIREMENTS.md:99`). The blocked shape is a constraint
**between** parameters: "subtraction with borrowing" needs `a > b`, the deployed model kept
authoring two independent 10..99 ranges, and the re-prompt did not rescue it (`:59-66`).

**The yield lever.** 1.0 states it directly: the rejection message is the lever, because every
reason reads as an instruction and a told retry converts most first refusals into a template
that lasts forever (`:1386-1393`; test at `tests/test_problem_templates.py:775-798`).

**Cost tracking.** 1.0 has none. The only telemetry is a Prometheus counter by result label
(`metrics.py:117-148`). There is no per-KP attempt count, no token count, and no money figure.
T3 is entirely new work in 2.0.

### 2.2 The 2.0 pipeline (A2)

**Shape.** A batch job in `cadus-worker`, never a request handler (R4, L6). The job takes a KP
list and, per KP and per kind (`template`, `teach`, `hint_ladder`, `diagnosis` — the four the
schema allows, `migrations/0005_content.sql:14`), runs:

```
author -> gate -> retry with the literal rejection message -> store `pending` -> human review
```

1. **Author.** One model call per attempt. The request carries the 2.0 document shape:
   statement, `params` (int / choice / rational / decimal), `constraints`, `answer_expr`,
   `solution_sketch`, `hints`, `distractors`, `samples`
   (`docs/reference/serving-1.0-spec.md:118-158`). The prompt ports the nine
   `TEMPLATE_SYSTEM` rules above and adds three: state inter-parameter constraints explicitly
   instead of narrowing domains (`docs/reference/serving-1.0-spec.md:161-176`); write the hint
   ladder so no rung reveals the final answer or the last step (port `HINT_SYSTEM`,
   `prompts.py:635-655`); write each distractor as `{answer, error_tag, note}` with
   `error_tag` inside the controlled vocabulary (`docs/reference/web-service-1.0-spec.md:341`).
2. **Gate.** `cadus_core::template::gate` — the 28 checks of
   `docs/reference/serving-1.0-spec.md:253-326`, which reproduce the 1.0 messages of §2.1
   byte for byte and add grammar membership, canonical round-trip, constraint checks on the
   samples, hint rules, and the satisfying-count space size (`docs/plans/M4.md:51`).
   The gate is pure core code: no network, no DB (R3).
3. **Retry.** On a rejection, re-prompt with the **literal** message, exactly as
   `template_user` does (`prompts.py:766-774`). 2.0 raises the bound from 2 to **5** attempts
   per slot, because 2.0 pays no per-serve penalty for a failed attempt — the pipeline is
   offline, so the 1.0 argument for stopping at two ("a doomed KP paid two model calls per
   serve forever", `:1024-1029`) no longer applies. T3 alerts above 3 attempts; it does not
   stop the job.
4. **Store.** Insert into `content_store` with `status = 'pending'`, `digest` over the body,
   `authoring_attempts`, and `authoring_cost_usd`
   (`migrations/0005_content.sql:11-24`). `cadus_app` holds SELECT only, so the worker writes
   as `cadus_admin` (`docs/SCHEMA.md:51`).
5. **Human review.** §3.

**Bank target.** Keep 1.0's `BANK_TARGET = 3` per KP for `template`
(`problem_templates.py:156`), so one KP does not serve one problem shape forever. `teach` and
`hint_ladder` are one approved document per KP. `diagnosis` is the `distractors` list inside
the template document, not a separate row, except where a KP serves exemplars only.

**T3 — authoring cost per KP.** Every attempt writes one `model_call_log` row with
`purpose = 'authoring'` and `user_id = NULL` (`migrations/0005_content.sql:52-57`). On store,
the job sets `content_store.authoring_attempts` and `authoring_cost_usd` to the sum over that
KP's attempts. A KP above 3 attempts raises an operator alert and appears in the flags
endpoint beside the A6 "no approved template" flag (`docs/plans/M4.md:36`).

**T5 and T6.** Every authoring request sets prompt caching, a reasoning-token cap, and a pinned
provider order (`docs/plans/M5.md:26-28`); the same client serves a local OpenAI-compatible
endpoint through `OPENAI_BASE_URL` (O2). Authoring takes a wider output budget than diagnosis,
because a template carries its worked samples — 1.0 used 2,048 tokens against 1,024 for the
other calls (`openai_engine.py:520`). T6 writes one row per HTTP attempt, a truncation retry
included, with cached and uncached input tokens, output tokens, reasoning tokens, latency,
model id, provider, and cost (`docs/plans/M5.md:31-33`).

**Prompt digest.** Port `template_prompt_digest` (`prompts.py:779-796`). 1.0 hashes system
prompt + tool name + schema and folds the result into the cache key, so a prompt edit retires
every stored template (`:1129-1163`). In 2.0 the digest is a **column or body field** on
`content_store`, not part of the digest key, because the C6 approval binds to content. A
prompt edit therefore marks affected rows for re-authoring; it does not silently unapprove
them.

---

## 3. Review tooling (C6)

### 3.1 What the 1.0 CLI did

`scripts/review_templates.py` — four sub-commands over `DATABASE_URL`:

- `list [--all]` (`:83-113`) — one line per row: state, key prefix, `space_size`, and the first
  64 characters of the text. It ends with a count and, when `0 < approved < BANK_TARGET`, a note
  that a knowledge point serves from its approved slots only.
- `show <key-prefix>` (`:116-163`) — the text, the answer expression, the sketch, the space
  size, each domain, and then **`SAMPLE_INSTANCES = 8` rendered instances with their computed
  answers** (`:52`). The docstring states why: the live failures — `Compute $12 - 70$` on a
  borrowing topic, `What is the opposite of $0$?` — are obvious in the instances and invisible
  in the expression (`:24-28`).
- `approve <key> ...` (`:176-198`) sets `approved: true`, then re-derives and stores
  `approved_digest`, so a later hand-edit in `psql` cannot inherit the approval.
  `reject <key> ... --reason <text>` (`:201-214`) replaces the row with a decline marker; the
  slot stops serving and is not re-authored.

Approval fails closed in three ways (`problem_templates.py:1056-1069`): no `approved` key;
`approved` truthy but not exactly `True`; `approved_digest` absent or not matching
`template_digest`. Rows from a superseded `TEMPLATE_VERSION` are filtered out of the queue as
orphans (`scripts/review_templates.py:74-82`).

### 3.2 The 2.0 design

**An admin-only HTTP surface, plus an SPA screen.** No CLI (`REQUIREMENTS.md:236`).

| Method + path | Request | Response |
|---|---|---|
| `GET /api/admin/content` | `?status=pending&kind=&kp=` | `{items: [{digest, kp_id, kind, status, authoring_attempts, authoring_cost_usd, created_at, summary}]}` |
| `GET /api/admin/content/{digest}` | — | the body, the gate notes, and `instances: [{text, answer}]` — N locally rendered instances |
| `POST /api/admin/content/{digest}/approve` | `{}` | `{digest, status: "approved", approved_at}` |
| `POST /api/admin/content/{digest}/reject` | `{reason}` | `{digest, status: "rejected"}` |

Rules:

- **Admin only.** `users.is_admin` is outside the reach of the runtime role's column grants
  (`docs/SCHEMA.md:46-47`), so the flag is trustworthy. The four routes refuse a non-admin
  session with `403`.
- **Approval binds to the digest (C6).** The route takes the digest in the path and writes
  `status`, `approved_by`, `approved_at` on that row only
  (`migrations/0005_content.sql:16-20`). An edited body is a new digest and a new row, so no
  approval carries over. This replaces 1.0's `approved_digest` re-derivation, which existed
  only because 1.0 kept approval **inside** the payload.
- **`cadus_app` holds no INSERT, UPDATE, or DELETE on `content_store`** (`docs/SCHEMA.md:51`).
  The approve and reject routes therefore run through an admin path, not the tenant-bound
  connection. Name that path explicitly in the unit, and pin it by a test.
- **The rendered instances are the point.** `GET /api/admin/content/{digest}` instantiates the
  pending template with the core renderer and returns 8 instances with computed answers —
  the 1.0 `show` behavior (`scripts/review_templates.py:52,145-155`). A reviewer reads
  instances, not expressions.
- **Gate notes travel with the row.** The gate records which checks it sampled rather than
  walked, the distinct satisfying-tuple count, and the draws spent
  (`docs/reference/serving-1.0-spec.md:197-201`). The review screen shows them, because a
  sampled space is where a bad corner survives.
- **The bank warning carries over.** When a KP holds fewer than 3 approved templates, the list
  response flags it, for the reason `cmd_list` states (`scripts/review_templates.py:100-112`).

**The SPA review screen** (`/review`, admin only): a left list of pending items grouped by KP
with kind, attempts, and cost; a right pane with the rendered instances, the hint ladder, the
distractors, the constraints, the gate notes, and two buttons — Approve, and Reject with a
required reason. Keyboard: `j`/`k` move the selection, `a` approves, `r` opens the reject
dialog. Approve and Reject are the only writes in the SPA that need a confirmation step,
because both are irreversible for that digest.

---

## 4. The SPA (O3)

### 4.1 Screens and routes

1.0 routes by a view name, not by URL (`static/app.js:43-52`; the port keeps it at
`web/src/app/Router.tsx:29-70`), and "URL routing" is a stated non-goal of the 1.0 port
(`docs/FRONTEND_ARCHITECTURE.md:109`). **2.0 adds real URL routing**, because the review screen
and the operator screen are pages an operator links to and reloads.

| Route | Screen | API |
|---|---|---|
| `/login`, `/signup`, `/forgot`, `/reset?token=`, `/verify?token=` | Auth, five modes plus `check-email` and `sent` (`web/src/views/Auth.tsx:19`) | `/api/auth/*`; OAuth start at `/api/auth/oauth/{id}/start` (`web/src/views/Auth.tsx:381-383`) |
| `/` | Dashboard / status | `GET /api/status` |
| `/session` | Session plan and the study loop | `GET /api/session/plan`, `POST /api/session/start\|end` |
| `/session` (in view) | Serve, answer, grade | `POST /api/task/{id}/serve\|answer` |
| `/session` (in view) | Teach | `POST /api/task/{id}/teach` |
| `/session` (in view) | Hint | `POST /api/task/{id}/hint` |
| `/quiz/{taskId}` | Quiz | serve/answer, whole-quiz clock |
| `/diagnostic` | Diagnostic placement | `POST /api/diag/start\|answer\|finish` |
| `/map` | Curriculum graph | `GET /api/graph` |
| `/export` (action) | JSONL event-log download | `GET /api/export` |
| `/ops` | Operator flags (A6, T3) | the M5 flags endpoint |
| `/review` | Template / teach / hint review (§3) | `/api/admin/content*` |

**New in 2.0, and the only structural change to the study loop.** The grade reply returns the
verdict from local CPU and carries a `diagnosis` field of `pending`, `ready`, or `not_offered`
(`docs/reference/web-service-1.0-spec.md:66-118`). The SPA therefore renders the verdict, the
worked solution, and the re-solve instruction **immediately**, and fills the diagnosis panel
later from `GET /api/diagnosis/stream` (SSE), with `GET /api/diagnosis/{id}` polling at 2 s as
the required fallback (`:110-118`). A job still `pending` after 30 s reads as `failed`; the
panel then says so and the deterministic verdict stands.

The SSE subscription is **one connection per session, not per problem**. It opens when the
session view mounts and closes on unmount, and it is registered with the view lifetime so it
cannot outlive the view (the F-37-1b rule, `static/ui.js:29-79`).

### 4.2 Design tokens (Tokyo Night) — carry over exactly

Dark is primary; light is a fallback only when the OS explicitly prefers light
(`static/app.css:5-57`; the token table is restated at `CLAUDE.md:112-124`).

| Token | Dark | Light | Role |
|---|---|---|---|
| `--bg` | `#1a1b26` | `#e1e2e7` | app background |
| `--bg-dark` | `#16161e` | `#d0d5e3` | recessed / bar |
| `--surface` | `#1f2335` | `#ffffff` | raised surface |
| `--surface-2` | `#24283b` | `#d5d6db` | secondary surface |
| `--text` | `#c0caf5` | `#24283b` | primary text (contrast 10.6) |
| `--muted` | `#7e88b4` | `#6b7089` | secondary text |
| `--border` | `#292e42` | `#c4c8da` | hairline divider — contrast 1.27, decorative only |
| `--accent` | `#7aa2f7` | `#2e7de9` | primary accent, focus ring |
| `--accent-2` | `#bb9af7` | `#9854f1` | secondary accent |
| `--accent-ink` | `#1a1b26` | `#ffffff` | glyphs on an accent fill |
| `--accent-weak` | `rgba(122,162,247,0.14)` | `rgba(46,125,233,0.10)` | accent wash |
| `--good` | `#9ece6a` | `#587539` | correct / pass |
| `--good-weak` | `rgba(158,206,106,0.13)` | `rgba(88,117,57,0.10)` | pass wash |
| `--bad` | `#f7768e` | `#f52a65` | incorrect / fail |
| `--bad-weak` | `rgba(247,118,142,0.13)` | `rgba(245,42,101,0.10)` | fail wash |
| `--warn` | `#e0af68` | `#8f5e15` | caution |
| `--info` | `#7dcfff` | `#007197` | info / links |
| `--scrim` | `rgba(10,12,20,0.55)` | `rgba(16,24,40,0.35)` | modal backdrop |
| `--ring-bg` | `#292e42` | `#d5d6db` | progress-ring track |

Geometry and type (`static/app.css:27-30,60-71`):

- `--radius: 14px`; buttons `10px`; chips `999px`. `--maxw: 720px` for the study column; the
  graph view goes wider; Anki uses `640px` (`app.css:520`).
- `--mono: ui-monospace, "SF Mono", "JetBrains Mono", "Fira Code", Menlo, Consolas, monospace`.
- Body font: `-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial,
  sans-serif`; `line-height: 1.5`; headings `1.25`. **No webfont** — the only font files the
  app ships are KaTeX's woff2 faces.

Two rules the tokens do not state but the code enforces:

- **Every technical readout is monospace with tabular figures** — timers, the countdown, the
  progress count, XP, and stat values (`app.css:415-424`).
- **`--muted` is lightened from Tokyo Night's `#565f89`** because that value scores contrast
  2.76 and fails WCAG AA (`app.css:16`, `CLAUDE.md:120`). Do not "correct" it back.

Aesthetic direction, in one line from `CLAUDE.md:98-107`: technical, precise, calm,
high-signal; hairline dividers over heavy frames; monospace for the technical; no gamified
motion. Anti-references are named there — keep them named in the 2.0 design brief.

### 4.3 KaTeX rendering rules

KaTeX 0.17.0, vendored, UMD globals, four delimiter pairs, `throwOnError: false`, raw text left
on failure (`static/ui.js:118-134`).

The React rule, and it is second only to the phase gate in risk
(`docs/FRONTEND_ARCHITECTURE.md:260-288`, implemented at `web/src/lib/katex.ts:1-86`):

1. **Never run `renderMathInElement` on a React-owned node.** It mutates the DOM in place. The
   session clock ticks at 1 Hz, so the problem card re-renders once a second; React then writes
   `textContent` back (the math flashes to raw LaTeX every second) or throws
   `NotFoundError: Failed to execute 'removeChild'` and blanks the page.
2. **Render to a string, memoized on the source text**, and hand React one opaque payload
   through `dangerouslySetInnerHTML`.
3. **The escaping step is `textContent` on a detached node.** Create a `div`, set
   `textContent` (never `innerHTML`), run `renderMathInElement`, take `.innerHTML`. The input
   is model output; a naive `dangerouslySetInnerHTML={{__html: text}}` is an XSS hole where
   none exists today (`web/src/lib/katex.ts:22-36`).
4. **Delimiter order matters**: `$$` before `$` (`web/src/lib/katex.ts:45-50`).
5. **`katex.min.js` loads before `auto-render.min.js`.** The reverse order makes the extension
   capture `window.katex` as `undefined`, and the only symptom is raw LaTeX on every problem —
   invisible to jsdom (`web/vite.config.ts:32-42`, `web/e2e/README.md:14-21`).
6. **The KaTeX stylesheet stays a `<link>` to the vendored tree**, ahead of the app bundle, so
   `.problem-text .katex { font-size: 1.15em; }` keeps the cascade (`static/app.css:193`,
   `web/src/main.tsx:19-38`).
7. **`assetsInlineLimit: 0`.** The CSP is `font-src 'self'` with no `data:`; Vite inlines assets
   under 4 KB by default and silently breaks three KaTeX fonts
   (`docs/FRONTEND_ARCHITECTURE.md:151-155`, `web/vite.config.ts:84-88`).

### 4.4 The state model

One phase discriminant per study view, set **synchronously before the first await**
(`static/views/session.js:24-27`): `loading | ready | submitting | feedback | done`. The
diagnostic adds `intro`; the quiz guards negatively on `done`
(`docs/FRONTEND_ARCHITECTURE.md:230-236`). There is no one shared `Phase` type; the hook is
generic over each view's union.

Three hooks carry the whole model (`docs/FRONTEND_ARCHITECTURE.md:184-245`,
`web/src/hooks/`):

- **`useLifetime`** — create at first render with `useState(createLifetime)`, `revive()` in the
  effect, `end()` in the cleanup. `revive()` must **not** bump the generation; `end()` already
  does. A ref-based registry is `alive === false` for the rest of the view under React 19
  StrictMode: it works in production and is dead in development.
- **`usePhase`** — an external store read through `useSyncExternalStore`, returning the tuple
  `[phase, gate]`. `tryEnter` takes a **guard set**, not a single `from`; returning an object
  poisons dependency arrays.
- **`useCall`** — the one request wrapper. A Retry re-runs the request **and** its continuation
  (F-36-1). Anything reachable from a retried `onOk` reads through refs or dispatches to a
  reducer, never through captured render state.

Additional state rules:

- **The answer field and the work field stay uncontrolled**, read through refs at submit
  (`docs/FRONTEND_ARCHITECTURE.md:289-306`). A controlled input re-renders the problem and its
  math on every keystroke, and it breaks `insertAtCursor`'s `setSelectionRange`.
- **`key={problem.problem_id}` on the problem subtree is mandatory.** Without it React reuses
  the nodes and the previous problem's working posts with the next problem's answer — corrupt
  data in an append-only log (`docs/FRONTEND_ARCHITECTURE.md:302-306`).
- **The toast store lives outside React**, because `app.call` toasts after navigation
  (`docs/FRONTEND_ARCHITECTURE.md:333-335`).
- **Boot reads `?verify=` and `?reset=` before `createRoot`.** Both tokens are single-use, and
  StrictMode spends them twice (`web/src/main.tsx:96-127`).
- **The clock is a leaf component.** It must not re-render the problem card.

### 4.5 Accessibility and keyboard rules

From the shipping SPA and the port's block Q (`web/test/react/a11y.test.tsx:1-18`):

- **Enter submits** from the answer field; Enter in the work field submits while Shift+Enter
  inserts a newline (`static/ui.js:239-246,267-273`). Enter bypasses the disabled button —
  `busy()` disables the button only, and the phase gate stops the double submit (F-37-1c).
  **`H` asks for a hint** when the answer field is empty (`static/ui.js:245`).
- **The math symbol palette** is a `role="toolbar"` with `aria-label="Math symbols"`; each key
  carries `aria-label="Insert {sym}"`. The symbols are
  `∞ π √( ^ ≤ ≥ ≠ ± × ÷ ° θ` (`static/ui.js:219,248-256`). `√(` inserts its opening paren so
  the radicand lands inside.
- **`mousedown` is suppressed on each symbol key**, so the caret position survives the tap
  (`static/ui.js:252-254`).
- **Focus moves on every transition**: the answer input on a fresh problem, the Continue button
  on feedback, the card (not the CTA) on the diagnostic intro (R15,
  `static/views/session.js:236`, `:394`, `docs/FRONTEND_ARCHITECTURE.md:78`).
- **All focus rings are `2px solid var(--accent)` with `outline-offset: 2px`**
  (`static/app.css:121,329,348,466,488,516`).
- **Minimum target height is 44 px** on every button (`static/app.css:117`).
- **Toasts live in an `aria-live="polite"` region**; each carries `role="status"`; an
  actionable toast never auto-dismisses (`static/index.html:19`, `static/app.js:86-101`).
- **`prefers-reduced-motion: reduce` strips every animation and transition**
  (`static/app.css:567-574`). **`prefers-color-scheme` selects the theme**; `<meta
  name="color-scheme" content="dark light">` and two `theme-color` values ship in the head
  (`static/index.html:6-8`).
- **The graph has an accessible list view beside the canvas** (`app.css:555-558`,
  `docs/FRONTEND_ARCHITECTURE.md:318-322`). **No credential in JS or `localStorage`** — the
  session is an HttpOnly `__Host-` cookie (SEC-cookie, `static/api.js:1-8`).
- Axe in jsdom cannot see color contrast, focus order as rendered, or layout
  (`web/test/react/a11y.test.tsx:6-14`). Pair axe with explicit focus and keyboard assertions.

### 4.6 The proposed stack

Vite + React + TypeScript `strict`, no server-side rendering, static files served by Caddy,
API on the same origin. This is the stack 1.0's port already runs
(`web/package.json:26-45`), so M6 inherits measured numbers, not guesses.

| Item | Choice | Evidence |
|---|---|---|
| Runtime | React 19.2, `react-dom` 19.2 | `web/package.json:27-29` |
| Build | Vite 7, `@vitejs/plugin-react` | `web/package.json:37,43` |
| Types | TypeScript 5.9, `strict` | `web/package.json:41` |
| Lint | ESLint 9, `typescript-eslint`, `eslint-plugin-react-hooks`, `eslint-plugin-jsx-a11y` | `web/package.json:33-35,42` |
| Unit tests | Vitest 3 + jsdom + Testing Library + user-event + MSW + `vitest-axe` | `web/package.json:30-45` |
| Browser tests | Playwright 1.62.1 | `web/e2e/node_modules/playwright/package.json` |
| Math | KaTeX 0.17.0, vendored, woff2 only | `docs/FRONTEND_ARCHITECTURE.md:156-159` |
| Graph | Cytoscape 3.34.1, dynamic `import()` behind a memo, marked `external` | `web/vite.config.ts:100-107` |

Measured on 1.0's port: 96 npm packages, the whole spike suite about 1.1 s
(`docs/FRONTEND_ARCHITECTURE.md:344`); entry bundle 190 KB raw / 60 KB gzipped, `dist/` 2.0 MB
(`:161`).

Build settings that are load-bearing (`web/vite.config.ts:74-110`):

`outDir: 'dist'`, never a tracked tree — `npm run check` calls `build`, and an `emptyOutDir`
aimed at the source deletes the product the gate gates. `assetsInlineLimit: 0` (§4.3 rule 7).
`sourcemap: false`, because a source map at the edge exposes the whole tree.
`external: [/^\/vendor\//]`, because `/vendor/**` is served rather than bundled. `base: '/'`,
so asset URLs stay root-absolute on one origin with no proxy rewriting.

**Serving and the image.** Caddy serves `dist/` and reverse-proxies `/api` to the axum service
on the same origin, so the `__Host-` cookie and the CSRF origin rules keep working unchanged
(`docs/reference/web-service-1.0-spec.md:121-157`). The Docker image builds the SPA in a node
stage and copies `dist/` into the runtime image; the runtime image carries no node.

**A gate that asserts on build output runs first against a known-bad input.** 1.0 records why:
its own first `data:` check reported FAIL on a passing build, because a pipeline's exit status
is `head`'s, and an untested assertion passes forever
(`docs/FRONTEND_ARCHITECTURE.md:167-170`). `npm run selftest` exists for exactly this
(`web/package.json:22`).

---

## 5. Parity and UX traps

| # | Trap | Rule | Evidence |
|---|---|---|---|
| T1 | **JSON eats LaTeX.** `"$\times$"` with one backslash decodes to `$<TAB>imes$`. Nothing downstream rejects it; KaTeX paints it red and the append-only log keeps it forever. | Port `repair_latex_escapes` and run it at the authoring boundary on `statement`, `solution_sketch`, and every hint — never on `answer_expr`. | `prompts.py:1021-1048`, `:1190-1192`; `tests/test_problem_templates.py:940-969` |
| T2 | **Braces.** Single braces are placeholders, so every literal LaTeX brace is doubled: `$7^{{2}}$`, `$7{{,}}329$`. A stray single brace is rejected. | Keep the scanner, not a regex — the two constructs interleave. | `problem_templates.py:233-260`, `:520` |
| T3 | **Dot-thousands.** `7.329` for `7329` is correct with the `notation` tag and the fixed note. | The checker owns it (M2). The SPA renders `error_tags` verbatim and never re-interprets them. | `docs/reference/checker-1.0-spec.md:156,276,567` |
| T4 | **Timers are display only.** The server measures session time from `state.active_secs`. | The client is never an authoritative timing source. No cumulative client counter. | `static/views/session.js:151-155` |
| T5 | **The countdown is drill-only.** A drill with `countdown` true and `time_budget_secs > 0` counts down and auto-submits at 0, even blank. | The auto-submit goes through the same phase gate, so it never fires on top of an in-flight grade. `urgent` turns the clock red at `left <= 3`. | `static/views/session.js:175-186,187-189,231-234` |
| T6 | **The quiz clock uses the task budget**, never the per-question serve value (QUIZ-budget). | | `docs/FRONTEND_ARCHITECTURE.md:70` |
| T7 | **A quiz timeout skips an in-flight problem**; it never re-posts the same `problem_id` (QUIZ-timeout). | | `docs/FRONTEND_ARCHITECTURE.md:72` |
| T8 | **The re-solve flow.** A hinted or reference-assisted correct answer returns the view to `ready`, not to a terminal state. The solution is revealed to study, the field is cleared, and the **same** submit sends the unaided re-solve. | `feedback` is not terminal. The server records the discounted pass only after the re-solve. | `static/views/session.js:307-333`, DD-3/P1 at `docs/FRONTEND_ARCHITECTURE.md:66` |
| T9 | **Auto-advance is 1400 ms, correct answers only, and only when `next` exists.** It lives in the lifetime registry; clicking Continue or End cancels it. | | `static/views/session.js:380-389` |
| T10 | **`next_unavailable` is not the end of a task.** The button reads "Next problem →" and the view re-serves the same task. | | `static/views/session.js:376-378,404-407` |
| T11 | **Placement feedback renders a tick or a cross only** — never `solution`, never `expected` (DIAG-nosol). | | `docs/FRONTEND_ARCHITECTURE.md:79` |
| T12 | **No view mount issues two model-billed POSTs** (NO-2BILL). In 2.0 nothing on the request path is billed, but the rule survives as "no view mount issues two writes". | | `docs/FRONTEND_ARCHITECTURE.md:86` |
| T13 | **`/serve` is idempotent on the server** and re-stamps `started_at`. A demo or mock backend that advances a cursor per call makes the learner practice the wrong problem. | Pin idempotence in the mock too. | `web/e2e/README.md:23-30`, `docs/reference/web-service-1.0-spec.md` U7 |
| T14 | **Auth calls bypass the central call wrapper**, so `invalid_credentials` never trips the session-expired path (AUTH-inline). | | `docs/FRONTEND_ARCHITECTURE.md:67` |
| T15 | **OAuth buttons appear only when `/api/health` advertises providers** (AUTH-7). | | `web/src/views/Auth.tsx:67`, `docs/FRONTEND_ARCHITECTURE.md:83` |

**Hard Rules 1–5 in the UI** (`docs/PEDAGOGY.md`, `docs/WEB_SERVICE.md` "Non-negotiable
rules"; `REQUIREMENTS.md:20-24`):

1. **No answer before an attempt.** The solution panel renders only from a grade response;
   hint text never contains `expected`; the statement never carries the method (`prompts.py:495-498`).
2. **Honest structural grades.** `correct` is mathematical correctness only; partial credit
   lives in `work_quality`. The SPA displays both and never derives one from the other.
3. **The core owns all scheduling.** The SPA renders `next`, `remediation`, and `task_status`
   as given, and decides nothing (`static/views/session.js:399-407`).
4. **Generation fidelity / anti-repeat.** The SPA shows the served instance, and nothing about
   its source. 5. **LaTeX math rendering** — §4.3. Rule 6, brisk tone with zero fluff, is the
   design brief of §4.2.

---

## 6. Pinned literals from the 1.0 tests

Carry these into 2.0 as literal assertions, not as re-derived values.

**Authoring and gate**

| Literal | Value | Source |
|---|---|---|
| `BANK_TARGET` | `3` | `problem_templates.py:156` |
| `_AUTHORING_ATTEMPTS` (1.0) | `2` | `:178` |
| `MIN_SPACE_SIZE` | `12` (= `SERVED_TEXT_MEMORY`) | `:174`, `state.py:133` |
| `MAX_DOMAIN_SIZE` | `10_000` | `:160` |
| `MAX_CHOICES` | `24` | `:188` |
| `GATE_SAMPLES` | `200` | `:183` |
| `EXHAUSTIVE_SPACE_LIMIT` | `4_096` | `:217` |
| `RESAMPLE_ATTEMPTS` | `24` | `:209` |
| `_NON_ANSWERS` | `{I, zoo, oo, -oo, nan, AccumBounds, True, False, true, false}` | `:203-205` |
| `REVIEW_BEFORE_SERVE` | `True`; default knob on | `:1021`, `tests/…:1431-1435` |
| `template_prompt_digest()` on this checkout | `f322b85a40b9ac50` | `docs/reference/serving-1.0-spec.md:113` |
| `SAMPLE_INSTANCES` in review `show` | `8` | `scripts/review_templates.py:52` |
| Rejection messages | every string of §2.1 | `problem_templates.py`, cited per row |

Behavioral pins already written as 1.0 tests, to port:

- A refused template is re-prompted with the reason and is rescued; the rescued template is
  stored **unapproved** and the serve still falls back (`tests/test_problem_templates.py:775-798`).
- An authored template is stored unapproved (`:1331`); a pending template is never served,
  occupies its slot, and is not re-authored (`:1340`); approval releases the saving — 30 serves,
  0 generate calls, 0 author calls (`:1358`).
- A row with no `approved` key is treated as pending and is not rewritten (`:1375`); an approval
  does not survive an edit to what was approved (`:1438`).
- A single bad sample rejects the whole template (`:158`); a choice the samples never use
  (`:378`) and an int edge they never reach (`:404`) are rejected; a sample outside its own
  domain is rejected (`:452`); a template that produces only one problem is rejected (`:1270`).
- A backslash in a choice value does not crash the serve (`:513`).
- A constraint between parameters cannot be expressed in 1.0 (`:1049`) — **this test inverts in
  2.0**: the constraint language exists, so the 2.0 test asserts that `a > b` is expressible and
  that no draw violates it.

**SPA-facing**

The whole invariant list is `web/test/invariants.json` (29 tags, 2 retired). The gate
`web/scripts/check-invariants.mjs` fails the build when a tag has zero tests whose **name**
contains it. Every invariant is a negative, which is why a coverage floor cannot see them
(`web/test/invariants.json:6-9`). Carry the tag list, the gate, and this rule. Literal UI
values: `MATH_SYMBOLS = ['∞','π','√(','^','≤','≥','≠','±','×','÷','°','θ']`
(`static/ui.js:219`); auto-advance `1400` ms (`session.js:386`); `urgent` at `left <= 3`
(`session.js:181`); the field hint text `answers like 3/4, 2x+1, sqrt(2) are fine`
(`static/ui.js:259`); the work-field label `Show your working (optional)…`
(`static/ui.js:265`); the diagnostic beat `750` ms (DIAG-750); `fmtClock` renders `m:SS`
(`static/ui.js:111-115`); the error envelope `{"error":{"code","message"}}`
(`static/api.js:26-30`).

---

## 7. Proposed unit breakdown for M6

Each unit is ≤ ~400 changed lines with its acceptance check named up front. Rust units and
TypeScript units are separated, because they run different gates.

### 7.1 Rust units — authoring and review

| Unit | Deliverable | Acceptance check |
|---|---|---|
| **R1** | `cadus-worker::authoring::prompt` — the 2.0 system prompt, the tool schema, the user message, and the retry block; the prompt digest; the four kinds (`template`, `teach`, `hint_ladder`, `diagnosis`) | the retry block reproduces the 1.0 wording of `prompts.py:766-774`; the digest is stable across a re-serialization and changes when the schema changes; a golden-file test pins the rendered user message for one KP |
| **R2** | `cadus-worker::authoring::job` — the per-KP batch loop: author → gate → retry with the literal message → store `pending`; 5 attempts; the decline path | a template refused for a missing low edge is re-prompted with that exact message and is rescued on attempt 2; a KP refused 5 times writes no `content_store` row and one decline record; the loop makes zero calls for an already-approved KP |
| **R3** | T3 accounting — `model_call_log` rows with `purpose='authoring'`, `authoring_attempts` and `authoring_cost_usd` on the stored row, the >3-attempt alert | a 4-attempt KP raises the alert and the row's `authoring_attempts` is `4`; a reply with no `usage` block writes a zeros row with a NULL cost; the cost on the row equals the sum of that KP's call rows |
| **R4** | `cadus-store::content` — insert `pending` as `cadus_admin`, read approved by `(kp_id, kind)`, approve and reject by digest | `cadus_app` cannot INSERT, UPDATE, or DELETE `content_store`; an approve on a digest that does not exist is `404`; approving twice is idempotent |
| **R5** | `/api/admin/content*` — the four routes of §3.2, admin-gated, with rendered instances and gate notes | a non-admin session gets `403` on all four; the show route returns 8 instances with computed answers; the list flags a KP with fewer than 3 approved templates; a reject without a reason is `422` |
| **R6** | Teach and hint authoring — the two prompts, their gates (a hint never contains the answer; a teach page carries concept + worked problem + steps), and their storage | a hint ladder whose last rung contains the expected answer is rejected with a literal message; a teach body missing `worked_example.steps` is rejected; an approved teach body serves through the M5 teach route with no model call (L4/L5) |
| **R7** | Distractor authoring and the A4 pre-authored path — `distractors: [{answer, error_tag, note}]`, vocabulary filter, wiring to the grade reply's `diagnosis: ready` | an `error_tag` outside the vocabulary is dropped at the gate; a learner answer matching a distractor returns `status:"ready"` and writes no `diagnosis_jobs` row |
| **R8** | The authoring CLI entry point (`cadus-worker author --kp … --kind …`) and its runbook section in `docs/` | a dry run makes zero model calls and prints the plan; the runbook commands run against the test database |

### 7.2 TypeScript units — the SPA

| Unit | Deliverable | Acceptance check |
|---|---|---|
| **S1** | `web/` scaffold: Vite + React 19 + TS strict, ESLint with `react-hooks` and `jsx-a11y`, Vitest + jsdom + Testing Library + MSW + `vitest-axe`, the `check` script chain, the CSP and invariant self-tests | `npm run check` passes on an empty app; `npm run selftest` proves both output gates fail on a known-bad input; the build emits zero `data:` URIs |
| **S2** | `src/api` — hand-written `types.ts` from `docs/reference/web-service-1.0-spec.md`, the fetch client, the error envelope, the 2xx-with-`error` asymmetry, the download helper | every route of the spec table has a typed method; a 2xx body carrying `error` throws; a 401 surfaces distinctly; no credential reaches `localStorage` (SEC-cookie) |
| **S3** | The three hooks — `useLifetime`, `usePhase`, `useCall` — plus `ErrorBoundary` and the out-of-React toast store | the StrictMode remount leaves `alive === true`; `revive()` does not bump the generation; a Retry re-runs the request and its continuation; an actionable toast never auto-dismisses |
| **S4** | Design system: the token stylesheet of §4.2, `primitives.tsx`, `Modal` (portal, Esc, focus trap, resolves `null` on unmount), the topbar and the app shell | the token contract test asserts every hex of §4.2 in both themes; axe reports zero violations on the shell; Esc closes the modal and focus returns to the opener |
| **S5** | `MathBlock`, `AnswerField`, `WorkField` — the KaTeX string idiom, the symbol palette, Enter and `H`, uncontrolled inputs read through refs | a 1 Hz re-render does not re-run KaTeX and does not flash raw LaTeX; `<img src=x onerror=alert(1)>` in problem text renders as visible text; Enter in the first frame after mount posts once, not zero times |
| **S6** | Auth screens: login, signup, forgot, reset, verify, check-email, sent; OAuth buttons behind `/api/health`; boot token handling before `createRoot` | a `?verify=` token is spent exactly once under StrictMode; an unconfigured provider renders no button; `invalid_credentials` renders inline and does not route to session-expired |
| **S7** | Dashboard and status: the single primary action, no dead ends, quiet secondaries, the JSONL export | an empty plan offers the diagnostic (W-C3); exactly one primary button (W-C2); the export downloads through the cookie, not a token |
| **S8** | The session loop: plan, serve, teach, hint, answer, feedback, the re-solve flow, auto-advance, `next_unavailable`, the exit paths | the DD-3/P1 re-solve returns to `ready` and the same submit sends the re-solve; a hint body never contains `expected`; auto-advance fires only on correct-with-next and cancels on click; `key={problem_id}` prevents cross-problem answer bleed |
| **S9** | The async diagnosis panel: SSE subscription per session, the 2 s poll fallback, the 30 s failure rule, `not_offered` and `ready` states | the verdict paints before any diagnosis arrives; an SSE drop falls back to polling and still lands; a `pending` job past 30 s reads as failed and the verdict stands; the subscription closes on unmount |
| **S10** | Quiz and diagnostic placement: the whole-quiz clock, no reveal before the last answer, the timeout skip, the three ground rules, the honest-skip control, tick/cross only | QUIZ-budget, QUIZ-reveal, QUIZ-timeout, P3, R15, DIAG-750, DIAG-nosol each claimed by a named test |
| **S11** | The curriculum map: Cytoscape behind a memoized dynamic `import()`, the `<Suspense>` boundary, the accessible list view, destroy-before-build | an overlapping load builds exactly one instance (F-38-1); a failed library load retries on the next mount; the list view is reachable by keyboard alone |
| **S12** | The operator screen (`/ops`: A6 flags, T3 cost per KP) and the review screen (`/review`: §3.2) | a non-admin sees neither route; the review list groups by KP and shows attempts and cost; Approve posts the digest and the row leaves the pending list; Reject requires a reason |
| **S13** | The browser click-through: Playwright scripts for the demo path and the authed path, run in the Playwright container; screenshots as CI artifacts | the three failures 1.0's click-through found are each reproduced by a deliberately broken build and caught; a clean build passes all flows |
| **S14** | Packaging: the node build stage in the Dockerfile, Caddy serving `dist/` with `/api` on the same origin, the CI job | the runtime image carries no node; a cookie-authed POST through Caddy is accepted and a cross-site one is `403`; the built bundle passes the CSP grep |

**Sequence.** R1 → R2 → (R3 ∥ R4) → R5 → (R6 ∥ R7) → R8, in parallel with S1 → S2 → S3 →
(S4 → S5) → (S6 ∥ S7) → S8 → S9 → S10 → S11 → S12 → S13 → S14. S12 depends on R5; S9 depends on
M5's U9. Then: mutation check → adversarial review (2 rounds, major and above) → fixes → close.

### 7.3 The toolchain this box has

Measured on 2026-08-27:

| Item | Result |
|---|---|
| `node --version` | `v22.21.1` (`/home/deploy/.nix-profile/bin/node`) |
| `npm --version` | `10.9.4` |
| `npx` | present, same prefix |
| Playwright package | 1.62.1, installed under `/home/deploy/dev/cadus/web/e2e/node_modules` |
| Playwright browsers | downloaded without sudo — `~/.cache/ms-playwright/chromium_headless_shell-1234`, `ffmpeg-1011` |
| Playwright browser **launch** | **fails**: `chrome-headless-shell: error while loading shared libraries: libnspr4.so`. Adding the nix `nspr` and `nss` store paths to `LD_LIBRARY_PATH` moves the failure to `libatk-1.0.so.0`. |
| `playwright install-deps` | needs root; there is no sudo on this box |
| `docker` | present at `/usr/bin/docker` and **usable by this user** (`docker info` succeeds) |

**Conclusion for S13.** Browser binaries install without sudo, but they do not launch, because
the system libraries are absent and `install-deps` needs root. Run the click-through in the
Playwright container, as 1.0 already documents (`web/e2e/README.md:32-38`):

```sh
docker run --rm -v "$PWD/e2e:/out" -w /out \
  -e PLAYWRIGHT_BROWSERS_PATH=/ms-playwright \
  -e BASE=http://host.docker.internal:PORT \
  mcr.microsoft.com/playwright:v1.62.1-noble sh -c 'npm i playwright@1.62.1 --silent && node demo.mjs'
```

Vitest, ESLint, `tsc`, and the Vite build all run natively on this box with no container. Only
the browser click-through needs docker. Keep the two in separate CI steps, so a docker outage
never blocks the unit gate. The TypeScript gate command is `npm run check`
(`web/package.json:22`): `selftest && lint && types && test && invariants && build && csp`.
Serialize it against the Rust gate — one full suite at a time on this box.
