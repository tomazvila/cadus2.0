# Cadus 2.0 — ground audit for a new implementation assignment

Date: 2026-09-06. Repository: `/home/deploy/dev/cadus2.0`. Branch: `quality/gate`.
HEAD: `d650364` ("Merge branch 'quality/u5-web' into quality/gate").
`main` is 177 commits behind HEAD and 0 commits ahead.
Working tree: two modified files, `web/index.html` and `web/src/styles/app.css`.

This audit is read only. It ran no build, no test, and no database query.

---

## 1. Instructions in force

### 1.1 What does not exist

The repository holds **no `AGENTS.md` and no `CLAUDE.md`**, at the root or in any
subdirectory. The only agent-facing instructions are the documents below plus the
user-level global instructions outside the repository.

### 1.2 The documents

| Path | What it prescribes | Applies to code work on a new branch |
|---|---|---|
| `REQUIREMENTS.md` (root) | The authority. Stable requirement IDs C1–C6, L1–L6, T1–T6, A1–A7, R1–R5, V1–V4, D1–D9, D-S1–D-S6, D-O1–D-O6, O1–O3. Cite the IDs in reviews and commits. Binds: grade honesty (C4), append-only events (C2), human review gate for authored content (C6), zero model tokens on the request path (T1, L6), core purity (R3), decidable answer grammar (V1, V2). | **Yes. It wins over every other document.** |
| `HANDOVER.md` (root) | The build process. Role split (orchestrator plus Opus subagents), the six-stage build cycle per milestone, worktree isolation for parallel agents, the gate command `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` plus `cargo sqlx prepare --check` for SQL units, "tests are not self-oracles", "one full gate suite at a time on this box", PR-size units (about 400 changed lines). Agents never run `git stash`, `git reset`, or `git checkout -- .`. | **Yes**, for process: unit size, worktree isolation, gate before integration. |
| `README.md` (root) | The gate and the deployment rules. `scripts/gate.sh` before every commit. `CADUS_TEST_DATABASE_URL` is required, not optional. Migrations are forward-only; after a new `migrations/NNNN_name.sql`, run `scripts/check_migrations.sh --freeze` and commit `migrations/CHECKSUMS`. Deploy with `scripts/deploy.sh`, never with `docker compose up -d` on a live stack. | **Yes.** |
| `PROGRESS.md` (root, 624 lines) | One entry per milestone cycle, newest first. M0 to M6 are closed. It holds the open-item backlog and the M7-P placement close of 2026-09-01. | Read only, for context and for the open backlog. |
| `NOTES.md` (root) | Four open owner questions, informal. It names the entry point `crates/web/src/bin/cadus-web.rs`. | No binding rule. |
| `docs/DECISIONS.md` | Owner decisions O1, O2, O3 and the later rulings D-M5-8 and **D6-dec** (the decimal-rounding rule). One line per decision, with the date and the effect. | **Yes**, for any change to grading or to the checker. |
| `docs/SCHEMA.md` | The database schema and the M5 auth contract (the call order of the five SECURITY DEFINER lookups before a tenant bind). | Yes, for store or auth work. |
| `docs/SELF_HOST.md` | The deployment runbook: the upgrade order, the proxy ports, the operator flags. | Yes, for ops work. |
| `docs/plans/M0.md` … `M6.md` | The per-milestone unit breakdown, each with its acceptance check. | Historical. |
| `docs/plans/QUALITY.md` | **The active plan.** Ten code limits over the whole tree: lines per file < 500, cyclomatic complexity < 22, cognitive complexity < 22, Halstead difficulty < 80, coverage 100 % of `src/`, CRAP < 25, dead code 0, redundant code 0, `any`/`unknown` 0. `scripts/quality.sh` runs every check. Units work in a worktree on a branch from `quality/gate`. The owner dropped mutation testing on 2026-09-05. | **Yes. New code on a new branch must hold these limits.** |
| `docs/reference/curriculum-1.0-spec.md` | The curriculum file schema and the loader rules (C5, D1, D2). | Yes, for curriculum work. |
| `docs/reference/checker-1.0-spec.md` | The 1.0 checker behavior and the 2.0 divergences. Section 3.2 holds the two decimal rules side by side. | Yes, for checker work. |
| `docs/reference/projector-1.0-spec.md` | The fold, the parity traps T1 to T21. | Yes, for projector work. |
| `docs/reference/serving-1.0-spec.md` | The serving pool, templates, anti-repeat. | Yes, for serve work. |
| `docs/reference/web-service-1.0-spec.md` | The HTTP contract, section by section. Sections 4.3 and 5 hold the grade path. | Yes, for route work. |
| `docs/reference/authoring-and-spa-1.0-spec.md` | The authoring pipeline and the SPA, units R1–R8 and S1–S14. | Yes, for worker or SPA work. |
| `docs/reference/l1-budget.md` | The L1 and L2 benchmark method. `scripts/bench.sh` runs it. | Yes, for hot-path work. |
| `docs/reference/undecidable-answers.md` | The measured V2 residue: 265 of 3,492 corpus answers leave the grammar, in 18 named groups, each with a recommended action. | **Yes. It is the direct input for answer-contract work.** |
| `docs/reviews/M0-*.md` … `M6-*.md` (18 files) | The adversarial review rounds and their confirmed findings. | Historical. |

### 1.3 Writing rules in force

The user-level global instruction file (`/home/deploy/.claude/CLAUDE.md`) makes
ASD-STE100 Simplified Technical English mandatory for every output: chat replies,
commit messages, PR bodies, code comments, docstrings, error messages, and
documentation. Active voice. No modal verbs. No `-ing` forms as verbs. Maximum 20
words per procedural sentence and 25 per descriptive sentence. American spelling.

Project memory adds: no self-descriptive phrases, no undefined shorthand, define a
term once.

### 1.4 Branching, approval, and deployment rules that bind new code work

1. **Branch.** Work on a branch from `quality/gate`, in a worktree of its own
   (`HANDOVER.md` §1, `docs/plans/QUALITY.md`). Do not work on `main`.
2. **Gate.** `scripts/gate.sh` before every commit. It runs `cargo fmt --all
   --check`, `cargo clippy --all-targets --workspace -- -D warnings`, `cargo test
   --workspace`, the route-table fixture, `cargo test --release -p cadus-core
   --test parity_events --test projector`, `scripts/bench.sh`, `cargo sqlx prepare
   --check`, `scripts/check_migrations.sh`, and `scripts/check_ops.sh`.
   `CADUS_TEST_DATABASE_URL` is required; the gate exits 2 without it.
3. **Quality limits.** `scripts/quality.sh` holds the ten limits of
   `docs/plans/QUALITY.md`.
4. **Content approval.** Authored content enters `content_store` as `pending` and
   never serves. A human approves it by digest (C6). No code path writes
   `approved` directly.
5. **Migrations.** Forward only. Add `migrations/NNNN_name.sql`, then run
   `scripts/check_migrations.sh --freeze`, then commit `migrations/CHECKSUMS`.
6. **Deployment.** `scripts/deploy.sh` only. Never `docker compose up -d` on a
   live stack.
7. **Serialize the gate.** One full gate suite at a time on this box.

---

## 2. Current state

### 2.1 The workspace

`Cargo.toml` declares `members = ["crates/*"]`, edition 2024, rust-version 1.94.
Six crates:

| Crate | Package name | Role |
|---|---|---|
| `crates/core` | `cadus-core` | Pure scheduling and pedagogy. No network, no database, no model call (R3). |
| `crates/store` | `cadus-store` | The Postgres adapter (sqlx, compile-time checked). |
| `crates/web` | `cadus-web` | The HTTP API (axum). |
| `crates/worker` | `cadus-worker` | Background jobs: pool refill, async diagnosis, offline authoring. |
| `crates/model-client` | `cadus-model-client` | The OpenAI-compatible client. |
| `crates/testkit` | `cadus-testkit` | Development only: the shared test instruments. |

Binaries: `cadus-web` (`crates/web/src/bin/cadus-web/main.rs`), `cadus-worker`
(`crates/worker/src/bin/cadus-worker.rs`), `cadus-migrate`
(`crates/store/src/bin/cadus-migrate/main.rs`), plus two core tools,
`dump_curriculum` and `lint_curriculum`.

### 2.2 Test count

`git grep -E '^\s*#\[(tokio::)?test\]' -- crates | wc -l` gives **2,156** Rust test
functions.

| Crate | Test functions |
|---|---:|
| `cadus-core` | 962 |
| `cadus-web` | 631 |
| `cadus-worker` | 257 |
| `cadus-store` | 240 |
| `cadus-model-client` | 34 |
| `cadus-testkit` | 32 |

309 integration test files under `crates/*/tests/`. 55 SPA test files under `web/`.
`PROGRESS.md` records 1,818 tests at the M6 close on `main`; the quality branch
added the difference.

### 2.3 Migrations

`migrations/` at the repository root, not under `crates/store`. Twelve files plus a
checksum list:

```
0001_roles.sql                  cadus_owner, cadus_app (NOBYPASSRLS), cadus_admin (BYPASSRLS)
0002_identity.sql               users, auth_sessions, oauth_accounts, auth_tokens, auth_rate_counters
0003_event_log.sql              events, learner_models
0004_scratch.sql                profiles, session_plans, diag_states, user_settings, web_states,
                                anki_queue, anki_cards_created, email_outbox
0005_content.sql                content_store, serving_pool, model_call_log, diagnosis_jobs
0006_grants_rls.sql             row-level security and grants on 18 tables
0007_worker_liveness.sql
0008_auth_session_absolute.sql
0009_metrics_readers.sql
0010_session_view.sql           ALTER learner_models ADD COLUMN session_view jsonb
0011_content_review.sql         ALTER content_store ADD COLUMN review_reason text
0012_content_prompt_digest.sql  ALTER content_store ADD COLUMN prompt_digest text
CHECKSUMS
```

### 2.4 Database schema outline

**Identity (0002).** `users`, `auth_sessions`, `oauth_accounts`, `auth_tokens`,
`auth_rate_counters`.

**The log (0003).**

```sql
CREATE TABLE events (
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    seq        bigint NOT NULL,
    ts         timestamptz NOT NULL,
    type       text NOT NULL,
    session_id text,
    v          smallint NOT NULL DEFAULT 1,   -- payload schema version
    attempt_id text,
    payload    jsonb NOT NULL,
    PRIMARY KEY (user_id, seq)
);
CREATE UNIQUE INDEX events_attempt_idem ON events (user_id, attempt_id)
    WHERE attempt_id IS NOT NULL;

CREATE TABLE learner_models (
    user_id           uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    model             jsonb NOT NULL,
    through_seq       bigint NOT NULL,
    projector_version integer NOT NULL,
    config_hash       text NOT NULL,
    curriculum_hash   text,
    built_at          timestamptz NOT NULL DEFAULT now()
);
```

`0010` adds `learner_models.session_view jsonb`.

**Per-user scratch and queues (0004).** `profiles`, `session_plans`, `diag_states`,
`user_settings`, `web_states` (column `doc jsonb`: the served problem, the answer
buffer, the timers), `anki_queue`, `anki_cards_created`, `email_outbox`.

**Content and telemetry (0005).** `content_store`, `serving_pool`,
`model_call_log`, `diagnosis_jobs`. Section 5 of this report quotes
`content_store` in full.

`serving_pool` carries `source text NOT NULL CHECK (source IN
('template','exemplar','generator'))`, `expected_answer jsonb NOT NULL`,
`instance_hash text NOT NULL`, and `UNIQUE (user_id, kp_id, instance_hash)`.

`diagnosis_jobs.status` is `CHECK (status IN
('pending','running','done','failed','capped'))`.

**Security (0006).** Row-level security is enabled and forced on 18 tables.
`content_store` stands **outside** row-level security: it holds curriculum content
and not learner data. `REVOKE INSERT, UPDATE, DELETE ON content_store FROM
cadus_app` (line 365).

### 2.5 Deployment layout

`Dockerfile` has four stages: `builder` (rust:1.98-bookworm), `spa-builder`
(node:22.21.1), `spa` (caddy:2, which bakes `web/dist` into `/srv`), and `runtime`
(debian:bookworm-slim), which carries `cadus-web`, `cadus-worker`, and
`cadus-migrate`. Default command `cadus-web`.

`docker-compose.yml` declares five services:

| Service | Image | Command |
|---|---|---|
| `db` | `postgres:16` | — |
| `migrate` | the app image | `cadus-migrate --admin-login` |
| `web` | the app image | `cadus-web` |
| `worker` | the app image | `cadus-worker` |
| `caddy` | built from the `spa` stage | — (host ports 80 and 443) |

Two networks (`frontend`, `backend`) and three volumes (`db-data`, `caddy-data`,
`caddy-config`). The Caddy configuration is `deploy/Caddyfile`, bind mounted as a
directory.

`scripts/deploy.sh` is the upgrade procedure: build the image, start `db` and wait
for its healthcheck, run the migrations in a one-shot, then replace `web`,
`worker`, and `caddy` with `--no-deps`, then prove all three stay up for
`START_LIMIT_SECS`. A non-zero migration exit stops the script and the old
containers keep serving.

`scripts/check_ops.sh` needs `docker`, `python3`, and `shellcheck`. It runs
`docker compose config`, then `docker compose build`, then one container per built
image, to prove that `cadus-web`, `cadus-worker`, and `cadus-migrate` are on the
image `PATH`. Its helper checks live in `scripts/check_ops.d/`: `caddy.sh`,
`commands.sh`, `compose.sh`, `deploy.sh`, `envpair.sh`, `images.sh`, `plan.sh`,
`reload.sh`.

Containers on this box, read from `docker ps`, and untouched by this audit:
`cadus2-web` (image `cadus2:latest`, up 4 days), `cadus2-worker` (same image, up 4
days), `cadus2-db` (`postgres:16`, up 5 days, healthy), `cadus2-edge`
(`cadus2-edge:latest`, up 4 days). Four test databases also run:
`cadus2-testdb`, `cadus2-testdb-b`, `cadus2-testdb-c`.

### 2.6 The HTTP surface

38 routes. The families:

- health and metrics: `/api/health`, `/api/ready`, `/metrics`
- session: `/api/status`, `/api/graph`, `/api/modules`, `/api/export`,
  `/api/enroll`, `/api/session/start`, `/api/session/end`, `/api/session/plan`
- task: `POST /api/task/{task_id}/serve`, `/teach`, `/hint`, `/answer`
- placement: `POST /api/diag/start`, `/api/diag/answer`, `/api/diag/finish`
- async diagnosis: `GET /api/diagnosis/stream`, `GET /api/diagnosis/{id}`
- operator: `GET /api/operator/flags`
- admin: `GET /api/admin/content`, `GET /api/admin/content/{digest}`,
  `POST /api/admin/content/{digest}/approve`,
  `POST /api/admin/content/{digest}/reject`
- authentication routes (the remainder)

Open from `PROGRESS.md`: `POST /api/task/{task_id}/abort` is absent.

---

## 3. Revalidation of the audit findings

Every finding below carries the current path and line. The recent refactors moved
`grade.rs` to `grade/` (`route.rs`, `verdict.rs`, `submission.rs`, `advance.rs`,
`reply.rs`, `mod.rs`) and `serve.rs` to `serve/` (`route.rs`, `teach.rs`,
`target.rs`, `draw.rs`, `hint.rs`, `payload.rs`, `fixture.rs`, `mod.rs`).

### (a) The grade route accepts `numeric` and `expression` only — HOLDS

`crates/web/src/grade/route.rs:21-35`:

```rust
fn graded_kind(state: &AppState, served: &ServedProblem) -> Result<AnswerKind, ApiError> {
    let Some(kind) = answer_kind(served) else {
        return Err(broken_state("the served problem names no answer kind"));
    };
    if matches!(kind, AnswerKind::Numeric | AnswerKind::Expression) {
        return Ok(kind);
    }
    state.metrics.count_grade(metrics::GRADE_UNDECIDABLE);
    Err(ApiError::new(
        StatusCode::CONFLICT,
        UNDECIDABLE_KIND,
        "This answer kind has no deterministic verdict, and this service never asks a model \
         for one.",
    ))
}
```

`crates/web/src/grade/route.rs:93` calls it: `let kind = graded_kind(&state, &served)?;`.

`AnswerKind` has four values (`crates/core/src/curriculum/model.rs:109-117`):
`Numeric`, `Expression`, `MultiStep`, `Proof`. Wire values `numeric`,
`expression`, `multi-step`, `proof`.

**Consequence.** A served `multi-step` or `proof` problem answers HTTP 409 with the
code `undecidable_kind`. The learner gets no verdict and no outcome. Nothing in
the session composer filters those topics out: `answer_kind` is read at
`crates/web/src/serve/payload.rs:129-134` for the payload, and the only route that
filters on it is the placement diagnostic
(`crates/web/src/diag/mod.rs:61-63`):

```rust
const fn deterministic(kind: AnswerKind) -> bool {
    matches!(kind, AnswerKind::Numeric | AnswerKind::Expression)
}
```

### (b) Foundations `multi-step` topics with ordinary numeric final answers — HOLDS

The curriculum is a YAML tree at `/home/deploy/dev/cadus2.0/curriculum/`, with
`courses.yaml` at the root and one directory per course. Foundations is course id
`foundations`, directory `curriculum/foundations/`, 11 unit files.

Counts (grep, cross-checked against a YAML subset parser that reproduces the
verbatim load facts of `docs/reference/curriculum-1.0-spec.md:128-142`):

```sh
grep -c "^  - id: " curriculum/foundations/*.yaml | awk -F: '{s+=$2} END{print s}'   # 285
grep -h "^    answer_kind: " curriculum/foundations/*.yaml | sort | uniq -c
#   120     answer_kind: expression
#    78     answer_kind: multi-step
#    87     answer_kind: numeric
```

**285 Foundations topics, 78 of them `multi-step`.** The audit number is exact.
Foundations holds no `proof` topic. Repository wide: 1,090 topics, of which 483 are
`multi-step` and 129 are `proof`.

Five `multi-step` Foundations topics whose final answers are ordinary numbers:

| Topic id | File | Lines |
|---|---|---|
| `percent-applications` | `curriculum/foundations/01-fractions-decimals.yaml` | 1568-1620 |
| `equation-word-problems` | `curriculum/foundations/03-expressions-equations.yaml` | 1393-1447 |
| `integer-word-problems` | `curriculum/foundations/02-integers-negatives.yaml` | 957-1000 |
| `money-geometry-problems` | `curriculum/foundations/03-expressions-equations.yaml` | 1345-1386 |
| `fraction-word-problems` | `curriculum/foundations/01-fractions-decimals.yaml` | 681-732 |

Proof, from `curriculum/foundations/01-fractions-decimals.yaml:1568,1573,1589-1590`:

```yaml
  - id: percent-applications
    answer_kind: multi-step
          - problem: 'A shirt costs €50 and is marked up $20\%$. What is the new price in euros?'
            answer: "60"
```

From `curriculum/foundations/03-expressions-equations.yaml:1393,1398,1416-1417`:

```yaml
  - id: equation-word-problems
    answer_kind: multi-step
          - problem: 'Maya and Liam sold $51$ raffle tickets in all; Maya sold twice as many as Liam. How many did Maya sell?'
            answer: "34"
```

From `curriculum/foundations/02-integers-negatives.yaml:957,962,980-981`:

```yaml
  - id: integer-word-problems
    answer_kind: multi-step
          - problem: 'An account has €50, then an €80 withdrawal is made. What is the balance in euros?'
            answer: "-30"
```

From `curriculum/foundations/03-expressions-equations.yaml:1345,1352,1366-1367`:

```yaml
  - id: money-geometry-problems
    answer_kind: multi-step
          - problem: 'Tickets cost €8 each and you spent €56. How many tickets did you buy?'
            answer: "7"
```

From `curriculum/foundations/01-fractions-decimals.yaml:681,688,714-715`:

```yaml
  - id: fraction-word-problems
    answer_kind: multi-step
          - problem: 'Of $24$ students, $\frac{3}{8}$ play football. How many students do NOT play football?'
            answer: "15"
```

The final answers `60`, `34`, `-30`, `7`, `15` are integers. The checker decides
each of them. The kind gate refuses them.

### (c) The verdict converts `Undecidable` into incorrect — HOLDS

`crates/web/src/grade/verdict.rs:28-43`:

```rust
    match check(expected, answer, kind) {
        Outcome::Decided(verdict) if verdict.correct => Grade {
            correct: true,
            work_quality: WorkQuality::NearlyPerfect,
            error_tags: if verdict.notation { vec![TAG_NOTATION.to_string()] } else { Vec::new() },
        },
        Outcome::Decided(_) | Outcome::Undecidable(_) => Grade {
            correct: false,
            work_quality: WorkQuality::NearlyPassable,
            error_tags: Vec::new(),
        },
    }
```

The one arm folds a decided miss and an undecidable answer into one outcome. The
doc comment states the rule at `verdict.rs:16-18`: "An answer the checker refuses
([`Outcome::Undecidable`]: the input cap, or an exit from the grammar) is a
deterministic MISS with no tag".

The checker itself keeps the two apart. `crates/core/src/answer/check.rs:68-76`:

```rust
pub enum Outcome {
    Decided(Verdict),
    Undecidable(Undecidable),
}
```

So the ungraded outcome exists in the core and dies at the web boundary.

### (d) `sqrt(2)` against `2^(1/2)` gives undecidable — HOLDS

The checker is `crates/core/src/answer/`. Four stages: `normalize`, `parse`,
`canon`, `check` (`crates/core/src/answer/mod.rs:7-16`).

The cause is the exponent production. `crates/core/src/answer/ast.rs:114` declares
`Pow(Box<Ast>, i64)` — the exponent is an `i64` and nothing else.
`crates/core/src/answer/parse/atom.rs:98-131`:

```rust
    /// Parse the exponent of a power. The grammar allows an integer literal only.
    fn parse_exponent(&mut self) -> Result<i64, Undecidable> {
        let parenthesized = self.eat(&Tok::LParen);
        ...
        let Some(Tok::Num(text)) = self.peek() else {
            return Err(Undecidable::new("an exponent that is not a whole number"));
        };
        if text.contains('.') {
            return Err(Undecidable::new("an exponent that is not a whole number"));
        }
        ...
        if parenthesized && !self.eat(&Tok::RParen) {
            return Err(Undecidable::new("an exponent that is not a whole number"));
        }
```

`2^(1/2)` takes the `LParen`, reads the number `1`, then finds a `/` where the
closing parenthesis must stand. The last guard fires and the parse returns
`Undecidable`. `crates/core/src/answer/check.rs:179-182` turns that into
`Outcome::Undecidable`:

```rust
    let learner_tree = match parse(&learner_text.source) {
        Ok(tree) => tree,
        Err(reason) => return (Outcome::Undecidable(reason), None),
    };
```

The expected side `sqrt(2)` parses: `crates/core/src/answer/parse/mod.rs:17-18` and
`crates/core/src/answer/canon/read.rs:58-62` build one `Ast::Sqrt` node for
`\sqrt{a}`, `√a`, and `sqrt(a)`. The learner side alone leaves the grammar.

Finding (c) then converts the refusal into `correct: false`. A learner who writes
`2^(1/2)` for `sqrt(2)` reads "wrong".

`docs/reference/undecidable-answers.md` counts 54 corpus answers in the
`expression_symbolic` bucket, "a rational or symbolic exponent" among them.

### (e) `1/3` against `0.3` is accepted — HOLDS, with one correction

The rule is real. The precision is **not** learner-selected in a setting; it is the
count of decimal digits the learner typed in the submitted answer. No route reads a
precision, and no `user_settings` field holds one.

The ruling is `D6-dec` in `docs/DECISIONS.md`: "A learner decimal is CORRECT when
it equals the exact expected value rounded half-to-even to the count of decimal
digits the learner typed."

Rung 5 of the checker, `crates/core/src/answer/check.rs:194-201`:

```rust
    // Rung 5. The learner side alone may carry a rounding (ruling `D6-dec`).
    match rounding_variant(&expected_value, &learner_tree) {
        Rounding::Same => return (Outcome::notation(), Some(Form::Rounding)),
        Rounding::Refused(reason) => {
            return (Outcome::Undecidable(Undecidable::new(reason)), None);
        }
        Rounding::Different | Rounding::NotANumber => {}
    }
```

The digit count comes from the learner's own text, `check.rs:227-245`:

```rust
fn typed_decimal(tree: &Ast) -> Option<(BigRational, u32)> {
    ...
            Ast::Decimal { mantissa, scale } => {
                let magnitude = BigRational::new(mantissa.clone(), BigInt::from(10_u32).pow(*scale));
                let value = if negative { -magnitude } else { magnitude };
                return (*scale >= 1).then_some((value, *scale));
            }
```

The rounding module is `crates/core/src/answer/rounding.rs`. `rounds_to(expected,
learner, scale)` at line 79 dispatches on the expected canonical form:
`Canon::Rational` takes one multiplication and one floor division
(`rational_rounds_to`, line 88); `Canon::Radical` brackets each root and refines
(`radical_rounds_to`). Every step is exact rational arithmetic; no float enters.

For `1/3` against `0.3`: scale is 1, `1/3 * 10 = 10/3`, half-to-even to an integer
gives 3, and `0.3 * 10 = 3`. The two agree, so the outcome is `Rounding::Same`. The
verdict is correct with the `notation` tag.

The route that reaches this is `crates/web/src/grade/route.rs:94`:

```rust
    let grade = deterministic_grade(&served.expected.answer, &submitted.answer, kind);
```

`deterministic_grade` (`crates/web/src/grade/verdict.rs:20`) calls `check`. There is
no precision parameter on any of the three levels.

`0.3334` for `1/3` stays wrong. `docs/reference/checker-1.0-spec.md` section 3.2
holds both rules side by side.

### (f) Exemplar parsing refuses `9 R2` — HOLDS

The parser refuses it in the implicit-factor rule,
`crates/core/src/answer/parse/term.rs:74-107`:

```rust
    /// - A number after a number is never a product. `2 3` is a typing slip, and
    ///   `9 R2` is a quotient with a remainder (spec section 8.3), not `9*R*2`.
    ...
        if previous.is_some_and(is_numeric_literal) {
            return Err(Undecidable::new("two numbers stand side by side"));
        }
        if !token.space_before {
            return Err(Undecidable::new(
                "a number glued to a name reads as a label",
            ));
        }
```

`9 R2` is a committed member of the refusal fixture,
`crates/core/tests/fixtures/answers/undecidable_1_0.jsonl:62`:

```json
{"answer":"9 R2","exemplar_index":0,"kp_id":"kp1","topic_id":"division-with-remainders"}
```

`docs/reference/undecidable-answers.md` section 3.3 names the whole group:

> ### 3.3 Quotient and remainder — 16 answers, 5 topics
> Topics: `division-with-remainders`, `long-division`, `long-division-one-digit`,
> `polynomial-division`, `synthetic-division`.
> Two spellings: `9 R2`, `6 R2`, `5 R3`, `8 R2`, `23 R14`, `41 R16`, `41 R8`,
> `241 R2`, `71 R3`, `152 R3`; and `x + 2 remainder 3` (twice), …
> **Action.** Author the answer as the tuple `(quotient, remainder)`. The tuple
> production already exists, and it compares element by element. If the authored
> spelling must stay, add a `q R r` production that reads both spellings into the
> same tuple.

There is no quotient-and-remainder production. `crates/core/src/answer/canon/quotient.rs`
is the canonical rational quotient of the algebra; it is not this.

The whole measured residue is 265 of 3,492 corpus answers (7.59 %), in 116 of 478
topics (`docs/reference/undecidable-answers.md` section 1).

Note: the regeneration command in that document is stale. It names `--test
answer_oracle` and `--test answer_parse`; the quality refactor split those files
into `answer_oracle_1.rs`, `answer_oracle_2.rs`, `answer_oracle_edges.rs`,
and `answer_parse_1.rs` to `answer_parse_4.rs`, `answer_parse_edges.rs`.

### (g) Work-quality tiers derive from final-answer correctness only — HOLDS

The whole tier is decided in `crates/web/src/grade/verdict.rs:20-44`:

- blank answer → `WorkQuality::Poor`, tag `blank-answer` (lines 21-27)
- correct → `WorkQuality::NearlyPerfect` (lines 29-37)
- every other outcome → `WorkQuality::NearlyPassable` (lines 38-42)

The doc comment at `verdict.rs:7-14` states the D-M5-2 ruling: "`perfect` is a
bonus for method the checker never reads, so it is never awarded, and every tier
below is a penalty for a flaw the checker never observes."

`WorkQuality` has six values (`crates/core/src/event/kind.rs:48-67`): `Perfect`,
`NearlyPerfect`, `Passable`, `NearlyPassable`, `Poor`, `Blowoff`. Three of the six
are unreachable from a graded attempt.

The task-level tier copies the attempt tier.
`crates/web/src/grade/advance.rs:133` and `:170`:

```rust
    let quality = attempt.work_quality;
    ...
        quality_tier: quality,
```

The same at `advance.rs:169` and `:188` for the failed lesson.

The quiz tier is also correctness alone
(`crates/core/src/projector/handlers.rs:122-127`):

```rust
            let quality = if row.correct {
                WorkQuality::NearlyPerfect
            } else {
                WorkQuality::Poor
            };
```

The learner's shown work reaches no tier. `submitted.work` is used twice only: it
is stored on the attempt (`crates/web/src/grade/submission.rs:192`) and it is
passed to the async diagnosis job (`crates/web/src/grade/route.rs:155`,
`crates/web/src/diagnosis/decide.rs:126`). No grader reads it.

The tier prices XP (`crates/core/src/xp.rs:107-112`) and the FIRe grade `q`
(`crates/core/src/fire/mod.rs:49`, `QUALITY_Q`). Correctness therefore drives both.

### (h) Deployed `content_store` has zero rows — CONSISTENT with the code

This audit ran no query. The code and the migrations say the following.

1. **No migration seeds the table.** `migrations/0005_content.sql` creates
   `content_store` and inserts nothing. `0011` and `0012` add two nullable
   columns and no rows.
2. **One writer.** `cadus_store::content::insert_pending`
   (`crates/store/src/content/mod.rs:236-257`) is the only INSERT:
   `INSERT INTO content_store (digest, kp_id, kind, body, status,
   authoring_attempts, authoring_cost_usd, prompt_digest) VALUES (…) ON CONFLICT
   (digest) DO NOTHING`. The status parameter is fixed to `pending`; no caller
   passes a status.
3. **One caller.** `store_pending` in
   `crates/worker/src/authoring/job/store.rs:357-398`, reached from
   `author_one` → `store_verified` in `crates/worker/src/authoring/job/pass.rs:157`.
4. **One trigger.** The CLI subcommand `cadus-worker author`
   (`crates/worker/src/bin/cadus-worker.rs:63-79`,
   `crates/worker/src/authoring/cli.rs:38-41`). The tick loop of `cadus-worker`
   runs refill and diagnosis only (`crates/worker/src/lib.rs:283-324`). No HTTP
   route and no queue table starts an authoring pass.
5. **The pass needs a model.** `authoring_job()`
   (`crates/worker/src/bin/cadus-worker.rs:157-176`) treats an empty
   `OPENAI_API_KEY` as a hard error.

`PROGRESS.md`, "End-to-end verification on the final tree", states the same
outcome: "`teach` and `hint` answer `no_instruction` and `no_hint_ladder` on a
fresh deployment, which is correct: `content_store` is empty until an authoring
pass runs against a reachable model." The same section records that a real model
provider was never called.

**Consequence.** With an empty `content_store`, the serve path has no approved
template, so `A6` exemplar rotation is the only source
(`crates/core/src/pool/source/exemplar.rs`), `teach` answers 409 `no_instruction`,
and `hint` answers 409 `no_hint_ladder`.

### (i) Foundations knowledge-point and exemplar counts — ALL SEVEN HOLD, with one caveat

Recounted over `curriculum/foundations/*.yaml`. PyYAML is not installed on this
box (`python3 -c 'import yaml'` fails), so the recount used a block-YAML subset
parser plus a plain grep cross-check. The parser reproduces the verbatim load facts
of `docs/reference/curriculum-1.0-spec.md:128-142` exactly (13 courses, 88 units,
1,090 topics, 3,138 knowledge points, 6,800 exemplars).

| Audit claim | Recount | Match |
|---|---:|---|
| 809 knowledge points | 809 | yes |
| 1,695 fixed exemplars | 1,695 | yes |
| 732 knowledge points with two examples | 732 | yes |
| 77 knowledge points with three examples | 77 | yes |
| 679 exemplars without a solution sketch | 679 | yes |
| 48 knowledge points with no parseable fallback example | 48 | yes, over a 583-KP subset |
| 33 with one | 33 | yes, same subset |

Method:

```sh
grep -hc "^      - id: kp"                curriculum/foundations/*.yaml | awk '{s+=$1} END{print s}'  # 809
grep -hc "^          - problem: "         curriculum/foundations/*.yaml | awk '{s+=$1} END{print s}'  # 1695
grep -hc "^            solution_sketch: " curriculum/foundations/*.yaml | awk '{s+=$1} END{print s}'  # 1016
# 1695 - 1016 = 679
```

The exemplars-per-knowledge-point distribution is exactly `{2: 732, 3: 77}`. No
Foundations knowledge point holds 0, 1, or 4 exemplars. 732 + 77 = 809.

**Terminology.** The curriculum YAML has no field named "fallback example". The
field is `exemplars`, a list of `{problem, answer, solution_sketch}`
(`crates/core/src/curriculum/model.rs`, `struct Exemplar`). A6 serves those
exemplars when no approved template exists
(`crates/core/src/pool/source/exemplar.rs`), so "fallback example" means
"exemplar".

**The caveat, and it matters.** "Parseable" is not a YAML property. It is the
verdict of the Rust grammar (`canonical_form` in
`crates/core/src/answer/check.rs:295`), and `ExemplarSource::refusals()` is what
marks an exemplar unusable. The recount joined the two committed fixtures —
`crates/core/tests/fixtures/answers/corpus_1_0.jsonl` (3,492 answers) and
`crates/core/tests/fixtures/answers/undecidable_1_0.jsonl` (265 refusals) — to the
parsed curriculum by `(topic_id, kp_id, exemplar_index)`. That gives the Foundations
distribution `{0: 48, 1: 33, 2: 447, 3: 55}`.

The corpus covers `numeric` and `expression` topics only. In Foundations that is
583 of the 809 knowledge points. The 226 knowledge points on the 78 `multi-step`
topics are **excluded**, not counted as parseable. The audit therefore mixes two
denominators: the five structural numbers run over 809 knowledge points, and the
two parseability numbers run over 583.

**Scope.** Every audit number is Foundations only. Repository wide the figures are
about four times larger: 3,138 knowledge points, 6,800 exemplars, 1,401 exemplars
with no solution sketch, 2,616 knowledge points with two exemplars, 520 with
three, 2 with four, 69 with no decidable exemplar, 62 with exactly one.

### (j) The SPA falls back to practice when teach fails; the server needs approved instruction — HOLDS

`web/src/views/session/Session.tsx:208-220`:

```tsx
    if (task.task_type === 'lesson') {
      // Teach FIRST, and teach ALONE (NO-2BILL). Every topic has knowledge points, so the
      // view needs no served problem to know a fresh lesson must teach.
      void call(() => api.taskTeach(task.task_id), (instruction) => {
        taughtKp.current = instruction.kp;
        setTeaching(instruction);
        gate.enter('teaching');
      }).then((instruction) => {
        // Teach failed and was toasted with a Retry. Practice is still servable, so fall
        // through rather than strand the task on a spinner.
        if (!instruction && life.alive()) serveThenShow();
      });
      return;
    }
```

The server side, `crates/web/src/serve/teach.rs:43-52`:

```rust
    let doc = store(&state, approved_document(&mut *tx, &key, KIND_TEACH)).await?;
    ...
    let page: TeachDoc = read_document(
        doc.ok_or_else(no_instruction)?,
        "teach",
        "The authored teach page is not readable.",
    )?;
```

`approved_document` filters on the approval state
(`crates/store/src/content/mod.rs:88-94`):

```sql
        SELECT digest AS "digest!", body AS "body!"
        FROM content_store
        WHERE kp_id = $1 AND kind = $2 AND status = 'approved'
        ORDER BY approved_at DESC NULLS LAST, created_at DESC, digest
        LIMIT 1
```

Every other task type is `409 no_instruction`
(`crates/web/src/serve/teach.rs:33-35`).

**Consequence.** With `content_store` empty (finding h), every lesson skips the
worked example and goes straight to practice. The learner practices a skill the
service never taught. Hard Rule 1 ("no answer before an attempt") is not violated,
but the instruction stage is silently absent.

### (k) Two consecutive correct, or three of the first four; `lesson.kp_pass` is not read — HOLDS

`crates/core/src/projector/entry.rs:162-180`:

```rust
/// Whether a lesson knowledge point is mastered (`projector.py:860-867`).
///
/// The rule is `lesson.kp_pass = "2consec|3of4"`: two correct answers in a row at
/// the tail, or three correct out of the first four. 1.0 hard-codes both arms and
/// reads the config string for neither, so this port takes no config either. A
/// second rule spelling would need a parser in both tiers, and 1.0 has none.
#[must_use]
pub fn kp_passed(seq: &[bool]) -> bool {
    let len = seq.len();
    if len >= 2 && seq[len - 1] && seq[len - 2] {
        return true;
    }
    if len >= 4 && seq.iter().take(4).filter(|correct| **correct).count() >= 3 {
        return true;
    }
    false
}
```

The function takes no `Config`. The config string exists at
`crates/core/src/config.rs:73` (`pub kp_pass: String`) with the default
`"2consec|3of4"` at line 83. It reaches the config hash preimage
(`crates/core/tests/numeric.rs:367` shows `"lesson":{"kp_pass":"2consec|3of4",…}`)
and nothing else. A grep for `kp_pass` over `crates/` finds no reader other than
the serializer.

The neighbor gate does read the config: `kp_failed`
(`crates/core/src/projector/entry.rs:186-189`) reads `cfg.lesson.fail_after`.

One precision on the first arm: the two consecutive correct answers must sit at the
**tail** of the sequence (`seq[len-1] && seq[len-2]`), not anywhere in it.

### (l) `xp::is_mastered` includes Learning, Placed, and Floor — HOLDS

`crates/core/src/xp.rs:70-80`:

```rust
/// The statuses that count as mastered for course progress (`selector.py:150`).
///
/// Implicit credit alone never masters a topic; the status gate is the rule. The
/// selector of U4 reads the same predicate.
#[must_use]
pub fn is_mastered(state: &TopicState) -> bool {
    matches!(
        state.status,
        TopicStatus::Learning | TopicStatus::Placed | TopicStatus::Floor
    )
}
```

`TopicStatus` has five values (`crates/core/src/event/kind.rs:73-90`):
`Untouched`, `Frontier`, `Learning`, `Placed`, `Floor`. The doc comments name them:
`Learning` is "Learned and on the review schedule", `Placed` is "Placed by the
diagnostic", `Floor` is "Below the mastery floor of the enrolled course".

So a topic the learner never practiced counts as mastered when the diagnostic
placed it, and so does a topic under the course mastery floor from `POST
/api/enroll`.

Five call sites read the predicate:
`crates/core/src/selector/multistep.rs:126`,
`crates/core/src/selector/reserve.rs:135`,
`crates/core/src/selector/task.rs:296`,
`crates/core/src/selector/topic_set.rs:151`,
`crates/core/src/xp.rs:333`. It is re-exported as
`cadus_core::selector::is_mastered` (`crates/core/src/selector/mod.rs:42`).

**Consequence.** The frontier, the course-completion test, the drill schedule, the
multi-step cadence, and the course-progress percentage all treat placement credit
and floor credit as mastery.

### (m) The assisted-correct rework clears the input beside the solution; a successful rework stays assisted — HOLDS, both halves

**The server stashes and does not record.**
`crates/web/src/grade/route.rs:125-133`:

```rust
    if stash_required(assisted, &grade, &served) {
        scratch
            .served
            .entry(task_id)
            .and_modify(|live| live.rework = Some(stash));
        return save_and_commit(&state, tx, user_id, &scratch)
            .await
            .map(|()| Json(rework_reply(&served)));
    }
```

`rework_reply` hands the client the solution and the authored answer
(`crates/web/src/grade/reply.rs:8-16`):

```rust
    json!({
        "rework_required": true,
        "problem_id": served.problem_id,
        "solution": served.solution_sketch,
        "expected": served.expected.answer,
        "re_solve": RE_SOLVE,
    })
```

**The client clears the field.** `web/src/views/session/useGrade.ts:116-130`:

```ts
        if (isRework(reply)) {
          // DD-3/P1: back to `ready`, deliberately. The next submit of this same problem is
          // the unaided re-solve, and only that locks the assisted pass in.
          ...
          setRework(reply);
          setElapsed(0);
          field.clear();
          gate.enter('ready');
          return;
        }
```

**The solution stays on screen beside the live answer field.**
`web/src/views/session/Session.tsx:451` renders `{rework ? <Rework res={rework} />
: null}` under the still-enabled Submit and Hint buttons (lines 435-449). The panel
prints the solution (`web/src/views/session/Feedback.tsx:106-116`):

```tsx
    <div className="feedback feedback-rework">
      ...
      <MathBlock className="feedback-text">{String(res.re_solve)}</MathBlock>
      <div className="solution">
        <div className="solution-label">Solution</div>
        <MathBlock className="solution-text">{String(res.solution ?? res.expected)}</MathBlock>
      </div>
    </div>
```

**A successful rework stays assisted.**
`crates/web/src/grade/route.rs:219-241`:

```rust
fn recorded_attempt(
    served: &ServedProblem,
    attempt: Attempt,
    grade: &Grade,
    now: Timestamp,
    session: Option<String>,
) -> Result<(Attempt, String), ApiError> {
    let Some(stash) = &served.rework else { ... };
    let mut stashed: Attempt = serde_json::from_value(stash.clone()) ...;
    stashed.ts = now;
    stashed.session = session;
    stashed.attempt_id = format!("{}-rework", attempt.attempt_id);
    if !grade.correct {
        stashed.correct = false;
        stashed.assisted = false;
    }
```

The `assisted` flag is cleared **only** on a failed re-solve. A successful re-solve
records the stashed attempt with `assisted` still true. The flag then halves the
FIRe credit (`crates/core/src/fire/mod.rs:74`, `ASSISTED_CREDIT: f64 = 0.5`), and
the passing lesson marks itself assisted when any of its attempts was
(`crates/web/src/grade/advance.rs:136-141`).

### (n) Review scoring weights later answers more and needs the final answer correct — HOLDS

`crates/core/src/fire/attempt.rs:311-342`:

```rust
/// The order-sensitive review grade (`fire.py:539-554`).
///
/// Position weights `1..n`, normalized by their sum. The review passes only when
/// the weighted score reaches `review.pass_weighted` AND the FINAL question is
/// correct, so an improving trajectory passes and a deteriorating one fails. An
/// empty result list is `(false, 0.0)`.
pub fn grade_review(question_results: &[bool], cfg: &Config) -> (bool, f64) {
    let n = question_results.len();
    if n == 0 { return (false, 0.0); }
    let count = n as i64;
    let total_weight = (count * (count + 1)) as f64 / 2.0;
    let mut earned: i64 = 0;
    for (index, &correct) in question_results.iter().enumerate() {
        if correct { earned += index as i64 + 1; }
    }
    let score = earned as f64 / total_weight;
    let last_correct = question_results.last().copied().unwrap_or(false);
    let passed = score >= cfg.review.pass_weighted && last_correct;
    (passed, score)
}
```

`review.pass_weighted` defaults to 0.65 and `review.questions` to 4
(`crates/core/src/config.rs:96-104`).

The two examples check out. With four questions the total weight is
`4 * 5 / 2 = 10`.

- wrong, wrong, correct, correct: earned `3 + 4 = 7`, score 0.70. 0.70 ≥ 0.65 and
  the last answer is correct. **Passes.**
- correct, correct, correct, wrong: earned `1 + 2 + 3 = 6`, score 0.60. 0.60 < 0.65
  and the last answer is wrong. **Fails**, on both terms.

### (o) `TaskType::MultiStep` is one independent component-topic question per position — HOLDS

`crates/web/src/serve/target.rs:74-84`:

```rust
    match task.task_type {
        TaskType::MultiStep => {
            let component = part_of(
                &task.component_topics,
                position,
                MULTISTEP_EXHAUSTED,
                "This multi-step task has no further part to serve.",
            )?;
            let kp = kp_or_refuse(graph, component, index)?;
            Ok(Target::new(component.clone(), component.clone(), kp))
        }
```

Each position takes the next entry of `task.component_topics` and draws one whole
problem from that component topic's own pool. The attempt records against the
component topic, not against a parent. The task ends when the component list runs
out (`MULTISTEP_EXHAUSTED`). The unit test at lines 169-178 pins the behavior.

The core side builds the component list.
`crates/core/src/selector/multistep.rs:17-27` and `:29-60`:
`multistep_is_due(n_mastered_reviewable, n_closed)` fires one integration task per
`MULTISTEP_CADENCE` (4) mastered reviewable topics;
`multistep_components(candidates, graph)` orders the components by in-set ancestor
count and caps the list at `MULTISTEP_MAX_COMPONENTS` (4).

This is **not** `AnswerKind::MultiStep`. The two are unrelated:

- `TaskType::MultiStep` is an event and selector enum: a session task made of
  several separate single-answer questions, one per component topic.
- `AnswerKind::MultiStep` is a curriculum topic field: an answer shape the checker
  refuses (finding a).

Nothing composes one problem that needs several skills at once and grades the
intermediate steps.

### (p) The five baseline test files — ALL FIVE EXIST

`cargo test -p cadus-core --test diagnostic --test selector_plan --test
fire_attempt --test instruction_gate --test pool_source`

| Named target | Current file | Present |
|---|---|---|
| `diagnostic` | `crates/core/tests/diagnostic.rs` | yes |
| `selector_plan` | `crates/core/tests/selector_plan.rs` | yes |
| `fire_attempt` | `crates/core/tests/fire_attempt.rs` | yes |
| `instruction_gate` | `crates/core/tests/instruction_gate.rs` | yes |
| `pool_source` | `crates/core/tests/pool_source.rs` | yes |

The command still runs. The quality refactor split neighbors out of some of them,
so related coverage now sits in sibling files:

- beside `instruction_gate.rs`: `instruction_gate_served.rs`
- beside `pool_source.rs`: `pool.rs`, `pool_recheck.rs`, `pool_row.rs`,
  `pool_window.rs`
- beside `selector_plan.rs`: `selector.rs`, `selector_compress.rs`,
  `selector_gap_fill.rs`, `selector_l1.rs`, `selector_multistep.rs`,
  `selector_quiz.rs`, `selector_quiz_strata.rs`, `selector_reserve.rs`
- beside `fire_attempt.rs`: `fire.rs`, `fire_golden.rs`

To run the baseline plus its split siblings, add the sibling names as further
`--test` flags.

---

## 4. The event model and the learner model

### 4.1 Event types — `crates/core/src/event/`

Four files: `mod.rs` (the `Event` enum and the envelope), `body.rs` (one payload
struct per type), `kind.rs` (the shared enums), `scalar.rs` (the checked scalars),
`note.rs` (the regrade note). `Event` is an internally tagged enum on the key
`type` (`crates/core/src/event/mod.rs:55-106`) with 16 members: `session_start`,
`session_end`, `enrolled`, `task_served`, `attempt`, `lesson_result`,
`review_result`, `quiz_result`, `remediation_triggered`, `diagnostic_answer`,
`diagnostic_placed`, `profile_reset`, `regraded`, `anki_card_created`,
`config_changed`, `curriculum_changed`. `attempt` is the load-bearing type; it
carries `attempt_id`, `correct`, `work_quality`, `error_tags`, `secs`, `assisted`,
`answer_kind`, `kp`, `given_answer`, and `work`. Shared enums live in `kind.rs`:
`WorkQuality` (6 tiers, line 48), `TopicStatus` (5 statuses, line 73), `KpProgress`
(line 93), `TaskType`. The checked scalars of `scalar.rs` are `Slug`, `Timestamp`,
`Secs`, `PositiveSecs`, `Weight`, and `SchemaVersion`. `Event::v()` at
`mod.rs:216` reports the envelope version, and every payload struct carries
`v: SchemaVersion` with a serde default.

### 4.2 The learner-state projection — `crates/core/src/projector/` and `learner.rs`

Five files: `mod.rs` (the `Projector<'a>` struct at line 132 and `ProjectorError`),
`entry.rs` (the entry points), `handlers.rs` (one handler per event type),
`state.rs` (the topic-state arithmetic), `regrade.rs` (`apply_regrades`). The
public entry points are `project`, `project_incremental`, `ProjectionInput`,
`canonical_blob`, `blob_digest`, `kp_passed`, and `kp_failed`
(`crates/core/src/projector/mod.rs:54-58`). The output type is
`cadus_core::learner::LearnerModel` (`crates/core/src/learner.rs:216`), which
holds a `BTreeMap<String, TopicState>` plus `XpState`, `QuizState`,
`VelocityState`, and `PendingRemediation`. `TopicState`
(`crates/core/src/learner.rs:65-97`) carries `status`, `rep_num`, `memory_base`,
`t0`, `interval_days`, `ability`, `speed`, `conditional`, `explicit_only`,
`last_problems` (a 20-entry ring of problem digests), and `kp_progress`. The fold
is incremental: `learner_models.through_seq` records the last folded `events.seq`
(D4), and a full replay runs only for a regrade or a version bump (D-O6).

### 4.3 The selector — `crates/core/src/selector/`

Thirteen files. The entry point is `compose_session`
(`crates/core/src/selector/compose.rs`), which returns a `SessionPlan` of `Task`
values (`crates/core/src/selector/task.rs`). The parts:
`frontier.rs` (the course frontier split by the lesson-fail retry delay),
`topic_set.rs` (`TopicSet`, `course_scope`, `frontier`, `mastered_set`),
`review.rs` (`due_reviews`, `importance`, `nearly_due`, `order_lessons`,
`review_mix`, `retry_available_at`, `in_retry_delay`),
`compress.rs` (`Compression`, `compress` — the greedy weighted knockout set cover),
`interleave.rs` (`SlotKind`, `arrange_lessons`, `assign_ids`, `interleave`),
`quiz.rs` (`QuizPlan`, `QuizQuestion`, `QuizSampler`, `SeededSampler`,
`quiz_budget`, `quiz_composer`, `quiz_is_due`, `quiz_retake_available_at`),
`multistep.rs` (`multistep_is_due`, `multistep_components`, `remediation_tasks`),
`gap_fill.rs` (`is_course_complete`, `blocking_gap_ancestors`,
`resolve_gap_fill_stack`, `serveable_gap_frontier`),
`reserve.rs` (`ValidityContext`, `reserve_open_plan`, `task_still_valid`),
`context.rs` (`SessionContext`), and `compress.rs`. Named constants pin the
pedagogy: `MULTISTEP_CADENCE = 4`, `MULTISTEP_MIN_COMPONENTS = 3`,
`MULTISTEP_MAX_COMPONENTS = 4`, `DRILL_MASTERY_ABILITY = 0.95`,
`DRILL_INTERVAL_DAYS = 3.5`, `QUIZ_RECENT_DAYS = 14`, `CORE_BONUS = 0.5`.

### 4.4 The FIRe and XP memory model — `crates/core/src/fire/` and `xp.rs`

`fire/` holds `mod.rs`, `ability.rs`, `attempt.rs`, and a test-only `testing.rs`.
Every function is a deterministic function of `(state, attempt, curriculum,
config, t)`; nothing reads a clock and nothing mutates its inputs, so the log stays
re-projectable (`crates/core/src/fire/mod.rs:1-5`). The public surface:
`memory_at`, `review_state` (`ReviewState`), `interval_for`, `speed_for`,
`decay_for`, `raw_delta`, `apply_attempt`, `apply_attempt_checked`,
`Propagation`, `PropagationKind`, `grade_review`, `knockout`, `ability_update`,
`difficulty`, `initial_ability`, `is_pass_quality`, `has_review_history`,
`quality_q`. The constants pin the model: `QUALITY_Q` maps each tier to a grade
`q`, `PASS_QUALITY_THRESHOLD = 0.7`, `NEARLY_DUE_THRESHOLD = 0.6`,
`INTERVAL_CAP_DAYS = 730.0`, `ASSISTED_CREDIT = 0.5`, `NEUTRAL_ABILITY = 0.5`.
`xp.rs` holds the XP accounting, the streak, and the velocity: `base_xp`,
`task_xp`, `xp_per_day`, `daily_totals`, `is_mastered`, with
`LESSON_XP_PER_KP = 3.5`, `RUSH_PENALTY_MULT = 0.5`,
`VELOCITY_WINDOW_DAYS = 28`, `DEFAULT_XP_PER_TOPIC = 12.0`. Both modules copy 1.0
instruction for instruction, including `py_max`, `py_min`, and the Neumaier sum,
so the fold reproduces the 1.0 numbers bit for bit.

### 4.5 The answer checker pipeline — `crates/core/src/answer/`

Four stages in one module tree (`crates/core/src/answer/mod.rs:7-16`).
`normalize.rs` applies the whole-string V4 rules and writes a reader source plus a
string key (`Normalized`, `MAX_ANSWER_CHARS`). `lexer/` turns every LaTeX and
glyph construct into one token — `\frac{A}{B}`, `\sqrt{A}`, `^{n}`, `\cdot`,
`\times`, `%`, the vulgar glyphs, `√`, the superscript digits — because a string
rewrite loses structure and gave four false positives in review round 3.
`parse/` (`mod.rs`, `atom.rs`, `term.rs`, `build.rs`) reads the tokens into an
`Ast` (`ast.rs`: `Ast`, `Const`, `IneqOp`) or refuses with `Undecidable`.
`canon/` (`mod.rs`, `read.rs`, `arith.rs`, `sum.rs`, `quotient.rs`) reduces an
`Ast` to the canonical form `Canon`, with `Atom`, `Basis`, `Monomial`, and `Poly`,
in exact rational and big-integer arithmetic. `check.rs` compares two canonical
forms over six rungs and returns `Outcome` (`Decided(Verdict)` or
`Undecidable(Undecidable)`); `rounding.rs` owns rung 5 (`Rounding`, `rounds_to`).
No stage uses a float, no stage runs a search, and no stage panics on any input.

### 4.6 Versioning of events and config

The tree already carries five independent version and drift markers.

| Marker | Where | Value | What a bump does |
|---|---|---|---|
| `events.v` | `migrations/0003_event_log.sql:18`; `SchemaVersion` at `crates/core/src/event/scalar.rs:28-51` | `SCHEMA_VERSION` = 1 | The type reads and writes 1 and nothing else. 1.0 applies `SHIMS[v]` below `SCHEMA_VERSION`; the table is empty, so any other value is an error. |
| `learner_models.projector_version` | `migrations/0003_event_log.sql:34`; `PROJECTOR_VERSION` at `crates/core/src/projector/mod.rs:64` | 3 | A bump forces a full replay (D-O6). |
| `learner_models.config_hash` | `migrations/0003_event_log.sql:35`; `Config::config_hash` at `crates/core/src/config.rs:342` | the first 16 hex characters of the SHA-256 of the serialized `Config`; a default config hashes to `797575e985c12149` | Config drift detection. A `config_changed` event also exists. |
| `learner_models.curriculum_hash` | `migrations/0003_event_log.sql:36`; `curriculum_hash` at `crates/core/src/curriculum/dump/mod.rs:79` | nullable | Content drift detection. A `curriculum_changed` event also exists. |
| `POOL_ROW_VERSION` | `crates/core/src/pool/row.rs:50` | 1 | A bump retires every unclaimed `serving_pool` row, because the reader refuses a version it does not know. |
| `content_store.digest` and `prompt_digest` | `migrations/0005_content.sql:12`, `0012_content_prompt_digest.sql:23` | `sha256(kp_id, kind, body)` | Approval binds to the body digest (C6). A prompt change makes a row stale, and `cadus-worker author --stale` lists it. |

A new event type, a new attempt field, or a new outcome value therefore needs a
decision on `SCHEMA_VERSION`, and any change to the fold needs a
`PROJECTOR_VERSION` bump.

---

## 5. The content pipeline

### 5.1 The worker authoring code — `crates/worker/src/authoring/`

Eleven files under five modules (`crates/worker/src/authoring/mod.rs:16-20`):
`cli`, `cost`, `job`, `prompt`, `repair`. The module head states the rule:
"Authoring is a batch job of `cadus-worker`. It never runs in a request handler
(R4, L6)."

**`prompt/`** builds the request and makes no model call.
`Kind` (`prompt/mod.rs:55-66`) is the enum of the four products:

```rust
pub enum Kind {
    Template,     // a problem template
    Teach,        // a teach page: the concept and one worked example (L4)
    HintLadder,   // the rungs the hint route serves (L5)
    Diagnosis,    // a distractor list: the pre-authored diagnosis of A4
}
```

`Kind::as_str` (`prompt/mod.rs:123-130`) maps them to the database strings
`template`, `teach`, `hint_ladder`, `diagnosis`. `KINDS`
(`prompt/mod.rs:69-74`) fixes the order, template first. The forced tool names are
`emit_template`, `emit_teach`, `emit_hint_ladder`, `emit_distractors`
(`prompt/mod.rs:77-91`). Main functions: `user_message` (`:244`), `request`
(`:297`), `retry_block` (`:215`), `render_exemplars` (`:224`), `prompt_digest`
(`:348`). The input type is `AuthoringSpec` (`:191`), built from the curriculum.
`prompt/schema.rs` holds `tool_schema` and `tool_spec`; `prompt/system.rs` holds
`system_prompt` (the cached prefix, T5).

**`job/`** runs the loop and writes the row. `job/mod.rs` fixes the constants:
`AUTHORING_ATTEMPTS = 5`, `BANK_TARGET = 3`, `STATUS_PENDING`, `STATUS_APPROVED`,
`STATUS_REJECTED`, `DIGEST_PREFIX = "sha256:"`. `document_digest(kp_id, kind,
body)` covers the knowledge point, the kind, and the body, NUL separated. `Outcome`
(`job/mod.rs:174`) has six values: `Skipped`, `Stored`, `Duplicate`, `Refreshed`,
`Rejected { same_body }`, `Declined`.

`job/pass.rs:157`, `author_one(db, job, kind, spec) -> Report`, runs five steps:
1. `slots_taken` and `stale_slots`; a full bank returns `Skipped` with no model
   call.
2. `undecidable(kind, spec)` declines a non-decidable answer kind with no call, for
   `Template` and `Diagnosis` only (`pass.rs:26-35`).
3. Up to `job.attempts` model calls. Each reply goes to `verify_kind`. A gate
   rejection's literal message becomes the next attempt's feedback — the A2 lesson
   that the rejection message is the biggest yield lever.
4. A gate pass calls `store_verified` → `store_pending`.
5. `run_batch` (`pass.rs:309`) orders the specs stale first.

`job/verify.rs:264-281`, `verify_kind`, repairs the LaTeX escapes first on every
kind, then dispatches to `verify`, `verify_teach`, `verify_hint_ladder`, or
`verify_diagnosis`. The instruction gates are in the core
(`crates/core/src/instruction/`): the teach gate needs a non-empty `concept`, a
non-empty `worked_example.problem`, at least one non-empty step, a worked problem
that is not an exemplar, and a last step that names no other served problem's
answer; the hint gate needs at least one rung, no repeated rung, and no rung that
names an answer the knowledge point serves. Both gates read the exemplar answers
plus every rendered instance of every `approved` **and** `pending` template of the
knowledge point.

`job/store.rs` holds the database side: `served_instances` (`:48`), `slots_taken`
(`:90`), `stale_slots` (`:131`), `stale_rows` (`:176`), and
`store_pending(db, kp_id, kind, body, attempts, spend)` (`:357-398`), which is the
one path an artifact takes to the table.

`cost.rs` is the T3 accounting: `ATTEMPT_ALERT = 3`, `spend(attempts)`,
`alerting(db)` (a SELECT on `content_store WHERE authoring_attempts > $1`).
`repair.rs` repairs LaTeX escapes and refuses a stray control character;
`NO_REPAIR_FIELDS = ["answer_expr", "expected"]`.
`cli.rs` parses the `author` subcommand and renders the plan and the batch report.

### 5.2 The store — `content_store` and the review columns

There is **no separate review or approval table**. Review is three columns on
`content_store`.

`migrations/0005_content.sql:11-26`:

```sql
CREATE TABLE content_store (
    digest             text PRIMARY KEY,  -- content address of body; approval binds to it (C6)
    kp_id              text NOT NULL,
    kind               text NOT NULL CHECK (kind IN ('template','teach','hint_ladder','diagnosis')),
    body               jsonb NOT NULL,
    status             text NOT NULL DEFAULT 'pending'
                       CHECK (status IN ('pending','approved','rejected')),  -- C6: 'pending' is never served
    approved_by        uuid NULL REFERENCES users(id) ON DELETE SET NULL,
    approved_at        timestamptz NULL,
    authoring_attempts integer NOT NULL DEFAULT 1,  -- T3: alert above 3 attempts for one KP
    authoring_cost_usd numeric(12,6) NULL,  -- T3: exact money; never a float
    created_at         timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX content_store_kp_kind_status ON content_store (kp_id, kind, status);
```

Two later columns:

```sql
-- migrations/0011_content_review.sql:21
ALTER TABLE content_store ADD COLUMN review_reason text;
-- migrations/0012_content_prompt_digest.sql:23-27
ALTER TABLE content_store ADD COLUMN prompt_digest text;
CREATE INDEX content_store_kind_prompt_digest ON content_store (kind, prompt_digest);
```

`migrations/0006_grants_rls.sql:365`: `REVOKE INSERT, UPDATE, DELETE ON
content_store FROM cadus_app`. The table stands outside row-level security. The
runtime role reads it; only `cadus_admin` writes it.

The Rust side, `crates/store/src/content/mod.rs`:

| Item | Line | SQL |
|---|---|---|
| `approved_document(executor, kp_id, kind)` | 79-105 | `SELECT digest, body FROM content_store WHERE kp_id=$1 AND kind=$2 AND status='approved' ORDER BY approved_at DESC NULLS LAST, created_at DESC, digest LIMIT 1` |
| `insert_pending(admin, doc) -> bool` | 236-257 | `INSERT INTO content_store (…) VALUES (…) ON CONFLICT (digest) DO NOTHING`; status is always `pending` and no parameter takes it |
| `refresh_prompt_digest(admin, digest, prompt_digest)` | 285-304 | `UPDATE … SET prompt_digest=$2 WHERE digest=$1 AND prompt_digest IS NOT NULL AND prompt_digest <> $2` |
| `approve(admin, digest, approved_by) -> Decision` | 328-352 | `UPDATE … SET status=$2, approved_by=COALESCE(approved_by,$3), approved_at=COALESCE(approved_at, now()) WHERE digest=$1 RETURNING …` |
| `reject(admin, digest, reason) -> Decision` | 378-396 | `UPDATE … SET status=$2, review_reason=$3, approved_by=NULL, approved_at=NULL WHERE digest=$1 RETURNING …` |
| `verdict(db, digest)` | 423-437 | `SELECT status, review_reason FROM content_store WHERE digest=$1` |

`crates/store/src/content/review.rs` is the read side of the review screen:
`ReviewFilter`, `ReviewItem`, `review_list` (`:155`), `RegateRow`, `regate_rows`
(`:201-224`), `StoredDoc`, `document` (`:247`). `BANK_TARGET = 3`,
`LIST_LIMIT = 200`.

`crates/store/src/pool/refill.rs` is the serve side: `approved_template`
(`:201-225`) filters `status='approved'`, and `retire_unapproved` (`:38-70`)
claims every unclaimed `serving_pool` row whose content row is no longer
`approved`. A rejection therefore withdraws pooled instances.

### 5.3 The web admin — `crates/web/src/admin/`

Three modules: `mod.rs` (the gate and the shared JSON), `queue.rs` (`list`,
`show`), `review.rs` (`approve`, `reject`). Four routes
(`crates/web/src/lib.rs:253-256`):

```rust
.route("/api/admin/content", get(admin::list))
.route("/api/admin/content/{digest}", get(admin::show))
.route("/api/admin/content/{digest}/approve", post(admin::approve))
.route("/api/admin/content/{digest}/reject", post(admin::reject))
```

The gate is the `AdminUser` extractor (`crates/web/src/admin/mod.rs:205-225`):
no session gives 401, `!user.is_admin` gives 403. `admin_path(state)` gives 503
`admin_path_unavailable` when the deployment configured no `cadus_admin` handle.

`approve` (`crates/web/src/admin/review.rs:46-76`) calls `content::approve`, then
`regate_after_approval` (`:154-199`): it re-reads `regate_rows`, re-runs the two
instruction gates, and rejects every **pending** `teach` or `hint_ladder` row that
the newly approved material now gives away (`:184`). An already approved page is
never re-judged. `reject` (`:243-260`) requires a body `{"reason": "…"}`;
`reason_of` (`:207-224`) answers 422 for a missing, blank, or over-length reason
(`REASON_MAX_CHARS = 1000`).

The SPA calls all four (`web/src/api/endpoints.ts:141-160`,
`web/src/api/contract.ts:200-203`). The screens are `/review`
(`web/src/views/admin/Review.tsx`, `ReviewDocument.tsx`) and `/ops`
(`web/src/views/admin/Ops.tsx`). Nothing links to them:
`web/src/app/routes.ts:11-16` explains that `users.is_admin` is outside the runtime
role's column grants, so the SPA cannot tell whether the account is an operator.

### 5.4 The approval states

There is **no Rust enum for the status**. The three states are `&str` constants,
declared twice and kept identical:

| State | Store crate | Worker crate | Database |
|---|---|---|---|
| `"pending"` | `crates/store/src/content/mod.rs:118` | `crates/worker/src/authoring/job/mod.rs:146` | `content_store.status` DEFAULT |
| `"approved"` | `crates/store/src/content/mod.rs:121` | `crates/worker/src/authoring/job/mod.rs:149` | — |
| `"rejected"` | `crates/store/src/content/mod.rs:124` | `crates/worker/src/authoring/job/mod.rs:152` | — |

The CHECK constraint that enumerates them is `migrations/0005_content.sql:16-17`.
The one true Rust enum in the pipeline is the kind, not the status
(`crates/worker/src/authoring/prompt/mod.rs:55-66`).

Transitions:

| From | To | Who | Where |
|---|---|---|---|
| none | `pending` | the authoring pass, as `cadus_admin` | `insert_pending`, `crates/store/src/content/mod.rs:236-257`. No caller writes `approved` directly. |
| `pending` | `approved` | a human admin over HTTP | `POST /api/admin/content/{digest}/approve` |
| `pending` | `rejected` | a human admin over HTTP, with a mandatory reason | `POST /api/admin/content/{digest}/reject` |
| `pending` | `rejected` | the service, as a side effect of an approval | `regate_knowledge_point`, `crates/web/src/admin/review.rs:184`; `teach` and `hint_ladder` rows only |
| `rejected` | `approved` | a human admin | documented at `crates/store/src/content/mod.rs:319-321`: "a reviewer who refused a digest by mistake needs a way back" |
| `approved` | `rejected` | a human admin | the same `reject` statement; it clears `approved_by` and `approved_at` |

`approve` is idempotent: `COALESCE` keeps the first stamp. A prompt edit changes no
status; it only marks rows stale through `prompt_digest`.

Only `approved` serves: `approved_document`
(`crates/store/src/content/mod.rs:91`) and `approved_template`
(`crates/store/src/pool/refill.rs:212`) both filter on it.

### 5.5 End to end

A human operator runs `cadus-worker author`
(`crates/worker/src/bin/cadus-worker.rs:63-79`;
`crates/worker/src/authoring/cli.rs:38-41`). The pass reads the curriculum, builds
the specs, connects as `cadus_admin`, prints the plan, and runs one batch per kind
in `prompt::KINDS` order, template first
(`crates/worker/src/bin/cadus-worker.rs:140-143`). `--dry-run` stops before any
model call. `--stale` prints the rows a prompt change invalidated. Each verified
document lands as `status = 'pending'` and serves nothing. A human then opens
`/review`, and only after `approve` do `approved_document` and
`approved_template` see the row.

`diagnosis_jobs` is the only queue table in the neighborhood, and it is the runtime
A4 miss-diagnosis queue, not an authoring queue. Its states are `pending`,
`running`, `done`, `failed`, `capped`.

---

## 6. Gaps against the assignment's outcomes

### (1) Answer contracts with an ungraded outcome

**Exists.**
- The core already carries the third outcome. `Outcome::Undecidable(Undecidable)`
  with a fixed, log-safe reason string
  (`crates/core/src/answer/check.rs:68-76`, `crates/core/src/answer/mod.rs:45-62`).
- Four answer kinds are declared: `numeric`, `expression`, `multi-step`, `proof`
  (`crates/core/src/curriculum/model.rs:109-117`).
- The refusal set is measured and committed: 265 of 3,492 corpus answers, in 18
  named groups with a recommended action each
  (`docs/reference/undecidable-answers.md`;
  `crates/core/tests/fixtures/answers/undecidable_1_0.jsonl`).
- One route already refuses to ask an undecidable question:
  `crates/web/src/diag/mod.rs:61-63`.
- The requirement is written: V2 says "Undecidable kinds (`proof`, free-form
  multi-step) are graded per A4's async path and never claim a deterministic
  verdict."

**Missing.**
- The web layer collapses the third outcome into wrong.
  `crates/web/src/grade/verdict.rs:38-42` folds `Outcome::Undecidable` into
  `correct: false`, `WorkQuality::NearlyPassable`.
- The grade route refuses `multi-step` and `proof` with HTTP 409
  (`crates/web/src/grade/route.rs:21-35`), so those topics have no path at all.
- The session composer applies no answer-kind filter. Only the placement
  diagnostic filters. A session plan can therefore hold a task the grade route
  refuses.
- The `attempt` event has no ungraded value. `Attempt.correct` is a `bool`
  (`crates/core/src/event/body.rs`), and `WorkQuality` has six tiers, none of them
  "not graded" (`crates/core/src/event/kind.rs:48-67`).
- No production reads `q R r`, and no tuple authoring exists for the 16
  quotient-and-remainder answers (`docs/reference/undecidable-answers.md` §3.3).
- No route or client string exists for "we cannot mark this".

### (2) Readiness audit and eligibility

**Exists.**
- A prerequisite DAG with encompassing weights
  (`crates/core/src/curriculum/graph.rs`, arena at
  `crates/core/src/curriculum/arena/`).
- The frontier: `frontier`, `mastered_set`, `course_scope`, `TopicSet`
  (`crates/core/src/selector/topic_set.rs`), wrapped by
  `crates/core/src/selector/frontier.rs`.
- The cross-course gap fill: `is_course_complete`, `blocking_gap_ancestors`,
  `resolve_gap_fill_stack`, `serveable_gap_frontier`
  (`crates/core/src/selector/gap_fill.rs`).
- A placement diagnostic: `crates/core/src/diagnostic/` plus
  `crates/web/src/diag/`, three routes, greedy set-cover probes, four placement
  bands.
- Task-level validity re-checks: `task_still_valid`, `reserve_open_plan`,
  `ValidityContext` (`crates/core/src/selector/reserve.rs`).
- An operator view of content coverage: `GET /api/operator/flags`
  (`crates/web/src/operator.rs`).

**Missing.**
- Mastery is a status test and nothing else.
  `crates/core/src/xp.rs:75-80` counts `Learning`, `Placed`, and `Floor` as
  mastered, so placement credit and the enrollment floor equal practiced mastery
  in every downstream decision (finding l).
- No readiness score, no audit report, and no per-topic eligibility record. A grep
  over `crates/` for `eligib` finds only `drill_eligible`
  (`crates/core/src/selector/reserve.rs:32`) and quiz re-eligibility
  (`crates/core/src/selector/quiz.rs:309`).
- No eligibility check couples the answer kind to the content state, so the
  selector can plan a task that neither serves (no content, finding h) nor grades
  (undecidable kind, finding a).
- The knowledge-point pass rule is hard coded and ignores its own config string
  (`crates/core/src/projector/entry.rs:171-180`, finding k), so an audit cannot
  change the pass bar.

### (3) Independent practice after feedback

**Exists.**
- The H3 assisted-correct rework loop, end to end: the server stashes the assisted
  attempt and keeps the problem live
  (`crates/web/src/grade/route.rs:125-133`, `:208-210`), the reply names the
  solution and the stock instruction
  (`crates/web/src/grade/reply.rs:8-16`), and the client clears the field and
  returns to `ready` (`web/src/views/session/useGrade.ts:116-130`).
- A stock re-solve instruction on every miss:
  `RE_SOLVE` at `crates/web/src/grade/mod.rs:188-190`, added to the reply at
  `crates/web/src/grade/reply.rs:64-66`.
- The re-solve is untimed and the drill countdown stops
  (`web/src/views/session/useSessionClock.ts:45-49`).
- The re-solve attempt has its own id: `{attempt_id}-rework`
  (`crates/web/src/grade/route.rs:234`).

**Missing.**
- After a **wrong** answer there is no enforced independent attempt. The reply
  carries `re_solve` prose and the **next** problem at the same time
  (`crates/web/src/grade/reply.rs:53-65`, `crates/web/src/grade/route.rs:189-199`).
  Nothing verifies that the learner solved the original problem again.
- The solution stays on screen beside the live answer field during the assisted
  rework (`web/src/views/session/Session.tsx:451`,
  `web/src/views/session/Feedback.tsx:106-116`), so the "unaided" re-solve is not
  unaided (finding m).
- A successful rework still records `assisted: true`
  (`crates/web/src/grade/route.rs:235-238`), so the learner gets half FIRe credit
  (`ASSISTED_CREDIT = 0.5`, `crates/core/src/fire/mod.rs:74`) for work that was in
  fact independent. The flag clears only on failure.
- No event and no field records "practiced independently after feedback".

### (4) Integrated tasks

**Exists.**
- `TaskType::MultiStep` as a session task type, with a cadence and a component
  chooser: `multistep_is_due`, `multistep_components`
  (`crates/core/src/selector/multistep.rs:17-60`), `MULTISTEP_CADENCE = 4`,
  `MULTISTEP_MIN_COMPONENTS = 3`, `MULTISTEP_MAX_COMPONENTS = 4`
  (`crates/core/src/selector/mod.rs:115-124`).
- Its own XP tier: `MULTISTEP_XP` (`crates/core/src/xp.rs:96`).
- `AnswerKind::MultiStep` as a curriculum field, on 483 topics repository wide and
  78 in Foundations.

**Missing.**
- The served task is not integrated. `crates/web/src/serve/target.rs:75-84` serves
  **one independent whole problem per component topic**, records the attempt
  against that component, and ends when the list runs out. Nothing composes a
  single problem that needs several skills at once (finding o).
- `AnswerKind::MultiStep` has no grading path at all. The grade route answers 409
  (finding a), so the 78 Foundations topics and the 483 repository-wide topics
  carry authored exemplars nobody can be graded on. Many of their final answers are
  plain integers (finding b).
- There is no step model: no intermediate-answer contract, no per-step verdict, no
  partial credit. `WorkQuality` carries the only partial-credit axis (C4) and
  correctness alone sets it (finding g).
- `crates/core/src/selector/mod.rs:124` carries `MULTISTEP_ENABLED: bool = true`,
  so the cadence is live in every plan.

### (5) Delayed-retention measurement

**Exists.**
- A full memory model: `TopicState` with `memory_base`, `t0`, `interval_days`,
  `rep_num`, `ability`, `speed` (`crates/core/src/learner.rs:65-97`).
- Decay and due tests: `memory_at`, `review_state`, `decay_for`, `interval_for`
  (`crates/core/src/fire/mod.rs:167-265`), with
  `NEARLY_DUE_THRESHOLD = 0.6`, `INTERVAL_CAP_DAYS = 730.0`.
- Order-sensitive review grading that demands an improving trajectory:
  `grade_review` (`crates/core/src/fire/attempt.rs:311-342`, finding n).
- A quiz cadence with a retake rule: `quiz_is_due`, `quiz_retake_available_at`
  (`crates/core/src/selector/quiz.rs`), `QUIZ_RECENT_DAYS = 14`.
- Drills at a 3.5-day cadence for mastered topics with high ability
  (`DRILL_INTERVAL_DAYS`, `DRILL_MASTERY_ABILITY`,
  `crates/core/src/selector/task.rs:296`).
- The append-only event log preserves every attempt with its timestamp, so a
  delayed measure is derivable after the fact
  (`migrations/0003_event_log.sql:12-25`).

**Missing.**
- No delayed-retention measure is computed or stored. A grep over `crates/` for
  `retention` finds only the pool retention job comments
  (`crates/store/src/pool/refill.rs:18`, `crates/store/src/pool/pop.rs:223`).
- No event type records a retention probe. The 16 event types
  (`crates/core/src/event/mod.rs:55-106`) hold no such member.
- `LearnerModel` carries no retention field
  (`crates/core/src/learner.rs:216`); `memory_base` is a model prediction, not a
  measurement.
- The review pass is a single pass-or-fail plus a weighted score. Nothing separates
  "correct at a one-day delay" from "correct at a thirty-day delay", and nothing
  records the delay of the answer beside the answer itself.
- The dashboard reports XP, streak, and velocity (`crates/core/src/xp.rs`), not
  retention.

### 6.1 Two cross-cutting facts that shape all five

1. **`content_store` is empty on the deployment.** Finding h. Until an authoring
   pass runs against a real model provider, `teach` and `hint` answer 409, every
   problem comes from exemplar rotation, and the SPA silently skips the worked
   example (finding j). Any outcome that needs instruction, hints, or templates
   needs that pass first.
2. **The undecidable outcome dies at one line.**
   `crates/web/src/grade/verdict.rs:38-42`. Outcomes (1), (3), and (4) all pass
   through it. A contract with a third outcome needs a new `WorkQuality` value or
   a new attempt field, and that touches `SCHEMA_VERSION`
   (`crates/core/src/event/scalar.rs:28-51`) and `PROJECTOR_VERSION`
   (`crates/core/src/projector/mod.rs:64`).

---

## 7. Files a new assignment touches first

| Concern | File |
|---|---|
| The kind gate and the third outcome | `crates/web/src/grade/route.rs:21-35`, `crates/web/src/grade/verdict.rs:20-44` |
| The checker outcome type | `crates/core/src/answer/check.rs:68-76`, `crates/core/src/answer/mod.rs:45-62` |
| New grammar productions | `crates/core/src/answer/parse/atom.rs`, `crates/core/src/answer/parse/term.rs`, `crates/core/src/answer/canon/read.rs` |
| The event contract | `crates/core/src/event/body.rs`, `crates/core/src/event/kind.rs`, `crates/core/src/event/scalar.rs` |
| The fold | `crates/core/src/projector/handlers.rs`, `crates/core/src/projector/entry.rs`, `PROJECTOR_VERSION` at `crates/core/src/projector/mod.rs:64` |
| Mastery and eligibility | `crates/core/src/xp.rs:75-80`, `crates/core/src/selector/topic_set.rs`, `crates/core/src/selector/gap_fill.rs` |
| Integrated tasks | `crates/web/src/serve/target.rs:66-109`, `crates/core/src/selector/multistep.rs` |
| Independent practice | `crates/web/src/grade/route.rs:125-133,206-241`, `web/src/views/session/useGrade.ts:116-130`, `web/src/views/session/Feedback.tsx:94-117` |
| The content pipeline | `crates/worker/src/authoring/`, `crates/store/src/content/`, `crates/web/src/admin/` |
| The curriculum | `curriculum/`, `crates/core/src/curriculum/model.rs`, `crates/core/src/curriculum/load/` |
| The residue list | `docs/reference/undecidable-answers.md`, `crates/core/tests/fixtures/answers/undecidable_1_0.jsonl` |
