# Web service 1.0 — the survey M5 builds from

Source: `/home/deploy/dev/cadus` (read only). Every claim cites `file:line`. Requirements: C1–C4,
A3, A4, R2, R4, L1–L6, T1–T6, D-S6, D-O2, D-O3, D-O5, D7. Owner decisions: O2 (DeepSeek V4 via
OpenRouter, no T4 caps), O3 (React + TypeScript in M6).

## 1. Where things live

| Concern | 1.0 location |
|---|---|
| The frozen HTTP contract | `docs/WEB_SERVICE.md:83-120` (routes), `:22-31` (Hard Rules), `:122-199` (serve/answer notes) |
| Route handlers | `cadus_web/api.py` (2,429 lines); router `:87`. Paths below are relative to `cadus_web/` |
| Grade path | `api.py:1256-1272` (`_grade_answer`), `:1275-1387` (`task_answer`), `:1106-1125` (`_known_correctness`), `:1457-1536`, `:1539-1659`; H3 `:1361-1379`, `:1520-1532` |
| Deterministic grade | `deterministic_grade.py:109-152`; tier `:72`; blank `:94-106`; override `prompts.py:1271-1299`, `:1334-1361` |
| Timing, caps, thresholds | `api.py:729-741` (`_measure_secs`), `:91-92` + `:1292-1293` (caps), `:97` + `:1856-1859` (stuck hint) |
| Other handlers | teach `api.py:1064-1103`; hint `:1800-1865`; quiz reveal `:1702-1797`; diagnostic `:1896-2091`; export `:2245-2269`; record/close `:599-710` |
| Prompts + model client | `prompts.py:540-584` (`GRADE_SYSTEM`), `:212-232` (schema), `:885-897` (user msg), `:50-63` (error tags); `openai_engine.py:398-600`, routing `:151-185`, retries `:531-559`; shared contract `engine.py:57-58`, `:111-125`, `:128-175` |
| Auth cookie + session | `authn/cookies.py:47`, `:51`, `:101-113`, `:165-186`; `authn/deps.py:40`, `:72-128`; lifetimes `authn/service.py:43`, `:48`, `:375`, `:436` |
| CSRF + headers | `app.py:50-59`, `:65-89`, `:136-205`, `:282-292` |
| Rate limits, passwords, tokens, OAuth | `authn/routes.py:101-104`, `:215-268`; `authn/passwords.py:33-37`, `:54-57`; `authn/tokens.py:21-40`; `authn/oauth.py:60-68`, `:186-199`; `authn/oauth_routes.py:98-219` |
| Session state, queue claim | `state.py:43-190` (ring bound `:133`); lock `webstate.py:52-58`, `:316-395`; `cadus_pg/repos.py:396-530`; `cadus_worker/email.py:100-175` |
| Metrics, config, health | `metrics.py:83-166`, `:222-293`; `config.py:30-232`; `health.py:42-176` |
| Contract tests | `tests/test_web_api.py` (2,517), `test_web_security.py`, `test_authn_endpoints.py`, `test_authn_reset.py`, `test_authn_oauth.py`, `test_deterministic_grade.py`, `test_service.py:226-598` |

2.0 pieces M5 composes: `crates/store/src/lib.rs:387-399` (`begin_tenant`), `:314-328`
(`bounded`), `:364` (`assert_rls_enforced`); `crates/core/src/answer/check.rs:89` (`check`);
`crates/core/src/selector.rs`; `docs/SCHEMA.md` "The M5 auth contract";
`migrations/0005_content.sql` (`serving_pool`, `model_call_log`, `diagnosis_jobs`);
`migrations/0004_scratch.sql:37-41` (`web_states`); `docs/plans/M4.md` (pool pop, ring, exemplar
fallback); `docs/reference/projector-1.0-spec.md:282-306` (fold vs replay).

---

## 2. The HTTP contract

Envelope for every 4xx/5xx: `{"error": {"code", "message"}}` (`cadus_web/errors.py:30-31`).
Auth column: **S** = session required, **P** = public.

| Method + path | Auth | Request | Response (key fields) | Statuses | 2.0 |
|---|---|---|---|---|---|
| GET `/api/health` | P | — | `{ok, fake_llm, model, engine, providers}` (`docs/WEB_SERVICE.md:86-88`) | 200 | keep, drop `fake_llm` |
| GET `/api/ready` | P | — | `{ok, db, redis, worker:{heartbeat_age_secs, stale}}` (`cadus_web/health.py:139-167`) | 200/503 | keep; drop `redis` (D8) |
| GET `/metrics` | P | — | Prometheus text (`cadus_web/metrics.py:303-307`) | 200 | keep |
| GET `/api/status` | S | — | `{course, placed, courses, test_prep, xp, velocity, quiz, pending_remediation, quiz_due, drill_due, **counts}` (`cadus_web/api.py:787-799`) | 200 | keep |
| GET `/api/graph` | S | `?scope=` | `{now, scope, courses, modules, counts, nodes, edges}` (`docs/WEB_SERVICE.md:94`) | 200/404 `unknown_course` | keep |
| POST `/api/enroll` | S | `{course}` | `{enrolled, mastery_floor, floor_size}` | 200/404/422 | keep |
| POST `/api/session/start` | S | `{}` | `{session, reopened, xp, frontier, due_reviews}` (`:874-880`) | 200 | keep |
| POST `/api/session/end` | S | `{minutes?}` | `{session, xp_earned, minutes, xp, anki}` (`:919-925`) | 200/409 | keep |
| GET `/api/session/plan` | S | — | `{session, tasks[], quiz_due, constraints, course_complete, frontier_blocked_until}`; task trimmed to `{task_id, task_type, topic{id,name,module}, kp, start_at_kp, n_problems, mix, component_topics, time_budget_secs, difficulty_target, why, progress{answered, done}}` (`:947-956`, `:993-1006`) | 200/409 | keep |
| POST `/api/task/{id}/serve` | S | `{}` | `{problem_id, index, total, text, kp, time_budget_secs, countdown}` (`:519-527`) | 200/404 `unknown_task`/409 `task_complete`, `multistep_exhausted` | keep; body comes from the M4 pool pop |
| POST `/api/task/{id}/answer` | S | `{problem_id, answer, work?, assisted?}` | see below | 200/404 `unknown_problem`/409 `task_complete`/413 `answer_too_large`/422 `invalid_request` | **change (A3/A4)** |
| POST `/api/task/{id}/hint` | S | `{problem_id}` | `{hint, hint_number, reference_lesson?}` (`:1849-1864`) | 200/404/409 `no_hints_in_quiz`/422 | keep; body from the authored hint ladder (L5) |
| POST `/api/task/{id}/teach` | S | `{}` | `{kp, concept, worked_example:{problem, steps}}` (`:1096-1103`) | 200/409 `no_instruction` | keep; body from `content_store` kind `teach` (L4) |
| POST `/api/task/{id}/abort` | S | `{}` | close payload `{task_id, task_type, aborted, xp, state_delta, …}` (`:686-710`) | 200/404 | keep |
| POST `/api/diag/start` | S | `{course?}` | `{probe:{topic, problem_id, text}, asked, cap}` (`:1977-1981`) | 200/400 `no_course`/404 `unknown_course` | keep |
| POST `/api/diag/answer` | S | `{problem_id, answer}` | `{correct, next_probe\|{done:true}}` (`:2055-2057`) | 200/404/409 `no_diagnostic`/422 | keep; deterministic-only verdict |
| POST `/api/diag/finish` | S | `{}` | `{placed, conditional, frontier}` (`:2087-2091`) | 200/409 | keep |
| GET `/api/modules` | S | — | `{course, modules[]}` | 200 | keep |
| GET `/api/export` | S | — | JSONL stream, one event per line, `Content-Disposition` attachment (`:2245-2269`) | 200 | keep |
| GET `/api/anki/queue`, POST `/api/anki/mark`, PUT `/api/anki/settings`, GET `/api/anki/apkg` | S | — | (`:2099-2243`) | | defer past M5; the schema already carries `anki_queue`, `anki_cards_created` |
| POST `/api/anki/push`, `/sync`, `/launch`, GET `/api/anki/status` | S | — | always `409 not_supported_on_hosted` (`:150-160`) | 409 | **drop** — dead stubs |
| POST `/api/worksheet`, `/api/refsheet`, GET `/worksheet/{id}.html\|.tex`, `/refsheet/…` (`:2308-2429`); GET `/`, `/app/*`, `/vendor/*` (the 1.0 SPA) | S / P | — | — | | **drop** — no `worksheets` table in the 2.0 schema, and O3 rewrites the SPA in M6 |

**New in 2.0:** `GET /api/diagnosis/{diagnosis_id}` and `GET /api/diagnosis/stream`.

### 2.1 The grade response changes (A3 + A4)

1.0 returns `{correct, feedback, work_quality, task_status, remediation, next, solution?, xp?,
next_unavailable?}` (`api.py:1646-1658`) and waits for the model on every miss (`:1272`). 2.0
returns the whole verdict from local CPU and hands the prose off:

```
200 POST /api/task/{task_id}/answer
{
  "attempt_id":   "t-review-fractions-3",     // M3 rule {task_id}-{n}
  "correct":      false,
  "work_quality": "nearly_perfect" | ...,     // deterministic tier, §5
  "error_tags":   ["notation"],               // deterministic only; never invented
  "secs":         37,
  "solution":     "…worked solution…",        // revealed after the attempt commits
  "re_solve":     "…the stock instruction, §5.5…",
  "task_status":  "continue" | "kp_advance" | "task_passed" | "task_failed"
                  | "already_recorded",
  "remediation":  [ {kind, targets} ],
  "next":         { …serve shape… } | null,
  "next_unavailable": true,                   // only with next: null on an open task
  "xp":           12.5,                       // when the core priced one
  "diagnosis":    { "id": "<uuid>", "status": "pending" }
                  | { "status": "ready", "error_tags": [...], "prose": "..." }
                  | { "status": "not_offered" }
}
```

`diagnosis` rules: **`not_offered`** — correct, blank, or an undecidable kind with no template
diagnosis; no job row is written. **`ready`** — a pre-authored distractor diagnosis matched the
learner's answer, read from `content_store` kind `diagnosis` in the same transaction, no model
call (A4 last clause); the common case once banks are warm. **`pending`** — a `diagnosis_jobs`
row was inserted in the grade transaction and `id` is its primary key; the client subscribes or
polls.

```
GET /api/diagnosis/{id}          200 {"id", "status": "pending"|"ready"|"failed"|"capped",
                                      "error_tags": [...], "prose": "...", "model_id": "..."}
                                 404 {"error":{"code":"unknown_diagnosis"}}
GET /api/diagnosis/stream        text/event-stream; event: diagnosis
                                 data: {"id","status","error_tags","prose"}
                                 heartbeat comment every 15 s
```

Push (D7): the worker runs `NOTIFY diagnosis_done, '<job_id>:<user_id>'` in the transaction that
writes `result`; one process-wide `LISTEN` connection fans the id to that `user_id`'s SSE
subscribers. **`LISTEN/NOTIFY` carries no RLS and no tenant binding**, so the payload holds ids
only and the SSE handler re-reads the row through `begin_tenant` before it writes a byte. The
poll route is a required fallback (a proxy that buffers SSE, a client that reconnects): interval
2 s, and a job still `pending` after 30 s is reported `failed`, never left open.

---

## 3. Auth

### 3.1 Cookie, CSRF, tokens, hashing

| Item | 1.0 value | Cite |
|---|---|---|
| Cookie | `__Host-cadus_session`, `HttpOnly`, `Secure`, `SameSite=Lax`, `Path=/`, `Max-Age = SESSION_IDLE`; dev fallback `cadus_session` behind `CADUS_WEB_INSECURE_COOKIE=1` | `cookies.py:47`, `:51`, `:101-113`, `:165-186`; `routes.py:58` |
| Guards | a `__Host-` name without `Secure` refuses to start; a `__Host-` cookie at any path but `/` raises | `cookies.py:233-263`, `:62-84` |
| Second channel | `Authorization: Bearer <session-token>`, bearer before cookie | `cookies.py:153-162` |
| Session token | `secrets.token_urlsafe(32)` = 256 bits, 43 chars; stored as SHA-256 hex only | `tokens.py:21-40` |
| Idle / absolute window | 30 days; 90 days from `created_at`, never slid | `service.py:43`, `:48`, `deps.py:104-106` |
| `last_seen_at` touch | at most once per hour | `deps.py:40`, `:109-110` |
| Reset / verify TTL | 30 minutes / 24 hours, both single use | `service.py:375`, `:436` |
| Password policy | 8–256 chars, ≤ 1024 UTF-8 bytes, length only (NIST 800-63B) | `passwords.py:33-37` |
| Argon2id | prod `time_cost=3, memory_cost=65536 KiB, parallelism=4`, `Type.ID`; test `1 / 8192 / 1`; rehash on login via `check_needs_rehash` | `passwords.py:55-56`, `:64-69`, `:134-145` |
| Email normalization | NFKC → strip → lower, idempotent | `passwords.py:148-156` |
| CSRF | authed writes: non-safe `/api/*` + session cookie + no bearer + not same-origin → `403 cross_origin_rejected`. Pre-auth: `/api/auth/login`, `/signup`, `/verify-email` refuse an *explicitly* cross-origin POST | `app.py:57-59`, `:153-170`, `:194-205`, `:282-292` |
| Same-origin test | `Sec-Fetch-Site: same-origin\|none`, or `Origin == scheme://host`; no signal fails **closed** for authed writes and fails **open** for pre-auth | `app.py:136-191` |
| Security headers | CSP `default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; font-src 'self'; base-uri 'none'; frame-ancestors 'none'; connect-src 'self' http://localhost:* http://127.0.0.1:*`, `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`, `Cache-Control: no-cache` | `app.py:65-89` |
| OAuth providers | Google (`openid email profile`), GitHub (`read:user user:email`); absent client id/secret → the routes 404 | `oauth.py:186-199`, `:207-215` |
| OAuth handshake | PKCE S256 + constant-time `state`; cookie `cadus_oauth_handshake`, `HttpOnly`, `SameSite=Lax`, path-scoped to the OAuth routes, `Max-Age=600` | `oauth.py:63-68`, `oauth_routes.py:44-48`, `:120-122` |

### 3.2 Rate limits — the four paired rules

Every rule bounds the **normalized email first**, then the **client IP**, in one shared fixed
window, and both counters increment **before any account lookup**, so a 429 is byte-identical
for a known and an unknown address (`routes.py:240-268`). The `(limit+1)`-th call in a window
is the first refused (`routes.py:226-227`). Counters live in `auth_rate_counters`, keyed
`(scope, key, window_start)`, where `scope` is `"{prefix}_email"` / `"{prefix}_ip"`
(`routes.py:88-94`) and `window_start` is `now` floored to the bucket from the 1970 epoch
(`routes.py:209-212`).

| Rule | prefix | per email | per IP | window | Cite |
|---|---|---|---|---|---|
| Sign-up | `signup` | 3 | 5 | 1 h | `routes.py:101` |
| Login | `login` | 10 | 30 | 5 min | `routes.py:102` |
| Password forgot | `forgot` | 3 | 10 | 1 h | `routes.py:103` |
| Verify resend | `verify_resend` | 3 | 10 | 1 h | `routes.py:104` |

### 3.3 The 2.0 call order (binding — `docs/SCHEMA.md` "The M5 auth contract")

"Unbound" = a `cadus_app` connection with no `app.user_id`; "bound" = inside a transaction that
ran `begin_tenant` (`crates/store/src/lib.rs:387-399`). Calling any of the five SECURITY DEFINER
functions **after** the bind returns zero rows — the guard tests the caller, not the argument.

**Login.** (1) unbound `auth_user_by_email($email)`; zero rows = unknown account, and the handler
still spends one Argon2 verify against a fixed dummy hash so the timing matches
(`authn/service.py:83-105`). Refuse a NULL `password_hash` (OAuth-only), a NULL
`email_verified_at`, a non-NULL `disabled_at` — every refusal is the same
`401 invalid_credentials`. (2) `begin_tenant(id)`. (3) bound `INSERT INTO auth_sessions
(token_hash, user_id, created_at, last_seen_at, expires_at)`; WITH CHECK refuses any other
`user_id`.

**Sign-up.** (1) unbound `INSERT INTO users (email, password_hash)` with **no `RETURNING`** — the
SELECT policy hides the row from an unbound session — and naming neither `id` nor `is_admin`
(the column grant holds neither). (2) unbound `auth_user_by_email($email)` for the new id.
(3) bind, then write the session as in login step 3. The response is always the generic
`{"status": "verification_required"}` and sets no cookie (`routes.py:319-325`).

**Cookie check** (every guarded request). (1) unbound `auth_session_by_token_hash(sha256(cookie))`;
zero rows = no session; refuse `expires_at <= now` and `now - created_at >= 90 days`. (2) unbound
`auth_user_by_id(user_id)`; refuse a non-NULL `disabled_at`. (3) `begin_tenant`. (4) bound, at
most hourly, `UPDATE auth_sessions SET last_seen_at = now() WHERE token_hash = $1`. (5) sign-out
is bound `DELETE FROM auth_sessions WHERE token_hash = $1`; "everywhere" drops the WHERE clause.

**Password reset / email verify.** (1) unbound `auth_token_by_hash(sha256(token))`; refuse a
non-NULL `consumed_at`, an expired `expires_at`, and a `purpose` that is not this endpoint's.
(2) bind. (3) bound, in ONE transaction, `UPDATE auth_tokens SET consumed_at = now() WHERE
token_hash = $1 AND consumed_at IS NULL` — a zero row count means another request spent it, so
roll back — then `UPDATE users SET password_hash = $2` or `SET email_verified_at = now()`.
(4) bound `DELETE FROM auth_sessions WHERE token_hash <> $current`. Token *creation* has the
same shape: unbound `auth_user_by_email`, bind, bound INSERT into `auth_tokens`.

**OAuth callback.** (1) unbound `oauth_account_lookup($provider, $provider_account_id)`; one row →
bind and write the session. (2) zero rows → unbound `auth_user_by_email($email_from_provider)`.
One row → bind, then bound `INSERT INTO oauth_accounts (…)`, and **only** when the provider
states the email verified. Zero rows → run the sign-up steps with a NULL `password_hash`, bind,
insert the `oauth_accounts` row.

**Rate limiting** stays unbound throughout: `auth_rate_counters` carries no `user_id` and no
policy, so `INSERT … ON CONFLICT (scope, key, window_start) DO UPDATE` works at any point.
**After the bind** a plain `SELECT` on `users`, `auth_sessions`, `auth_tokens`, `oauth_accounts`
reads the caller's own rows — the profile page and the session list need no function.

---

## 4. Session state (D-S6)

### 4.1 The 1.0 document and what 2.0 keeps

`WebState` (`cadus_web/state.py:142-166`), one JSONB row per user in `web_states`
(`migrations/0004_scratch.sql:37-41`):

| Field | 1.0 | 2.0 |
|---|---|---|
| `session` | the session id the scratch belongs to; a drift resets the doc | keep |
| `served: {task_id → ServedProblem}` | `{problem_id, task_id, topic, kp, answer_kind, text, expected, solution_sketch, started_at, hints_given[], index, rework}` (`:43-60`) | keep; **`expected` becomes the pool row's `expected_answer` document, not a string** |
| `tasks: {task_id → TaskProgress}` | `{task_id, task_type, total, served, answered, done, current_kp}` (`:63-73`) | keep verbatim |
| `quizzes: {task_id → QuizBuffer}` | buffered answers only; count and completion live on `TaskProgress` (`:88-105`) | keep |
| `multistep: {task_id → MultistepBuffer}` | pre-generated parts (`:118-125`) | keep; parts come from the pool, not a model |
| `served_texts: {task_id → [statement]}` | oldest first, capped at 12 (`:133`, `:494-496`) | **becomes the D5 task memory** (M4: 12 statement hashes) — store hashes, not statements: 2.0 has no prompt to feed |
| `active_secs` | server-measured accumulator (`:155`) | keep |
| — | — | **add** `rings: {topic_id → [instance_hash; 20]}` (D5, M4) |
| — | — | **add** `pending_diagnoses: {attempt_id → job_id}` so a reconnecting client re-reads what it missed |

`served` is keyed by **task id, not problem id** (`state.py:145-151`): one problem per task is
answerable, a re-serve overwrites, a stale `problem_id` 404s. Keying by a fresh uuid per serve
left superseded problems answerable, and since `attempt_id` derived from `problem_id` the FR-14
dedup could not catch the double record — a stale quiz id appended a second answer and advanced
the cursor twice.

### 4.2 The advisory-lock pattern

1.0 serializes each tenant's read-modify-write with a transaction-scoped Postgres advisory lock
on a `web_state`-namespaced key, bounded by `lock_timeout`, derived in exactly one place
(`cadus_web/webstate.py:52-58`, `:316-395`). It is distinct from the learner-event lock.

The rule that shapes every handler: **a slow call never runs inside that lock.** Grading
(`api.py:1299-1345`), serving (`api.py:1023-1060`) and hinting (`api.py:1829-1848`) all run
*snapshot under the lock → slow work unlocked → re-open, re-validate, commit*. The re-validation
is `_validate()` (`api.py:1303-1318`): re-read the state, refuse a `done` task with
`409 task_complete`, refuse a `problem_id` that is not the task's current one with
`404 unknown_problem`.

**In 2.0 the pattern survives but its reason mostly evaporates:** no request handler makes a
model call (R4, L6), and the one remaining slow segment, the pool pop, is itself a
`FOR UPDATE SKIP LOCKED` claim. Keep one advisory lock per `(user, 'web_state')` around the whole
grade transaction — now short — and keep the `_validate` re-check, because a second browser tab
is still a live race.

### 4.3 The one-transaction grade path (D-O2)

1.0 splits the write across three units of work: `record_attempt` opens its own
(`api.py:1672-1676`), the state save is a second, the task close is a third — the direct cause of
`next_unavailable` (`docs/WEB_SERVICE.md:323-338`) and of the whole `tolerate_duplicate` /
`tolerate_closed` recovery machinery (`api.py:591-616`, `:664-683`). 2.0 collapses it into one
transaction:

```
tx = begin_tenant(pool, user_id)                       // RLS bound for every statement below
 1. SELECT doc FROM web_states WHERE user_id = $u FOR UPDATE   // the D-S6 row is the lock
 2. validate: task not done; served.problem_id == body.problem_id
 3. secs, timing_tags = measure(started_at, now, topic.expected_time_secs)   // pure CPU
 4. outcome = cadus_core::answer::check(expected, learner, kind)             // pure CPU
 5. grade   = deterministic_grade(outcome, work, secs)                       // pure CPU, §5
 6. INSERT INTO events (user_id, seq, ts, type, session_id, v, attempt_id, payload)
       VALUES (...) ON CONFLICT (user_id, attempt_id) WHERE attempt_id IS NOT NULL DO NOTHING
 7. fold:  if the new events hold a `regraded` → FULL REPLAY, else fold one event forward
    UPDATE learner_models SET model, through_seq, projector_version, config_hash, built_at
 8. pool pop for the next problem (FOR UPDATE SKIP LOCKED, ring filter, claim)   // M4
 9. INSERT INTO diagnosis_jobs (...) ON CONFLICT (user_id, attempt_id) DO NOTHING  -- if A4 applies
10. UPDATE web_states SET doc = $new, updated_at = now()
COMMIT
```

- **One transaction, one tenant.** `begin_tenant` runs `set_config('app.user_id', $1, true)`, so
  the binding is transaction-local and a pooled connection carries no tenant into the next unit of
  work (`crates/store/src/lib.rs:387-399`). Every statement above sits inside `tenant_isolation`,
  and the events INSERT's WITH CHECK refuses another tenant's `user_id`.
- **Append-only holds by grant, not convention.** `cadus_app` has SELECT + INSERT on `events` and
  no UPDATE/DELETE. A wrong grade is a later `regraded` row.
- **Idempotency moves into the database.** The M3 rule `{task_id}-{n}` plus the partial unique
  index on `(user_id, attempt_id)` makes a retried request a no-op INSERT
  (`migrations/0003_event_log.sql:24-26`). Step 6 returning zero rows means "already recorded":
  skip step 7, reply with the state read — exactly 1.0's `already_recorded` (`api.py:596`,
  `:1592-1593`) without the three-way recovery.
- **Full replay on `regraded`** (`PROGRESS.md:45-49`, `projector-1.0-spec.md:299`): forced when
  the new events hold a `Regraded` or the cached `projector_version` differs. The store layer owns
  that branch; the request path never takes it (a regrade is an admin/worker operation).
- **The diagnosis job is inside the transaction** (step 9) — never enqueued for an attempt that
  rolled back. `NOTIFY` belongs to the worker, not this handler.
- **Every statement runs under `bounded()`** (`crates/store/src/lib.rs:314-328`), so a vanished
  server cannot hang a request past the client timeout.

D-O3 (hint/teach read) is a separate single-statement path: one `content_store` read by
`(kp_id, kind, status='approved')`, cacheable in memory by digest, no transaction of its own.

---

## 5. Grading

### 5.1 The deterministic path (A3)

1.0 stops half-way. `deterministic_grade` (`deterministic_grade.py:109-152`) answers blank and
symbolically-**correct**, and returns `None` for a symbolically-**incorrect** answer, sending the
miss to the model (`:26-31`) — with the reason stated: a miss owes an error-specific diagnosis,
and inventing a misconception from the final answer alone is the fabrication Hard Rule 2 forbids.

**2.0 changes exactly that branch and nothing else.** The verdict is decided locally for right
AND wrong (A3, L2), and the diagnosis the miss owes arrives asynchronously (A4). The mapping
onto `cadus_core::answer::check(expected, learner, kind) -> Outcome`
(`crates/core/src/answer/check.rs:89`):

| Submission | 1.0 | 2.0 |
|---|---|---|
| blank / whitespace | `blank_answer_grade()`, no model call | same |
| `numeric`/`expression`, `Decided{correct: true}` | deterministic pass | same |
| `numeric`/`expression`, `Decided{correct: false}` | `None` → model grades | **deterministic miss + async diagnosis** |
| `Undecidable` (input cap, grammar exit) | n/a | deterministic **miss**, `error_tags: []`, async diagnosis; never a model verdict |
| `multi-step` / `proof` | `None` → model grades | **no synchronous verdict at all** — the task type never reaches the pool (V2 keeps it out), so M5 has no such route to serve |

Kind coverage: 478 of 1,090 topics (44 %) are `numeric` or `expression`
(`docs/WEB_SERVICE.md:365`); under A6 a KP with no approved template serves exemplars, whose
answers are authored and decidable.

### 5.2 The work-quality tier

`nearly_perfect` (`deterministic_grade.py:72`), and the honesty argument is the whole reason
(`:35-52`): it is the NEUTRAL tier — XP multiplier `1.0`, FIRe `q = 0.85`, above
`PASS_QUALITY_THRESHOLD = 0.7`. `perfect` (×1.3) is a bonus for method the checker never reads,
so it is never awarded; the tiers below are penalties for flaws it never observes, so they are
never imposed. The live inconsistency that removes: the deployed model returned `nearly_perfect`
once and `poor` + `["incomplete"]` the other time for the same shape of submission — and `poor`
is XP ×0.0, FIRe `q = 0.15`, i.e. a correct answer recorded as a failure.

For a **wrong** answer 2.0 must now pick a tier the model used to pick. Proposal: `passable` for
a decided miss with no other signal, `blowoff` for a blank (1.0 stamps `poor` there — `:103`;
`blowoff` matches `_complete_task(abort=True)`). **Open decision D-M5-2.** Rushing is **not**
folded in: `cadus.xp.is_rushing` owns it in the core and fires only on a wrong answer, because a
correct fast answer is fluency, not rushing (`:47-50`). Timing tags are not added here either —
`_measure_secs` produces them and the caller appends them (`:51-52`).

### 5.3 The error-tag vocabulary

`prompts.py:50-63`, 11 tags; the grade schema constrains the model to them (`:220-223`) and
`coerce_error_tags` drops anything outside (`:1133-1142`):

```
sign-error, arithmetic-slip, algebra-slip, wrong-method, formula-recall,
misread-problem, incomplete, notation, units, timing-unreliable, blowoff
```

Two are produced by the *server*, never the model: `notation` (the dot-thousands variant,
`deterministic_grade.py:146-149`) and `timing-unreliable` (`api.py:740`). A correct verdict
carries no tag except `notation`, a claim about form, not mathematics.

**Trap:** `blank_answer_grade` emits `"blank_answer"` (`deterministic_grade.py:104`), which is
**not** in `ERROR_TAGS` and is the only underscore-spelled tag. It bypasses `coerce_error_tags`
and reaches the append-only log. 2.0 must either add `blank-answer` to the vocabulary or drop the
tag; it must not ship the 1.0 inconsistency.

### 5.4 The H3 re-solve rule

`api.py:1361-1379`, `:1520-1532`. An attempt is **reference-assisted** when `served.hints_given`
is non-empty or the client set `assisted` (`:1360`). An assisted attempt that grades **correct**
is *not recorded*: the payload is stashed in `served.rework`, the reply is
`{rework_required: true, problem_id, solution, expected}`, and the problem stays live. The next
submission is the unaided re-solve: the stashed payload is recorded under `-rework`, and **if the
re-solve is wrong the stashed pass is rewritten to `correct: false` with its `assisted` flag
dropped** (`:1373-1375`) — the assisted pass does not stand. 2.0 ids: `{task_id}-{n}` and
`{task_id}-{n}-rework`.

### 5.5 The stock re-solve instruction (T4 fallback, A4 immediate text)

1.0 has no literal: the requirement lives inside `GRADE_SYSTEM` prose (`prompts.py:573-582`,
PEDAGOGY pp. 427, 431) and the model rewrites it every time. 2.0 needs a constant, because the
verdict now ships before any prose exists. Proposed literal, pinned by a test:

> Study the worked solution above until you can see why each step follows. Then close it and
> solve the original problem again yourself, from memory and unaided. Do that before you move on.

Served on every miss and on every assisted-correct `rework_required` reply — the L2/L3 guarantee
that the learner is never blocked on a model.

### 5.6 Timing

`_measure_secs` (`api.py:729-741`): `secs = round(max(0, now - started_at))`, **clamped** to
`topic.expected_time_secs * 10` and tagged `timing-unreliable` above it. `started_at` is
(re)stamped at every hand-off, including a re-serve of a live problem (`api.py:563`), because that
is the moment the problem goes on screen — before this fix a lesson stamped it at *generation*, so
the teach round trip and the reading of the worked example were billed as solve time (a live
`Compute $8 - 5$.` attempt recorded `secs: 166`, hit the cap, and had its `work_quality` and hence
its XP downgraded for it — `docs/WEB_SERVICE.md:129-138`).

### 5.7 Input caps

`_MAX_ANSWER_CHARS = 4000`, `_MAX_WORK_CHARS = 20000` (`api.py:91-92`); over either →
`413 answer_too_large` (`:1292-1293`). 2.0 keeps both: the first stated reason (LLM cost) is gone,
the second (checker CPU on hostile input) is not, and `answer::check` has its own length cap that
returns `Undecidable` (`crates/core/src/answer/check.rs:97`).

---

## 6. The diagnosis worker (A4)

### 6.1 Claim (D-O5)

1.0 has no `SKIP LOCKED`. It claims with a conditional UPDATE plus a **fencing token**
(`cadus_pg/repos.py:396-424`), and every completion is guarded on `status='sending' AND
attempts = <the value the claim returned>` (`:425-501`); stale claims are reclaimed by lease age
(`:503-530`, lease 15 min; `cadus_worker/email.py:57-58`: `MAX_SEND_ATTEMPTS = 8`). That
machinery exists because dramatiq redelivers. **2.0 does not need it** — the worker picks its own
row:

```sql
WITH job AS (
  SELECT id FROM diagnosis_jobs
   WHERE status = 'pending'
   ORDER BY created_at
   FOR UPDATE SKIP LOCKED
   LIMIT 1)
UPDATE diagnosis_jobs d
   SET status = 'running', claimed_at = now(), attempts = attempts + 1
  FROM job WHERE d.id = job.id
RETURNING d.id, d.user_id, d.attempt_id, d.payload, d.attempts;
```

The index `diagnosis_jobs_pending (created_at) WHERE status = 'pending'` serves it
(`migrations/0005_content.sql`). The worker connects as `cadus_admin`, which holds BYPASSRLS, so
the claim crosses tenants (`docs/SCHEMA.md`). Keep the `attempts` counter: it is the dead-letter
rule, not a fence — at `attempts >= 3` the row goes to `failed` and the client is told so. A row
stuck in `running` past a 5-minute lease is reset to `pending` by the same sweep that 1.0 runs
(`cadus_worker/sweeps.py:72`, debounce 2 min, cadence 5 min).

### 6.2 What skips the call

Before anything is claimed, the *request* path checks `content_store` for a kind-`diagnosis` row
whose body names the learner's exact wrong answer (the M4 distractor analysis). A hit returns
inline as `diagnosis.status = "ready"` and **writes no job row** (A4, last clause); only a miss
enqueues. That is the difference between a bill that scales with attempts and one that scales
with distinct misconceptions.

### 6.3 The prompt and the expected JSON

Reuse 1.0's grade contract minus the verdict: `GRADE_SYSTEM` (`prompts.py:540-584`) with the
`'correct'` paragraph (`:543-547`) and the timing paragraph (`:565-568`) deleted — 2.0 knows both
already — and the mandatory-re-solve paragraph (`:573-582`) kept. The user message is
`grade_user`'s shape (`:885-897`) plus the verdict the server reached:

```
Problem: …
Correct final answer (reference): …
Answer kind: numeric
Learner's answer: '…'
Learner's shown work: … | (none provided)
The answer is WRONG; the server decided that. Do not restate the verdict.
Name the misconception and write the diagnosis via the emit_diagnosis tool.
```

Expected JSON, one forced tool `emit_diagnosis`, `additionalProperties: false`:

```json
{ "error_tags": ["sign-error"],     // enum = the §5.3 vocabulary, filtered again server-side
  "prose": "1-3 brisk sentences; math in $...$",
  "confidence": "high" | "low" }    // low ⇒ store it, do not show tags
```

Required: `error_tags`, `prose`. Filter the result through the vocabulary before writing it — the
1.0 lesson (`prompts.py:529-536`): a tag the prompt invites and the filter lacks is dropped
silently, and the diagnosis is lost with no error anywhere.

### 6.4 The request (T5 defaults, both endpoints)

Request shape (`openai_engine.py:561-600`): `POST {base}/chat/completions`, body `{model,
max_tokens, messages:[{role:"system"},{role:"user"}], tools:[…],
tool_choice:{type:"function",function:{name}}, temperature}`; headers
`Authorization: Bearer $OPENAI_API_KEY`, `Content-Type: application/json`, `X-Title: Cadus`
(`:634-641`); client timeout 60 s (`:125`); `temperature = 0.0` for grading-shaped calls (`:465`).

| Knob | 1.0 default | 2.0 default (T5) |
|---|---|---|
| `provider.order` | unset (`config.py:116`) | **pinned**, `OPENROUTER_PROVIDER_ORDER`, non-empty in the shipped env |
| `provider.allow_fallbacks` | `true` (`config.py:122`) | `true` — a hard pin turns one provider's outage into a dead worker |
| `reasoning.max_tokens` | unset (`config.py:129`) | **600** |
| prompt caching | **absent in 1.0** — nothing sets a caching flag anywhere | `cache_control` on the system block (Anthropic-style) / `prompt_cache_key` (OpenAI-style); on OpenRouter the system prefix is cached automatically once the provider supports it |
| `max_tokens` | per-op literal, 1024 for grade (`:466`) | 600 output (T4 knob), escalated ×4 on a truncation retry |

`provider` and `reasoning` are **OpenRouter extensions**: posting them to `api.openai.com` earns a
non-retryable 400. 1.0 gates them on a parsed hostname, never a substring
(`openai_engine.py:133-148`) — `"openrouter.ai" in url` also matched
`https://openrouter.ai.attacker.example/v1`. Port the parsed check. **The local Qwen 3.6
endpoint** (`http://10.8.0.3:8080/v1`, O2's later pivot) is the same code path with the routing
block empty (`:172-173` returns `{}` for a non-routing host), so the body is the plain OpenAI
shape. Nothing else changes — that is the value of one client.

### 6.5 Retries, timeouts, truncation

`MAX_ATTEMPTS = 2`, `DEFAULT_BACKOFF_SECS = 0.5`, doubling (`engine.py:57-58`, `:128-175`).
Retryable: HTTP 429, HTTP ≥ 500, transport errors (`openai_engine.py:385-390`). A schema-shaped
reply that fails validation also retries; a persistent one is a hard failure.

**A truncated reply is not a malformed one.** A reasoning model shares its completion budget with
its hidden reasoning, so the forced tool call is cut off before a single visible byte: measured
live, `completion_tokens=1024`, `reasoning_tokens=1083`, `finish_reason="length"`, no
`tool_calls` at all (`openai_engine.py:268-285`). An identical retry reproduces it, so the retry
carries the **attempt index** and widens the ceiling ×4 (`engine.py:111-125`,
`openai_engine.py:549-552`). Truncation has three shapes, all mapping to the widened retry: no
tool call (`:325-328`), arguments cut off mid-JSON (`:306-314`), and arguments that parse but
miss a required field (`:368-380`) — the last checked **only after validation fails**, so a
complete reply is never discarded for its `finish_reason`.

Parsing tolerates a model that ignores the forced tool and emits the object as message `content`
(`:317-321`), and strips a markdown fence (`:241-251`). Port both.
A final failure sets `status = 'failed'` and the client is told. **The learner already holds the
verdict, the solution and the re-solve instruction, so a failed diagnosis costs prose and nothing
else.**

### 6.6 The T4 knobs

O2: no caps. Both knobs exist and default to `0 = unlimited`:

| Knob | Default | Meaning |
|---|---|---|
| `DIAGNOSIS_CALLS_PER_SESSION` | `0` | max jobs per `session_id`; `0` = unlimited |
| `DIAGNOSIS_OUTPUT_TOKENS` | `600` | per-call output cap — a **latency** bound (T5), not a spend cap, so it keeps a real value |
| `DIAGNOSIS_REASONING_MAX_TOKENS` | `600` | T5, same reason |

When a configured cap is hit the row is written `status = 'capped'` and the client still gets the
deterministic verdict plus the §5.5 stock instruction — a cap must never withhold the verdict.

---

## 7. The model-call log (T6)

`migrations/0005_content.sql` defines the row; 1.0 has **no equivalent** — nothing in
`cadus_web/` reads `usage` from any response. This is net-new work.

| Column | Source |
|---|---|
| `id bigserial` | — |
| `ts timestamptz` | call start |
| `purpose text` | `'diagnosis'` \| `'authoring'` — T2 names no third spender |
| `model_id text` | the id the request sent, not the one configured |
| `provider text` | OpenRouter's `provider` field on the response body; NULL elsewhere |
| `user_id uuid` | the job's tenant; NULL for an offline authoring call |
| `session_id text` | the attempt's session; feeds the per-session T4 count |
| `input_tokens_cached` | see below |
| `input_tokens_uncached` | see below |
| `output_tokens` | `usage.completion_tokens` minus reasoning where the provider double-counts |
| `reasoning_tokens` | `usage.completion_tokens_details.reasoning_tokens` |
| `latency_ms integer` | wall clock around the HTTP call, per attempt |
| `cost_usd numeric(12,6)` | exact money, never a float |
| `request_id text` | the provider's `id` field, for a support ticket |

Reading tokens from an OpenAI-compatible reply:

```
usage.prompt_tokens                                  -> total input
usage.prompt_tokens_details.cached_tokens            -> input_tokens_cached  (0 when absent)
total input - cached                                 -> input_tokens_uncached
usage.completion_tokens                              -> output (gross)
usage.completion_tokens_details.reasoning_tokens     -> reasoning_tokens     (0 when absent)
```

Every field is optional on some provider, so each read defaults to `0` and **a missing `usage`
block writes a zeros row with a `NULL` cost**, never a dropped row: an unmeasured call must be
visible as unmeasured. `cost_usd` comes from OpenRouter's `usage.cost` when the body carries it,
else from a per-model price table in configuration, else `NULL`.

**One row per HTTP attempt, not per job** — a truncation retry is two calls and two bills.
`cadus_app` holds **no privilege** on this table or its sequence (`docs/SCHEMA.md`, findings #5
and #12), so only the worker (`cadus_admin`) writes it and the table stays outside RLS on that
basis; an operator dashboard reads it through an admin connection.

New `/metrics` series: `cadus_deterministic_grade_total{result=correct|notation|blank|incorrect|undecidable}`
(1.0: `metrics.py:109-114`), `cadus_diagnosis_jobs_total{result=ready_preauthored|enqueued|done|failed|capped}`,
`cadus_model_call_tokens_total{purpose,kind=cached|uncached|output|reasoning}`,
`cadus_model_call_latency_seconds{purpose}`. Keep `cadus_http_requests_total` and
`cadus_http_request_duration_seconds` labelled by **route template**, never the concrete path
(`metrics.py:169-178`) — the L\* benchmarks read those. Drop `cadus_grade_cache_total` and
`cadus_problem_template_total`: 2.0 has no runtime grade cache.

---

## 8. Budgets — route to L\* line

Style follows `serving-1.0-spec.md:540-550`; this table belongs in `docs/reference/l1-budget.md`,
which M4 U5 creates and M5 extends.

| Route | Line | Budget | Split |
|---|---|---|---|
| POST `/api/task/{id}/serve` | **L1** | 150 ms | arena+scheduler 5 ms · render/eval/hash/ring 5 ms · Postgres (model read, pool pop, claim, state write, commit) 100 ms · framework+session+RLS 40 ms |
| POST `/api/task/{id}/answer` | **L2** (verdict) / **L3** (prose) | 300 ms | `answer::check` 5 ms · Postgres (one INSERT, two UPDATEs, one pool pop, one job INSERT) 150 ms · framework+session+RLS 40 ms · 105 ms headroom |
| POST `/api/task/{id}/teach` | **L4** | 150 ms | one `content_store` read by `(kp_id,'teach','approved')` 20 ms · in-memory digest cache hit ~0 · framework 40 ms · 90 ms headroom |
| POST `/api/task/{id}/hint` | **L5** | 150 ms | one `content_store` read (`hint_ladder`) 20 ms · state write 60 ms · framework 40 ms · 30 ms headroom |
| POST `/api/diag/answer` | **L2** | 300 ms | same split as answer, minus the pool pop |
| GET `/api/diagnosis/{id}` | — | 100 ms | one indexed row read; it is a poll, so it must be cheap |
| GET `/api/diagnosis/stream` | — | n/a | long-lived; the budget is the *notify-to-flush* delay, target < 250 ms |
| GET `/api/status`, `/api/session/plan` | — | 300 ms | one model read + scheduler compose; not an L\* line, but the dashboard is on the critical path of every session start |
| GET `/api/graph` | — | 500 ms | ~1,100 nodes, layout precomputed server-side |
| POST `/api/session/start`, `/end` | — | 300 ms | one event append + one fold |
| GET `/api/export` | — | streamed | a full per-user scan; never in a p95 |
| **Every route above** | **L6** | **0 model calls** | asserted by a test that fails the build if a handler crate can reach the model client |

**T1 as a merge gate:** serve, teach, hint and the whole grade path spend **0 model tokens**; the
only spenders are the worker's diagnosis job and the offline authoring pipeline (T2). Enforce it
with a crate boundary — `cadus_web` does not depend on the model-client crate at all, so L6 is a
compile error, not a review note.

---

## 9. Parity traps and 2.0 decisions to make

**Traps carried from 1.0.**

| # | Trap | Cite |
|---|---|---|
| W1 | `blank_answer` is not in the error-tag vocabulary and is the only underscore-spelled tag; it bypasses the filter into the append-only log | `deterministic_grade.py:104` vs `prompts.py:50-63` |
| W2 | `why` is display copy **and** scheduler control state — the selector re-parses substrings (`"nearly-due"`, `"remediation"`), so a copy edit silently changes which tasks survive a re-serve | `docs/WEB_SERVICE.md:182-193` |
| W3 | `progress.done` is load-bearing: a **failed** review stays due and is recomposed under the same `task_id` while the state row has it closed, so a client that tracks completion itself gets `409 task_complete`. Listing the plan must stay a pure read (`_plan_progress`, never `_progress_for`, which installs a row per task merely listed) | `docs/WEB_SERVICE.md:310-321`, `api.py:940-945`, `:959-976` |
| W5 | `next_unavailable` exists because a bare `next: null` on an unfinished task reads as "task over"; 2.0 keeps the flag even though its cause (a model call after the commit) is gone | `docs/WEB_SERVICE.md:323-338` |
| W6 | The `_next_serve_after_answer` guard catches **every** exception on purpose — a narrower tuple let `re.PatternError` and `ValueError` past it, after the attempt was committed | `api.py:1686-1699` |
| W7 | Quiz batch reveal: no feedback and no `expected` in any pre-reveal body — test by scanning raw JSON, not by reading fields. Hints never leak the answer either (`hint_user` omits `problem.expected`) | `docs/WEB_SERVICE.md:25-27`, `:716-718`; `prompts.py:900-902` |
| W9 | A `__Host-` cookie at any path but `/` is **silently discarded** by the browser — invisible in every log | `cookies.py:62-84` |
| W10 | The CSRF origin check depends on `--proxy-headers` staying in the compose command; drop it and every https request reconstructs as `http://`, so a genuine same-origin `Origin` stops matching. Nothing in 1.0 asserts the flag | `app.py:124-130` |
| W11 | `_is_same_origin` fails **closed**, `_is_explicit_cross_origin` fails **open** — deliberately not each other's negation | `app.py:173-191` |
| W12 | Rate-limit `prefix` strings are persisted data — changing one resets that rule's live windows | `routes.py:78-80` |
| W13 | `events.payload` is `jsonb` in 2.0, so key order is not preserved; the projector must not depend on it | `docs/SCHEMA.md`, "Two deliberate differences" |
| W14 | `LISTEN/NOTIFY` bypasses RLS entirely — the payload may carry ids only, and the handler re-reads under the tenant binding | new in 2.0 |

**Decisions the owner or the orchestrator must make before M5 units start.**

| ID | Decision | Default if unanswered |
|---|---|---|
| D-M5-1 | Push mechanism: SSE + poll fallback (proposed), or poll only | SSE + poll |
| D-M5-2 | `work_quality` for a deterministic **miss** — `passable` for a decided miss and `blowoff` for a blank (proposed), or a single tier for both | `passable` / `blowoff` |
| D-M5-3 | The stock re-solve text (§5.5) — approve the literal | ship the proposed text, pinned by a test |
| D-M5-4 | `error_tags` on a deterministic miss: always `[]` until the diagnosis lands, or stamp `timing-unreliable` alone | `[]` plus timing tags only |
| D-M5-5 | Keep the bearer channel (`Authorization: Bearer` + `Accept-Session-Token: true`) for scripts and tests; Anki and worksheet routes defer to a later milestone | keep the bearer (the CSRF exemption depends on it); defer both route families |
| D-M5-6 | `/api/ready`: drop `redis` and `worker.heartbeat`, or keep a worker liveness probe on a Postgres row | drop `redis`; keep liveness, sourced from `diagnosis_jobs` claim age |
| D-M5-7 | Add `blank-answer` to the vocabulary, or drop the tag (W1) | add it, hyphenated |

---

## 10. Pinned literals from the 1.0 tests

Each must be a **literal** in the 2.0 test, never re-read from the constant under test
(HANDOVER §3).

| Literal | Value | Cite |
|---|---|---|
| Error envelope | `{"error": {"code", "message"}}`; `502 tutor_unavailable`; `422 invalid_request` | `errors.py:30-31`, `:43-55` |
| Unauth / unknown course / teach on a non-lesson | `401` on missing, wrong-scheme and `Basic`; `404 unknown_course` on all four routes; `409 no_instruction` | `test_web_api.py:272-274`, `:542-543`, `:1579-1580`, `:635-636` |
| Serve/answer a closed task | `409 task_complete` | `test_web_api.py:1701-1710`, `:1828-1829`, `:1923-1924`, `:2286-2287` |
| Stale problem id / hint in a quiz | `404 unknown_problem` (answer AND hint) / `409 no_hints_in_quiz` | `test_web_api.py:1880-1888`, `:1951-1952` |
| CSRF | `403 cross_origin_rejected` on a cookie-authed cross-site write; a bearer write is not 403'd | `test_web_security.py:193-213`, `app.py:292` |
| Login-CSRF | `403` on cross-origin POST to `/api/auth/login`, `/signup`, `/verify-email` | `test_web_security.py:255-279` |
| Cookie string | contains `__Host-cadus_session=`, `httponly`, `secure`, `samesite=lax`; the delete matches attribute-for-attribute | `test_web_security.py:137-169` |
| Rate limit | `429`; signup 3/h email + 5/h IP, counted independently — the 4th for one address and the 6th overall are the first refused | `test_authn_endpoints.py:372-387` |
| Login / forgot / resend limits | 10 + 30 per 5 min; 3 + 10 per hour (both) | `routes.py:102-104`, `:414` |
| Bad / reused token | `400 invalid_token` both times | `test_authn_endpoints.py:190`, `:283-293` |
| Weak password / wrong current password | `422 weak_password` / `401 invalid_credentials` | `test_authn_endpoints.py:249`; `test_authn_reset.py:175`, `:270` |
| Signup / unverified login | always `{"status": "verification_required"}`, no cookie, never `409 email_taken`; an unverified login is the same generic `401 invalid_credentials` as a wrong password; an unknown-email login spends a real Argon2 verify, within 3× of the known-email path | `routes.py:319-325`; `docs/WEB_SERVICE.md:51-58`; `test_authn_endpoints.py:409-428` |
| OAuth | unconfigured → `404 not_found` on start and callback; failure → `400 oauth_error` with no session opened | `test_authn_oauth.py:165-177`, `:246-262`, `:592-598` |
| Lifetimes | session idle 30 d, absolute 90 d, touch 1 h, reset 30 min, verify 24 h, OAuth handshake 600 s | `service.py:43`, `:48`, `:375`, `:436`; `deps.py:40`; `oauth.py:68` |
| Argon2id prod / password / token | `3 / 65536 KiB / 4`; 8–256 chars, ≤1024 bytes; 32 bytes → 43 chars | `passwords.py:55`, `:33-37`; `tokens.py:21` |
| Answer / work caps | 4,000 / 20,000 chars → `413 answer_too_large` | `api.py:91-92`, `:1292-1293` |
| Stuck-hint threshold / served-text memory | 3 (review + multi_step only) / 12 | `api.py:97`, `:1856-1859`; `state.py:133` |
| Timing cap | `expected_time_secs * 10`, then tag `timing-unreliable` | `api.py:737-740` |
| Deterministic tier / note | `nearly_perfect`; blank → `poor` + `["blank_answer"]`; `grader_note="deterministic"` | `deterministic_grade.py:72`, `:103-104`, `:68` |
| Fast-path cases / error tags | `12`/`12`, `12`/`12.0`, `12`/`sqrt(144)`, `7329`/`7,329`, `7400`/`7400`, `2*x+1`/`1 + 2x` all decide with no engine; the 11-tag tuple, in order | `test_deterministic_grade.py:43-62`; `prompts.py:50-62` |
| Retry contract | 2 attempts, 0.5 s backoff doubling, ×4 budget escalation, 60 s client timeout; truncation is `finish_reason` or `native_finish_reason == "length"` | `engine.py:57-58`, `:111-125`; `openai_engine.py:125`, `:265`, `:282-285` |
| Ready | a configured dep reporting `"down"` → `ok:false` → `503`; a heartbeat > 60 s is a warning, never a 503 | `health.py:44`, `:139-167` |
| One transaction / duplicate attempt | the record cycle and the complete cycle each run inside exactly one `store.transaction()`; a replayed `attempt_id` appends nothing and raises `attempt_already_recorded`, and the caller must opt into recovery | `test_service.py:187`, `:230`, `:345-364`; `api.py:591-616` |

---

## 11. Proposed unit breakdown for M5

Each unit is ≤ ~400 changed lines with its acceptance check named up front.

| Unit | Deliverable | Acceptance check |
|---|---|---|
| **U1** | `cadus_web` axum skeleton: `create_app`, the `{"error":{code,message}}` envelope, the security-header layer (§3.1 literals), the CSRF origin layer (both polarities), the request-metrics layer labelled by route **template**, `/api/health`, `/api/ready`, `/metrics`; `assert_rls_enforced` and the cookie-posture guard at boot | a cookie-authed cross-site POST to `/api/*` is `403 cross_origin_rejected` and a bearer one is not; a cross-origin POST to `/api/auth/login` is 403; a header-less curl POST is not; an unmatched path is one `__unmatched__` metric label; boot refuses a BYPASSRLS role |
| **U2** | `cadus_web::auth` primitives: Argon2id profiles, the password policy, `generate_token`/`hash_token`/constant-time compare, email normalization, the cookie writer/reader/clearer with the `__Host-`/path guard, the bearer-then-cookie selector | the §10 Argon2 and token literals; a `__Host-` cookie at `/api` raises; `Authorization: Bearer` with an empty value selects nothing; NFKC+strip+lower is idempotent |
| **U3** | `cadus_store::auth`: the five SECURITY DEFINER lookups and the bound writes, in the §3.3 call order, plus `auth_rate_counters` upsert | a bound caller gets zero rows from all five functions; sign-up with `RETURNING` fails and without it succeeds; a session INSERT naming another `user_id` is refused by WITH CHECK; a token UPDATE that returns zero rows rolls the transaction back |
| **U4** | `/api/auth/*` routes: signup, login, logout, logout-all, me, password change/forgot/reset, verify-email, verify-email/resend; the four paired rate rules; the anti-enumeration dummy verify | every §10 auth literal, including the independent email/IP counters (4th and 6th requests) and the ≤3× timing bound on an unknown email |
| **U5** | OAuth: Google + GitHub, PKCE S256, constant-time `state`, the path-scoped 600 s handshake cookie, the §3.3 callback order, link-only-on-verified-email | unconfigured provider → `404 not_found`; a mismatched `state` → `400 oauth_error` with no session; an unverified provider email never links |
| **U6** | `cadus_web::state`: the D-S6 document (§4.1) with the D5 ring and task memory, the per-tenant advisory lock, the snapshot/validate/commit shape, session start/end/plan, `/api/status`, `/api/graph`, `/api/enroll`, `/api/modules`, `/api/export` | a second concurrent tab answering a closed task gets `409 task_complete`; listing the plan writes nothing; `progress.done` is true for a recomposed failed review; export round-trips through the event reader |
| **U7** | Serve + teach + hint: pool pop against M4, `served` keyed by task id, `started_at` re-stamped on every hand-off, the authored teach page and hint ladder from `content_store`, the 3-hint reference-lesson escalation | a re-serve returns the same `problem_id` and a fresh `started_at`; a stale id 404s on both answer and hint; a hint body never contains `expected`; teach on a review is `409 no_instruction` |
| **U8** | The one-transaction grade path (§4.3): normalize + `check`, the deterministic tier, `{task_id}-{n}` ids, the ON CONFLICT no-op, incremental fold with full replay on `regraded`, the H3 re-solve rule, timing and the caps | the §10 fast-path cases decide with no model client linked; a replayed request appends nothing and returns `already_recorded`; an H3 re-solve that fails rewrites the stashed pass to a miss; `secs` clamps at `expected_time_secs * 10` with `timing-unreliable` |
| **U9** | A4 client surface: the `diagnosis` field on the grade reply, the pre-authored distractor lookup, `diagnosis_jobs` enqueue inside the grade transaction, `GET /api/diagnosis/{id}`, `GET /api/diagnosis/stream` over `LISTEN/NOTIFY` with the poll fallback | a matching distractor returns `status:"ready"` and writes no job row; a rolled-back grade leaves no job row; a NOTIFY for tenant A never reaches tenant B's stream; a poll for another tenant's id is `404 unknown_diagnosis` |
| **U10** | `cadus_worker::diagnosis`: the `SKIP LOCKED` claim, the dead-letter and lease sweep, the OpenAI-compatible client with the T5 defaults, the three truncation shapes, the retry contract, the vocabulary filter, `NOTIFY` on completion | two workers × 100 claims never take the same row; a `finish_reason:"length"` reply retries with a ×4 budget and the second reply is accepted; a 400 does not retry; a tag outside the vocabulary is dropped; three failures dead-letter the row |
| **U11** | T6: the `model_call_log` writer, the token reader (cached/uncached/output/reasoning, all defaulting to 0), cost, one row per HTTP attempt, the new `/metrics` series | a reply with no `usage` block writes a zeros row with a NULL cost; a truncation retry writes two rows; `cadus_app` cannot read, write, or `nextval` the table or its sequence |
| **U12** | Budgets: extend `docs/reference/l1-budget.md` with the §8 table; benchmark A (core: check + tier + hash) and benchmark B (the grade transaction against the CI Postgres); the L6 crate-boundary test; CI step serialized after `cargo test` | A: p95 < 5 ms and the allocation bound; B: grade p95 < 150 ms, serve p95 < 100 ms; the L6 test fails when a handler crate is given a model-client dependency |

**Sequence.** U1 → (U2 ∥ U6) → (U3 → U4 → U5) ∥ (U7 → U8) → (U9 → U10 → U11) → U12 → mutation
check → adversarial review (2 rounds, major+) → fixes → close.

**Oracle parity (HANDOVER §2.5).** The grade path is core logic: U8's checker and tier decisions
run against the 1.0 corpus through the M2/M3 harness, and the event stream a 2.0 session records
is diffed against the 1.0 shape field by field, `attempt_id` excepted (M3 trap T12).
