//! The task bodies: the served task, the graded attempt, and the three task
//! results, with the nested payloads they carry.

use serde::{Deserialize, Serialize};

use super::{
    AnswerKind, AttemptOutcome, Exposure, ItemSource, PositiveSecs, SchemaVersion, Secs, Slug,
    TaskType, Timestamp, WorkQuality,
};

/// Whether a flag is off. It keeps a default flag off the wire (C2).
const fn is_off(flag: &bool) -> bool {
    !*flag
}

/// The outcome a body carries before [`Attempt::normalize`] reads `correct`.
fn incorrect() -> AttemptOutcome {
    AttemptOutcome::Incorrect
}

/// A problem stub recorded on `task_served`.
///
/// `text_hash` is inert in 1.0: nothing computes or reads it. The live near-duplicate
/// guard hashes the ATTEMPT's problem text through
/// [`crate::learner::problem_text_hash`]. The field stays on the wire because a
/// removed field would break replay of a historical event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServedProblem {
    /// The id of the served problem instance.
    pub id: String,
    /// The inert digest of the problem text.
    pub text_hash: String,
    /// The time the author expects the problem to take.
    pub expected_time_secs: PositiveSecs,
}

/// The problem body carried on an `attempt` event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptProblem {
    /// The problem text the learner saw. [`crate::learner::problem_text_hash`] hashes
    /// this text, with no whitespace normalization (trap T17).
    pub text: String,
    /// The policy captured with this item; absence preserves legacy semantics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_contract: Option<Box<crate::answer::AnswerContract>>,
    /// The expected answer.
    pub expected: String,
}

/// One topic's row inside a `quiz_result`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuizTopicResult {
    /// The topic the question tested.
    pub topic: Slug,
    /// Whether the answer was mathematically correct.
    pub correct: bool,
    /// The time the learner took.
    pub secs: Secs,
}

/// The replacement grade fields for one superseded `attempt`.
///
/// `correct` is absent: a correction restates how well the work was done, and the
/// checker owns whether the answer was right (C4). The one exception is
/// `outcome`, and it exists because an UNGRADED attempt has NO checker verdict to
/// own (D-F2): a human is then the only grader, and the recovery path gives the
/// attempt the verdict the checker could not reach.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegradedAttempt {
    /// The attempt this correction supersedes.
    pub attempt_id: String,
    /// The replacement outcome. `None` leaves the recorded outcome standing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<AttemptOutcome>,
    /// The replacement work-quality tier.
    pub work_quality: WorkQuality,
    /// The replacement error tags. They replace the whole list, in order.
    #[serde(default)]
    pub error_tags: Vec<String>,
    /// The replacement grader note.
    #[serde(default)]
    pub grader_note: Option<String>,
}

/// A task was served. The fold treats it as a no-op.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskServed {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default = "SchemaVersion::current")]
    pub v: SchemaVersion,
    /// The id of the served task.
    pub task_id: String,
    /// The kind of task.
    pub task_type: TaskType,
    /// The topic of the task, when it has one.
    #[serde(default)]
    pub topic: Option<Slug>,
    /// The knowledge point of a lesson task.
    #[serde(default)]
    pub kp: Option<Slug>,
    /// The served problem stubs.
    #[serde(default)]
    pub problems: Vec<ServedProblem>,
    /// The component topics of a multi-step task.
    #[serde(default)]
    pub component_topics: Vec<Slug>,
    /// The seed the composer used.
    #[serde(default)]
    pub seed: Option<i64>,
    /// Whether the task confirms an inferred topic (D-F6). NEW IN 2.0.
    ///
    /// The writer SKIPS a false value, so every event written before D-F6 keeps
    /// its canonical bytes and its digest.
    #[serde(default, skip_serializing_if = "is_not_set")]
    pub confirm: bool,
}

/// Whether a boolean field stays at its default. The canonical writer skips it.
const fn is_not_set(flag: &bool) -> bool {
    !*flag
}

/// One graded problem attempt. It is the load-bearing event.
///
/// The projector reads `problem.text`, `correct`, and `outcome` from it (spec section
/// 4.1, D-F2). It never reads `work_quality`, `error_tags`, `secs`, `assisted`,
/// `work`, `answer_kind`, or `kp`. FIRe fires from the result events instead.
///
/// Schema v2 (D-F2, D-F9)
/// ----------------------
/// `outcome` names the third outcome, and `correct` stays and equals
/// `outcome == Correct`. The writer skips `outcome` when `correct` alone spells it,
/// so a v1 row that this build reads and writes back keeps its bytes (C2). A v1 row
/// carries no `outcome` key, and [`Attempt::normalize`] derives one from `correct`.
///
/// `exposure` and `timing_reliable` stay `None` on a v1 row: the reliability of old
/// evidence is not reconstructible, so the reader never invents a value for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default = "SchemaVersion::current")]
    pub v: SchemaVersion,
    /// The idempotency key of the attempt. The fold uses it for regrade matching only.
    pub attempt_id: String,
    /// The task the attempt belongs to.
    pub task_id: String,
    /// The topic the attempt practiced.
    pub topic: Slug,
    /// The knowledge point, for a lesson attempt.
    #[serde(default)]
    pub kp: Option<Slug>,
    /// The kind of task.
    pub task_type: TaskType,
    /// The problem the learner saw.
    pub problem: AttemptProblem,
    /// The answer the learner gave.
    pub given_answer: String,
    /// The learner's written work.
    #[serde(default)]
    pub work: Option<String>,
    /// The shape of the answer.
    #[serde(default)]
    pub answer_kind: Option<AnswerKind>,
    /// Whether the answer was mathematically correct (C4).
    ///
    /// It equals `outcome == AttemptOutcome::Correct` after [`Attempt::normalize`].
    pub correct: bool,
    /// The graded outcome (D-F2). A v1 row derives it from `correct`.
    #[serde(
        default = "incorrect",
        skip_serializing_if = "AttemptOutcome::is_derivable"
    )]
    pub outcome: AttemptOutcome,
    /// The digest of the served problem (D-F9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_digest: Option<String>,
    /// Where the served item came from (D-F9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_source: Option<ItemSource>,
    /// Whether the learner saw this digest before (D-F9). `None` on a v1 row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposure: Option<Exposure>,
    /// Whether the timer of this attempt is trustworthy (D-F9). `None` on a v1 row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing_reliable: Option<bool>,
    /// The knowledge points the item exercised (D-F9).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    /// Whether the learner answered a fresh item alone after feedback (D-F8).
    #[serde(default, skip_serializing_if = "is_off")]
    pub independent_after_feedback: bool,
    /// The time the learner took.
    pub secs: Secs,
    /// The grader's error tags.
    #[serde(default)]
    pub error_tags: Vec<String>,
    /// The grader's work-quality tier. Partial credit lives here, not in `correct`.
    pub work_quality: WorkQuality,
    /// The grader's note.
    #[serde(default)]
    pub grader_note: Option<String>,
    /// Whether the learner used help.
    #[serde(default)]
    pub assisted: bool,
}

/// A lesson closed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LessonResult {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default = "SchemaVersion::current")]
    pub v: SchemaVersion,
    /// The topic the lesson taught.
    pub topic: Slug,
    /// Whether the lesson passed.
    pub passed: bool,
    /// The knowledge point the lesson stopped at, when it failed.
    #[serde(default)]
    pub failed_at_kp: Option<Slug>,
    /// The XP the lesson priced.
    #[serde(default)]
    pub xp: f64,
    /// The work-quality tier of the closing attempt.
    pub quality_tier: WorkQuality,
    /// Whether any attempt of the task used help.
    #[serde(default)]
    pub assisted: bool,
}

/// A spaced review closed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewResult {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default = "SchemaVersion::current")]
    pub v: SchemaVersion,
    /// The topic the review tested.
    pub topic: Slug,
    /// Whether the review passed.
    pub passed: bool,
    /// The weighted score of the review.
    pub weighted_score: f64,
    /// The XP the review priced.
    #[serde(default)]
    pub xp: f64,
    /// The work-quality tier of the closing attempt.
    pub quality_tier: WorkQuality,
    /// Whether any attempt of the task used help.
    #[serde(default)]
    pub assisted: bool,
    /// The task the review closed. A null value binds through the preceding attempt.
    #[serde(default)]
    pub task_id: Option<String>,
    /// Whether the review ended without a pass or a fail decision (D-F7).
    ///
    /// Unit f13 sets it. The writer skips it when it is off, so a v1 row keeps its
    /// bytes (C2).
    #[serde(default, skip_serializing_if = "is_off")]
    pub inconclusive: bool,
    /// Skills that require one independent confirmation (D-F7).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub confirmation_skills: Vec<String>,
}

/// A quiz closed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuizResult {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default = "SchemaVersion::current")]
    pub v: SchemaVersion,
    /// The id of the quiz.
    pub quiz_id: String,
    /// The fraction of questions the learner answered correctly.
    pub score: f64,
    /// One row per question, in the order the quiz asked them.
    #[serde(default)]
    pub per_topic: Vec<QuizTopicResult>,
    /// The XP the quiz priced.
    #[serde(default)]
    pub xp: f64,
}

impl Attempt {
    /// Reconcile `outcome` and `correct` after a read (the v1 shim, D-F2).
    ///
    /// A v1 row carries no `outcome` key, so the field arrives at its serde default
    /// and this step replaces it with the outcome `correct` spells. An ungraded
    /// outcome survives the step, and its `correct` goes off, because an ungraded
    /// attempt claims no correctness (C4).
    ///
    /// [`crate::event::Event::from_json`] is the one caller. The step is idempotent.
    pub fn normalize(&mut self) {
        if self.outcome.is_ungraded() {
            self.correct = false;
        } else {
            self.outcome = AttemptOutcome::of_correct(self.correct);
        }
    }
}

/// A delayed retention probe on one knowledge point (D-F11).
///
/// The selector serves an unseen item some days after the lesson passed, and this
/// event records what came back. The fold treats it as a no-op until unit f19.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionProbe {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default = "SchemaVersion::current")]
    pub v: SchemaVersion,
    /// The knowledge point the probe tested.
    pub kp: Slug,
    /// The topic the knowledge point belongs to.
    pub topic: Slug,
    /// The days between the lesson pass and the probe.
    pub delay_days: u32,
    /// The digest of the probed item.
    #[serde(default)]
    pub item_digest: Option<String>,
    /// The outcome of the probe.
    pub outcome: AttemptOutcome,
    /// Whether the learner used help.
    #[serde(default)]
    pub assisted: bool,
    /// Whether the learner saw this digest before.
    #[serde(default)]
    pub exposure: Option<Exposure>,
    /// The time the learner took.
    pub secs: Secs,
}
