//! The D-S6 session-state document, its validate rule, and the tenant seam.
//!
//! Requirements: D-S6 (one hot state document per learner), D5 (the anti-repeat
//! ring and the per-task memory), C3 (every read and write of the row runs
//! inside `begin_tenant`), R4 (local CPU and database I/O only).
//!
//! Spec: `docs/reference/web-service-1.0-spec.md` section 4.1 lists the fields
//! and what 2.0 keeps. Three differences from 1.0, each one stated there:
//!
//! - `served[].expected` is the pool row's `expected_answer` DOCUMENT
//!   ([`cadus_core::pool::PoolAnswer`]), not a bare string;
//! - 1.0's `served_texts` becomes [`WebState::task_memory`], the D5 per-task
//!   window of 12 statement HASHES — 2.0 has no prompt to feed, so it stores no
//!   statements;
//! - `rings` and `pending_diagnoses` are new: the D5 per-topic window of 20, and
//!   the map a reconnecting client re-reads its missed diagnoses from.
//!
//! # `served` is keyed by task id
//!
//! Section 4.1 gives the reason and the incident. One problem per task is
//! answerable, a re-serve overwrites the entry, and a stale `problem_id` is
//! `404 unknown_problem`. Keying by a fresh uuid per serve left superseded
//! problems answerable, and the FR-14 dedup could not catch the double record.
//!
//! # The snapshot / validate / commit shape
//!
//! Section 4.2: a handler takes the advisory lock, snapshots the row, does its
//! work, and re-validates before it commits, because a second browser tab is a
//! live race. [`WebState::validate`] is that re-check, and it has exactly two
//! refusals: a `done` task is `409 task_complete`, and a `problem_id` that is
//! not the task's current one is `404 unknown_problem`.

mod content_policy;

use std::collections::BTreeMap;

use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use cadus_core::config::Config;
use cadus_core::curriculum::{Curriculum, FiniteCaseRole};
use cadus_core::event::{Exposure, ItemSource};
use cadus_core::integrated::IntegratedSet;
use cadus_core::pool::{PoolAnswer, Ring, TaskMemory};
use cadus_core::readiness::ReadinessIndex;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use sqlx::types::Uuid;

use crate::error::ApiError;

/// The code of a write against a task that is already closed.
pub const TASK_COMPLETE: &str = "task_complete";

/// The code of an answer or a hint against a superseded `problem_id`.
pub const UNKNOWN_PROBLEM: &str = "unknown_problem";

/// The code of a request that names a course the curriculum does not hold.
pub const UNKNOWN_COURSE: &str = "unknown_course";

/// The code of a body that does not carry the fields the route needs.
pub const INVALID_REQUEST: &str = "invalid_request";

/// The code of a guarded route that carries no session credential.
pub const UNAUTHORIZED: &str = "unauthorized";

/// The code of a session route called with no session open.
pub const NO_OPEN_SESSION: &str = "no_open_session";

/// The code of a route that needs a curriculum the process did not load.
pub const CURRICULUM_UNAVAILABLE: &str = "curriculum_unavailable";

/// The code of a state document the process cannot read or write.
pub const STATE_UNAVAILABLE: &str = "state_unavailable";

/// The curriculum and the scheduler config the request tier composes with.
///
/// The binary loads the tree once at boot and shares it. It is read-only, so one
/// `Arc` serves every request and no handler re-reads a file (L1, R4).
#[derive(Debug)]
pub struct Content {
    /// The curriculum arena (D2).
    pub curriculum: Curriculum,
    /// The scheduler constants (M3).
    pub cfg: Config,
    /// The curriculum half of the readiness audit (D-F5).
    ///
    /// The build reads every authored exemplar answer once, so it stands here
    /// beside the arena and no request pays for it. A request adds the approved
    /// documents of `content_store` with
    /// [`ReadinessIndex::resolve`](cadus_core::readiness::ReadinessIndex::resolve),
    /// which is map lookups only.
    pub readiness: ReadinessIndex,
    /// The hand-authored integrated tasks (D-F10).
    ///
    /// The set is loaded beside the curriculum and shared read-only, the same
    /// way the arena is. A deployment with no authored file holds an empty set,
    /// and every multi-step task then keeps its per-component serve.
    pub integrated: IntegratedSet,
    /// Stable semantic fingerprint of every effective curriculum topic.
    curriculum_context_digest: Option<String>,
    /// Build-time fingerprint shared with the offline content gates.
    review_engine_digest: &'static str,
}

impl Content {
    /// Pair a loaded curriculum with the default scheduler config.
    #[must_use]
    pub fn new(curriculum: Curriculum) -> Self {
        Self::with_config(curriculum, Config::default())
    }

    /// Pair a loaded curriculum with the scheduler config the caller names.
    #[must_use]
    pub fn with_config(curriculum: Curriculum, cfg: Config) -> Self {
        let readiness = ReadinessIndex::build(&curriculum);
        let curriculum_context_digest =
            cadus_core::curriculum::review_context_digest(&curriculum).ok();
        Self {
            curriculum,
            cfg,
            readiness,
            integrated: IntegratedSet::empty(),
            curriculum_context_digest,
            review_engine_digest: cadus_core::review_engine::DIGEST,
        }
    }

    /// Attach the authored integrated set (D-F10).
    ///
    /// The boot path reads the set from the curriculum root once and hands it
    /// over here, so no request reads a file.
    #[must_use]
    pub fn with_integrated(mut self, integrated: IntegratedSet) -> Self {
        self.integrated = integrated;
        self
    }

    /// Effective curriculum semantics reviewed with authored content.
    pub fn curriculum_context_digest(&self) -> Result<&str, ApiError> {
        self.curriculum_context_digest.as_deref().ok_or_else(|| {
            ApiError::internal("The current curriculum review context could not be read.")
        })
    }

    /// Exact executable renderer/evaluator/gate source fingerprint.
    #[must_use]
    pub const fn review_engine_digest(&self) -> &'static str {
        self.review_engine_digest
    }

    /// Trusted content-review dimensions for one objective policy.
    pub fn review_context<'a>(
        &'a self,
        policy_digest: Option<&'a str>,
    ) -> Result<cadus_store::content::CurrentContext<'a>, ApiError> {
        Ok(cadus_store::content::CurrentContext {
            policy_digest,
            curriculum_digest: self.curriculum_context_digest()?,
            review_engine_digest: self.review_engine_digest(),
        })
    }
}

/// The tenant of the request.
///
/// Unit U6 runs beside unit U2, so the credential reader is not written yet.
/// This extractor is the seam between the two: the auth layer of U2 and U4 puts
/// a `Tenant` into the request extensions after it binds, and every guarded
/// handler takes it. A request with no `Tenant` is `401 unauthorized`, so the
/// failure mode is closed: a route that escapes the auth layer serves nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tenant(pub Uuid);

impl<S: Sync> FromRequestParts<S> for Tenant {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<Self>().copied().ok_or_else(|| {
            ApiError::new(
                StatusCode::UNAUTHORIZED,
                UNAUTHORIZED,
                "This route needs a session. Send the session cookie or a bearer token.",
            )
        })
    }
}

/// Server-owned exposure facts frozen when one ordinary problem is handed off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProblemHandoff {
    /// Digest of the rendered statement.
    pub item_digest: String,
    /// Authored source of the item.
    pub item_source: ItemSource,
    /// Immutable source document used for hint compatibility after bank changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_content_digest: Option<String>,
    /// Effective curriculum semantics that produced this problem.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_curriculum_digest: Option<String>,
    /// Compiled render/check semantics that produced this problem.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_review_engine_digest: Option<String>,
    /// Stable reviewed finite case id, when this KP owns a finite universe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finite_case_id: Option<String>,
    /// Curriculum-owned role of that finite case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finite_case_role: Option<FiniteCaseRole>,
    /// Exposure classification at hand-off.
    pub exposure: Exposure,
}

/// One problem that is live on the learner's screen (`state.py:43-60`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServedProblem {
    /// The id the client sends back with its answer.
    pub problem_id: String,
    /// The task this problem belongs to.
    pub task_id: String,
    /// The topic the attempt records against.
    #[serde(default)]
    pub topic: Option<String>,
    /// The topic the statement was drawn from.
    ///
    /// It differs from `topic` for a review that micro-interleaves a component
    /// skill: the review records its FIRe against the parent topic, and the
    /// statement comes from the component (`api.py:352-357`). The serving key of
    /// `serving_pool` and `content_store` is built from THIS topic, so the hint
    /// ladder and the pre-authored diagnosis name the knowledge point that
    /// produced the statement (M5 review 1, findings F10 and F16).
    ///
    /// A document written before this field reads back `None`. The two readers
    /// then fall back to `topic`, which is the behavior of that older document.
    #[serde(default)]
    pub serve_topic: Option<String>,
    /// The knowledge point of the problem, inside `serve_topic`.
    #[serde(default)]
    pub kp: Option<String>,
    /// The answer kind the checker reads.
    #[serde(default)]
    pub answer_kind: Option<String>,
    /// The rendered statement.
    pub text: String,
    /// The pool row's `expected_answer` document (section 4.1).
    pub expected: PoolAnswer,
    /// The authored solution, revealed after the attempt commits.
    #[serde(default)]
    pub solution_sketch: Option<String>,
    /// When the problem went on screen, in Unix seconds. It is re-stamped at
    /// every hand-off, a re-serve included (section 5.6).
    pub started_at: f64,
    /// A repeated hand-off invalidates a continuous solve-time claim.
    #[serde(default)]
    pub timing_interrupted: bool,
    /// The hints already handed out. A non-empty list makes the attempt
    /// reference-assisted (H3, section 5.4).
    #[serde(default)]
    pub hints_given: Vec<String>,
    /// The 0-based position of this problem inside its task.
    #[serde(default)]
    pub index: i64,
    /// The stashed assisted pass that waits for its unaided re-solve (H3).
    #[serde(default)]
    pub rework: Option<Json>,
    /// Item-level hand-off evidence. Historical D-S6 rows carry none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff: Option<ProblemHandoff>,
}

impl ServedProblem {
    /// The topic the serving key is built from.
    ///
    /// `serving_pool.kp_id` and `content_store.kp_id` both hold
    /// [`kp_key`](cadus_core::pool::kp_key) of THIS topic and of `kp`, so the
    /// hint ladder and the pre-authored diagnosis read the knowledge point that
    /// produced the statement. A document written before `serve_topic` gives
    /// `topic`.
    #[must_use]
    pub fn serving_topic(&self) -> Option<&str> {
        self.serve_topic.as_deref().or(self.topic.as_deref())
    }
}

/// How far one task has got (`state.py:63-73`). 2.0 keeps it verbatim.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskProgress {
    /// The task id.
    #[serde(default)]
    pub task_id: String,
    /// The task kind, in its wire spelling.
    #[serde(default)]
    pub task_type: String,
    /// How many problems the task serves.
    #[serde(default)]
    pub total: i64,
    /// How many problems went on screen.
    #[serde(default)]
    pub served: i64,
    /// How many problems the learner answered.
    #[serde(default)]
    pub answered: i64,
    /// Whether the task is closed. It is the load-bearing field of the plan
    /// (section 9, trap W3).
    #[serde(default)]
    pub done: bool,
    /// The knowledge point a lesson stands at.
    #[serde(default)]
    pub current_kp: Option<String>,
}

/// The buffered answers of a quiz. The count and the completion live on the
/// [`TaskProgress`] (`state.py:88-105`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuizBuffer {
    /// Whether the post-reveal independent-practice queue was started.
    #[serde(default)]
    pub practice_started: bool,
    /// The answers, in serve order.
    #[serde(default)]
    pub answers: Vec<Json>,
    /// When the WHOLE quiz started, in Unix seconds — the same time base as
    /// [`ServedProblem::started_at`].
    ///
    /// QUIZ-budget times the quiz, not the screen. The first serve of the task
    /// stamps this field, and no later serve rewrites it, so the client reads
    /// one clock across a re-mount AND across a page reload (M6-review-2, V6).
    /// `None` is an open quiz written before this field, and it stamps the
    /// clock at the next serve.
    #[serde(default)]
    pub started_at: Option<f64>,
}

/// The pre-generated parts of a multi-step task (`state.py:118-125`). In 2.0 the
/// parts come from the serving pool, never from a model (T1).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MultistepBuffer {
    /// The parts, in serve order.
    #[serde(default)]
    pub parts: Vec<Json>,
}

mod review_task;
pub use review_task::RecordedReview;

/// The D-S6 document: one JSONB row per learner in `web_states`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebState {
    /// Original server-selected reviews, retained until this session ends.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub review_tasks: BTreeMap<String, RecordedReview>,
    /// Approved instruction shown before a whole-item application, keyed by task.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub integrated_instruction: BTreeMap<String, String>,
    /// The session this scratch belongs to. A drift resets the document.
    #[serde(default)]
    pub session: Option<String>,
    /// The live problem of each task, keyed by TASK id.
    #[serde(default)]
    pub served: BTreeMap<String, ServedProblem>,
    /// The progress of each task, keyed by task id.
    #[serde(default)]
    pub tasks: BTreeMap<String, TaskProgress>,
    /// The answer buffer of each quiz, keyed by task id.
    #[serde(default)]
    pub quizzes: BTreeMap<String, QuizBuffer>,
    /// The pre-generated parts of each multi-step task, keyed by task id.
    #[serde(default)]
    pub multistep: BTreeMap<String, MultistepBuffer>,
    /// The D5 per-task window of 12 statement hashes, keyed by task id. It is
    /// 1.0's `served_texts`, with hashes in place of statements.
    #[serde(default)]
    pub task_memory: BTreeMap<String, TaskMemory>,
    /// The D5 per-topic window of 20 instance hashes, keyed by topic id.
    #[serde(default)]
    pub rings: BTreeMap<String, Ring>,
    /// `attempt_id` to `diagnosis_jobs.id`, so a reconnecting client re-reads
    /// what it missed.
    #[serde(default)]
    pub pending_diagnoses: BTreeMap<String, String>,
    /// Fresh same-skill practice owed after feedback, keyed by task.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub feedback_practice: BTreeMap<String, Json>,
    /// Whole integrated-item clocks keyed by the server item identity.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub integrated_timing: BTreeMap<String, crate::integrated::TimingWindow>,
    /// The server-measured active time of the session, in seconds.
    #[serde(default)]
    pub active_secs: f64,
}

/// Why [`WebState::validate`] refused (section 4.2, `api.py:1303-1318`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidateError {
    /// The task is closed. A second tab that answers it gets this.
    TaskComplete,
    /// The `problem_id` is not the task's current one, or the task serves none.
    UnknownProblem,
}

impl From<ValidateError> for ApiError {
    fn from(error: ValidateError) -> Self {
        match error {
            ValidateError::TaskComplete => Self::new(
                StatusCode::CONFLICT,
                TASK_COMPLETE,
                "This task is already complete.",
            ),
            ValidateError::UnknownProblem => Self::new(
                StatusCode::NOT_FOUND,
                UNKNOWN_PROBLEM,
                "This problem is not the one the task serves now.",
            ),
        }
    }
}

impl WebState {
    /// An empty document bound to `session`.
    #[must_use]
    pub fn for_session(session: &str) -> Self {
        Self {
            session: Some(session.to_string()),
            ..Self::default()
        }
    }

    /// Read the document out of the `web_states.doc` column.
    ///
    /// # Errors
    ///
    /// Returns the `serde_json` message when the column is not this document.
    pub fn from_doc(doc: &Json) -> Result<Self, String> {
        serde_json::from_value(doc.clone()).map_err(|err| err.to_string())
    }

    /// Write the document for the `web_states.doc` column. Every field is a
    /// plain value, so the conversion always succeeds.
    #[must_use]
    pub fn to_doc(&self) -> Json {
        serde_json::json!(self)
    }

    /// Bind the document to `session`, and reset it when the session drifted.
    ///
    /// The answer is `true` when the reset happened. 1.0 resets on drift because
    /// the scratch of a closed session names tasks that no longer exist
    /// (`state.py:142-166`).
    pub fn bind(&mut self, session: &str) -> bool {
        if self.session.as_deref() == Some(session) {
            return false;
        }
        *self = Self::for_session(session);
        true
    }

    /// The re-check of section 4.2, in its two refusals.
    ///
    /// # Errors
    ///
    /// Returns [`ValidateError::TaskComplete`] when the task is closed, and
    /// [`ValidateError::UnknownProblem`] when `problem_id` is not the live one.
    pub fn validate(
        &self,
        task_id: &str,
        problem_id: &str,
    ) -> Result<&ServedProblem, ValidateError> {
        if self.tasks.get(task_id).is_some_and(|task| task.done) {
            return Err(ValidateError::TaskComplete);
        }
        match self.served.get(task_id) {
            Some(served) if served.problem_id == problem_id => Ok(served),
            _ => Err(ValidateError::UnknownProblem),
        }
    }

    /// How the plan reports this task, as a pure lookup (`_plan_progress`).
    ///
    /// It NEVER installs a row. Trap W3: 1.0's `_progress_for` writes a
    /// `TaskProgress` for every task merely listed, and listing the plan must
    /// stay a pure read.
    #[must_use]
    pub fn plan_progress(&self, task_id: &str) -> (i64, bool) {
        match self.tasks.get(task_id) {
            Some(task) => (task.answered, task.done),
            None => (0, false),
        }
    }

    /// The D5 anti-repeat ring of one topic. An absent topic gives an empty ring.
    #[must_use]
    pub fn ring(&self, topic_id: &str) -> Ring {
        self.rings.get(topic_id).cloned().unwrap_or_default()
    }

    /// The D5 per-task memory of one task. An absent task gives an empty memory.
    #[must_use]
    pub fn memory(&self, task_id: &str) -> TaskMemory {
        self.task_memory.get(task_id).cloned().unwrap_or_default()
    }

    /// Start the whole-quiz clock of one task, and give the stamp back.
    ///
    /// The FIRST call of a task stamps `now`; every later call answers with that
    /// first stamp, so the clock runs on and a reload resumes it. It is the one
    /// write point of [`QuizBuffer::started_at`] (M6-review-2, V6).
    pub fn start_quiz_clock(&mut self, task_id: &str, now: f64) -> f64 {
        *self
            .quizzes
            .entry(task_id.to_string())
            .or_default()
            .started_at
            .get_or_insert(now)
    }

    /// Record one served instance hash in BOTH D5 windows (M4, section 4.1).
    pub fn record_served(&mut self, topic_id: &str, task_id: &str, instance_hash: &str) {
        self.rings
            .entry(topic_id.to_string())
            .or_default()
            .push(instance_hash);
        self.task_memory
            .entry(task_id.to_string())
            .or_default()
            .push(instance_hash);
    }
}
