# Calibration protocol (D-F12)

Unit f20. It states which numbers of Cadus 2.0 are ENGINEERING DEFAULTS, which real
outcomes calibrate them, how large a sample each one needs, and what a software test
is allowed to prove.

## 1. The rule about evidence

A software test, a simulated learner, and a fixture stream are ENGINEERING evidence.
They show that the code does what the specification says. They show nothing about a
person, and no test result sets `policy_version.calibrated` to `true`.

Learner evidence comes from one source only: the answers real learners gave, in the
event log, with their provenance attached. `docs/plans/FRAMEWORK.md` D-F11 names that
provenance and `crates/core/src/retention/state.rs` keeps it apart.

Until a number passes the protocol below, the report prints
`v<N> (uncalibrated)` and every rate carries `"sufficient": false` below the minimum
sample. Nothing in the code claims an effect on a learner.

## 2. What the version covers

`Config.policy_version` names the version of the 2.0 policy set.
`Config::policy_digest()` is its drift digest, over these numbers:

| Number | Where | Default | Calibrate against |
|---|---|---|---|
| `lesson.kp_pass` | `crates/core/src/config.rs` | `2consec` | The 7-day probe of the knowledge points that passed under each rule. |
| `fire.interval_table` | `crates/core/src/config.rs` | `[2, 4.5, 10, 21, 45, 100, 220, 480]` | The 30-day and 90-day probes, per repetition number at the time of the pass. |
| `fire.due_threshold` | `crates/core/src/config.rs` | `0.5` | The independent accuracy of the reviews the memory level called due against the ones it did not. |
| `ability.ewma_alpha` | `crates/core/src/config.rs` | `0.3` | The placement error: the inferred topics that failed confirmation. |
| `retention.probe_delays_days` | `crates/core/src/retention/policy.rs` | `[7, 30, 90]` | The shape of the retention curve. A delay that never separates from its neighbor is noise. |
| `retention.max_per_session` | `crates/core/src/retention/policy.rs` | `1` | The session-abandon rate with and without a probe. |
| `readiness.enforce` | `crates/core/src/config.rs` | `true` | The lesson-fail rate of the knowledge points the rule blocked against the ones it served. |
| `mastery.confirm_inferred` | `crates/core/src/config.rs` | `true` | The placement error, as above. |

The 1.0 `config_hash` stays out of this. It detects drift of the 1.0 SCHEDULER
CONSTANTS, every stored `learner_models` row carries `797575e985c12149` for the
defaults, and a 2.0 policy bump must never invalidate a stored projection. The two
digests answer two different questions, and both appear in the report payload.

## 3. The delayed outcomes the protocol reads

The one measurement instrument is the delayed retention probe of D-F11:

- An UNSEEN item on a knowledge point whose lesson passed, `delay_days` after that
  pass, at most one per session.
- The event records the outcome, whether the learner used help, whether the item was
  a first exposure, and the seconds.
- Only a GRADED, UNASSISTED, FIRST-EXPOSURE answer counts as independent evidence.
  `crates/core/src/retention/state.rs` holds that rule, and the report never mixes
  the four classes into one rate.

An ungraded probe (D-F2) enters no numerator and no denominator. It is a failure of
the checker, not a failure of the learner, and it is reported on its own line.

## 4. The minimum sample

| Statement | Minimum independent probes | Why |
|---|---|---|
| One retained-accuracy rate, per delay | 20 | `retention.min_sample`. Below it the report sets `"sufficient": false`. |
| A comparison of two policy versions, per delay | 100 per version | A difference below about 10 points is not readable under 100 answers per arm. |
| A change to `interval_table` | 100 per repetition band, at 30 and at 90 days | The table is per repetition number, so the sample splits per band. |
| A change to `kp_pass` | 100 per rule, at 7 days | The rule decides the pass, so the earliest probe carries the signal. |
| A statement about assistance dependence | 50 probes of any provenance, per delay | The rate is over ALL probes, not over the independent ones. |

The counts are the floor of readability and no significance test. State the count
beside every rate; the report payload does that with `provenance`.

## 5. The protocol

1. FREEZE the policy. Note `policy_version.version` and `policy_digest()`.
2. COLLECT. Let the probes run under the frozen policy until every delay reaches its
   minimum sample. Do not change a versioned number while the sample builds; a change
   moves the digest and starts a new arm.
3. READ `GET /api/report/retention`. Read the `provenance` block of each row before
   the rate. A row with a large `assisted` or `repeated` count reports a familiarity
   effect and not recall.
4. COMPARE against the previous version, arm by arm, delay by delay. Compare only the
   INDEPENDENT rates.
5. DECIDE. If a number changes, bump `policy_version.version` and record here what
   the delayed outcomes said and how large the sample was.
6. HOLD the old arm. Never rewrite an event, and never restate an old rate under the
   new version. `learner_models` is derived and rebuildable; `events` is not (C2).

`policy_version.calibrated` becomes `true` only after step 5 changes or confirms a
number against a sample that meets section 4.

## 6. What is NOT calibrated today

Every number in the table of section 2. Version 1 is the 1.0 default set plus the 2.0
policies of D-F5, D-F6, D-F7 and D-F11, and no delayed outcome has fed back into any
of them. The report says so on its face: `v1 (uncalibrated)`.
