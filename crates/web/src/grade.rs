//! The one-transaction grade path of unit U8: `POST /api/task/{task_id}/answer`.
//!
//! Requirements: A3 (the verdict is decided locally for a right AND a wrong
//! answer), A4 (the prose arrives later; the verdict never waits for it), C2
//! (the log is append-only), C3 (every statement runs inside `begin_tenant`),
//! C4 (`correct` comes from [`cadus_core::answer::check`] alone), D-O2 (one
//! transaction), L2, L6, R4, T1 (this path spends no model token).
//!
//! Spec: `docs/reference/web-service-1.0-spec.md` section 2.1 (the reply),
//! section 4.3 (the ten steps of the transaction), section 5 (grading, the
//! tier, the tags, H3, timing, the caps), section 10 (the pinned literals), and
//! row U8 of section 11. Rulings D-M5-2, D-M5-3, D-M5-4 and D-M5-7 of
//! `docs/plans/M5.md` are binding.
//!
//! # The steps, in one transaction
//!
//! The advisory lock, the state read, the validate, the timing, the check, the
//! tier, one `INSERT … ON CONFLICT DO NOTHING`, the fold, the pool pop for
//! `next`, the state write, commit. Nothing here is slow: there is no model call
//! (R4, L6), so the whole request holds one short transaction and the 1.0
//! three-way `tolerate_duplicate` / `tolerate_closed` recovery is gone.
//!
//! # The attempt number and idempotency
//!
//! `attempt_id` is `"{task_id}-{n}"`, `n` being the 1-based position of the
//! attempt inside its task (`docs/plans/M3.md`, trap T12). [`attempt_index`]
//! reads that position from the LOG, inside this transaction and under the
//! tenant's advisory lock. The D-S6 row is loss-tolerant — `POST /api/enroll`
//! deletes it inside an open session — so a counter that lives there restarts
//! while the task ids stay, and every later attempt of that task then repeats an
//! id the log already holds. The log does not restart.
//!
//! # The log read (M5 review 1, finding F15)
//!
//! The window this path reads is `Open::events`, the events of the OPEN SESSION
//! and not the whole log. A task id is `{session}-{task_type}-{topic}`
//! (`cadus_core::selector::assign_ids`) and an attempt id is `{task_id}-{n}`, so
//! every attempt of every task of the open session stands in that window, and
//! [`attempt_index`], [`stored_attempt`] and [`advance`] are all task-scoped.
//! The whole-log read this path carried before grew with the lifetime event
//! count and passed the 150 ms Postgres segment of L2 on its own.
//! `crates/store/tests/bench_long_log.rs` is the gate.
//!
//! The H3 unaided re-solve takes the same id with `-rework` after it.
//!
//! # The history read (M5 review 2, finding V2)
//!
//! ONE decision of this path reads more than the open session: the repeat-fail
//! peel-back. A lesson task id is `{session}-lesson-{topic}`, and the failed
//! task is `done` in the D-S6 row for the rest of its own session, so a second
//! failure of one lesson is only reachable in a LATER session and the window
//! above can never hold the earlier `lesson_result`.
//! `cadus_store::state::load_session_view` answers it from the whole-log map
//! `SessionView::lesson_failures`, which costs one `learner_models` row plus
//! the events above the fold cursor and never grows with the lifetime event
//! count.
//!
//! A log this build writes is dense, so the number of a new attempt is free. A
//! log with a gap in it — an operator repair, or a 1.0 log whose ids came from
//! the problem id (spec section 4.1) — can still put the computed id on a row
//! that stands. The partial unique index on `(user_id, attempt_id)` then makes
//! the INSERT a no-op, and the reply is [`STATUS_ALREADY_RECORDED`] with the
//! STORED verdict and nothing appended (spec section 4.3 step 6: "reply with the
//! state read").
//!
//! # What this unit does NOT do
//!
//! The explicit task close of 1.0 (`service.complete_task`: the `review_result`,
//! the `quiz_result` and the multi-step closes, their weighted scores, and their
//! XP) is not ported yet, and neither is `POST /api/task/{id}/abort`. A
//! non-lesson task therefore reaches `done` by count parity, exactly as
//! `api.py:1610-1614` does, but no close event stands behind it.
//!
//! # The `diagnosis` field (unit U9)
//!
//! [`crate::diagnosis`] owns every rule of it. This path calls
//! `diagnosis::decide` at ONE point, after the attempt is appended and before
//! the commit, so the enqueue of spec section 4.3 step 9 shares the fate of the
//! attempt: a grade that rolls back leaves no job row. A quiz reply carries no
//! `diagnosis` and starts no job — a quiz reveals nothing before its batch
//! reveal (trap W7), and the reveal unit is the one that hands the prose out.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use cadus_core::answer::check::{Outcome, check};
use cadus_core::config::Config;
use cadus_core::curriculum::{AnswerKind, Curriculum};
use cadus_core::event::{
    Attempt, AttemptProblem, Event, LessonResult, RemediationTriggered, SchemaVersion, Secs, Slug,
    TaskType, Timestamp, WorkQuality,
};
use cadus_core::projector::{kp_failed, kp_passed};
use cadus_core::selector::{
    REMEDIATION_LESSON_FAIL, REMEDIATION_REPEAT_FAIL, Task, remediation_for_repeat_fail,
};
use cadus_core::xp::{is_rushing, task_xp};
use cadus_store::state::{
    EventRow, SessionView, append_event, load_session_view, project_and_save,
};
use serde_json::{Value, json};

use crate::AppState;
use crate::diagnosis::{self, Miss, Pending};
use crate::error::ApiError;
use crate::metrics;
use crate::serve::{Open, find, install_next, open, progress_for, unix_seconds};
use crate::session::{bound, content, failed, now_pair, projection_input, write_state};
use crate::state::{Content, INVALID_REQUEST, STATE_UNAVAILABLE, ServedProblem, Tenant, WebState};

/// The code of an answer or a work field over its cap (`api.py:1292-1293`).
pub const ANSWER_TOO_LARGE: &str = "answer_too_large";

/// The code of an answer whose kind the checker never decides.
///
/// Spec section 5.1: a `multi-step` or a `proof` answer gets NO synchronous
/// verdict. V2 keeps both kinds out of the serving pool, so this refusal guards
/// a state the M5 routes cannot reach. It exists so that no path can fabricate a
/// `correct` the checker did not decide (C4), and it enqueues nothing.
pub const UNDECIDABLE_KIND: &str = "undecidable_kind";

/// The task is open and the learner owes it more problems.
pub const STATUS_CONTINUE: &str = "continue";

/// A lesson knowledge point passed and the lesson moved to the next one.
pub const STATUS_KP_ADVANCE: &str = "kp_advance";

/// The task closed with a pass.
pub const STATUS_TASK_PASSED: &str = "task_passed";

/// The task closed with a failure.
pub const STATUS_TASK_FAILED: &str = "task_failed";

/// The INSERT of step 6 met an `attempt_id` that already stands, so nothing was
/// appended (`api.py:596`, `:1592-1593`).
pub const STATUS_ALREADY_RECORDED: &str = "already_recorded";

/// The largest answer the route accepts, in characters (`api.py:91`).
pub const MAX_ANSWER_CHARS: usize = 4_000;

/// The largest work field the route accepts, in characters (`api.py:92`).
pub const MAX_WORK_CHARS: usize = 20_000;

/// `secs` is clamped at this multiple of the topic's `expected_time_secs`
/// (`api.py:737-740`).
pub const TIMING_CAP_MULTIPLIER: i64 = 10;

/// The note every grade of this path stamps (`deterministic_grade.py:68`).
pub const GRADER_NOTE: &str = "deterministic";

/// The form tag of a period-grouped integer (`deterministic_grade.py:146-149`).
pub const TAG_NOTATION: &str = "notation";

/// The tag of an elapsed time over the cap (`api.py:740`).
pub const TAG_TIMING_UNRELIABLE: &str = "timing-unreliable";

/// The tag of a blank submission (D-M5-7).
///
/// 1.0 writes `blank_answer`, the only underscore-spelled tag and the only one
/// outside its own vocabulary (trap W1). D-M5-7 adds the hyphenated spelling to
/// the vocabulary, so 2.0 ships one spelling rule and not the 1.0 exception.
pub const TAG_BLANK_ANSWER: &str = "blank-answer";

/// The stock re-solve instruction (D-M5-3, spec section 5.5).
///
/// The verdict now ships before any prose exists, so the instruction is a
/// constant and not model output. It is served on every miss and on every
/// assisted-correct `rework_required` reply (L2, L3).
pub const RE_SOLVE: &str = "Study the worked solution above until you can see why each step \
                            follows. Then close it and solve the original problem again \
                            yourself, from memory and unaided. Do that before you move on.";

// --------------------------------------------------------------------------- //
// The pure grade (spec section 5)
// --------------------------------------------------------------------------- //

/// One deterministic verdict: what the log records and what the client reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grade {
    /// Whether the answer is the authored answer (C4).
    pub correct: bool,
    /// The work-quality tier (D-M5-2).
    pub work_quality: WorkQuality,
    /// The error tags the SERVER observed (D-M5-4). The diagnosis of unit U9
    /// adds its own later; it never moves the tier.
    pub error_tags: Vec<String>,
}

/// Grade one submission with no model call at all (A3, spec section 5.1).
///
/// The tiers are the D-M5-2 ruling:
///
/// - correct: `nearly_perfect`, the NEUTRAL tier. `perfect` is a bonus for
///   method the checker never reads, so it is never awarded, and every tier
///   below is a penalty for a flaw the checker never observes.
/// - a decided miss: `nearly_passable` (XP ×0.3, FIRe q 0.4), below the pass
///   threshold of 0.7, so a wrong answer is never priced as a pass.
/// - a blank: `poor`, which is the tier of 1.0's `blank_answer_grade`.
///
/// An answer the checker refuses ([`Outcome::Undecidable`]: the input cap, or an
/// exit from the grammar) is a deterministic MISS with no tag, which is the
/// section 5.1 rule. It is never a model verdict and never a pass.
#[must_use]
pub fn deterministic_grade(expected: &str, answer: &str, kind: AnswerKind) -> Grade {
    if answer.trim().is_empty() {
        return Grade {
            correct: false,
            work_quality: WorkQuality::Poor,
            error_tags: vec![TAG_BLANK_ANSWER.to_string()],
        };
    }
    match check(expected, answer, kind) {
        Outcome::Decided(verdict) if verdict.correct => Grade {
            correct: true,
            work_quality: WorkQuality::NearlyPerfect,
            error_tags: if verdict.notation {
                vec![TAG_NOTATION.to_string()]
            } else {
                Vec::new()
            },
        },
        Outcome::Decided(_) | Outcome::Undecidable(_) => Grade {
            correct: false,
            work_quality: WorkQuality::NearlyPassable,
            error_tags: Vec::new(),
        },
    }
}

/// Whether one submission is reference-assisted (H3, spec section 5.4).
///
/// An attempt is assisted when the learner took a hint on this problem or the
/// client flags it (`api.py:1360`).
///
/// A QUIZ attempt is never assisted. 1.0 answers a quiz in `_quiz_answer` and
/// returns from it BEFORE the assisted rule runs (`api.py:1349-1358`), so no
/// quiz answer of 1.0 carries the flag and no quiz answer reaches the H3 reply.
/// The order matters here and not only for parity: the H3 reply names
/// `expected` and `solution`, and a quiz reveals NOTHING before its batch reveal
/// (trap W7). Without this rule a client that sends `"assisted": true` with a
/// correct quiz answer reads the authored answer of every remaining question.
#[must_use]
pub fn reference_assisted(task_type: TaskType, client_flag: bool, hints_given: usize) -> bool {
    if task_type == TaskType::Quiz {
        return false;
    }
    client_flag || hints_given > 0
}

/// The server-measured solve time and its timing tags (`_measure_secs`).
///
/// `expected_time_secs` is `None` when the topic is not in the arena. 1.0 skips
/// the clamp in that case, so this port skips it too: no cap, and no tag.
#[must_use]
pub fn measure_secs(
    started_at: f64,
    now_micros: i64,
    expected_time_secs: Option<i64>,
) -> (i64, Vec<String>) {
    let elapsed = (unix_seconds(now_micros) - started_at).max(0.0).round();
    let Some(expected) = expected_time_secs else {
        return (whole_secs(elapsed), Vec::new());
    };
    let cap = expected.saturating_mul(TIMING_CAP_MULTIPLIER);
    if whole_secs(elapsed) > cap {
        return (cap, vec![TAG_TIMING_UNRELIABLE.to_string()]);
    }
    (whole_secs(elapsed), Vec::new())
}

/// A rounded, non-negative elapsed time as whole seconds.
///
/// The value is bounded BEFORE the cast, so no instant can truncate and the
/// function cannot panic.
fn whole_secs(elapsed: f64) -> i64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "the ceiling only has to be safe, not exact"
    )]
    let ceiling = i64::MAX as f64;
    if elapsed <= 0.0 {
        return 0;
    }
    if elapsed >= ceiling {
        return i64::MAX;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the two guards above bound the value inside the i64 range"
    )]
    let secs = elapsed as i64;
    secs
}

/// Round an XP total to two places, as `service.py` does.
fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

// --------------------------------------------------------------------------- //
// The lesson advance (`service.advance_task`, `service.py:417-500`)
// --------------------------------------------------------------------------- //

/// What one recorded attempt did to its task.
struct Advance {
    /// The `task_status` of the reply.
    status: &'static str,
    /// The close event of a lesson, when the attempt closed one.
    result: Option<Event>,
    /// The remediation events the close triggered.
    remediation: Vec<Event>,
    /// The XP the core priced, when it priced one.
    xp: Option<f64>,
}

impl Advance {
    /// The answer for every attempt that closes nothing.
    const fn carry_on() -> Self {
        Self {
            status: STATUS_CONTINUE,
            result: None,
            remediation: Vec::new(),
            xp: None,
        }
    }
}

/// Decide the task status of one attempt (`advance_task`, `service.py:417-427`).
///
/// Only a lesson advances here. A review, a drill, a quiz and a multi-step task
/// close explicitly, because their pass rule is order-sensitive over the whole
/// question set, so every non-lesson attempt is [`STATUS_CONTINUE`].
fn advance(
    graph: &Curriculum,
    cfg: &Config,
    now: Timestamp,
    attempt: &Attempt,
    prior: &[EventRow],
    history: &SessionView,
) -> Advance {
    if attempt.task_type != TaskType::Lesson {
        return Advance::carry_on();
    }
    let Some(idx) = graph.idx_of(attempt.topic.as_str()) else {
        return Advance::carry_on();
    };
    let kp_ids: Vec<String> = graph
        .knowledge_points(idx)
        .iter()
        .map(|kp| kp.id.as_str().to_string())
        .collect();
    let kp = attempt
        .kp
        .as_ref()
        .map(|slug| slug.as_str().to_string())
        .or_else(|| kp_ids.first().cloned());

    let mut sequence: Vec<bool> = prior
        .iter()
        .filter_map(|row| match &row.event {
            Event::Attempt(body)
                if body.task_id == attempt.task_id
                    && body.kp.as_ref().map(|slug| slug.as_str().to_string()) == kp =>
            {
                Some(body.correct)
            }
            _ => None,
        })
        .collect();
    sequence.push(attempt.correct);

    if kp_failed(&sequence, cfg) {
        return lesson_failed(graph, cfg, now, attempt, kp.as_deref(), history);
    }
    if !kp_passed(&sequence) {
        return Advance::carry_on();
    }
    // The knowledge point passed. A lesson with a later knowledge point advances
    // to it; a lesson at its last one is over.
    let position = kp
        .as_ref()
        .and_then(|id| kp_ids.iter().position(|k| k == id));
    if position.is_some_and(|at| at + 1 < kp_ids.len()) {
        return Advance {
            status: STATUS_KP_ADVANCE,
            ..Advance::carry_on()
        };
    }
    let quality = attempt.work_quality;
    let count = i64::try_from(kp_ids.len()).unwrap_or(i64::MAX);
    let xp = task_xp(TaskType::Lesson, quality, cfg, count, 0, false);
    // The passing lesson is reference-assisted when ANY of its attempts was.
    let assisted = attempt.assisted
        || prior.iter().any(|row| {
            matches!(&row.event, Event::Attempt(body)
                if body.task_id == attempt.task_id && body.assisted)
        });
    Advance {
        status: STATUS_TASK_PASSED,
        result: Some(Event::LessonResult(LessonResult {
            ts: now,
            session: attempt.session.clone(),
            v: SchemaVersion,
            topic: attempt.topic.clone(),
            passed: true,
            failed_at_kp: None,
            xp,
            quality_tier: quality,
            assisted,
        })),
        remediation: Vec::new(),
        xp: Some(round2(xp)),
    }
}

/// The failed lesson close and its remediation (`_lesson_failed`).
fn lesson_failed(
    graph: &Curriculum,
    cfg: &Config,
    now: Timestamp,
    attempt: &Attempt,
    kp: Option<&str>,
    history: &SessionView,
) -> Advance {
    let quality = attempt.work_quality;
    let expected = graph
        .idx_of(attempt.topic.as_str())
        .and_then(|idx| graph.topic(idx))
        .map_or(0, |topic| topic.expected_time_secs);
    // A failing lesson answer that was implausibly fast trips the rushing
    // penalty. A passing lesson never reaches here.
    let rushing = is_rushing(attempt.correct, attempt.secs.get(), expected);
    let xp = task_xp(TaskType::Lesson, quality, cfg, 1, 0, rushing);
    let failed_at_kp = kp.and_then(|id| Slug::new(id).ok());
    let result = Event::LessonResult(LessonResult {
        ts: now,
        session: attempt.session.clone(),
        v: SchemaVersion,
        topic: attempt.topic.clone(),
        passed: false,
        failed_at_kp: failed_at_kp.clone(),
        xp,
        quality_tier: quality,
        assisted: false,
    });

    // A SECOND failure at one knowledge point peels back to its key
    // prerequisites; the first one queues a plain lesson-fail remediation.
    //
    // The test reads the WHOLE-LOG map and not the open session (V2): the first
    // failure closed its lesson task, and that task stays done for the rest of
    // its own session, so the earlier `lesson_result` always stands in an
    // earlier session. The map holds the knowledge point of the event, so the
    // lookup spells the key the same way (`_topic_already_failed`).
    let repeat = history.already_failed(
        attempt.topic.as_str(),
        failed_at_kp.as_ref().map(Slug::as_str),
    );
    let mut remediation = Vec::new();
    if repeat {
        let queued =
            remediation_for_repeat_fail(attempt.topic.as_str(), kp.unwrap_or_default(), graph);
        if !queued.targets.is_empty() {
            remediation.push(Event::RemediationTriggered(RemediationTriggered {
                ts: now,
                session: attempt.session.clone(),
                v: SchemaVersion,
                kind: REMEDIATION_REPEAT_FAIL.to_string(),
                source_topic: attempt.topic.clone(),
                targets: queued.targets,
            }));
        }
    } else {
        remediation.push(Event::RemediationTriggered(RemediationTriggered {
            ts: now,
            session: attempt.session.clone(),
            v: SchemaVersion,
            kind: REMEDIATION_LESSON_FAIL.to_string(),
            source_topic: attempt.topic.clone(),
            targets: Vec::new(),
        }));
    }
    Advance {
        status: STATUS_TASK_FAILED,
        result: Some(result),
        remediation,
        xp: Some(round2(xp)),
    }
}

/// The knowledge point a passed one advances to (`_next_kp`, `api.py:230-236`).
fn next_kp(graph: &Curriculum, topic_id: &str, current: Option<&str>) -> Option<String> {
    let idx = graph.idx_of(topic_id)?;
    let ids: Vec<&str> = graph
        .knowledge_points(idx)
        .iter()
        .map(|kp| kp.id.as_str())
        .collect();
    let at = ids.iter().position(|id| Some(*id) == current)?;
    ids.get(at + 1).map(|id| (*id).to_string())
}

// --------------------------------------------------------------------------- //
// The request body and its caps (spec section 5.7)
// --------------------------------------------------------------------------- //

/// What the client sends. Every field is read defensively: the handler must not
/// panic on ANY body (the no-panic rule).
struct Submission {
    /// The id of the problem the learner answers.
    problem_id: String,
    /// The answer text. An absent field is a blank answer.
    answer: String,
    /// The learner's written work.
    work: Option<String>,
    /// Whether the client flags the attempt reference-assisted (H3).
    assisted: bool,
}

/// Read the body, or the `422`/`413` that refuses it.
fn submission(body: Option<&Value>) -> Result<Submission, ApiError> {
    let field = |name: &str| body.and_then(|value| value.get(name));
    let problem_id = field("problem_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if problem_id.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            INVALID_REQUEST,
            "answer requires a problem_id.",
        ));
    }
    let answer = match field("answer") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(_) => {
            return Err(ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                INVALID_REQUEST,
                "answer must be a string.",
            ));
        }
    };
    let work = match field("work") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text.clone()),
        Some(_) => {
            return Err(ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                INVALID_REQUEST,
                "work must be a string.",
            ));
        }
    };
    // The caps bound the checker's CPU on hostile input (spec section 5.7). The
    // count is in CHARACTERS, so a multi-byte answer is not refused for its
    // encoding.
    if answer.chars().count() > MAX_ANSWER_CHARS
        || work
            .as_ref()
            .is_some_and(|text| text.chars().count() > MAX_WORK_CHARS)
    {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            ANSWER_TOO_LARGE,
            "answer or work exceeds the size limit.",
        ));
    }
    Ok(Submission {
        problem_id,
        answer,
        work,
        assisted: field("assisted").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// `500 state_unavailable`: the stored D-S6 row names something the arena does
/// not hold, so the request cannot be graded honestly.
fn broken_state(reason: &str) -> ApiError {
    tracing::error!(reason = %reason, "answer: the served problem is not gradable");
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        STATE_UNAVAILABLE,
        "The stored session state is not usable for this answer.",
    )
}

/// The answer kind of a served problem, in the checker's spelling.
fn answer_kind(served: &ServedProblem) -> Option<AnswerKind> {
    match served.answer_kind.as_deref() {
        Some("numeric") => Some(AnswerKind::Numeric),
        Some("expression") => Some(AnswerKind::Expression),
        Some("multi-step") => Some(AnswerKind::MultiStep),
        Some("proof") => Some(AnswerKind::Proof),
        _ => None,
    }
}

/// The event spelling of an answer kind.
const fn event_kind(kind: AnswerKind) -> cadus_core::event::AnswerKind {
    match kind {
        AnswerKind::Numeric => cadus_core::event::AnswerKind::Numeric,
        AnswerKind::Expression => cadus_core::event::AnswerKind::Expression,
        AnswerKind::MultiStep => cadus_core::event::AnswerKind::MultiStep,
        AnswerKind::Proof => cadus_core::event::AnswerKind::Proof,
    }
}

/// Drop every scratch entry a finished task owns (`_clear_task_scratch`).
///
/// The buffers hold each question's hidden `expected` and its solution sketch,
/// and the D-S6 row is persisted, so a closed task must not keep them.
fn clear_task_scratch(scratch: &mut WebState, task_id: &str) {
    scratch.served.remove(task_id);
    scratch.quizzes.remove(task_id);
    scratch.multistep.remove(task_id);
    scratch.task_memory.remove(task_id);
}

// --------------------------------------------------------------------------- //
// POST /api/task/{task_id}/answer
// --------------------------------------------------------------------------- //

/// Grade one answer, record it, and hand back the whole verdict (A3, A4).
///
/// The route never calls a model and never waits for one. Section 4.3 gives the
/// order of the steps and this function follows it top to bottom.
pub async fn answer(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    Path(task_id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let graph = &content.curriculum;
    let submitted = submission(body.as_ref().map(|Json(value)| value))?;
    let (_, now) = now_pair();

    let Open {
        mut tx,
        events,
        mut scratch,
        plan,
    } = open(&state, content, user_id, now, true).await?;
    let task = find(&plan, &task_id)?.clone();
    // The answer path MAY install the progress row (`_validate`, `api.py:1303`).
    // The plan route may not: trap W3.
    progress_for(&mut scratch, &task, graph);
    // The section 4.2 re-check, in its two refusals: a closed task is
    // `409 task_complete`, a superseded id is `404 unknown_problem`.
    let served = scratch.validate(&task_id, &submitted.problem_id)?.clone();

    // The clock is measured BEFORE the grade, so no grading work inflates it.
    let expected_time = served
        .topic
        .as_deref()
        .and_then(|id| graph.idx_of(id))
        .and_then(|idx| graph.topic(idx))
        .map(|topic| topic.expected_time_secs);
    let (secs, timing_tags) = measure_secs(served.started_at, now.micros(), expected_time);

    let Some(kind) = answer_kind(&served) else {
        return Err(broken_state("the served problem names no answer kind"));
    };
    if !matches!(kind, AnswerKind::Numeric | AnswerKind::Expression) {
        // T6, spec section 7: the one `undecidable` decision of the counter.
        // This service asks no model for a verdict, so the kind ends here.
        state.metrics.count_grade(metrics::GRADE_UNDECIDABLE);
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            UNDECIDABLE_KIND,
            "This answer kind has no deterministic verdict, and this service never asks a \
             model for one.",
        ));
    }
    let grade = deterministic_grade(&served.expected.answer, &submitted.answer, kind);
    // T6, spec section 7: one count per grade DECISION, taken with no model call.
    state.metrics.count_grade(metrics::grade_result(&grade));
    let mut error_tags = grade.error_tags.clone();
    error_tags.extend(timing_tags);

    #[expect(
        clippy::cast_precision_loss,
        reason = "a solve time in seconds is far below 2**53"
    )]
    let elapsed = secs as f64;
    scratch.active_secs += elapsed;

    // H3, section 5.4.
    let assisted = reference_assisted(task.task_type, submitted.assisted, served.hints_given.len());
    let Some(session) = scratch.session.clone() else {
        return Err(broken_state("the state row names no session"));
    };
    let attempt = build_attempt(
        &task,
        &served,
        &submitted,
        &grade,
        &error_tags,
        secs,
        kind,
        assisted,
        &session,
        now,
        attempt_index(&events, &task.task_id),
    )?;

    // H3 first branch: an assisted attempt that grades CORRECT is NOT recorded.
    // It is stashed, the problem stays live, and the next submission is the
    // unaided re-solve (`api.py:1520-1532`).
    if assisted && grade.correct && served.rework.is_none() {
        let stash = serde_json::to_value(&attempt)
            .map_err(|err| broken_state(&format!("the attempt does not serialize: {err}")))?;
        if let Some(live) = scratch.served.get_mut(&task_id) {
            live.rework = Some(stash);
        }
        write_state(&state.db, &mut tx, user_id, &scratch).await?;
        tx.commit().await.map_err(|err| failed(&err.into()))?;
        return Ok(Json(json!({
            "rework_required": true,
            "problem_id": served.problem_id,
            "solution": served.solution_sketch,
            "expected": served.expected.answer,
            "re_solve": RE_SOLVE,
        })));
    }

    // H3 second branch: this submission IS the unaided re-solve. The STASHED
    // attempt is what gets recorded, under `-rework`; a failed re-solve rewrites
    // it to a miss and drops its `assisted` flag, so the assisted pass does not
    // stand (`api.py:1373-1375`).
    let (recorded, attempt_id) = match &served.rework {
        Some(stash) => {
            let mut stashed: Attempt = serde_json::from_value(stash.clone())
                .map_err(|err| broken_state(&format!("the stashed attempt did not read: {err}")))?;
            stashed.ts = now;
            stashed.session = Some(session.clone());
            stashed.attempt_id = format!("{}-rework", attempt.attempt_id);
            if !grade.correct {
                stashed.correct = false;
                stashed.assisted = false;
            }
            let id = stashed.attempt_id.clone();
            (stashed, id)
        }
        None => {
            let id = attempt.attempt_id.clone();
            (attempt, id)
        }
    };

    // Step 6. One INSERT. Zero rows back means the attempt already stands, so
    // the fold, the advance and the state write are all skipped and the whole
    // transaction rolls back with nothing appended (FR-14).
    let event = Event::Attempt(recorded.clone());
    let appended = bound(
        &state.db,
        append_event(&mut tx, user_id, &event, Some(&attempt_id)),
    )
    .await
    .map_err(|err| failed(&err))?;
    if appended.is_none() {
        // Spec section 4.3 step 6: reply with the state READ. The verdict this
        // request graded is NOT what the log holds, so the log's own verdict is
        // what the client reads. `stored` falls back to the graded attempt only
        // when the standing row is outside this transaction's read, which the
        // advisory lock rules out.
        let stored = stored_attempt(&events, &attempt_id).unwrap_or(&recorded);
        let standing = Grade {
            correct: stored.correct,
            work_quality: stored.work_quality,
            error_tags: stored.error_tags.clone(),
        };
        // The replay reads the pre-authored answer and the job id the FIRST
        // request wrote, and writes neither. A retried request therefore names
        // one job, not two, and the rollback below leaves the queue as it was.
        let replayed = diagnosis::decide(
            &state,
            &mut tx,
            user_id,
            &mut scratch,
            &pending(
                content,
                &served,
                kind,
                &attempt_id,
                &session,
                &task_id,
                &submitted,
                &standing,
                false,
            ),
        )
        .await?;
        let body = json!({
            "attempt_id": attempt_id,
            "correct": stored.correct,
            "work_quality": stored.work_quality,
            "error_tags": stored.error_tags,
            "secs": stored.secs.get(),
            "task_status": STATUS_ALREADY_RECORDED,
            "remediation": Vec::<Value>::new(),
            "next": Value::Null,
            "diagnosis": replayed,
        });
        tx.rollback().await.map_err(|err| failed(&err.into()))?;
        return Ok(Json(body));
    }

    // Step 7. The lesson advance, its close event, and its remediation.
    //
    // The repeat-fail peel-back reads HISTORY, not the open session (V2), so the
    // advance takes the whole-log session view beside the session window.
    let history = bound(&state.db, load_session_view(&mut tx, user_id))
        .await
        .map_err(|err| failed(&err))?;
    let moved = advance(graph, &content.cfg, now, &recorded, &events, &history);
    for extra in moved.result.iter().chain(moved.remediation.iter()) {
        bound(&state.db, append_event(&mut tx, user_id, extra, None))
            .await
            .map_err(|err| failed(&err))?;
    }
    let input = projection_input(content, now);
    bound(&state.db, project_and_save(&mut tx, user_id, &input, None))
        .await
        .map_err(|err| failed(&err))?;

    // Step 9. The pre-authored lookup and, on a miss with none, the enqueue.
    // Both run inside THIS transaction (spec section 4.3, D-M5-1). A quiz
    // reveals nothing before its batch reveal, so it asks for nothing here.
    let diagnosis = if task.task_type == TaskType::Quiz {
        Value::Null
    } else {
        diagnosis::decide(
            &state,
            &mut tx,
            user_id,
            &mut scratch,
            &pending(
                content,
                &served,
                kind,
                &attempt_id,
                &session,
                &task_id,
                &submitted,
                &grade,
                true,
            ),
        )
        .await?
    };

    // Step 8 and step 10: move the task on, draw the next problem, write the row.
    let closed = task_moved_on(&mut scratch, &task, &moved, graph, &served);

    // A quiz reveals NOTHING until its batch reveal (trap W7): no verdict, no
    // solution, and no expected answer in any pre-reveal body. The answer goes
    // into the buffer the close reads, the reply is a bare receipt, and the
    // buffer outlives the close, because the close unit builds the reveal from
    // it (`_quiz_answer`, `api.py:1748-1793`).
    if task.task_type == TaskType::Quiz {
        buffer_quiz_answer(&mut scratch, &task_id, &served, &recorded);
        scratch.served.remove(&task_id);
        let answered = scratch.plan_progress(&task_id).0;
        write_state(&state.db, &mut tx, user_id, &scratch).await?;
        tx.commit().await.map_err(|err| failed(&err.into()))?;
        return Ok(Json(json!({
            "accepted": true,
            "remaining": task.n_problems.unwrap_or(answered).saturating_sub(answered).max(0),
            "quiz_complete": closed,
        })));
    }

    let next = if closed {
        clear_task_scratch(&mut scratch, &task_id);
        None
    } else {
        scratch.served.remove(&task_id);
        install_next(
            &state,
            content,
            &mut tx,
            user_id,
            &task,
            &mut scratch,
            unix_seconds(now.micros()),
        )
        .await
        .map_err(|err| {
            // Trap W6: the attempt is already recorded, so a failed draw is
            // reported, never raised. A bare `next: null` on an open task reads
            // as "task over" (trap W5), so `next_unavailable` says otherwise.
            tracing::warn!(task_id = %task_id, code = %err.code, "answer: no next problem");
        })
        .ok()
    };
    let unavailable = !closed && next.is_none();
    write_state(&state.db, &mut tx, user_id, &scratch).await?;
    tx.commit().await.map_err(|err| failed(&err.into()))?;

    Ok(Json(reply(
        &recorded,
        &moved,
        &served,
        next,
        unavailable,
        &task,
        diagnosis,
    )))
}

/// What the diagnosis is about: THIS submission, never the stashed H3 attempt.
///
/// A failed re-solve records the stashed CORRECT answer with `correct` rewritten
/// to false (section 5.4), so the recorded row names an answer that is not a
/// mistake. The mistake the learner just made is `submitted.answer`, and that is
/// what the worker must read.
#[expect(
    clippy::too_many_arguments,
    reason = "the call sites of one function; every value comes from a different source"
)]
fn pending<'a>(
    content: &'a Content,
    served: &'a ServedProblem,
    kind: AnswerKind,
    attempt_id: &'a str,
    session: &'a str,
    task_id: &'a str,
    submitted: &'a Submission,
    grade: &Grade,
    write: bool,
) -> Pending<'a> {
    Pending {
        cfg: &content.cfg,
        served,
        kind,
        miss: Miss {
            attempt_id,
            session: Some(session),
            task_id,
            answer: &submitted.answer,
            work: submitted.work.as_deref(),
            correct: grade.correct,
        },
        write,
    }
}

/// The 1-based position of THIS attempt inside its task, read from the log.
///
/// It counts the `attempt` events the task already holds and adds one. An id of
/// the task is `{task_id}-{n}`, so the count takes the ids that carry the
/// `{task_id}-` prefix AND a digit after it. A topic id may hold a hyphen, so
/// one task id can be the prefix of another; the digit keeps the attempts of
/// task `{task}-{other}` out of the count of task `{task}`. The H3 re-solve id
/// `{task_id}-{n}-rework` passes both tests, so a re-solve counts as the one
/// attempt it records.
///
/// `events` is the OPEN SESSION's window, and a task id carries its session id,
/// so every attempt of the task is inside it (F15).
///
/// The caller holds the tenant's advisory lock and reads the log inside the same
/// transaction as the INSERT, so no second request of this tenant computes the
/// same number.
fn attempt_index(events: &[EventRow], task_id: &str) -> i64 {
    let prefix = format!("{task_id}-");
    let recorded = events
        .iter()
        .filter(|row| match &row.event {
            Event::Attempt(attempt) => attempt
                .attempt_id
                .strip_prefix(&prefix)
                .is_some_and(|rest| rest.starts_with(|first: char| first.is_ascii_digit())),
            _ => false,
        })
        .count();
    i64::try_from(recorded)
        .unwrap_or(i64::MAX)
        .saturating_add(1)
}

/// The attempt the log already holds under `attempt_id`.
///
/// Step 6 appends nothing when that row stands, so this is the "state read" the
/// [`STATUS_ALREADY_RECORDED`] reply answers with.
fn stored_attempt<'a>(events: &'a [EventRow], attempt_id: &str) -> Option<&'a Attempt> {
    events.iter().find_map(|row| match &row.event {
        Event::Attempt(attempt) if attempt.attempt_id == attempt_id => Some(attempt),
        _ => None,
    })
}

/// Build the `attempt` event of one submission (`_graded_answer`).
#[expect(
    clippy::too_many_arguments,
    reason = "the event has this many fields, and each one comes from a different source"
)]
fn build_attempt(
    task: &Task,
    served: &ServedProblem,
    submitted: &Submission,
    grade: &Grade,
    error_tags: &[String],
    secs: i64,
    kind: AnswerKind,
    assisted: bool,
    session: &str,
    now: Timestamp,
    index: i64,
) -> Result<Attempt, ApiError> {
    let Some(topic) = served.topic.as_deref().and_then(|id| Slug::new(id).ok()) else {
        return Err(broken_state("the served problem names no topic"));
    };
    let secs = Secs::new(secs).map_err(|err| broken_state(&format!("secs: {err}")))?;
    Ok(Attempt {
        ts: now,
        session: Some(session.to_string()),
        v: SchemaVersion,
        attempt_id: format!("{}-{index}", task.task_id),
        task_id: task.task_id.clone(),
        topic,
        kp: served.kp.as_deref().and_then(|id| Slug::new(id).ok()),
        task_type: task.task_type,
        problem: AttemptProblem {
            text: served.text.clone(),
            expected: served.expected.answer.clone(),
        },
        given_answer: submitted.answer.clone(),
        work: submitted.work.clone().filter(|text| !text.is_empty()),
        answer_kind: Some(event_kind(kind)),
        correct: grade.correct,
        secs,
        error_tags: error_tags.to_vec(),
        work_quality: grade.work_quality,
        grader_note: Some(GRADER_NOTE.to_string()),
        assisted,
    })
}

/// Move the task's progress on, and say whether the task is now closed.
///
/// A lesson closes on the core's status. Every other kind closes by count
/// parity, exactly as `api.py:1610-1614` does.
fn task_moved_on(
    scratch: &mut WebState,
    task: &Task,
    moved: &Advance,
    graph: &Curriculum,
    served: &ServedProblem,
) -> bool {
    let next_point = if moved.status == STATUS_KP_ADVANCE {
        task.topic
            .as_deref()
            .and_then(|topic| next_kp(graph, topic, served.kp.as_deref()))
    } else {
        None
    };
    let Some(progress) = scratch.tasks.get_mut(&task.task_id) else {
        return false;
    };
    if task.task_type == TaskType::Lesson {
        if let Some(point) = next_point {
            progress.current_kp = Some(point);
        }
        let closed = matches!(moved.status, STATUS_TASK_PASSED | STATUS_TASK_FAILED);
        progress.done = closed;
        return closed;
    }
    progress.answered = progress.answered.saturating_add(1);
    let closed = progress.total > 0 && progress.answered >= progress.total;
    progress.done = closed;
    closed
}

/// The client reply of section 2.1.
///
/// It names every field it emits. `solution` is revealed only after the attempt
/// commits, and `expected` never reaches the client on this path at all.
fn reply(
    recorded: &Attempt,
    moved: &Advance,
    served: &ServedProblem,
    next: Option<Value>,
    unavailable: bool,
    task: &Task,
    diagnosis: Value,
) -> Value {
    let remediation: Vec<Value> = moved
        .remediation
        .iter()
        .filter_map(|event| match event {
            Event::RemediationTriggered(body) => Some(json!({
                "kind": body.kind,
                "targets": body.targets.iter().map(Slug::as_str).collect::<Vec<_>>(),
            })),
            _ => None,
        })
        .collect();
    let mut payload = json!({
        "attempt_id": recorded.attempt_id,
        "correct": recorded.correct,
        "work_quality": recorded.work_quality,
        "error_tags": recorded.error_tags,
        "secs": recorded.secs.get(),
        "task_status": moved.status,
        "remediation": remediation,
        "next": next,
        "diagnosis": diagnosis,
    });
    let Some(map) = payload.as_object_mut() else {
        return payload;
    };
    // A quiz reveals nothing until its batch reveal (trap W7), so no solution
    // and no re-solve text leaves this route for one.
    if task.task_type != TaskType::Quiz {
        if let Some(solution) = &served.solution_sketch {
            map.insert("solution".to_string(), json!(solution));
        }
        if !recorded.correct {
            map.insert("re_solve".to_string(), json!(RE_SOLVE));
        }
    }
    if unavailable {
        map.insert("next_unavailable".to_string(), json!(true));
    }
    if let Some(xp) = moved.xp {
        map.insert("xp".to_string(), json!(xp));
    }
    payload
}

/// Put one answered quiz question into the buffer the batch reveal reads.
///
/// The buffer holds the hidden solution sketch of each question, so it is the
/// one place a quiz keeps it; the reply carries none of it (trap W7).
fn buffer_quiz_answer(
    scratch: &mut WebState,
    task_id: &str,
    served: &ServedProblem,
    recorded: &Attempt,
) {
    scratch
        .quizzes
        .entry(task_id.to_string())
        .or_default()
        .answers
        .push(json!({
            "problem_id": served.problem_id,
            "topic": served.topic,
            "text": served.text,
            "given_answer": recorded.given_answer,
            "correct": recorded.correct,
            "secs": recorded.secs.get(),
            "solution_sketch": served.solution_sketch,
        }));
}
