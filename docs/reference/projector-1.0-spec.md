Source: survey of /home/deploy/dev/cadus on 2026-08-27 (read-only). Line citations point at 1.0 files.

# Cadus 2.0 — M3 port specification (surveyed from 1.0)

Read-only survey of `/home/deploy/dev/cadus`. Nothing was edited. No 1.0 test suite ran.

---

## 1. Where things live

| Concern | File | Key lines |
|---|---|---|
| Event union, learner model, config models | `/home/deploy/dev/cadus/cadus/model.py` | events `212–529`; `LearnerModel` `566–620`; `Config` `691–791` |
| Event validation + shims + seeded RNG | `/home/deploy/dev/cadus/cadus/events.py` | `SCHEMA_VERSION=1` `24`; `SHIMS={}` `39`; `validate_event` `56–63`; `seeded_rng` `71–73` |
| The fold | `/home/deploy/dev/cadus/cadus/projector.py` | `PROJECTOR_VERSION=3` `89`; `Projector` `131–682`; `apply_regrades` `698–770`; `project` `773–791`; `project_incremental` `794–832` |
| FIRe engine | `/home/deploy/dev/cadus/cadus/fire.py` | constants `40–89`; `memory_at` `169–179`; `review_state` `214–248`; `interval_for` `256–275`; `speed_for` `278–283`; `decay_for` `286–296`; `raw_delta` `304–335`; `ability_update` `369–401`; `_apply_update` `409–431`; `apply_attempt` `439–525`; `grade_review` `539–554` |
| XP / streak / velocity | `/home/deploy/dev/cadus/cadus/xp.py` | constants `39–60`; `base_xp` `68–86`; `quality_multiplier` `101–114`; `is_rushing` `117–123`; `task_xp` `126–143`; `current_streak` `172–184`; `compute_velocity_state` `277–314` |
| Selector | `/home/deploy/dev/cadus/cadus/selector.py` | constants `72–145`; `due_reviews` `431–454`; `compress` `502–590`; `order_lessons` `617–629`; `quiz_composer` `682–754`; `multistep_components` `1055–1068`; `compose_session` `1235–1481`; `_assign_ids` `1484–1496`; `_task_still_valid` `1527–1605` |
| Record cycle | `/home/deploy/dev/cadus/cadus/service.py` | `project_and_save` `257–280`; `rebuild_model` `283–290`; `record_attempt` `341–414`; `_advance_lesson` `429–459`; `complete_task` `548–640`; `review_result` `739–757`; `lesson_close` `760–774`; `multistep_close` `902–966`; `start_session` `1003–1024`; `end_session` `1027–1057` |
| Graph (edge weights `W`) | `/home/deploy/dev/cadus/cadus/graph.py` | `_relax` `178–198`; `_add_enc` `301–321`; `neighborhood` `408–431`; `reach_weights` `435–443`; `upward_weights` `445–453`; `mastery_floor` `392–404`; unit-file sort `593` |
| Diagnostic constants | `/home/deploy/dev/cadus/cadus/diagnostic.py` | `PLACEMENT_REPNUM_CAP=4.0` `71`; `PLACEMENT_MEMORY_BASE=1.0` `76`; `refreshed_repnum` `318–326`; `answer_weight` `192–205` |
| Config values | `/home/deploy/dev/cadus/config.yaml` | whole file, `1–94` |
| Docs | `/home/deploy/dev/cadus/docs/DATA_MODEL.md` §2 `102`, §2.1 `196`, §3 `244`, §8 `347`; `/home/deploy/dev/cadus/docs/PEDAGOGY.md` |

**`attempt_id` is NOT content-derived in 1.0.** It is a caller-supplied idempotency key (`model.py:167`, docstring `service.py:344–353`). The web tier builds it as `f"web-{served.problem_id}"` (`cadus_web/api.py:1480`, `:1722`), where `problem_id = uuid.uuid4().hex` (`cadus_web/api.py:473`, `:1906`); the H3 re-solve path uses `f"web-{...}-rework"` (`:1372`); the fallback is a bare `uuid.uuid4().hex` (`:621`). **2.0 must define its own deterministic rule** — see §7 trap T12.

---

## 2. The event types and their exact JSON shapes

Full JSON Schema for all 16 types plus `LearnerModel`: `docs/reference/event-schemas-1.0.json`.

**Envelope** (`_EventBase`, `model.py:212–218`), on every event: `ts: datetime` (required), `session: str|null` (default `null`), `v: int` (default `1`). Every model is `extra="forbid"` (`model.py:29`) — an unknown key is a hard validation error. `type` is the union discriminator (`model.py:529`).

`ts` serializes as `"2026-03-02T09:00:00Z"` — RFC 3339, `Z` suffix, seconds precision when microseconds are zero. A naive `ts` is read as UTC (`projector.py:690–691`, `xp.py:158–159`).

| type | fields (type, default; **bold** = required) |
|---|---|
| `session_start` | envelope only |
| `session_end` | `xp_earned: float = 0.0`, `minutes: float = 0.0` |
| `enrolled` | **`course: Slug`**, `reason: "gap-fill"\|"gap-return"\|null = null`, `return_to: Slug\|null = null` |
| `task_served` | **`task_id: str`**, **`task_type: TaskType`**, `topic: Slug\|null = null`, `kp: Slug\|null = null`, `problems: [ServedProblem] = []`, `component_topics: [Slug] = []`, `seed: int\|null = null` |
| `attempt` | **`attempt_id: str`**, **`task_id: str`**, **`topic: Slug`**, `kp: Slug\|null = null`, **`task_type: TaskType`**, **`problem: {text: str, expected: str}`**, **`given_answer: str`**, `work: str\|null = null`, `answer_kind: AnswerKind\|null = null`, **`correct: bool`**, **`secs: int (≥0)`**, `error_tags: [str] = []`, **`work_quality: WorkQuality`**, `grader_note: str\|null = null`, `assisted: bool = false` |
| `lesson_result` | **`topic: Slug`**, **`passed: bool`**, `failed_at_kp: Slug\|null = null`, `xp: float = 0.0`, **`quality_tier: WorkQuality`**, `assisted: bool = false` |
| `review_result` | **`topic: Slug`**, **`passed: bool`**, **`weighted_score: float`**, `xp: float = 0.0`, **`quality_tier: WorkQuality`**, `assisted: bool = false`, `task_id: str\|null = null` |
| `quiz_result` | **`quiz_id: str`**, **`score: float`**, `per_topic: [{topic: Slug, correct: bool, secs: int≥0}] = []`, `xp: float = 0.0` |
| `remediation_triggered` | **`kind: str`**, **`source_topic: Slug`**, `targets: [Slug] = []` |
| `diagnostic_answer` | **`topic: Slug`**, **`correct: bool`**, **`secs: int≥0`**, **`weight: float ∈[0,1]`** |
| `diagnostic_placed` | `balances: {str: float} = {}`, `conditional: [Slug] = []`, `refresh: bool = false` |
| `profile_reset` | `topics: [Slug] = []` |
| `regraded` | **`task_id: str`**, **`topic: Slug`**, `attempts: [{attempt_id: str, work_quality: WorkQuality, error_tags: [str] = [], grader_note: str\|null = null}] = []`, `quality_tier: WorkQuality\|null = null`, `xp: float\|null = null`, **`reason: str`** |
| `anki_card_created` | **`topic`**, **`deck: str`**, **`note_id: int`**, **`front_hash: str`** |
| `config_changed` / `curriculum_changed` | **`summary: str`**, `git_ref: str\|null = null` |

Enums (`model.py:37–89`): `TaskType = lesson|review|quiz|drill|diagnostic|multi-step`; `WorkQuality = perfect|nearly_perfect|passable|nearly_passable|poor|blowoff`; `TopicStatus = untouched|frontier|learning|placed|floor`; `KPProgress = passed|failed_once|failed_twice`; `AnswerKind = numeric|expression|multi-step|proof`.

`ServedProblem.text_hash` is **inert** — nothing computes or reads it (`model.py:228–241`). `RegradedAttempt` deliberately has **no `correct`** field (`model.py:449–452`).

**Versioning** (`events.py:42–63`): read `v` (default `SCHEMA_VERSION`); while `v < SCHEMA_VERSION`, apply `SHIMS[v]`; a missing shim raises. `SHIMS` is empty and `SCHEMA_VERSION == 1`, so `v != 1` fails today. `v: 0` raises (pinned, `tests/test_events.py:30–32`).

---

## 3. The learner model shape

`LearnerModel` (`model.py:608–620`), `TopicState` (`model.py:566–579`):

| field | type | initial |
|---|---|---|
| `status` | `TopicStatus` | `untouched` |
| `repNum` | `float` | `0.0` |
| `memoryBase` | `float` | `0.0` |
| `t0` | `datetime\|null` | `null` |
| `interval_days` | `float` | `0.0` |
| `ability` | `float` | `0.0` |
| `speed` | `float` | `1.0` |
| `conditional` | `bool` | `false` |
| `explicit_only` | `bool` | `false` |
| `last_problems` | `[str]` | `[]` |
| `kp_progress` | `{str: KPProgress}` | `{}` |

The camelCase spellings `repNum` / `memoryBase` are load-bearing on the wire (`model.py:9–10`, pinned `tests/test_model.py:242–246`).

**`explicit_only` is dead.** `grep` finds exactly one hit — its declaration at `model.py:577`. Nothing writes or reads it. It is always `false`. Keep the field for shape parity; never write it.

Top level: `built_from_ts: datetime|null`; `topics: {str: TopicState}`; `xp: {total: int = 0, today: int = 0, goal: int = 40, streak_days: int = 0}`; `quiz: {last_at: date|null, xp_since: int = 0, retake_pending: bool = false}`; `velocity: {xp_per_day_28d: float = 0.0, topics_per_week_28d: float = 0.0, course_progress: float = 0.0, eta: date|null}`; `pending_remediation: [{kind: str, targets: [Slug]}]`; `config_hash: str|null`; `projector_version: int|null`.

**The event cursor.** 1.0 has **no `seq` cursor.** `built_from_ts` is wall-clock `now` (`projector.py:674`), not a cursor. `config_hash` + `projector_version` are the staleness detectors (`projector.py:680–681`). D-S3/D4 requires a `seq` cursor — that is **new in 2.0**, and it must not enter the parity-compared bytes.

**Assembly** (`finalize`, `projector.py:653–682`): `t_ref = self.last_ts or now`; topics are emitted `sorted()` by id and **filtered to those unequal to a default `TopicState()`** (`projector.py:668–672`). Velocity, "today", and streak use `t_ref` (the last event's ts), never wall clock. Only `built_from_ts` carries `now`.

---

## 4. The fold — exact state transitions

`Projector.apply` (`projector.py:161–194`) first sets `last_ts = max(last_ts, as_utc(ts))`, then dispatches.

**No-op event types** (`projector.py:190–194`): `task_served`, `session_start`, `session_end`, `anki_card_created`, `config_changed`, `curriculum_changed`. `regraded` never reaches `apply` — `apply_regrades` consumes it first.

### 4.1 The two rules the brief names

**"The projector never reads `work_quality` of an attempt."** `_on_attempt` (`projector.py:213–224`) touches only `last_problems` and the conditional peel-back. It reads `event.problem.text` and `event.correct`. It does **not** read `work_quality`, `error_tags`, `secs`, `assisted`, `work`, `answer_kind`, or `kp`. FIRe fires only from `lesson_result` / `review_result` / `quiz_result`. Pinned by the field list at `tests/test_events.py:125–128`.

**A task result prices XP from the last attempt.** In the service layer, not the projector: `review_result` takes `attempts[-1].work_quality` (`service.py:744`); `lesson_close` takes `attempts[-1].work_quality` (`service.py:763`); `_advance_lesson` takes the closing `attempt.work_quality` (`service.py:451`). `assisted` is `any(...)` over the task's attempts (`service.py:454`, `:746`, `:767`).

### 4.2 `attempt`

```
last_problems ← (last_problems ++ [sha1(problem.text)[:12]])[-20:]
if not correct: peel_back_conditional(topic, ts)
```
`LAST_PROBLEMS_WINDOW = 20` (`projector.py:92`); `problem_text_hash` = `sha1(utf8(text)).hexdigest()[:12]`, **no whitespace normalization** (`projector.py:108–118`).

Peel-back (`projector.py:452–482`): candidates `{topic} ∪ graph.dependents[topic]`. For each candidate whose state exists and `conditional` is true: `repNum ← repNum/2`; `interval_days ← interval_for(repNum)`; `conditional ← false`. `status` is unchanged. `t` is unused.

### 4.3 `lesson_result`

```
record_xp(ts, xp); last_practice[topic] = ts
if passed and topic ∉ learned_at: learned_at[topic]=ts; completions += (ts,topic)
if passed:  apply_fire_result(topic, passed=True, quality=quality_tier, assisted=assisted)
            set_status(topic, learning); mark_kps_passed(topic)
else:       mark_lesson_fail(topic, failed_at_kp, ts)     # NO FIRe, NO propagation
```
`mark_lesson_fail` (`projector.py:544–568`): stamps `t0 ← t`; walks the topic's KPs in author order and `setdefault(kp.id, passed)` for every KP **before** `failed_at_kp`; then `failed_at_kp ← failed_twice` if it was already `failed_once`/`failed_twice`, else `failed_once`. It does **not** change `status`.

### 4.4 `review_result`

```
record_xp(ts, xp); last_practice[topic] = ts
apply_fire_result(topic, passed=passed, quality=quality_tier, assisted=assisted)
```
No status change. A `review_result` on an `untouched` topic leaves it `untouched` while writing `repNum`/`memoryBase`/`t0` — observable in the fixture (`absolute-value-inequalities`).

### 4.5 `quiz_result`

```
record_xp(ts, xp); quiz_last_at = ts.date(); quiz_last_ts = ts
quiz_retake_pending = (score < cfg.quiz.retake_below)
for row in per_topic: if row.correct and row.topic in graph.topics: last_practice[row.topic]=ts
for row in per_topic (ORDER AS GIVEN):
    if row.topic not in graph.topics: continue
    apply_fire_result(row.topic, passed=row.correct,
                      quality = nearly_perfect if row.correct else poor)
```
`assisted` is never set on a quiz row. Quiz-miss remediation is **not** derived here — the service emits `remediation_triggered` events (`service.py:793–818`, `projector.py:264–270`).

### 4.6 `remediation_triggered`, `diagnostic_answer`, `profile_reset`

- `remediation_triggered` → append `(ts, kind, [t for t in targets if t in graph.topics])` (`projector.py:285–287`). Ungated by `apply_fire`.
- `diagnostic_answer` → `_diag_answers[topic] += [(correct, weight)]` (`projector.py:291–299`). Ungated.
- `profile_reset` → `topics[tid] = TopicState()` for each in-graph tid (`projector.py:424–435`). Gated.

### 4.7 `enrolled`

`enrolled_course ← course`; for each topic in `graph.mastery_floor(course)` that is in `graph.topics` **and** currently `untouched`: `status ← floor` (`projector.py:198–211`). Gated on `apply_fire`. **Iteration is over a set** — see trap T5.

### 4.8 `diagnostic_placed`

Consume and reset `_diag_answers` **unconditionally**, before the `apply_fire` gate (`projector.py:331–334`). Then, if `refresh` → `_refresh_placement`; else initial placement (`projector.py:338–371`):

```
placed = [(tid,b) for tid,b in balances.items() if tid in graph.topics and b > 0.0]
answered = {tid for tid,_ in placed if diag_answers.get(tid)}
Pass 1 (directly answered): seed(tid, b, ability_from_answers(diag_answers[tid]))
Pass 2 (inferred):          seed(tid, b, clamp01(initial_ability(tid,...) * 0.9))
seed(tid, b, ability): status=placed; repNum=min(b, 4.0); memoryBase=1.0; t0=ts;
                       interval_days=interval_for(repNum); ability=ability;
                       speed=speed_for(ability, difficulty(tid)); conditional=(tid in conditional)
```
`ability_from_answers` (`projector.py:437–450`): `a ← 0.5`; for each `(correct, weight)` **in list order**: `a += 0.3 * weight * ((1.0 if correct else 0.0) - a)`; return `clamp01(a)`.

`_refresh_placement` (`projector.py:373–422`) iterates `balances.items()` in dict order; skips `tid ∉ graph.topics`; **skips when `balance ≤ 0.0` and `old.status is untouched`** (the H2 promote-guard); `new_rep = (old.repNum + (min(b,4.0) if b>0 else 0.0)) / 2.0`; ability `= clamp01((old.ability + fresh)/2.0)` when this session answered it, else `old.ability`; then the same `placed` / `memoryBase=1.0` / `t0=ts` re-seed.

### 4.9 `_apply_fire_result` — the FIRe core

`projector.py:486–542`:

```
1. states = copy(topics); current = states.get(topic, TopicState())
2. FIRST-TOUCH SEED — iff current.status is untouched AND current.ability == 0.0:
       seed = initial_ability(topic, graph, states, cfg)
       current.ability = seed; current.speed = speed_for(seed, difficulty(topic))
3. deltas = ability_update(states, topic, correct=passed, graph, cfg)
   for other, delta in deltas.items():          # dict order; keys inserted topic-first then sorted
       a' = clamp01(states[other].ability + delta)
       states[other] = {ability: a', speed: speed_for(a', difficulty(other))}
4. topics, _ = apply_attempt(states, AttemptResult(topic, passed, quality, assisted), graph, cfg, t)
```

`difficulty(tid)` = the curriculum topic's `difficulty`, or `0.5` when absent (`projector.py:585–587`).

**Formulas** (`fire.py`):

```
QUALITY_Q = {perfect:1.0, nearly_perfect:0.85, passable:0.7,
             nearly_passable:0.4, poor:0.15, blowoff:0.0}          # fire.py:40-47
PASS_QUALITY_THRESHOLD = 0.7   NEARLY_DUE_THRESHOLD = 0.6
INTERVAL_CAP_DAYS = 730.0      ASSISTED_CREDIT = 0.5
TEST_PREP_DUE_THRESHOLD = 0.7  _SECONDS_PER_DAY = 86400.0

days_since(t0,t)   = (t - t0).total_seconds() / 86400.0            # fire.py:145-146
memory_at(s,t)     = s.memoryBase                     if s.t0 is None or s.interval_days <= 0.0
                   = s.memoryBase * 0.5**(days_since(s.t0,t)/s.interval_days)   # fire.py:169-179
interval_for(r)    = table[last]                      if floor(max(0,r)) >= last
                   = table[i] + frac*(table[i+1]-table[i]),  i=floor(max(0,r)), frac=max(0,r)-i
                   then min(value, 730.0)                                        # fire.py:256-275
speed_for(a,d)     = clamp((0.5+a)/(0.5+d), 0.33, 3.0)                           # fire.py:278-283
decay_for(s,t)     = 1.0                              if s.t0 is None or s.interval_days <= 0.0
                   = min(3.0, 1.0 + max(0.0, days_since(s.t0,t)/s.interval_days - 1.0))
raw_delta(q,m,pass,assisted):
   if pass: span = 1.0 - 0.5
            early = 1.0  if span <= 0.0  else clamp((1.0-m)/span, 0.15, 1.0)
            credit = q*early ;  return credit*0.5 if assisted else credit
   else:    return -(1.0 - q)                                                    # fire.py:304-335
_apply_update(s, raw, t, failed):                                                # fire.py:409-431
   factor     = decay_for(s,t) if failed else 1.0
   repNum'    = max(0.0, s.repNum + s.speed * factor * raw)
   memoryBase'= max(0.0, memory_at(s,t) + raw)
   t0' = t ;  interval_days' = interval_for(repNum')
```

`ability_update` (`fire.py:369–401`): `alpha = 0.3`; `target = 1.0 if correct else 0.0`; `deltas = {topic: alpha*(target - states[topic].ability if present else 0.0)}`; then `weights = reach_weights(topic) if correct else upward_weights(topic)`, and **`for other, w in sorted(weights.items())`** — skip `other == topic`, `w <= 0.0`, or `other ∉ states`; `deltas[other] = alpha*w*(target - states[other].ability)`.

`apply_attempt` (`fire.py:439–525`):

```
explicit = states.get(topic, TopicState())
raw = raw_delta(q, memory_at(explicit,t), passed, assisted)
states[topic] = _apply_update(explicit, raw, t, failed=not passed)

if passed and raw > 0.0:                      # DOWNWARD credit
    for target, w in sorted(reach_weights(topic).items()):
        skip target==topic or w<=0.0
        recipient = states.get(target, TopicState())
        skip if recipient.speed < 1.0                        # forced-explicit
        credit = raw_delta(q, memory_at(recipient,t), True, assisted) * w
        skip if abs(credit) < 0.05                           # min_credit
        states[target] = _apply_update(recipient, credit, t, failed=False)

elif not passed and raw < 0.0:                # UPWARD penalty
    for target, w in sorted(upward_weights(topic).items()):
        skip target==topic or w<=0.0
        recipient = states.get(target, TopicState())
        skip if recipient.t0 is None                         # never-learned absorb nothing
        penalty = raw * w
        skip if abs(penalty) < 0.05
        states[target] = _apply_update(recipient, penalty, t, failed=True)
```

Note the asymmetry: downward credit is computed from **the recipient's own memory** (`fire.py:498`); upward penalty is `raw * w` (`fire.py:519`). The forced-explicit gate applies to credit only; `t0 is None` gates penalties only.

### 4.10 XP, streak, velocity, quiz

```
xp_state:      total       = int(round(sum(xp for _,xp in xp_events)))     # projector.py:628-632
               today       = int(round(daily.get(local_day(t_ref,tz), 0.0)))
               goal        = profile.daily_xp_goal
               streak_days = current_streak(daily, goal, local_day(t_ref,tz))
daily_totals:  out[local_day(ts,tz)] = out.get(day,0.0) + xp   # NAIVE loop  # xp.py:163-169
current_streak: day = today; if daily.get(today,0.0) < goal: day -= 1 day
                while daily.get(day,0.0) >= goal: streak += 1; day -= 1 day  # xp.py:172-184
quiz_state:    xp_since = 0.0; for ts,xp in xp_events:
                   if quiz_last_ts is None or ts > quiz_last_ts: xp_since += xp   # NAIVE loop
               return {last_at, xp_since: int(round(xp_since)), retake_pending}   # projector.py:616-625
velocity:      VelocityState() when enrolled_course is None                 # projector.py:638-651
               xp_per_day_28d      = round(sum(xp in trailing-28-local-day window)/28, 4)
               topics_per_week_28d = round(len(distinct topics in window)/(28/7.0), 4)
               course_progress     = round(mastered_in_course/total_in_course, 4)
               eta  = today                     if remaining <= 0
                    = None                      if xp_per_day_recent <= 0.0
                    = today + ceil(remaining*per_topic / rate) days
                      where per_topic = total_xp/done if done>0 else 12.0     # xp.py:250-274
window_start = local_day(t,tz) - (28-1) days ; membership test is `>= start`  # xp.py:192-194
```

XP pricing (`xp.py:68–143`): `base = 3.5*kp_count | 5.0 review | 15.0 quiz | 15.0 multi-step | 5.0 drill | 0.0 diagnostic`. `xp = base * tier_mult`, with `tier_mult *= 1.5**(max(1,run)-1)` for `blowoff`. If `rushing and xp > 0.0`: `xp *= 0.5`. `diagnostic` short-circuits to `0.0`. `is_rushing(correct, secs, expected)` = `False` when `correct` or `secs<=0` or `expected<=0`, else `secs < 0.5*expected`.

`pending_remediation` (`projector.py:596–614`): in trigger order, keep targets whose `last_practice` is missing or **strictly before** the trigger ts; drop empty; dedup on `(kind, tuple(open_targets))`, first wins.

---

## 5. Full replay vs incremental fold

**Full replay** (`project`, `projector.py:773–791`): `apply_regrades(events)`, then every event with `apply_fire=True`, then `finalize`.

**Incremental** (`project_incremental`, `projector.py:794–832`):
```
corrected = apply_regrades(prior_events + new_events)        # over the WHOLE stream
split     = len(corrected) - count(e in new_events if not isinstance(e, Regraded))
proj.topics = deep-copy of cached.topics
for e in corrected[:split]: apply(e, apply_fire=False)       # light indices only
for e in corrected[split:]: apply(e, apply_fire=True)
```

**What `apply_fire=False` skips:** all `TopicState` writes — `_on_attempt` returns immediately (`projector.py:214–215`); `_on_enrolled` skips the floor stamp; `_on_lesson_result` / `_on_review_result` / `_on_quiz_result` return after their light tallies; `_on_diagnostic_placed` returns after consuming `_diag_answers`; `_on_profile_reset` returns.

**What is always recomputed** (never cached): `enrolled_course`, `xp_events`, `completions`, `learned_at`, quiz cadence fields, `_remediation`, `_last_practice`, `_diag_answers`, `last_ts` — hence XP, streak, velocity, quiz state, and pending remediation.

**`PROJECTOR_VERSION = 3`** (`projector.py:89`). History: 1→2 for the methodology fixes (peel-back direction, diagnostic ability seeding, the `t0` penalty-skip, the refresh promote-guard, assisted discount, drill aggregation); 2→3 for `apply_regrades`. `service.project_and_save` (`service.py:257–280`) forces a **full replay** when either (a) `new` contains a `Regraded`, or (b) `cached.topics` is non-empty and `cached.projector_version != PROJECTOR_VERSION`. Rule for 2.0: **any change to the fold bumps the version.**

**`apply_regrades` semantics** (`projector.py:698–770`):
1. Scan the stream. Build `by_attempt: {attempt_id → RegradedAttempt}` (later wins) and `by_task: {task_id → Regraded}` for corrections carrying `quality_tier` or `xp` (later wins).
2. If both maps are empty, return the input list unchanged (identity fast path).
3. Re-scan. Drop every `Regraded`. For an `Attempt`: record `task_of_topic[topic] = task_id`, then substitute `work_quality`, `error_tags` (replaced wholesale, order preserved), `grader_note` if its id is named. For a `LessonResult`/`ReviewResult`: `task_id = event.task_id or task_of_topic.get(event.topic)`; if a correction exists for it, substitute `quality_tier` and/or `xp` (only the non-`None` ones).

**The original event stays in the log** — the substitution happens on an in-memory copy (`model_copy`), never on stored bytes. `regraded` events are removed from the returned stream and never reach a handler. A `LessonResult` with no preceding attempt on its topic is left alone. **The ordering rule is load-bearing:** a `LessonResult` binds to the task of the *most recent preceding* attempt on its topic, because `service.lesson_close` emits the result immediately after that task's last attempt (`projector.py:715–721`).

---

## 6. The selector

`compose_session` (`selector.py:1235–1481`). Reads `states`, `graph`, `cfg`, `t`, `rng`, and the keyword context (`course_id`, `pending_remediation`, `quiz_state`, `learned_at`, `last_drill_at`, `gap_fill_chain`, `active_study_days`, `quiz_high_score_streak`, `test_prep_topics`, `multistep_closed`, `closed_task_ids`, `open_multistep_components`, `n`).

If `open_plan` is given, it delegates to `_reserve_open_plan` (`selector.py:1630–1736`) and composes nothing new.

**Fresh composition, in order:**

1. `mastered = {tid ∈ graph.topics : status ∈ {learning, placed, floor}}` (`selector.py:153–163`).
2. `frontier_topics = sorted(graph.frontier(mastered) & course_topics)`; when `gap_fill_chain` is given, intersect with it (`:1311–1315`).
3. `available` / `blocked` split on `in_retry_delay` (`:1318–1319`). `in_retry_delay` = `t < t0 + retry_delay_days` when any `kp_progress` value is `failed_once`/`failed_twice` and `t0` is set (`:402–423`).
4. `course_complete = gap_fill_chain is None and every course topic mastered` (`:1324–1326`).
5. `frontier_blocked_until = min(retry times)` when `frontier_topics` is non-empty and `available` is empty (`:1327–1331`).
6. `remediation_tasks = _remediation_tasks(pending, ...)` (`:1337`), deduped by target, first occurrence wins; a mastered target gets a review, an unmastered one gets a lesson; every task carries `is_remediation=True` (`:1108–1152`).
7. `due = due_reviews(...)` — **`sorted()`** over `states.items()` filtered to `review_state(...) is due` (`:449–454`).
8. `comp = compress(due, ..., frontier_candidates=set(available))` (`:1344`).
9. `surviving = sorted(comp.surviving, key=lambda r: (-len(comp.knockouts.get(r,[])), r))` (`:1345`).
10. `nearly = _nearly_due(...)`, sorted by **`(memory_at(state,t), id)`** (`:457–472`).
11. Branch (`:1361–1371`): if `frontier_blocked_until is not None` → `nearly_served = set(nearly) - set(surviving)`; `review_topics = surviving + [nearly in order]`; no lessons. Else → `nearly_served = set(nearly_knockers)`; `review_topics = surviving + nearly_knockers`; `lessons_ordered = _arrange_lessons(order_lessons(available, graph, set(due)|set(nearly), course_topics), graph)`.
    `nearly_knockers = sorted(k for k in comp.knockouts if k in set(nearly) and k not in set(surviving))` (`:1352–1354`).
12. Drop any review/lesson whose topic is already a remediation target (`:1374–1375`).
13. Multi-step (`:1384–1430`), gated on `MULTISTEP_ENABLED`, `multistep_is_due(n_reviewable, multistep_closed)` = `n >= 4 and n_closed < n // 4`, and `multistep_id ∉ closed_task_ids`. Candidates are `review_topics` members that are in `due` and have review history; need `>= 3`; `multistep_components` sorts by **`(len(graph.ancestors(tid) & pool), tid)`** and truncates to 4 (`:1055–1068`). Components are removed from `review_topics`.
14. `seq = _interleave(review_topics, lessons_ordered, ...)` (`:1190–1219`): drain reviews in order, but force a lesson once `reviews_since_lesson >= max_reviews_per_lesson` (3); a lesson resets the counter.
15. Build tasks: remediation first, then `seq` in order, then the multi-step task, then the quiz (if `quiz_is_due`, sampled by `quiz_composer` with the passed `rng`), then `sorted()` drills (`:1434–1467`).
16. `tasks = tasks[:n]`; then `_assign_ids`.

**`compress`** (`selector.py:502–590`) — greedy weighted set-cover, two phases:
- `free_candidates = sorted((frontier | nearly) - due_set)`. Loop: for each `c` **in that sorted order**, `cov = {d in uncovered : d != c and W(c→d) >= 0.8}`; keep the first `c` with **strictly greater** `len(cov)` (so ties break to the lowest id); record `knockouts[best] = sorted(cov)`; remove. Stop when nothing covers anything.
- Phase 2: while `remaining`, for each `c` in `sorted(remaining)` take `cov = {c} | covers(c, remaining)`, pick the strict-max (lowest id on ties), append to `surviving`, record `knockouts[best] = sorted(cov - {best})` when non-empty, remove `cov`.
- Return `Compression(sorted(surviving), knockouts)`.

**`order_lessons`** (`:617–629`): key `(-importance(tid), tid)`; `importance = knockout_mass + |descendants(tid) & course_topics| + (0.5 if core else 0.0)`; `knockout_mass = sum(reach_weights(tid).get(d, 0.0) for d in review_targets)` — **a float sum over a set** (`:598–602`).

**`_arrange_lessons`** (`:1160–1187`) — module interleaving. Group by `graph.topic_module`, keeping first-seen module order and within-module importance order. Repeatedly pick `max(avail, key=lambda m: (len(groups[m]), -module_order.index(m)))` where `avail` excludes the last module used, falling back to all non-empty modules when none remains.

**Task ids** (`_assign_ids`, `:1484–1496`): `f"{session}-{task_type.value}-{topic.id}"` when a topic exists, else `f"{session}-{task_type.value}"`. Content-stable, never positional. Re-served remediation gets `f"{session}-rem-{topic.id}"` (`:1698`).

**`_task_still_valid`** (`:1527–1605`): `quiz` → `quiz_due`; `multi_step` → `False` if in `closed_task_ids`, else any component is `due`; `topic is None` → `False`; `is_remediation` → topic in `pending_targets`; `review` → state must exist and, when `task.nearly_due`, band ∈ `{due, nearly_due}`, else band is `due`; `lesson` → in `frontier_topics`, not mastered, not retry-delayed; `drill` → in `drill_eligible`; else `True`.

**Anti-repeat.** `Task.recent_problem_hashes = list(state.last_problems)` for a review (`:961`) and a remediation review (`:1138`); for a multi-step it is the pooled, deduped, order-preserving union across components (`:1085–1091`). Lessons carry none.

**Randomness.** The only RNG consumer is `quiz_composer` (`:682–754`), via `rng.sample(...)` at `:735` and `:747`. It is a `random.Random` the caller supplies; `events.seeded_rng(seed)` = `random.Random(seed)` (`events.py:71–73`).

**Mastery floor** enters through `enrolled` (`projector.py:198–211`) and `graph.mastery_floor`. `floor` counts as mastered for the frontier (`selector.py:72–74`) but is **off-schedule** for reviews (`fire.py:64–66`).

---

## 7. Parity traps

| # | Trap | Site | Proposed 2.0 rule |
|---|---|---|---|
| **T1** | **CPython `sum()` uses Neumaier compensated summation** (3.12+). Verified on this box: `sum([0.1]*10) == 1.0` exactly, while a naive `+=` loop gives `0.9999999999999999`; `sum([1.0, 1e100, 1.0, -1e100]) == 2.0` vs `0.0` naive. | `fire.py:361`, `fire.py:551`, `selector.py:602`, `selector.py:674`, `projector.py:628`, `projector.py:641`, `xp.py:207`, `service.py:833`, `service.py:893`, `service.py:948` | Implement `neumaier_sum(&[f64]) -> f64` and use it at **every** `sum()` site. `iter().sum::<f64>()` diverges. |
| **T2** | **The same code base also uses naive `+=` accumulation.** The two algorithms must not be unified. | naive: `projector.py:618–620` (`xp_since`), `xp.py:168` (`daily_totals`), `service.py:332` (`session_xp`) | Port each site with its own algorithm. Naive loop stays a naive loop. |
| **T3** | **Python `round()` is banker's rounding**; Rust `f64::round` is half-away-from-zero. Verified: `round(2.5)==2`, `round(0.5)==0`, `round(-3.5)==-4`, `round(4.5)==4`. | `projector.py:623`, `:628`, `:633`; `xp.py:310–312`; `service.py:459`, `:492`, `:520–525`, `:648`, `:752`, `:768`, `:834`, `:952`, `:986` | Implement `round_half_even(x)` and `round_half_even_dp(x, n)`. Never `f64::round`. |
| **T4** | **`round(x, n)` is correctly-rounded decimal, not scale-round-divide.** Verified: `round(2.675,2)==2.67`, `round(1.0000005,6)==1.000001`, `round(0.5000005,6)==0.5`. | `xp.py:310–312` (`,4`); `service.py:520–525` (`,4`); every `round(...,2)`; `diagnostic.py:312` (`,6`) | Format to `n` decimals with round-half-even on the exact binary value, then parse back. `(x*10f64.powi(n)).round()/10f64.powi(n)` diverges. |
| **T5** | **Set iteration order is hash-randomized** and varies per process. Verified: the same 62-element neighborhood yielded the sorted-first element at positions 30, 16, 18, 52, 9 under `PYTHONHASHSEED` 0–4. | float sums over sets: `fire.py:354–361` (`neighborhood`), `selector.py:598–602` (`review_targets`). Set loops: `projector.py:204–211` (mastery floor), `:470–482` (peel-back candidates) | Sort by topic id before iterating, everywhere. Compensated summation (T1) masks this for realistic ability values — the fixture is stable across seeds 0–99999 — but it is unguaranteed. Sorting removes the class of bug. |
| **T6** | **Dict iteration order is insertion order and is load-bearing** in `_refresh_placement` (`balances.items()`) and the ability-delta apply loop (`deltas.items()`, keyed topic-first then sorted neighbors). | `projector.py:392`, `:524`; `fire.py:397` | Use an insertion-ordered map (`IndexMap`) for `balances`; for `deltas`, apply the attempted topic first, then the rest by sorted id. |
| **T7** | **`3.5*3*0.85 == 8.924999999999999`** enters the event log **unrounded** from `_advance_lesson`, while `lesson_close` writes the **rounded** `8.92` for identical inputs. Both are legal history. | `service.py:452` + `:457` (raw) vs `service.py:768` (rounded) | Reproduce both paths bit-exactly. Never normalize XP on read. Pinned at `tests/test_regrade.py:470`. |
| **T8** | **`t0` is a `datetime`; all FIRe time math goes through `(t - t0).total_seconds() / 86400.0`.** A naive `ts` is silently read as UTC. | `fire.py:145–146`; `projector.py:690–691`; `xp.py:158–159` | Store instants as UTC microseconds since epoch; compute `days = (t_us - t0_us) as f64 / 86_400_000_000.0`. Reject or explicitly UTC-coerce naive input at the boundary. |
| **T9** | **Time zone enters only via `local_day`**, for streak, "today", and the velocity window — using `ZoneInfo`, with `None` meaning UTC. Effective tz is `cfg.timezone or profile.timezone`. | `xp.py:151–160`; `service.py:253–254` | Use a tzdb crate pinned to a known release. Compute the local date, never a local datetime. |
| **T10** | **`velocity`/`today`/`streak` use `t_ref = last_ts or now`, not wall clock.** Only `built_from_ts` is wall clock. | `projector.py:653–682` | Take `now` as an explicit parameter. Exclude `built_from_ts` from every parity comparison. |
| **T11** | **`random.Random.sample` is the Mersenne-Twister-backed CPython algorithm.** A Rust RNG will not reproduce it. Pinned at `tests/test_selector.py:322–327`. | `selector.py:735`, `:747`; `events.py:71–73` | Selector sampling is **out of M3 scope** (M3 is the projector/fold). When the quiz composer is ported, either re-implement MT19937 + CPython's `sample` exactly, or record the sampled ids on `task_served` and treat the seed contract as changed. Decide before M4. |
| **T12** | **`attempt_id` is a uuid4 in 1.0** — not deterministic, not content-derived. | `cadus_web/api.py:473`, `:621`, `:1480`, `:1722` | Define a 2.0 rule, e.g. `blake3(user_id ‖ task_id ‖ problem_id ‖ attempt_index)`. Keep it opaque to the fold: the fold uses `attempt_id` only for `apply_regrades` matching and store-level dedup. |
| **T13** | **Integer division and `int()` truncation.** `multistep_is_due` uses `n // 4` (`selector.py:1050–1052`); `median` uses `n // 2` (`service.py:877`); `generic_close` uses `int(median(timed))` — truncation toward zero (`service.py:984`); `quiz_composer` uses `len(older) // 2` (`selector.py:719`). | as listed | Use `usize` division for the index cases. For `int(median(...))` use `as i64` (truncation), **not** rounding. |
| **T14** | **`math.ceil` in the ETA**, over a float quotient. | `xp.py:273` | `(xp_remaining / rate).ceil() as i64`. Reproduce the division first, then ceil. |
| **T15** | **`_relax` composes `W` as a product along a path and a max over paths**, over an LIFO stack with a **strict `>`** relaxation and **no depth cap**. Cycle-safe because `w <= 1.0`. Dangling prerequisite ids **do** enter `_enc` (the `in self.topics` guard does not cover `_add_enc`), so `reach_weights` can name a non-topic and `apply_attempt` will create state for it. | `graph.py:178–198`, `:294`, `:301–321` | Port `_relax` verbatim, including the LIFO order and strict `>`. **Do not "fix" the dangling-id path** — it changes the fold. Keep the `_add_enc` zero-weight asymmetry (forward drops `w == 0.0`, reverse keeps it) because `neighborhood` reads the reverse key set. |
| **T16** | **`config_hash` is `sha256(cfg.model_dump_json())[:16]`** — the preimage is pydantic's exact JSON serialization, including `[0.33, 3.0]` for the tuple and `2.0` for the int-literal `2` in `interval_table`. | `projector.py:121–124` | Emit the preimage byte-for-byte. Verified value and preimage in §9. |
| **T17** | **`problem_text_hash` is `sha1(utf8(text))[:12]` with no normalization** — `" padded "` hashes differently from `"padded"`. | `projector.py:108–118` | Port verbatim. It is the one definition; never re-derive it elsewhere. |
| **T18** | **Curriculum load order is load-bearing.** `sorted(course_dir.glob("*.yaml"))` fixes unit-file order, hence topic insertion order. `sorted()` on `str` compares Unicode code points, not locale. | `graph.py:593`; `loader.py:85` | Sort paths byte-wise by the full path string. Sort ids by `char` scalar value. |
| **T19** | **`mastery_floor` unions both forms and uses an inclusive `<=` on `order`** — every course with an equal `order` is included. It can return ids that are not topics. | `graph.py:201–220` | Port the inclusive comparison and the union. Filter non-topics at the consumer, as `projector.py:204` does. |
| **T20** | **`_find_cycle` iterates a set**, so with more than one cycle the reported cycle varies per process. | `graph.py:128` | Sort before the DFS. Lint-only, but it makes error messages non-reproducible. |

---

## 8. Pinned behaviors from the 1.0 tests

**There is no `tests/test_projector*.py`.** Fold behavior is pinned in `test_regrade.py`, `test_ability_seeding.py`, and `test_peelback.py`. `tests/test_service.py` is Postgres-gated (skips without `CADUS_TEST_DATABASE_URL`), but its literals remain valid oracles.

### FIRe — `tests/test_fire.py` (`CFG = Config()`, `T = datetime(2026,7,14,12,0,0)` naive)

- `:132–143` `QUALITY_Q` exact map; `quality_q(perfect)==1.0`; `is_pass_quality(passable) is True`, `(nearly_passable) is False`.
- `:152–164` `memory_at`: base 1.0 / interval 10 / `t0=T-10d` → `≈0.5`; `t0=T-20d` → `≈0.25`; `TopicState()` → `0.0`; `memoryBase=0.9, t0=None` → `0.9`.
- `:170–215` `review_state` table (assertion `:215`). Default: `TopicState()`→`off_schedule` (both `test_prep` values); `frontier`→`off_schedule`; `floor` even with memory 0.1→`off_schedule`; `placed`→`due`; memory `0.0/0.49/0.5`→`due`; `0.5001/0.55/0.6`→`nearly_due`; `0.6001/0.65/0.7/1.0`→`on_schedule`. Test-prep: `0.5/0.55/0.6/0.65/0.7`→`due`; `0.7001/1.0`→`on_schedule`. Note `0.65` is `due` but **not** `nearly_due`.
- `:225–230` untouched satisfies `memory <= 0.5` yet is `off_schedule`.
- `:233–240` band progression `on_schedule → on_schedule(+7d) → nearly_due(+8d) → due(+10d)`.
- `:249–257` `interval_for`: `0→2.0`, `1→4.5`, `3.5→≈33.0`, `7→480.0`, `20→480.0`, `-4→2.0`.
- `:267–272` `speed_for`: `(0.5,0.0)→≈2.0`, `(0.3,0.3)→≈1.0`, `(-5.0,1.0)→0.33`, `(50.0,0.0)→3.0`.
- `:282–293` `raw_delta`: `(1.0,0.5,True)→≈1.0`; `(1.0,0.7,True)→≈0.6`; `(1.0,1.0,True)→≈0.15`; `(0.15,0.5,False)→≈-0.85`; `(0.15,1.0,False)→≈-0.85`; `(0.0,0.5,False)→≈-1.0`.
- `:304–313` `decay_for` (interval 10): `t0=T-10d→≈1.0`; `T-20d→≈2.0`; `T-50d→3.0`; `T-3d→1.0`.
- `:321–341` **p.364 credit.** Graph: `two-digit-mult` prereqs `[(one-digit-mult,0.8,key),(addition,0.6,False)]`; all three `_learned(0.5)`; `perfect` pass on `two-digit-mult`. → `two-digit-mult.memoryBase≈1.5`; `one-digit-mult≈1.3`; `addition≈1.1`; `props[one-digit-mult].raw_delta≈0.8`, `kind=="credit"`; `props[addition].raw_delta≈0.6`.
- `:344–367` **p.364 penalty.** All `_learned(1.0, rep=3.0)`; `poor` fail on `addition`. → `addition.repNum≈2.15`; `two-digit-mult.repNum≈2.49`, `memoryBase≈0.49`; `one-digit-mult.repNum≈3.0`; **ORDER** `[p.topic for p in props]==["two-digit-mult"]`; `kind=="penalty"`.
- `:387–392` overdue failure: `t0=T-10d`→`repNum≈4.15`; `t0=T-30d`→`repNum≈2.45`; `step_ov ≈ 3*step_on`.
- `:408–418` early-credit floor: `_learned(1.0,rep=3.0)`+perfect → `memoryBase≈1.15`, `repNum≈3.15`; `_learned(0.5,rep=3.0)` → `≈1.5`, `≈4.0`; `0.15 == 0.15*1.0`.
- `:439–441` speed: `d_fast≈2.0`, `d_slow≈1.0`, `d_fast≈2*d_slow`.
- `:449–466` `initial_ability`: neighbors at 0.4 and 0.8 → `≈0.6`; empty states → `0.5`; untouched-only neighbor → `0.5`.
- `:479–497` `ability_update` (all ability 0.5, α=0.3): correct on `two-digit-mult` → self `≈0.15`, `one-digit-mult≈0.12`, `addition≈0.09`; incorrect on `addition` → self `≈-0.15`, `two-digit-mult≈-0.09`, `one-digit-mult` **absent**.
- `:508–512` `knockout`: `(two-digit-mult, one-digit-mult)`→`True` (0.8≥0.8); `(two-digit-mult, addition)`→`False` (0.6); `(addition, addition)`→`True`.
- `:523–549` `grade_review`: `[F,T,F,T,T]→(True, 11/15)`; `[T,T,F,T,F]→(False, 7/15)`; `[T,T,T,T,F]→ score 10/15, passed False`; `[T]→(True,1.0)`; `[F]→(False,0.0)`; `[T]*4→(True,1.0)`; `[F]*4→(False,0.0)`; `[]→(False,0.0)`.
- `:557–589` gates: recipient `speed=0.5` absorbs nothing and is absent from `props`; `W=0.04` credit is dropped by `min_credit`; `apply_attempt` never mutates inputs.
- Hypothesis: `:620–644` (300 ex.) `repNum>=0 and memoryBase>=0`; `:646–663` (300) memory `>=0` and non-increasing (slack `1e-9`); `:666–689` (300) `implicit <= direct + 1e-12` and `implicit ≈ direct*weight`; `:692–707` (200) forced-explicit absorbs zero; `:710–715` a wrong final answer never passes.

### XP — `tests/test_xp.py`

- `:43–49` `base_xp`: lesson kp2→`≈7.0`, kp4→`≈14.0`; review `5.0`; quiz `15.0`; drill `5.0`; diagnostic `0.0`; multi-step `15.0`.
- `:57–62` multipliers exact: `1.3 / 1.0 / 0.85 / 0.3 / 0.0`.
- `:70–74` blow-off escalation: `n=1→≈-0.5`, `n=2→≈-0.75`, `n=3→≈-1.125`, `n=0→≈-0.5`.
- `:78–109` `task_xp`: `(lesson,perfect,kp2)→≈9.1`; `(review,passable)→≈4.25`; `(review,poor)→≈0.0`; `(review,blowoff,run=2)→≈-3.75`; every diagnostic tier → `0.0` exact; `(review,passable,rushing)→≈2.125`; `(review,blowoff,run=1,rushing)→≈-2.5` (rushing never rewards a negative).
- `:97–100` `is_rushing`: `(False,10,30)→True`; `(True,10,30)→False`; `(False,20,30)→False`; `(False,0,30)→False`.
- `:118–122` `local_day(2026-07-14T01:00Z, "America/New_York")→2026-07-13`; UTC→`2026-07-14`; naive `01:00`→`2026-07-13`.
- `:129–150` streak: daily `{07-10:50, 07-11:45, 07-12:60, 07-13:40, 07-14:30}`, goal 40, today 07-14 → `4`; with `07-14:45` → `5`; `{07-13:40, 07-12:10, 07-11:90}` today 07-14 → `1`.
- `:158–177` velocity: `xp_per_day ≈ (100+180)/28`; `topics_per_week ≈ 2/4`.
- `:185–250` progress/ETA on `tests/fixtures/curriculum_mini` (12 topics): 6 learning → `≈6/12`; all untouched with `repNum=3.0` → `0.0`; ETA `today+12d`; no velocity → `None`; `compute_velocity_state` → `xp_per_day_28d≈10.0`, `course_progress≈0.5`.

### Projector fold

- **`PROJECTOR_VERSION == 3`** and the projected model stamps `projector_version == 3` — `tests/test_regrade.py:358–361`.
- `problem_text_hash` = `sha1(text)[:12]`, no stripping (`" padded "` is a test input) — `tests/test_problem_templates.py:594–607`.
- Ability seeding (`tests/test_ability_seeding.py`, `T = 2026-07-28T12:00`): `:104–123` fast answer (`weight=1.0`) → ability `≈0.65`; slow (`weight=0.5`) → `≈0.575`; `speed == speed_for(0.65, 0.5, CFG)`. `:131–142` an upward penalty never stamps `t0` on an untouched dependent (`ns["A"] == states["A"]`, absent from `props`). `:181–197` a refresh with `balance=-1.0` leaves a never-learned topic `untouched`. `:200–219` a refresh folds only its own session's answers. `:222–237` an untouched topic is not re-seeded on every event (`a2 < a1`).
- Peel-back (`tests/test_peelback.py`): `:72–92` a miss on prereq `P` peels conditional dependent `C`: `repNum 0.8→≈0.4`, `interval_days == interval_for(0.4)`, `conditional False`, `status placed`; unrelated `D` keeps `conditional True`, `repNum 0.8`. `:95–110` a miss on `C` itself: `repNum 2.0→≈1.0`, `conditional False`, `status placed`.

### Regrade — `tests/test_regrade.py` (`T0 = 2026-08-17T16:30Z`)

- `:125–148` attempt tier `poor`, tags `["incomplete","timing-unreliable"]`; correction → `nearly_perfect`, **ORDER** `error_tags == ["timing-unreliable"]`, `grader_note == "regraded"`, `correct` and `given_answer` untouched.
- `:151–162` the `Regraded` event is consumed (`len(out)==1`).
- `:165–167` no correction → `apply_regrades(events) == events`.
- `:170–184` a later correction wins.
- `:187–199` a correction touches only the named `attempt_id`.
- `:207–224` a `LessonResult` takes the correction's `quality_tier` and `xp == 7.0`.
- `:227–250` a `ReviewResult` matches by its own `task_id`; `xp == 5.0`.
- `:253–273` an uncorrected sibling task keeps `xp == 5.95` and tier `passable`.
- `:316–333` model: `xp.total == -4` before (from `-3.5`), `== 7` after; `interval_days` grows.
- `:336–355` a correction written 40 days later still lands the XP on the **graded** day: `xp.today == 7`.
- `:364–384` incremental == full replay across a correction (`both == 7`).
- **`:470` the float literal `8.924999999999999`.** The re-price gate is the tier, never the XP value; a `0.005` artifact must not trigger a correction.
- `:496–520` the ops script leaves the original row untouched (`log[0].work_quality is blowoff`) and is idempotent (second pass `== []`).

### Selector — `tests/test_selector.py` (`T = 2026-07-14T12:00`)

ORDER assertions (list equality): `:151` `due_reviews == ["a"]`; `:156–157` `[]` / `["a"]`; `:171–172` `surviving == []`, `knockouts["parent"] == ["child"]`; `:181–182` `surviving == ["sub"]`, `knockouts["sub"] == ["add"]`; `:190` `sorted(surviving) == due` with `knockouts == {}` at `W=0.3`; `:275` `order_lessons(["lb","la"],…)[0] == "la"`; `:286` `mix[:2] == ["kp1","kp2"]`; `:353` no two adjacent lessons share a module; `:441` a retry-blocked lesson absorbs nothing (`surviving == ["child"]`); `:566–568` and `:620–622` remediation occupies `tasks[0]`; `:635/:637/:640` `schedule_drills == ["d"]` / `[]` (ability 0.96) / `[]` (drilled 1 day ago).

Other: `:253–256` throttle `maxrun <= 3`, `throttle_ok`, `lesson_ratio_ok`, `lessons >= 3`; `:322–327` `seeded_rng(42)` gives an identical quiz sample; `:380/:501` task ids are stable across a re-serve; `:389–398` course complete → `tasks == []`; `:406–424` `frontier_blocked_until == failed_at + 1 day`; `:527–545` legacy flags recovered from `why` prose; `:656–669` retake delay `False / False(+6h) / True(+1d1h) / False(not pending)`; `:693–698` difficulty bands distinct, saturating at index 2, clamping at 0; `:722–739` F12 activity-day cadence. Hypothesis `:200–222` (250 ex.): `surviving ∪ knocked == due`, disjoint, both `⊆ due`.

### Gap-fill — `tests/test_gap_fill.py`

`:202` `resolve_gap_fill_stack == ["top","mid","low"]`; `:220` `== ["top","mid"]` once `low-a` is mastered; `:227` `== ["low"]`; `:171–176` `chain == {"mid-a"}`; `:230–248` the serve loop terminates, clearing `top-a` while never touching `mid-b` / `mid-free`.

### Service — `tests/test_service.py` (`NOW = 2026-07-01T12:00Z`)

- **`4.55`** — a perfect 1-KP lesson pass (`3.5 × 1 × 1.3`), `:173–175`; also the `conftest.py:303–323` `mastery_events` literal.
- **`0.7333`** — a `✗✓✗✓✓` review passes at `pytest.approx(0.7333, abs=1e-3)` (exactly `11/15`) for `5.0` XP, `:612–618`. **This is the only non-default `approx` tolerance in the whole set;** everything else uses the pytest default `rel=1e-6`.
- `:169–184` `first.status=="continue"`, `first.xp is None`; `second.status=="task_passed"`, `xp≈4.55`, `after.status=="learning"`; every event in one record call shares `ctx.now`.
- `:187–211` a duplicate `attempt_id` raises `ServiceError("attempt_already_recorded")`, appends nothing, and `RecordResult` has **no** `duplicate` field.
- `:230–249`, `:571–592` `attempt_id="web-abc123"` / `"web-pg-1"` replay is a no-op.
- `:623–652` a second `complete_task` raises `task_already_closed`; exactly one `QuizResult`.
- `:707` **ORDER** `mastery_floor == ["counting"]`; `:766` **ORDER** `[t.task_id for t in plan2.tasks] == served_ids`.
- `:844`, `:862–863` gap-fill switches `["mid","low"]`, stack `["top","mid","low"]`.
- Full `ServiceError` code vocabulary: `no_open_session`, `attempt_already_recorded`, `unknown_task`, `task_already_closed`, `unknown_course`, `no_course`, `invalid_answer`, `unknown_topic`, `no_diagnostic`.
- Transaction pins `:345–568`: `record_attempt`, `complete_task`, `start_session`, `end_session`, `enroll`, `diag_answer`, `diag_finish` each run in exactly **one** transaction with **zero** writes outside it; an injected failure leaves the log untouched and a retry succeeds.

### Schema — `tests/test_events.py`, `tests/test_model.py`

- **The projector-read field tuple, asserted twice** (`test_events.py:125–128`, `:139–142`): `("topic","kp","task_type","correct","secs","error_tags","work_quality","assisted","attempt_id","task_id")`. `work` and `answer_kind` are evidence and must not move the model.
- `test_events.py:25–32` unknown `type` raises; `v: 0` raises matching `"v0"`; `:35–63` the shim path upgrades a v1 dict; `:69–72` `seeded_rng(1234)` is reproducible.
- `test_model.py:192–208` JSON round-trip is the identity for every sample and the discriminator is complete; `:211–224` a bad `work_quality` and a missing `attempt_id` both raise; `:227–239` weight/difficulty ranges and `extra="forbid"`; `:242–246` camelCase survives the dump; `:261–279` `config.yaml` equals `Config()` and the headline values (`due_threshold 0.5`, `interval_table [2,4.5,10,21,45,100,220,480]`, `knockout_weight 0.8`, `daily_goal 40`, `tiers.blowoff -0.5`, `quiz.questions 8`).

---

## 9. The proposed M3 oracle

### Artifacts

In this repo: `scripts/oracle/dump_projector_1_0.py`, `scripts/oracle/gen_stream_1_0.py`, `crates/core/tests/fixtures/events/stream_1.jsonl` (= m3_stream_1.jsonl), `crates/core/tests/fixtures/events/model_1.json` (= m3_model_1.json). Original scratchpad names and digests:

| file | sha256 |
|---|---|
| `dump_projector_1_0.py` | `5176eb8851a5284d6cfd8294e08336aaff63e8001f3bcd0d29028336f04ea06f` |
| `gen_stream_1_0.py` | `86fdfe7f1221a3e7a2e268eb2b927f2e9b0c87d56be2d1f3da9f37b72eead67f` |
| `m3_stream_1.jsonl` | `df18ea2a7c960f034cd805914c4a40ce81799d1a7643a0baf447604fa8188535` |
| `m3_model_1.json` | `5eabb9ff385d94fbbc4d8c3397ebe3540ac6acf613f2da3a7836508343a8f0c7` (file, with trailing newline) |
| `m3_event_schemas.json` | pydantic `model_json_schema()` for all 16 event types + `LearnerModel` |

### Commands

```sh
cd /home/deploy/dev/cadus
SCRATCH=/tmp/claude-1000/-home-deploy-dev-cadus2-0/423a634f-40c8-4ad8-9fdd-67df2281434b/scratchpad

# 1. Generate the stream (deterministic; no wall clock, no store, no DB).
/home/deploy/dev/cadus/.venv/bin/python $SCRATCH/gen_stream_1_0.py

# 2. Fold it with the 1.0 projector. Model to stdout, digest to stderr.
/home/deploy/dev/cadus/.venv/bin/python $SCRATCH/dump_projector_1_0.py \
    $SCRATCH/m3_stream_1.jsonl > $SCRATCH/m3_model_1.json
```

### The oracle values

```
events           = 47
projector_version = 3
config_hash       = 797575e985c12149
sha256            = ba128459985e0815db7446cb2af16452ec07d304b7efaa0952fc6567404245f5
```

**The `sha256` is over the canonical model blob with `built_from_ts` removed and no trailing newline** — `json.dumps(payload, sort_keys=True, separators=(",",":"), ensure_ascii=False, allow_nan=False)`. The `m3_model_1.json` file digest differs because `print` adds a newline. 2.0 must match the **blob** digest.

`config_hash` preimage (`cfg.model_dump_json()`, `projector.py:123`):

```json
{"fire":{"due_threshold":0.5,"interval_table":[2.0,4.5,10.0,21.0,45.0,100.0,220.0,480.0],"early_floor":0.15,"decay_cap":3.0,"min_credit":0.05,"knockout_weight":0.8,"explicit_speed_threshold":1.0,"speed_clamp":[0.33,3.0]},"ability":{"ewma_alpha":0.3},"lesson":{"kp_pass":"2consec|3of4","fail_after":5,"retry_delay_days":1},"review":{"questions":4,"pass_weighted":0.65},"selector":{"lesson_ratio_min":0.25,"max_reviews_per_lesson":3},"quiz":{"cadence_days":7,"cadence_xp":200,"questions":8,"retake_below":0.8},"diag":{"max_questions":40,"coverage_radius":3,"sibling_credit":0.5,"conditional_max":1.0},"xp":{"daily_goal":40,"tiers":{"perfect":1.3,"nearly_perfect":1.0,"passable":0.85,"nearly_passable":0.3,"poor":0.0,"blowoff":-0.5}},"drill":{"questions":20,"target_secs":6},"error_tags":["sign-error","arithmetic-slip","algebra-slip","wrong-method","formula-recall","misread-problem","incomplete","notation","units","timing-unreliable","blowoff","blank_answer"],"timezone":null}
```

Also verified: `problem_text_hash("x") == "11f6ad8ec52a"`; the curriculum at `/home/deploy/dev/cadus2.0/curriculum` is **byte-identical** to 1.0's (`diff -rq` clean), 1090 topics, 13 courses.

### Verifications already run

- **Determinism:** 3 consecutive runs → identical digest.
- **Hash-seed independence:** `PYTHONHASHSEED` ∈ {0,1,2,7,12345,99999} → identical digest. (T5 is latent here, not triggered.)
- **Incremental == full replay (D4):** split at indices 5, 20, 45, 46 — including splits that put the `regraded` event in the new half — all match the full-replay digest exactly.

### Stream #1 coverage

47 events: `enrolled` 1, `session_start` 2, `session_end` 2, `task_served` 10, `attempt` 20, `lesson_result` 5, `review_result` 5, `remediation_triggered` 1, `regraded` 1. Five topics on one real `foundations` prerequisite chain (`absolute-value` → `basic-absolute-value-equations` → `absolute-value-equations` → `absolute-value-inequalities`, plus `adding-integers`). An 8-day gap between the lesson and review blocks makes the review block overdue, so `decay_for > 1` fires. The fold reaches **47 topics** through encompassing propagation — far past the 5 explicit ones.

Branches the fixture exercises: first-touch ability seed; downward credit; upward penalty; the `t0 is None` penalty skip; `min_credit` drop; `_mark_lesson_fail` (a topic with `t0` and `kp_progress` set while `status` stays `untouched`); mastery-floor stamping (`floor` status on `place-value`, `single-digit-addition`, `multiplication-tables`); `assisted` discount; `apply_regrades` matching a `ReviewResult` that carries `task_id=null` through its preceding attempt; two attempts where `correct` and `work_quality` deliberately disagree; the `repNum > 0` with `memoryBase == 0.0` floor case (`two-step-equations`).

### Generator design for property testing (stream #2 onward)

Extend `gen_stream_1_0.py` with `--seed N`, emitting `m3_stream_{N}.jsonl`. Draw from a seeded PRNG that lives **only** in the generator, never in the fold.

**Required coverage, per stream:**

1. Every one of the 16 event types at least once, including the six the projector treats as no-ops — the port must also treat them as no-ops.
2. **Every FIRe branch:** pass with `early` at the `0.15` floor, mid-range, and clamped at `1.0`; fail at `q=0.0` (`raw_delta == -1.0`) and `q=0.15`; `decay` at `1.0`, mid, and the `3.0` cap; `repNum` clamped at `0` by a large negative delta; `interval_for` below index 0, interpolated, at the last index, and at the `730.0` cap; `speed_for` clamped at both `0.33` and `3.0`; a `min_credit` drop; a forced-explicit skip (`speed < 1.0`); the `t0 is None` penalty skip; `assisted` on both a pass and a miss.
3. **A 3-KP `passable` lesson pass**, so `xp == 8.924999999999999` appears in the log. Stream #1 does **not** hit it (its tier cycle never lands `passable` last on a 3-KP topic) — this is a gap to close.
4. Both lesson-close paths for identical inputs: the raw `8.924999999999999` from `_advance_lesson` and the rounded `8.92` from `lesson_close`.
5. `diagnostic_answer` + `diagnostic_placed`, both initial and `refresh=true`, including the H2 promote-guard (a non-positive balance on an untouched topic) and a conditional peel-back.
6. `profile_reset` on a topic with accumulated state.
7. Rounding edges: XP totals whose sum lands exactly on `.5` (to pin banker's rounding), and velocity values that exercise `round(x, 4)` half-even.
8. A `quiz_result` with `per_topic` rows on and off the curriculum, and a score straddling `retake_below`.
9. Streak edges: a day exactly at goal, a day one XP below, and a gap day — across a non-UTC `profile.timezone`.
10. Multiple `regraded` events, including a later correction superseding an earlier one on the same target, and a correction whose `LessonResult` has no preceding attempt (must be left alone).

**Properties to assert** for each generated stream:
- `rust_fold(stream) == python_fold(stream)` byte-for-byte on the canonical blob.
- `project_incremental(cached_at_k, prior, new) == project(all)` for every split `k` (already verified for stream #1).
- `apply_regrades` is idempotent: applying it twice equals applying it once.
- `repNum >= 0.0` and `memoryBase >= 0.0` for every topic after every event.
- `memory_at` is non-increasing in `t` for `t >= t0`.
- Implicit credit never exceeds the direct credit it derives from.
- Removing the `regraded` events and re-folding gives the pre-correction model — which pins that the original events stay intact.

**Do not** let the generator sample through `random.Random` in a way the port must reproduce (T11). Keep the RNG on the generator side only; the fold itself reads no randomness.