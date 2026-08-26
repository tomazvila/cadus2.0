# M3 adversarial review — round 1 (2026-08-27)

Run on commit 1b4bcb1 (M3 U1–U5 integrated). Six lenses, two find/refute rounds, major+ only: 25 raised, 15 confirmed. Assigned to FIXM3a (sources, gate) and FIXM3b (tests, oracles, fixtures).

## Orchestrator rulings (binding)

- Release parity (#1/#2): `memory_at` and every other `powf` with a literal base call the
  platform `pow` through `std::hint::black_box` on the base (no LLVM `exp2` rewrite), with a
  comment naming the trap. `scripts/gate.sh` gains a release-profile step:
  `cargo test --release -p cadus-core --test parity_events --test projector` — the parity
  digests must hold in BOTH profiles. Add trap T21 to the spec §7.
- `Slug` in `event.rs` (#3/#5) trims surrounding whitespace and rejects an all-whitespace
  value, the same as `curriculum::model::Slug` (1.0 `strip_whitespace=True`). One `Slug`
  type for the crate if the two can be unified without churn; otherwise two with one rule.
- Non-finite memory (#11): where CPython raises `OverflowError` and the 1.0 fold produces no
  model, the 2.0 fold returns `Err(ProjectorError::NonFinite { topic, event_index })`.
- Integer range (#12): `round_half_even_i64` returns `Err` outside the i64 range; the fold
  propagates it as `ProjectorError::OutOfRange`. A documented divergence (1.0 returns a
  Python big integer); pin the boundary.
- `profile_reset` incremental divergence (#4): keep the port equal to 1.0's primitive; pin
  the divergent splits and their 1.0 digests in `incremental_1_0.json` (like the regrade
  case) and correct the justification text to what 1.0 does (`service.py:272` forces a
  full replay only on `Regraded` or a version bump). M5 requirement: no extra rule.
- Boundaries (#6, #7, #8, #9, #10, #14, #15): one literal test per boundary at equality,
  generated from the 1.0 oracle where a digest is involved; mutation-check each.
- Selector oracle (#13): regenerate `selector_1_0.json` with `n = 40` so the quiz,
  multi-step, and drill sections are compared; the quiz task by presence only.

| # | Sev | File | Unit | Title |
|---|---|---|---|---|
| 1 | blocker | `crates/core/src/fire.rs:163` | FIXM3a | memory_at compiles 0.5**x to exp2(-x), which is not CPython's pow: the release fold diverges from 1.0 on 10% of streams |
| 2 | blocker | `crates/core/src/fire.rs:163` | FIXM3a (dup of #1) | Optimized build rewrites the memory decay to exp2, so the release fold diverges from 1.0 and 8 parity tests fail |
| 3 | major | `crates/core/src/event.rs:184` | FIXM3a | Slug drops 1.0's strip_whitespace normalization, so a padded topic id folds onto a phantom topic |
| 4 | major | `crates/core/tests/projector.rs:434` | FIXM3b | The profile_reset D4 divergence is accepted on a 1.0 mitigation that does not exist |
| 5 | major | `crates/core/src/event.rs:184` | FIXM3a (dup of #3) | Slug keeps surrounding whitespace where 1.0 strips it, so a padded topic id silently drops the topic from the fold |
| 6 | major | `crates/core/src/projector.rs:335` | FIXM3b | The quiz retake threshold has no boundary test; a `<=` port diverges from 1.0 with the suite green |
| 7 | major | `crates/core/src/projector.rs:397` | FIXM3b | The initial `diagnostic_placed` balance filter has no zero-balance test; coverage.md's boundary row covers only the refresh path |
| 8 | major | `crates/core/src/xp.rs:279` | FIXM3b | Trap T1 (Neumaier summation) is pinned at only one of its four call sites |
| 9 | major | `crates/core/src/selector.rs:74` | FIXM3b | `schedule_drills` brackets neither the 0.95 automaticity bar nor the 3.5-day cadence window |
| 10 | major | `crates/core/src/selector.rs:52` | FIXM3b | `QUIZ_RECENT_DAYS` is free: the quiz strata test never brackets the 14-day recency window |
| 11 | major | `crates/core/src/fire.rs:163` | FIXM3a | memory_at returns a non-finite memoryBase where the 1.0 fold refuses to build a model |
| 12 | major | `crates/core/src/numeric.rs:92` | FIXM3a | round_half_even_i64 saturates to i64::MIN/MAX where 1.0 returns the exact Python integer, and a one-event stream reaches it |
| 13 | major | `scripts/oracle/dump_selector_1_0.py:42` | FIXM3b | The 1.0 selector oracle composes with n=8, so it never compares the quiz, multi-step, or drill sections of compose_session |
| 14 | major | `crates/core/src/xp.rs:275` | FIXM3b | The 28-day velocity window is never exercised at its first day, so an off-by-one window slip ships green |
| 15 | major | `crates/core/src/xp.rs:219` | FIXM3b | The streak rule's goal comparison is never read at equality on the reference day |

## FIXM3a

### #1 [blocker] memory_at compiles 0.5**x to exp2(-x), which is not CPython's pow: the release fold diverges from 1.0 on 10% of streams

File: `crates/core/src/fire.rs:163` — IDs: R5

**Claim.** `memory_at` computes `state.memory_base * 0.5_f64.powf(exponent)` with a literal base, so LLVM rewrites the call to `exp2(-exponent)`, which differs from the glibc `pow(0.5, x)` that CPython's `0.5 ** x` calls by one unit in the last place, and the whole FIRe fold inherits the error.

**Evidence.**

```
crates/core/src/fire.rs:163 -> `    state.memory_base * 0.5_f64.powf(exponent)`

$ cargo test -p cadus-core --release
  RUST BUG: stream_9.jsonl in UTC does not reproduce the 1.0 fold.
  rust ..."memoryBase":1.9366476620232926,"repNum":9.04755864153199...
  1.0  ..."memoryBase":1.9366476620232924,"repNum":9.04755864153199...
  the models diverge after event index 56
  RUST BUG: stream_17.jsonl in UTC does not reproduce the 1.0 fold.
  rust ..."interval_days":448.91186330660565,..."repNum":6.880430243486945...
  1.0  ..."interval_days":448.9118633066054,..."repNum":6.880430243486944...
  the models diverge after event index 60
  test result: FAILED. 142 passed; 8 failed

$ cargo test -p cadus-core --test parity_events   (debug profile, the one scripts/gate.sh runs)
  test result: ok. 150 passed; 0 failed

Isolation of the transform (same exponent, release build):
  const-base   0.5f64.powf(e)            = 0.9553215844447556 (3fee91fe924b4cd2)
  opaque-base  black_box(0.5f64).powf(e) = 0.9553215844447555 (3fee91fe924b4cd1)
  CPython      0.5 ** e                  = 0.9553215844447555 (3fee91fe924b4cd1)

Repair check: with the base made opaque so a real `pow` call is emitted,
  cargo test -p cadus-core --release --test parity_events -> 150 passed; 0 failed
```

**Failure scenario.** Fold `crates/core/tests/fixtures/events/stream_9.jsonl` with the release build. At event index 56, a `review_result` (`perfect`, `2026-08-25T09:00:00Z`) lands on `integer-addition-subtraction`, whose cached state is `memoryBase 1.4969625337795927`, `interval_days 454.9477814486727`, `t0 2026-07-26T09:00:00Z`. Both sides derive `days = 30.0` and `exponent = 0.06594163379470092`. 1.0 computes `memory = 1.4300806196247562` and stores `memoryBase = 1.580080619624756`; the port computes `memory = 1.4300806196247564` and stores `memoryBase = 1.5800806196247563`. The error then compounds through `repNum`, `interval_days`, and every propagated credit, so the final blob digest is `d1265b2dc63e672a788057e16079ebd88703c377eb9afa7d66ad82ff3eacc868` where 1.0 gives `4d4258859b1612bf08f3c019f9c59a27c0e0eb41ebc9a4fe75d54e17ad445043`. I reproduced the same class of divergence on 5 of 50 freshly generated streams (`scripts/oracle/gen_stream_1_0.py --seed 21..70`, divergent seeds 46, 51, 56, 58, 65), and 0 of 50 after forcing a real `pow` call. Because `scripts/gate.sh` runs `cargo test --workspace` in the debug profile, where the optimization does not fire, the gate stays green while the shipped release binary produces learner models that are not the 1.0 models.

**Refuter.** The claim reproduces completely and is a true R5 parity defect. In the release profile LLVM rewrites `0.5_f64.powf(exponent)` at crates/core/src/fire.rs:163 into a glibc `exp2(-exponent)` call. glibc `exp2` and glibc `pow` differ by one unit in the last place on real fold inputs, so the release build of the FIRe fold does not reproduce the 1.0 model, while the debug build (the profile scripts/gate.sh uses) does. I confirmed the rewrite in the linked symbols, in the disassembly at the memory_at site, in the numeric difference, and in the test outcome asymmetry. The shipped Docker image builds with --release, so the divergent path is the one that ships.

### #2 [blocker] Optimized build rewrites the memory decay to exp2, so the release fold diverges from 1.0 and 8 parity tests fail

File: `crates/core/src/fire.rs:163` — IDs: R5, D3, D4 — duplicate of #1

**Claim.** `memory_at` writes `0.5_f64.powf(exponent)`, and LLVM rewrites that literal-base call into `exp2(-exponent)`, which differs from CPython's `0.5 ** exponent` by one unit in the last place on about 0.1 percent of exponents, so an optimized build folds a different learner model than the 1.0 oracle.

**Evidence.**

```
crates/core/src/fire.rs:163:    state.memory_base * 0.5_f64.powf(exponent)

$ nm -C target/release/blob | grep exp2
                 U exp2          # no source file of cadus-core calls exp2
$ objdump -d target/release/blob | grep -c 'call.*exp2'
3

# same primitive, same release binary, base kept opaque:
x=1.2327056287469915  0.5f64.powf(x)                    = 0.4255186798354915
                      black_box(0.5).powf(black_box(x)) = 0.42551867983549146
                      CPython 0.5**x                    = 0.42551867983549146
215 of 200000 sampled exponents diverge.

# the 913 memory_at exponents that the 1.0 fold of stream_9 actually evaluates:
exponent 0.06594163379470092: rust 0.5f64.powf(x) = 0.9553215844447556
                              CPython 0.5**x      = 0.9553215844447555
3 of 913 memory_at exponents diverge

$ cargo test -p cadus-core --release --test parity_events
test result: FAILED. 142 passed; 8 failed; 0 ignored
  stream_9::folds_to_the_committed_digests ... FAILED
  stream_9::apply_regrades_is_idempotent ... FAILED
  stream_9::incremental_reproduces_the_1_0_fold_at_every_split ... FAILED
  stream_9::without_the_corrections_folds_to_the_pre_correction_digest ... FAILED
  stream_17::... (the same four)
  crates/core/tests/parity_events.rs:566:
    left:  "d1265b2dc63e672a788057e16079ebd88703c377eb9afa7d66ad82ff3eacc868"
   right:  "4d4258859b1612bf08f3c019f9c59a27c0e0eb41ebc9a4fe75d54e17ad445043"

$ cargo test -p cadus-core --test parity_events        # debug profile
test result: ok. 150 passed; 0 failed

# model diff, stream_9, release build against the live 1.0 oracle:
/topics/integer-addition-subtraction/memoryBase  rust 1.9366476620232926  1.0 1.9366476620232924
# model diff, stream_17:
/topics/parts-of-an-expression/repNum            rust 6.880430243486945   1.0 6.880430243486944
/topics/parts-of-an-expression/interval_days     rust 448.91186330660565  1.0 448.9118633066054

# 400 structured mutants of the 20 committed streams, folded by both sides:
#   release build: 29 digest mismatches against 1.0
#   debug build:    0 digest mismatches against 1.0
#   release against debug: 34 of 400 models differ, every differing field is
#   memoryBase, repNum, or interval_days

Dockerfile:40:RUN cargo build --release --workspace
scripts/gate.sh:43:cargo test --workspace          # debug profile only
```

**Failure scenario.** Build the workspace the way `Dockerfile:40` builds it (`cargo build --release`). Fold the committed `crates/core/tests/fixtures/events/stream_9.jsonl` with `project`. The release binary returns blob digest `d1265b2d...`, while `scripts/oracle/dump_projector_1_0.py` and the committed `digests_1_0.json` both give `4d425885...`. The topic `integer-addition-subtraction` gets `memoryBase` 1.9366476620232926 instead of 1.9366476620232924. Two of the twenty committed streams and 29 of 400 mutants land on a wrong model, and every `learner_models` row that a release binary writes carries values that a debug binary never reproduces. `scripts/gate.sh` runs `cargo test` in the debug profile, so the gate stays green while the shipped binary breaks R5.

**Refuter.** I could not refute the claim. I confirmed all four links of the chain on this machine, with the repository as it stands.

1. The rewrite is real. LLVM applies the exact transform `pow(2**n, x) -> exp2(n*x)`. For base 0.5, n is -1. The transform is not gated on fast-math, so a plain `cargo build --release` gets it. `objdump` of the release test binary shows `cadus_core::fire::memory_at` compute the exponent with `divsd`, then `call *...<exp2@GLIBC_2.29>`. The binary imports `exp2` (`U exp2`) and imports no libm `pow` at all. No source file in `crates/` writes `exp2`.

2. The two primitives give different f64 values. glibc `exp2(-x)` and glibc `pow(0.5, x)` are each under 1 ulp, so they disagree on a small fraction of inputs. A standalone rustc test gives 179 divergences of 200000 sampled exponents between `0.5_f64.powf(x)` at `-O` and `black_box(0.5).powf(black_box(x))`. CPython agrees wi

### #3 [major] Slug drops 1.0's strip_whitespace normalization, so a padded topic id folds onto a phantom topic

File: `crates/core/src/event.rs:184` — IDs: R5, C2

**Claim.** 1.0 declares every curriculum id as `Slug = Annotated[str, StringConstraints(min_length=1, strip_whitespace=True)]`, so pydantic strips surrounding whitespace before validation; the 2.0 `Slug::new` only rejects the empty string and never trims, so every Slug-typed event field with padding folds to a different model than 1.0 gives.

**Evidence.**

```
1.0 `cadus/model.py:23`: `Slug = Annotated[str, StringConstraints(min_length=1, strip_whitespace=True)]`. 2.0 `crates/core/src/event.rs:182-188`:
```rust
pub fn new(text: impl Into<String>) -> Result<Self, EventError> {
    let text = text.into();
    if text.is_empty() {
        return Err(EventError::Json("a curriculum id must not be empty".into()));
    }
    Ok(Self(text))
}
```
The doc comment above it (event.rs:169) records only `min_length=1` and omits `strip_whitespace`. `docs/reference/event-schemas-1.0.json` also carries only `{"minLength": 1, "type": "string"}` — JSON Schema cannot express the pydantic strip — so the padded value is schema-valid.

Two-event stream `{"type":"enrolled",...,"course":"foundations"}` + `{"type":"lesson_result","ts":"2026-03-02T09:00:00Z","topic":" absolute-value ","passed":true,"quality_tier":"perfect","xp":10.0,...}` folded by both:
  1.0 (`scripts/oracle/dump_projector_1_0.py`) sha256=af3cc77f069edf252691c90d0f32fcfbc6cf95d9882fdf677b3b6b256a2101e7
  2.0 (`project` + `blob_digest`)            sha256=acf5238d01cdf1d090841586517d4653eee30eb3a2ef8f08e67c4c0debf333e3
1.0 topics: `"absolute-value": {ability 0.65, repNum 1.5, interval_days 8.73076923076923, kp_progress {kp1:passed,kp2:passed}, status learning}`.
2.0 topics: `" absolute-value ": {ability 0.65, repNum 1.15, interval_days 5.324999999999999, kp_progress {}, status learning}` and no `absolute-value` at all.

The same one-line gap diverges on every Slug-typed field. Each pair below is a folded stream, 1.0 digest vs 2.0 digest:
  `lesson_result.topic` " absolute-value ": af3cc77f… vs acf5238d…
  `enrolled.course` " foundations": 2.0 resolves no course, so the mastery floor stamps nothing and `velocity.course_progress` is 0.0.
  `quiz_result.per_topic[].topic` "absolute-value\t": 1.0 applies FIRe to `absolute-value` (memoryBase 0.85, repNum 1.5038461538461536); 2.0 skips the row and writes no topic state (1564dbeb… vs f3de77e8…).
  `remediation_triggered.targets` [" absolute-value"]: 1.0 `pending_remediation` = `[{"kind":"lesson-fail","targets":["absolute-value"]}]`; 2.0 = `[]` (b009ebf5… vs 123ab5ef…).
  `profile_reset.topics` ["absolute-value\n"]: 1.0 clears the topic; 2.0 no-ops and keeps the learned state (031765a4… vs 64798e03…).
  `lesson_result.failed_at_kp` " kp2 ": 1.0 kp_progress `{kp1:passed, kp2:failed_once}`; 2.0 `{" kp2 ":failed_once, kp1:passed, kp2:passed}`.
  `diagnostic_placed.conditional` [" absolute-value "]: 1.0 sets `conditional: true`; 2.0
```

**Failure scenario.** An `attempt`/`lesson_result` reaches the events table with `"topic": " absolute-value "` — a stray space from an API payload, an import, or a hand-repaired row. C2 makes the event immutable, so the padding is permanent. The 2.0 fold creates a topic state keyed `" absolute-value "` with fabricated FIRe values (difficulty defaults to 0.5, the neighborhood is empty) and leaves the real `absolute-value` untouched: the learner's actual progress on that topic is lost, the phantom id is written into `learner_models`, and every later review, quiz stratum, and `course_progress` reading is computed against the wrong key. The 1.0 oracle folds the identical stream onto the real topic, so R5 parity fails on a schema-valid event.

**Refuter.** The claim is correct and I reproduced it on a real build. 1.0 declares `Slug = Annotated[str, StringConstraints(min_length=1, strip_whitespace=True)]` (`/home/deploy/dev/cadus/cadus/model.py:23`), and every event field the claim names carries that type: `Enrolled.course`, `LessonResult.topic`/`failed_at_kp`, `QuizTopicResult.topic`, `RemediationTriggered.targets`, `ProfileReset.topics`, `DiagnosticPlaced.conditional`, `Attempt.topic`/`kp`. The 2.0 `Slug::new` (`/home/deploy/dev/cadus2.0/crates/core/src/event.rs:182-188`) tests only `text.is_empty()`. It keeps the padding, so the fold keys the topic state on the padded text.

The 2.0 repository already holds the correct rule for the same 1.0 type. `/home/deploy/dev/cadus2.0/crates/core/src/curriculum/model.rs:70-77` trims and then tests for empty, and its doc comment cites `model.py:22-23`. `/home/deploy/dev/cadus2.0/docs/reference/curric

### #5 [major] Slug keeps surrounding whitespace where 1.0 strips it, so a padded topic id silently drops the topic from the fold

File: `crates/core/src/event.rs:184` — IDs: R5, C2 — duplicate of #3

**Claim.** `Slug::new` rejects only an empty string, but 1.0 declares `Slug` as `StringConstraints(min_length=1, strip_whitespace=True)` and strips the value before the length check, so 2.0 accepts and stores a padded or whitespace-only curriculum id that 1.0 normalizes or rejects.

**Evidence.**

```
1.0  cadus/model.py:23:
  Slug = Annotated[str, StringConstraints(min_length=1, strip_whitespace=True)]

2.0  crates/core/src/event.rs:184:
        if text.is_empty() {
            return Err(EventError::Json("a curriculum id must not be empty".into()));

2.0  crates/core/src/event.rs:169 (the doc comment that records the constraint):
  /// 1.0 declares these with `min_length=1`, so an empty id is a validation error
  # the `strip_whitespace=True` half of the constraint is absent from the port.

# stream_1.jsonl, every "topic":"absolute-value" replaced by "topic":" absolute-value "
# (8 fields), both sides folded with now=2000-01-01T00:00:00Z, tz=UTC, goal=40:
  a_clean.jsonl   2.0 ba128459985e0815db7446cb2af16452ec07d304b7efaa0952fc6567404245f5
  a_clean.jsonl   1.0 ba128459985e0815db7446cb2af16452ec07d304b7efaa0952fc6567404245f5
  b_padded.jsonl  1.0 ba128459985e0815db7446cb2af16452ec07d304b7efaa0952fc6567404245f5
  b_padded.jsonl  2.0 583ae1e7ccc1c7915aa4eeab8c4f35cd8221f4677275cbf88e09ff11b825ba18

# 4000 field-level mutations of one_per_type.jsonl, acceptance compared against
# cadus.events.validate_event: 4 events that 2.0 accepts and 1.0 rejects, and
# every one of them is a whitespace-only Slug, for example
  {"...","topic":" ","type":"lesson_result","v":1,"xp":8.924999999999999}
  {"...","failed_at_kp":" ","type":"lesson_result","v":1}
  {"...","topic":" ","type":"diagnostic_answer","v":1,"weight":1.0}
```

**Failure scenario.** Send a `lesson_result` whose `topic` is `" absolute-value "` (a client, a hand-written replay, or any writer that does not trim). 1.0 strips the value and credits the real topic `absolute-value`. 2.0 stores the padded id, `Curriculum::idx_of` finds no such topic, and `Projector::apply_fire_result` skips it, so the topic loses its FIRe state, its mastery date, and its place in the selector. The fold of `stream_1.jsonl` moves from digest `ba128459...` to `583ae1e7...`. A whitespace-only `topic` such as `" "` also enters the append-only `events` table, where C2 forbids any later edit, and it matches no topic for the life of the log.

**Refuter.** The claim is demonstrable, and I reproduced it end to end on both code bases. 1.0 declares `Slug = Annotated[str, StringConstraints(min_length=1, strip_whitespace=True)]` (`/home/deploy/dev/cadus/cadus/model.py:23`), and every event field that carries a curriculum id uses that type (`LessonResult.topic`/`failed_at_kp`, `Attempt.topic`/`kp`, `DiagnosticAnswer.topic`, `Enrolled.course`, `Regraded.topic`, `TaskServed.topic`/`kp`/`component_topics`, `RemediationTriggered.source_topic`/`targets`, `ProfileReset.topics`, `QuizTopicResult.topic`, `AnkiCardCreated.topic`). Every event of the 1.0 log passes through `cadus.events.validate_event`, which validates through that union (`/home/deploy/dev/cadus/cadus/events.py:56-63`), so pydantic strips the outer whitespace first and applies `min_length=1` to the stripped rest. 2.0's `Slug::new` (`/home/deploy/dev/cadus2.0/crates/core/src/event.rs:176-1

### #11 [major] memory_at returns a non-finite memoryBase where the 1.0 fold refuses to build a model

File: `crates/core/src/fire.rs:163` — IDs: R5, D4, C2

**Claim.** `memory_at` computes `memory_base * 0.5f64.powf(exponent)` with no guard on the result, so a `t0` later than the event instant by more than about 1024 intervals gives `inf`, while CPython's `0.5**exponent` raises `OverflowError` at the same input and the 1.0 fold produces no model at all.

**Evidence.**

```
crates/core/src/fire.rs:163   `state.memory_base * 0.5_f64.powf(exponent)`
crates/core/src/fire.rs:407   `let new_base = py_max(0.0, memory_at(state, t_us) + raw);`
1.0 cadus/fire.py:179        `return float(state.memoryBase * (0.5**exponent))`

Stream (3 events; `enrolled` 2026-01-01, `lesson_result` perfect on `absolute-value-inequalities` at 2060-01-01, `review_result` perfect on the same topic at 2026-01-02):

  1.0 oracle (scripts/oracle/dump_projector_1_0.py):
    FOLDERR OverflowError: (34, 'Numerical result out of range')
    File "/home/deploy/dev/cadus/cadus/fire.py", line 179, in memory_at

  2.0 (cadus_core::projector::project, debug build):
    memory_base = inf  is_finite=false
    stored json has null memoryBase: true
    READ BACK FAILS: invalid type: null, expected f64 at line 1 column 637
    sha256=51ceb27f237a4a0b7ba4ed23f8dee0ad492a0deadf1566fddb341fe34ada5872

The same divergence fired on 30 of 300 fuzzed `project_incremental` resumes that seeded FIRe from a cached model:
    Counter({('ERR', "OverflowError: (34, 'Numerical", 'OK'): 30})
Every other one of those 300 resumes matched 1.0 byte for byte.
```

**Failure scenario.** A learner's log holds one `lesson_result` on `absolute-value-inequalities` stamped 2060-01-01 (a client clock set to the wrong year, or an imported event), then a later `review_result` on the same topic stamped 2026-01-02. `interval_days` is 4.5 and `days_between` is -12418, so the exponent is -2759. 1.0 raises `OverflowError` and refuses to build the model. 2.0 folds it, writes `memoryBase = inf`, and `serde_json` renders that field as `"memoryBase":null`. Deserializing that stored model back into `LearnerModel` fails with `invalid type: null, expected f64`, so the `learner_models` row is unreadable and the D4 incremental resume can never seed from it again. Because C2 keeps the offending event in the log, every later full replay rebuilds the same unreadable model.

**Refuter.** The claim is demonstrable end to end, and I reproduced both halves of it on this box. 1.0 raises OverflowError in cadus/fire.py:179 for the described stream and builds no model. 2.0 folds the same stream, gives memory_base = inf on the topic, renders "memoryBase":null in the canonical blob, and then fails to deserialize that text back into LearnerModel. The stream is admissible: project() folds apply_regrades(events) in log order (projector.rs:1004) and sorts nothing by ts, and no timestamp monotonicity rule exists in REQUIREMENTS.md, docs/SCHEMA.md, or the event types, so C2 keeps the offending event and every later replay rebuilds the same unreadable model. The divergence is not a recorded ruling: the codebase documents its other OverflowError divergences in place (numeric.rs:85, xp.rs:214, xp.rs:242, xp.rs:362 each say "1.0 raises OverflowError there" with the reason the call site is 

### #12 [major] round_half_even_i64 saturates to i64::MIN/MAX where 1.0 returns the exact Python integer, and a one-event stream reaches it

File: `crates/core/src/numeric.rs:92` — IDs: R5

**Claim.** `round_half_even_i64` casts to `i64` with saturation, but Python `int(round(x))` has unbounded precision, so a finite XP value outside the `i64` range makes 2.0's `xp.total`, `xp.today`, and `quiz.xp_since` differ from 1.0's; the function's own doc claims no 1.0 call site reaches this, which the fuzz disproves.

**Evidence.**

```
numeric.rs:84-93:
/// A value outside the `i64` range saturates, and `NaN` becomes `0`, because the core
/// never panics. Python raises `OverflowError` there. No 1.0 call site reaches it:
/// every input is an XP total or a day count.
...
pub fn round_half_even_i64(x: f64) -> i64 {
    round_half_even(x) as i64
}
This divergence was found by fuzzing (stream m_30) and delta-minimized to one event; the two folds of that one event:
2.0: xp state = XpState { total: -9223372036854775808, today: -9223372036854775808, goal: 40, streak_days: 0 }, blob head
  {"config_hash":"797575e985c12149",...,"quiz":{"last_at":null,"retake_pending":false,"xp_since":-9223372036854775808},...
1.0 (cadus.projector.project, same curriculum and config): blob head
  {"config_hash":"797575e985c12149",...,"quiz":{"last_at":null,"retake_pending":false,"xp_since":-100000000000000001097906362944045541740492309677311846336810682903157585404911491537163328978494688899061249669721172515611590283743140088328307009198146046031271664502933027185697489699588559043338...
The two blobs first differ at byte 139, inside `xp_since`.
```

**Failure scenario.** Fold this single event against curriculum/ with the default config, now=2000-01-01T00:00:00Z, goal=40:
{"assisted":false,"passed":true,"quality_tier":"nearly_perfect","session":null,"task_id":null,"topic":"adding-subtracting-rational-expressions","ts":"2027-04-15T17:00:00Z","type":"review_result","v":1,"weighted_score":0.0,"xp":-1e+308}
`-1e308` is finite, so 1.0 does not raise: `int(round(-1e308))` yields the exact 309-digit integer and pydantic serializes it, so 1.0 produces a valid model with that value in `quiz.xp_since`, `xp.total`, and `xp.today`. 2.0 produces -9223372036854775808 for all three. The digests differ, so an R5 migration of any 1.0 log that carries an out-of-i64 XP total lands on a different learner model, silently, with no error from either side.

**Refuter.** The refutation fails. The claim is demonstrable end to end, and the reachability is worse than the reviewer states.

What the code does. `/home/deploy/dev/cadus2.0/crates/core/src/numeric.rs:92-94` is `round_half_even(x) as i64`. For a finite `x` with no fractional part, `round_half_even` returns `x` unchanged, and the Rust float-to-int cast saturates, so `-1e308` becomes `i64::MIN`. The three call sites that take a learner XP value are `/home/deploy/dev/cadus2.0/crates/core/src/projector.rs:739` (`xp_since`), `:763` (`total`), and `:764` (`today`). The 1.0 counterparts are `/home/deploy/dev/cadus/cadus/projector.py:623,632,633`, all `int(round(...))`, which has unbounded precision. The XP value is the raw event field on both sides: 1.0 appends `event.xp` at `projector.py:228,247,258` through `_record_xp`, and the 1.0 pydantic models bound it in no way (`/home/deploy/dev/cadus/cadus/mode

## FIXM3b

### #4 [major] The profile_reset D4 divergence is accepted on a 1.0 mitigation that does not exist

File: `crates/core/tests/projector.rs:434` — IDs: D4, D3

**Claim.** `COVERAGE_DIVERGING_SPLITS` pins a permanent incremental-vs-full-replay divergence after any `profile_reset` and justifies accepting it with the claim that 1.0's service layer routes such a stream down the full-replay path; `service.py:272` forces a full replay only for a `Regraded` event or a `projector_version` mismatch, so no such route exists in 1.0 and none is planned for 2.0.

**Evidence.**

```
The rationale at `crates/core/tests/projector.rs:432-434`: `/// 31 put the reset in the light half and a graded event after it. 1.0 answers this at /// the service layer, which sends such a stream down the full-replay path /// (service.py:257-280). The port reproduces the divergence rather than hides it.`

The cited code, `cadus/service.py:272-273`, is the whole guard:
```python
correcting = any(isinstance(event, Regraded) for event in new)
if correcting or (cached.topics and cached.projector_version != PROJECTOR_VERSION):
    model = project(prior + new, ctx.graph, ctx.cfg, now=ctx.now, tz=tz, goal=goal)
```
A `ProfileReset` matches neither arm. REQUIREMENTS.md:198-200 (D4) fixes the same two triggers for 2.0 — "Full replay (D-O6) is reserved for regrades and version bumps" — and the recorded M3 ruling covers only regrades, so the reset class is unguarded on both sides.

Minimal regrade-free repro (4 events: `enrolled foundations`; `lesson_result absolute-value passed`; `profile_reset [absolute-value]`; `lesson_result adding-integers passed`). Both implementations agree with each other and both leave the full replay at split 3:
```
        1.0                                  2.0
full  29a124406d034c61…                    29a124406d034c61…
split 3 bcbb0248e239cc1b…                  bcbb0248e239cc1b…
```
The lost value is the downward ability propagation onto the reset topic: full replay gives `absolute-value.ability = 0.15`, the resume gives `0.0`. Root cause is as the comment says — `finalize` drops a state equal to `TopicState::default()`, and `ability_update` (`fire.py:399`) skips a target `not in states`. `incremental_1_0.json` pins divergent splits only at 73-79 on the 20 committed streams, all around the regrade block, so no committed fixture carries this class. A 60-stream sweep of `project_incremental` at every split found 16 streams that leave the full replay; all 16 contain a `profile_reset`, 5 of them contain no `regraded` event at all.
```

**Failure scenario.** A learner does a profile reset on a topic, then completes any neighboring topic in the same session. `project_and_save` sees no `Regraded` and no version change, so it takes the incremental path and persists a model whose reset topic never received the propagated ability credit. The cached model is now permanently behind `rebuild`: the divergence is seeded into every subsequent incremental fold and is only cleared by a `PROJECTOR_VERSION` bump. The topic's `speed` is therefore too low, its review interval too short, and D4's "a grade folds one event forward" no longer equals the D-O6 ground truth — with no test, guard, or ruling covering the case, because the one recorded rationale points at a 1.0 mitigation that is not there.

**Refuter.** I cannot refute it. The load-bearing half of the claim is demonstrable at the cited line. `service.py:272-273` guards on `Regraded` or a `projector_version` mismatch and on nothing else, `service.py` never imports `ProfileReset`, and no 1.0 code path sends a reset stream to `rebuild_model` or to `project`. The rationale at `crates/core/tests/projector.rs:432-434` therefore justifies an accepted divergence with a 1.0 mitigation that is not in 1.0. The reset class is also real and separate from the documented regrade class: splits 21 to 29 of `stream_u3_coverage.jsonl` diverge with the correction and its target both in the fire half, so only the reset explains them.

I do refute the severity and the requirement framing. Parity is intact — 2.0 reproduces 1.0 exactly, and the test at `projector.rs:437` pins the divergent split set against the recorded 1.0 set, so R5 holds and the case is gua

### #6 [major] The quiz retake threshold has no boundary test; a `<=` port diverges from 1.0 with the suite green

File: `crates/core/src/projector.rs:335` — IDs: R5, D3

**Claim.** No M3 test pins the strictness of `event.score < cfg.quiz.retake_below`, so mutating it to `<=` leaves every test in `cargo test -p cadus-core` passing while the fold stops reproducing 1.0.

**Evidence.**

```
Mutation `self.quiz_retake_pending = event.score < ...` -> `<=` over the whole crate: `S4 quiz retake < -> <= (full): *** SURVIVED ***`. Every fixture score: `quiz scores in all fixtures: [0.75, 0.875]` / `retake_below = 0.8; exact hits: []`. 1.0 (`cadus/projector.py:262`) is `self.quiz_retake_pending = event.score < self.cfg.quiz.retake_below`. Two-event stream `session_start` + `quiz_result` with `"score":0.8`: 1.0 digest `9cc212bdf9691da689913bbb33b992778010f99a8d65fd3635ac29d001b72fbb`, pristine port the same, `<=` port `95532cb5c97435fcbb487c3f9d56e92ea9b76dbf99e40491a2d32de912e295b1`.
```

**Failure scenario.** A learner scores exactly 0.80 on a quiz. 1.0 sets `quiz.retake_pending = false`; a port that used `<=` sets it `true` and the selector serves a retake that 1.0 never serves. All 20 committed streams, the 150 selector tests, and the live-oracle tests stay green, because spec section 9 item 8 is satisfied by 0.75 and 0.875 alone and no fixture ever lands on 0.8.

**Refuter.** The claim is demonstrable. 1.0 uses strict `<` (cadus/projector.py:263) and the port at crates/core/src/projector.rs:335 matches it, so `<=` is a genuine parity break, not an equivalent mutant. `<` and `<=` differ only when `score == retake_below`, and no test in the crate ever reaches that point: the only quiz-score literals anywhere in crates/core/tests and crates/core/src are 0.75 (21x), 0.875 (19x), and one 1.0 in a parse-only test (tests/events.rs:250), while `retake_below` stays 0.8 (config.rs:148) and is never overridden by any test. No Rust struct literal sets a quiz score either. So the mutant is behaviorally identical on every committed input and survives the suite. The gap is systemic, not fixture luck: the generator emits only 0.875 and 0.75 (gen_stream_1_0.py:793, gen_one_per_type_1_0.py:124), so regenerated streams stay off the boundary, and the live-oracle tests (parity_ev

### #7 [major] The initial `diagnostic_placed` balance filter has no zero-balance test; coverage.md's boundary row covers only the refresh path

File: `crates/core/src/projector.rs:397` — IDs: R5, D3

**Claim.** No M3 test pins `**balance > 0.0` in the non-refresh placement filter, so mutating it to `>= 0.0` passes the whole crate suite while the fold places a topic 1.0 never places.

**Evidence.**

```
Mutation `**balance > 0.0` -> `**balance >= 0.0` over the whole crate: `S5 placed balance > -> >= (full): *** SURVIVED ***`. Fixture scan: `initial placements with a 0.0 balance: [] count 0` against `refresh placements with a 0.0 balance: ... count 19`, so `coverage.md:57` (`diag.promote_guard_zero | H2 guard at its boundary: a balance of exactly 0.0 | all`) records the `refresh_placement` guard at `projector.rs:449`, not this filter. 1.0 (`cadus/projector.py:340-344`) is `if tid in self.graph.topics and balance > 0.0`. Stream `session_start` + non-refresh `diagnostic_placed` with `"balances":{"absolute-value":0.0,"adding-integers":2.0}`: 1.0 digest `5d40022602a6135a3948140b70f5a9e2e7c75605c829180be0de349473de9eb6`, pristine port the same, `>=` port `600514d005f5cfbde84796b854e029b9ed683ac0980af939124ba0c4c78cf456`.
```

**Failure scenario.** A diagnostic reports a balance of exactly 0.0 for a topic. 1.0 leaves the topic `untouched`; a port with `>=` places it with `repNum = 0.0`, `memoryBase = 1.0`, `t0 = ts`, `status = placed`, which makes the topic review-eligible and shifts every later FIRe propagation through it. The reviewer reads `coverage.md` row `diag.promote_guard_zero` as `all` and concludes the boundary is measured.

**Refuter.** I could not refute the claim. Two parts must both hold, and both hold.

1. The guard is a real parity rule. 1.0 `cadus/projector.py:340-344` builds `placed` with `if tid in self.graph.topics and balance > 0.0`, and `docs/reference/projector-1.0-spec.md:160-166` pins the same strict form (`b > 0.0`). A balance of exactly 0.0 leaves the topic `untouched` in 1.0. The port at `crates/core/src/projector.rs:397` is correct; `seed_placed` (`projector.rs:434-455`) sets `status = Placed`, `rep_num = 0.0`, `memory_base = 1.0`, `t0 = ts`, so a `>=` port makes the topic review-eligible and moves every later FIRe result. The mutation is therefore NOT an equivalent mutant.

2. No input in the crate reaches the boundary. `>` and `>=` differ only at a balance of exactly 0.0 (or -0.0). I scanned every source of balances in the crate:
   - `crates/core/tests/fixtures/events/*.jsonl` holds 41 `diagnostic_p

### #8 [major] Trap T1 (Neumaier summation) is pinned at only one of its four call sites

File: `crates/core/src/xp.rs:279` — IDs: R5

**Claim.** Of the four sites where the port uses `neumaier_sum` to reproduce CPython 3.12+ `sum()`, only `projector.rs:799` is pinned by a test; replacing `xp.rs:279`, `fire.rs:329`, or `selector.rs:719` with a naive `iter().sum()` leaves the whole crate suite green, and the `xp.rs:279` replacement provably breaks parity.

**Evidence.**

```
Whole-crate mutation: `S6 xp_per_day neumaier->naive (full): *** SURVIVED ***`. M3-suite mutations: `M16 initial_ability neumaier->naive: *** SURVIVED ***`, `M19 selector:719 neumaier->naive: *** SURVIVED ***`, while `M18 projector:799 neumaier->naive: killed (1) :: the_whole_log_total_is_compensated_and_the_daily_total_is_naive`. 1.0 `cadus/xp.py:207` is `total = sum(xp for ts, xp in entries if local_day(ts, tz) >= start)`, and `./.venv/bin/python -c` on 3.13.5 gives `sum: 1.0` vs `naive: 0.0` for `[1e16, 1.0, -1e16]`. Stream `session_start` + `enrolled` + three `quiz_result` with `xp` 1e16, 1.0, -1e16: 1.0 digest `7a6837daa1548a82fe5372463d6a431b744691db90e1e9b10195e584787e597e` (`xp_per_day_28d: 0.0357`), pristine port the same, naive port `acd7eb0c7874554aabd9e794621bae916303cbaf4c2db98a2e7a74788c68a204` (`xp_per_day_28d: 0.0`).
```

**Failure scenario.** An XP window whose values cancel (a large award and a large `regraded` reversal in the same 28 days) makes `velocity.xp_per_day_28d` differ between 1.0 and a naive port, and the same class of error at `fire.rs:329` moves every untouched topic's ability seed. `the_whole_log_total_is_compensated_and_the_daily_total_is_naive` uses 25 records of 0.1 on one day, whose compensated and naive totals both round to the same 4-decimal velocity, so it discriminates only the `projector.rs:799` site.

**Refuter.** The refutation failed. I reproduced every part of the claim by direct experiment, so the finding is demonstrable and confirmed.

First, the code at all four sites is correct. Each Rust `neumaier_sum` call matches a 1.0 `sum()` call: `/home/deploy/dev/cadus/cadus/xp.py:207`, `/home/deploy/dev/cadus/cadus/fire.py:361`, `/home/deploy/dev/cadus/cadus/selector.py:602`, and the whole-log total in the projector. The defect is in the M3 test suite, not in the port.

Second, the mutation results are exactly as the reviewer reported. I copied the workspace to a scratchpad, ran `cargo test -p cadus-core` once per mutation, and restored the site each time. The baseline is green. Replacement of `neumaier_sum` with `iter().sum::<f64>()` at `crates/core/src/xp.rs:279`, at `crates/core/src/fire.rs:329`, and at `crates/core/src/selector.rs:719` leaves the whole crate suite green. The same replacement at 

### #9 [major] `schedule_drills` brackets neither the 0.95 automaticity bar nor the 3.5-day cadence window

File: `crates/core/src/selector.rs:74` — IDs: R5

**Claim.** `schedule_drills_respects_mastery_and_cadence` asserts three 1.0 literals that leave both drill constants free, so `DRILL_MASTERY_ABILITY` 0.95 -> 0.85 and `DRILL_INTERVAL_DAYS` 3.5 -> 7.0 both pass the whole crate suite.

**Evidence.**

```
Whole-crate mutations: `S2 DRILL_MASTERY_ABILITY 0.95->0.85 (full): *** SURVIVED ***` and `S3 DRILL_INTERVAL_DAYS 3.5->7.0 (full): *** SURVIVED ***`. The only test (`crates/core/tests/selector.rs:1247-1266`) uses `ability(0.8)`, `ability(0.96)`, and `recent.insert("d".to_owned(), T_US - days(1))`; 0.8 and 0.96 sit either side of both 0.95 and 0.85, and 1 day is inside both 3.5 and 7.0. `grep -rn "DRILL_MASTERY_ABILITY|DRILL_INTERVAL_DAYS" crates/core/tests/` returns nothing. 1.0 (`cadus/selector.py:111,114,866-869`) holds `DRILL_MASTERY_ABILITY = 0.95`, `DRILL_INTERVAL_DAYS = 3.5`, and a probe of the live 1.0 gives `ability=0.9 -> ['single-digit-addition']` and `last drill 5d ago (ability 0.8) -> ['single-digit-addition']`.
```

**Failure scenario.** A drill-tagged topic at ability 0.90, or one drilled 5 days ago, is scheduled by 1.0 and dropped by a port whose bar is 0.85 or whose window is 7.0 days. The learner loses the automaticity drill, and no M3 test reports it.

**Refuter.** The claim is correct and I cannot refute it. `schedule_drills_respects_mastery_and_cadence` (crates/core/tests/selector.rs:1239-1266) tests three points that all sit on the same side of both the old and the mutated constant: ability 0.8 is below 0.95 and below 0.85; ability 0.96 is at or above 0.95 and above 0.85; a 1-day gap is inside 3.5 days and inside 7.0 days. All three assertions hold for either constant value, so the test pins neither constant. No other test names the constants, and the event-stream parity test cannot substitute: it passes no `last_drill_at` (so DRILL_INTERVAL_DAYS is unreachable), and the 10 committed 1.0 plans hold only 70 lesson rows and 10 review rows with zero drill rows, each plan already full at the 8-task cap that truncates the drills appended last. One nuance limits the severity: the port values match 1.0 exactly (selector.py:111 and :114), so R5 parity h

### #10 [major] `QUIZ_RECENT_DAYS` is free: the quiz strata test never brackets the 14-day recency window

File: `crates/core/src/selector.rs:52` — IDs: R5

**Claim.** The only test of `quiz_composer` ages its topics at 5, 30, and 60 days, so any `QUIZ_RECENT_DAYS` between 6 and 29 passes; mutating 14 to 21 leaves the whole crate suite green.

**Evidence.**

```
Whole-crate mutation: `S1 QUIZ_RECENT_DAYS 14->21 (full crate): *** SURVIVED ***`. `crates/core/tests/selector.rs:668-676` inserts `T_US - days(5)` for the recent ids, `T_US - days(30)` for mid, and `T_US - days(60)` for old; `grep -rn "QUIZ_RECENT_DAYS" crates/core/tests/` returns nothing. 1.0 `cadus/selector.py:84` is `QUIZ_RECENT_DAYS = 14` and `cadus/selector.py:709-711` is `return (t - learned_at[tid]) <= timedelta(days=QUIZ_RECENT_DAYS)`. `selector_1_0.json` composes no quiz task at all in any of its 10 seeded states, so the 1.0 selector oracle does not reach the function either.
```

**Failure scenario.** A topic mastered 18 days ago falls in 1.0's `older` set, which splits into the `mid` and `old` strata; a port with a 21-day window puts it in `recent`. The two implementations then draw different quiz questions from different pools, and both the 150 selector tests and the 10-state 1.0 `compose_session` oracle stay green.

**Refuter.** The claim is demonstrable, so I cannot refute it. QUIZ_RECENT_DAYS has one read site, inside `is_recent` in `quiz_composer`. That predicate returns false for every topic when `learned_at` is None, and `learned_at` is Some in exactly one test, which ages topics at 5, 30, and 60 days and asserts only stratum sizes and membership. Any window from 5 to 29 days keeps the same 4/2/2 partition, so 21 passes. The gap is in fact wider than the reviewer states: `SessionContext::with_learned_at` is never called anywhere in the repo, so the parity test at parity_events.rs:975 and every other compose_session test run with recency disabled, and the 1.0 selector oracle composes no quiz task in any of its 10 states. Nothing in the suite pins the 14-day window (R5). One qualification that limits severity: the port's value is 14 and its comparison is `<=` on microseconds, which matches cadus/selector.py:8

### #13 [major] The 1.0 selector oracle composes with n=8, so it never compares the quiz, multi-step, or drill sections of compose_session

File: `scripts/oracle/dump_selector_1_0.py:42` — IDs: R5

**Claim.** `selector_1_0.json` is the only 1.0-versus-2.0 `compose_session` comparison, and it composes with `n=8`; all 10 seeded states fill that cap with remediation, review, and lesson tasks, so 42 tasks that 1.0 actually emits (24 drill, 12 lesson, 5 multi-step, 1 quiz) fall outside the compared window and no assertion in the repository pins any drill-task field against 1.0.

**Evidence.**

```
`dump_selector_1_0.py:42 DEFAULT_N = 8`; `parity_events.rs:66 const SELECTOR_N: usize = 8;`; `parity_events.rs:970 assert!(state.tasks.len() <= SELECTOR_N)`. Every one of the 10 committed states holds exactly 8 tasks, all `lesson`/`review`. Re-running the same oracle with `--n 100`: seed 2 -> `... 8 multi-step None, 9 drill multiplication-tables, 10 drill single-digit-addition`; seed 5 -> `... 9 multi-step, 10 quiz, 11-13 drill`. Aggregated over the 10 seeded states: `states with tasks beyond the n=8 cap: 10 of 10; tasks beyond the cap: 42 {'lesson': 12, 'drill': 24, 'multi-step': 5, 'quiz': 1}`. Mutation of `selector.rs:1530` `time_budget_secs: Some(target.saturating_mul(cfg.drill.questions))` to `Some(target)`: `CONFIRM_M3_drill_budget: *** SURVIVED ***`.
```

**Failure scenario.** A port slip in `drill_task` that writes the per-question target instead of the whole-task budget. On `stream_2.jsonl` at its own `t_ref`, 1.0 serves `s2-drill-multiplication-tables` with `n_problems 20, time_budget_secs 120`; the port as written matches. With the slip the served drill carries `n_problems 20, time_budget_secs 6` — a 20-question timed drill with a six-second budget — and the whole suite still reports 594 passed / 0 failed, because the oracle's n=8 window ends before the first drill and no unit test pins the field.

**Refuter.** The claim holds. Three independent checks confirm it, and no ruling in docs/plans/M3.md or docs/reviews/M*-review-*.md protects the gap (neither mentions the n=8 cap or the drill task).

1. The window. /home/deploy/dev/cadus2.0/scripts/oracle/dump_selector_1_0.py:42 sets DEFAULT_N = 8, and the committed fixture /home/deploy/dev/cadus2.0/crates/core/tests/fixtures/events/selector_1_0.json carries "n": 8 with exactly 8 tasks in each of the 10 states, all lesson or review (70 lesson, 10 review, 0 drill). /home/deploy/dev/cadus2.0/crates/core/tests/parity_events.rs:66 pins the same SELECTOR_N = 8. I re-ran the same oracle against the 1.0 tree with --n 100 and got the reviewer's numbers to the task: 10 of 10 states hold work beyond the cap, 42 tasks in total, Counter({'drill': 24, 'lesson': 12, 'multi-step': 5, 'quiz': 1}); seed 2's tail is 8 multi-step, 9 drill multiplication-tables, 10 dril

### #14 [major] The 28-day velocity window is never exercised at its first day, so an off-by-one window slip ships green

File: `crates/core/src/xp.rs:275` — IDs: R5, D3

**Claim.** No fixture and no unit test places an XP entry on the exact window-start day, so the `>= start` membership test of `xp_per_day` is never read at equality; a `> start` slip drops the boundary day from the trailing window and moves `velocity.xp_per_day_28d` and `velocity.eta`, both of which sit inside the compared parity blob.

**Evidence.**

```
xp.rs:275 `if local_day_in(ts_us, zone)? >= start {`. The only window test, `crates/core/tests/xp.rs:257 xp_per_day_over_window`, uses entries at 2026-07-14 (the reference day), 2026-07-01, and 2026-05-01 against a start of 2026-06-17 — one entry well inside, one well outside, none on the edge. Mutation to `>`: `CONFIRM_X4_window_start: *** SURVIVED ***` across all 20 committed streams and all 594 tests.
```

**Failure scenario.** Stream: `enrolled`(foundations) -> `review_result` xp 28.0 at 2026-04-07 -> `review_result` xp 56.0 at 2026-05-04. The reference day is 2026-05-04, so `window_start` is exactly 2026-04-07. 1.0 `project` gives `velocity {'xp_per_day_28d': 3.0, 'eta': '2033-07-18'}`; the 2.0 port as written gives the same. With the `>` slip it gives `{'xp_per_day_28d': 2.0, 'eta': '2037-02-23'}` — a 3.5-year ETA error inside the parity blob — with the suite green.

**Refuter.** The claim is demonstrable, so I cannot refute it. Two parts are true.

Part 1 — the boundary is never read at equality. `xp_per_day` (crates/core/src/xp.rs:275) and `topics_per_week` (xp.rs:302) both test `local_day_in(...) >= start`. The three unit tests that reach these functions put no day on `start`: `xp_per_day_over_window` (crates/core/tests/xp.rs:257) and `topics_per_week_over_window` (xp.rs:269) use a reference day of 2026-07-14 with `window_days` 28, so `start` is 2026-06-17, and the entry days are 2026-07-14, 2026-07-01, 2026-07-10, 2026-05-01, and 2026-04-01; `compute_velocity_state_integration` (xp.rs:363) and its no-course twin (xp.rs:390) use entry day 2026-07-14 and completion day 2026-07-12 against the same 2026-06-17 start. No test calls `window_start` directly. There is no property test and no `mod tests` block in xp.rs. I also ran the equivalent `>` mutation on the 1.0

### #15 [major] The streak rule's goal comparison is never read at equality on the reference day

File: `crates/core/src/xp.rs:219` — IDs: R5

**Claim.** In all 20 committed streams the day whose total is exactly the goal is always a PAST day, never `t_ref`'s local day, so `current_streak`'s first comparison — the one that decides whether today counts toward the streak — is only ever read strictly below or strictly above the goal; a `<=` slip loses one streak day and no test notices.

**Evidence.**

```
xp.rs:219 `if total_of(&today) < goal {`. Measured per stream in both recorded zones: stream_2..20 report `days_at_goal ['2027-04-14', '2027-04-15']` while `today` is 2027-04-17 with `today_total 20.25` (UTC) or 2027-04-16 with `today_total 40.5` (America/New_York); stream_1 has no at-goal day at all. `coverage.md:63 | 9 | streak.day_at_goal | a local day exactly at the goal | no | all |` records the probe as hit, but the probe scans the whole ledger, not the reference day. `crates/core/tests/xp.rs:212 streak_counts_consecutive_goal_days` uses today = 30.0 then 45.0, never 40.0. Mutation to `<=`: `CONFIRM_X1_streak_at_goal: *** SURVIVED ***`.
```

**Failure scenario.** Stream: `review_result` xp 40.0 at 2026-05-03T10:00Z, `review_result` xp 40.0 at 2026-05-04T10:00Z, goal 40, UTC. 1.0 `project` gives `xp {'today': 40, 'streak_days': 2}`; the 2.0 port as written gives the same. With the `<=` slip it gives `streak_days: 1` — the learner who lands exactly on the goal is told the day did not count — and the suite reports 594 passed / 0 failed.

**Refuter.** The claim holds. I could not refute it. Three parts are each demonstrated.

1. The shipped comparison is correct. `crates/core/src/xp.rs:219` `if total_of(&today) < goal {` is a literal port of 1.0 `cadus/xp.py:178` `if daily.get(today, 0.0) < goal:`. The finding is a test-coverage gap under R5, not a behavior defect.

2. No committed fixture puts the reference day exactly at the goal. I wrapped the 1.0 `cadus.projector.current_streak` with a probe and folded every fixture stream in both recorded zones. The reference-day total is 20.25 (UTC) or 40.5 (America/New_York) for stream_2 through stream_20, 0.0 or 22.25 for stream_1, and 45.0 or 0.0 for stream_u3_coverage. The goal is 40. The exact-goal days are 2027-04-14 and 2027-04-15, which are past days that only the `while` loop reads. So the first comparison never sees equality, and the `streak.day_at_goal` row of coverage.md does not cov
