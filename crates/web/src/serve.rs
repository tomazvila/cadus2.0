//! The serve, teach, and hint routes of unit U7.
//!
//! Requirements: A6 (a pool miss falls back to authored exemplars and raises the
//! operator flag; it never generates), D5 (the anti-repeat ring and the per-task
//! memory), D-O1 (one transaction on the serve path), D-O3 (one `content_store`
//! read on the teach and hint paths), D-S6, L4, L5, R4, T1 (these three paths
//! spend no model token).
//!
//! Spec: `docs/reference/web-service-1.0-spec.md` section 2 (the route table),
//! section 4.1 (`served` is keyed by task id), section 5.6 (`started_at`), and
//! section 11 row U7; `docs/reference/serving-1.0-spec.md` sections 6 and 7.2.
//!
//! # Hard Rule 1
//!
//! A serve, a teach page, and a hint NEVER carry `expected` or the solution
//! sketch. Both documents stay in the D-S6 row, which the client cannot read,
//! and the payload builders below name every field they emit. Trap W7 gives the
//! test rule: scan the raw JSON, do not read the fields.
//!
//! # One transaction, and the clock
//!
//! There is no model call (T1, R4), so the whole serve is one transaction
//! (`serving-1.0-spec.md` section 7.2): the advisory lock, the log read, the
//! projection, the pool pop, the authored-solution read of a template row, the
//! state write, commit. `started_at` is re-stamped
//! at EVERY hand-off, a re-serve of a live problem included, because that is the
//! moment the problem goes on screen (section 5.6). A re-serve therefore returns
//! the SAME `problem_id` with a FRESH `started_at`.
//!
//! # The A6 fallback
//!
//! An empty pool must not generate. The handler instantiates the knowledge
//! point's authored exemplars in process (pure CPU over the arena), writes them
//! into the pool with `source = 'exemplar'`, and pops again. A pair whose whole
//! exemplar list is already claimed rotates through
//! [`reclaim_exemplar_tx`], which re-stamps `claimed_at` and so keeps
//! `last_exemplar_at` of the A6 operator view current.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use cadus_core::curriculum::{Curriculum, KnowledgePoint};
use cadus_core::event::{TaskType, Timestamp};
use cadus_core::pool::{Avoid, ExemplarSource, ProblemSource, Ring, Source, TaskMemory, kp_key};
use cadus_core::selector::{SessionPlan, Task};
use cadus_core::template::{Bindings, Value as Binding, from_body, literal_to_rational, render};
use cadus_store::content::{KIND_HINT_LADDER, KIND_TEACH, approved_document};
use cadus_store::pool::{
    NewInstance, PoolRow, approved_template, insert_batch, pop_with_ring_tx, reclaim_exemplar_tx,
};
use cadus_store::state::{EventRow, load_events, lock_web_state, project_current};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::{Postgres, Transaction};

use crate::AppState;
use crate::error::ApiError;
use crate::session::{
    INTERNAL_ERROR, begin, bound, compose_plan, content, current_session, failed, now_pair,
    projection_input, read_state, write_state,
};
use crate::state::Content;
use crate::state::{
    INVALID_REQUEST, NO_OPEN_SESSION, ServedProblem, TASK_COMPLETE, TaskProgress, Tenant, WebState,
};

/// The code of a task id that is not in this session's plan.
pub const UNKNOWN_TASK: &str = "unknown_task";

/// The code of a multi-step task whose parts are all served (`api.py:449`).
pub const MULTISTEP_EXHAUSTED: &str = "multistep_exhausted";

/// The code of a quiz whose questions are all served (`api.py:244`).
pub const QUIZ_EXHAUSTED: &str = "quiz_exhausted";

/// The code of a teach request that has no worked example to give.
pub const NO_INSTRUCTION: &str = "no_instruction";

/// The code of a hint request inside a quiz (`api.py:1813`).
pub const NO_HINTS_IN_QUIZ: &str = "no_hints_in_quiz";

/// The code of a hint request whose knowledge point has no approved ladder.
///
/// New in 2.0. 1.0 always had a model to ask; L5 replaces it with authored
/// content, and M6 authors the ladders.
pub const NO_HINT_LADDER: &str = "no_hint_ladder";

/// The code of a serve whose knowledge point can produce no problem at all.
pub const POOL_UNAVAILABLE: &str = "pool_unavailable";

/// After this many hints, a review or a multi-step part points the learner back
/// at the reference lesson (`api.py:97`, `:1856-1859`).
pub const STUCK_HINT_THRESHOLD: usize = 3;

// --------------------------------------------------------------------------- //
// The authored documents (D-O3)
// --------------------------------------------------------------------------- //

/// The body of a `content_store` row of kind `teach` (L4).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeachDoc {
    /// The concept text.
    pub concept: String,
    /// The fully worked example.
    pub worked_example: WorkedExample,
}

/// The worked example of a teach page (`api.py:1096-1103`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkedExample {
    /// The example problem. It is self-contained: it never names the served
    /// practice problem and it never reveals its answer (Hard Rule 1).
    pub problem: String,
    /// The solution steps, in order.
    pub steps: Vec<String>,
}

/// The body of a `content_store` row of kind `hint_ladder` (L5).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HintLadder {
    /// The hints, from the widest to the narrowest. The authoring gate of M6
    /// refuses a ladder whose text holds the answer.
    pub hints: Vec<String>,
}

// --------------------------------------------------------------------------- //
// Pure helpers
// --------------------------------------------------------------------------- //

/// The Unix instant of `now`, in seconds, as the D-S6 document spells it.
pub(crate) fn unix_seconds(micros: i64) -> f64 {
    micros as f64 / 1_000_000.0
}

/// The index the next serve takes (`_next_serve_index`, `api.py:398-407`).
///
/// A quiz and a multi-step task index by ANSWERED count, so a mid-task reload
/// re-serves the current unanswered part instead of skipping ahead (R2).
/// Everything else indexes by served count.
fn next_serve_index(task_type: TaskType, progress: &TaskProgress) -> i64 {
    match task_type {
        TaskType::Quiz | TaskType::MultiStep => progress.answered,
        _ => progress.served,
    }
}

/// The first knowledge point of a topic (`_first_kp`, `api.py:227-228`).
fn first_kp(graph: &Curriculum, topic_id: &str) -> Option<String> {
    let idx = graph.idx_of(topic_id)?;
    let kp = graph.knowledge_points(idx).first()?;
    Some(kp.id.as_str().to_string())
}

/// The knowledge point a non-lesson serve of `topic_id` draws from.
///
/// 1.0 has no answer to give here: a review, a quiz, and a drill generate from
/// the topic's WHOLE exemplar bank and record `kp_id = None`
/// (`api.py:363`, `:379`). The 2.0 pool is keyed by `"<topic>/<kp>"`, so every
/// serve names a knowledge point. The rule is a rotation over the topic's
/// knowledge points by serve index: it covers the same bank across a task and it
/// is restart-safe, because the index lives in the D-S6 row.
fn rotating_kp(graph: &Curriculum, topic_id: &str, index: i64) -> Option<String> {
    let idx = graph.idx_of(topic_id)?;
    let kps = graph.knowledge_points(idx);
    if kps.is_empty() {
        return None;
    }
    let position = index.rem_euclid(kps.len() as i64) as usize;
    kps.get(position).map(|kp| kp.id.as_str().to_string())
}

/// The topic a quiz question comes from (`_quiz_topic_id`, `api.py:240-246`).
fn quiz_topic(entry: &str) -> &str {
    entry.strip_prefix("topic:").unwrap_or(entry)
}

/// Where the `index`-th problem of `task` comes from.
///
/// `record` is the topic the attempt records against, and `serve` is the topic
/// whose pool the statement is drawn from. They differ for a review that
/// micro-interleaves a component skill: the review's FIRe applies to the parent
/// topic and the component gets credit by encompassing propagation
/// (`api.py:352-357`).
struct Target {
    /// The topic the attempt records against.
    record: String,
    /// The topic the statement is drawn from.
    serve: String,
    /// The knowledge point of the statement.
    kp: String,
}

impl Target {
    /// The serving key of `serving_pool.kp_id` and `content_store.kp_id`.
    fn key(&self) -> String {
        kp_key(&self.serve, &self.kp)
    }
}

/// Resolve the target of the next serve, or the 409 that refuses it.
fn target_of(
    task: &Task,
    index: i64,
    progress: &TaskProgress,
    graph: &Curriculum,
) -> Result<Target, ApiError> {
    let position = usize::try_from(index).unwrap_or(usize::MAX);
    let (record, serve, kp) = match task.task_type {
        TaskType::MultiStep => {
            let Some(component) = task.component_topics.get(position) else {
                return Err(ApiError::new(
                    StatusCode::CONFLICT,
                    MULTISTEP_EXHAUSTED,
                    "This multi-step task has no further part to serve.",
                ));
            };
            let kp = kp_or_refuse(graph, component, index)?;
            (component.clone(), component.clone(), kp)
        }
        TaskType::Quiz => {
            let Some(entry) = task.mix.get(position) else {
                return Err(ApiError::new(
                    StatusCode::CONFLICT,
                    QUIZ_EXHAUSTED,
                    "This quiz has no further question to serve.",
                ));
            };
            let topic = quiz_topic(entry).to_string();
            let kp = kp_or_refuse(graph, &topic, index)?;
            (topic.clone(), topic, kp)
        }
        TaskType::Lesson => {
            let topic = topic_or_refuse(task)?;
            let kp = progress
                .current_kp
                .clone()
                .or_else(|| task.start_at_kp.clone())
                .or_else(|| first_kp(graph, &topic));
            let Some(kp) = kp else {
                return Err(no_problem(&topic));
            };
            (topic.clone(), topic, kp)
        }
        TaskType::Review | TaskType::Drill | TaskType::Diagnostic => {
            let topic = topic_or_refuse(task)?;
            // A review mix entry `component:<id>` moves the DRAW to the
            // component skill and leaves the RECORD on the parent.
            let serve = task
                .mix
                .get(position.checked_rem(task.mix.len()).unwrap_or(0))
                .and_then(|entry| entry.strip_prefix("component:"))
                .filter(|id| graph.idx_of(id).is_some())
                .map_or_else(|| topic.clone(), ToString::to_string);
            let kp = kp_or_refuse(graph, &serve, index)?;
            (topic, serve, kp)
        }
    };
    Ok(Target { record, serve, kp })
}

/// The task's own topic, or the 409 of a task that names none.
fn topic_or_refuse(task: &Task) -> Result<String, ApiError> {
    task.topic.clone().ok_or_else(|| {
        ApiError::new(
            StatusCode::CONFLICT,
            POOL_UNAVAILABLE,
            "This task names no topic to serve from.",
        )
    })
}

/// The rotating knowledge point of a topic, or the 409 of a topic with none.
fn kp_or_refuse(graph: &Curriculum, topic_id: &str, index: i64) -> Result<String, ApiError> {
    rotating_kp(graph, topic_id, index).ok_or_else(|| no_problem(topic_id))
}

/// `409 pool_unavailable`: this knowledge point can produce no problem.
fn no_problem(topic_id: &str) -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        POOL_UNAVAILABLE,
        format!("Topic {topic_id:?} has no problem to serve."),
    )
}

/// Install the progress row of a task when it has none
/// (`_progress_for`, `api.py:567-583`).
///
/// The serve path may install it. `GET /api/session/plan` may NOT: trap W3.
pub(crate) fn progress_for<'state>(
    scratch: &'state mut WebState,
    task: &Task,
    graph: &Curriculum,
) -> &'state mut TaskProgress {
    scratch
        .tasks
        .entry(task.task_id.clone())
        .or_insert_with(|| TaskProgress {
            task_id: task.task_id.clone(),
            task_type: task.task_type.as_str().to_string(),
            total: task.n_problems.unwrap_or_default(),
            served: 0,
            answered: 0,
            done: false,
            current_kp: match task.task_type {
                TaskType::Lesson => task
                    .start_at_kp
                    .clone()
                    .or_else(|| task.topic.as_deref().and_then(|id| first_kp(graph, id))),
                _ => None,
            },
        })
}

/// The client-safe serve payload (`_serve_payload`, `api.py:501-527`).
///
/// It names every field it emits, so `expected` and `solution_sketch` cannot
/// reach the client by accident (Hard Rule 1).
///
/// `total` is the TASK's `n_problems`, not the stored `TaskProgress.total`. A
/// lesson names no count and 1.0 answers `null` for it (`api.py:520`); the D-S6
/// row flattens the absent count to 0, so reading it back would tell the client
/// a lesson has zero problems.
fn serve_payload(
    served: &ServedProblem,
    total: Option<i64>,
    task_type: TaskType,
    graph: &Curriculum,
    drill_secs: i64,
) -> Value {
    let (time_budget_secs, countdown) = if task_type == TaskType::Drill {
        (Some(drill_secs), true)
    } else {
        let secs = served
            .topic
            .as_deref()
            .and_then(|id| graph.idx_of(id))
            .and_then(|idx| graph.topic(idx))
            .map(|topic| topic.expected_time_secs);
        (secs, false)
    };
    json!({
        "problem_id": served.problem_id,
        "index": served.index + 1,
        "total": total,
        "text": served.text,
        "kp": served.kp,
        "time_budget_secs": time_budget_secs,
        "countdown": countdown,
    })
}

// --------------------------------------------------------------------------- //
// The shared opening of all three routes
// --------------------------------------------------------------------------- //

/// What every one of the three routes reads before it does its own work.
pub(crate) struct Open {
    /// The tenant-bound transaction. The caller commits it or rolls it back.
    pub(crate) tx: Transaction<'static, Postgres>,
    /// The whole append-only log of the tenant, in `seq` order.
    pub(crate) events: Vec<EventRow>,
    /// The D-S6 document, bound to the open session.
    pub(crate) scratch: WebState,
    /// The plan of the open session.
    pub(crate) plan: SessionPlan,
}

/// Open the transaction, read the log and the state, and compose the plan.
///
/// `lock` takes the tenant's advisory lock first. A route that WRITES the D-S6
/// row takes it; the read-only teach route does not (section 4.2).
///
/// The whole read is inside ONE transaction, so the plan a route serves from and
/// the state row it validates against cannot disagree.
pub(crate) async fn open(
    state: &AppState,
    content: &Content,
    user_id: Uuid,
    now: Timestamp,
    lock: bool,
) -> Result<Open, ApiError> {
    let input = projection_input(content, now);
    let mut tx = begin(state, user_id).await?;
    if lock {
        bound(&state.db, lock_web_state(&mut tx, user_id))
            .await
            .map_err(|err| failed(&err))?;
    }
    let events = bound(&state.db, load_events(&mut tx, user_id))
        .await
        .map_err(|err| failed(&err))?;
    let Some(session) = current_session(&events) else {
        return Err(no_open_session());
    };
    let projection = bound(&state.db, project_current(&mut tx, user_id, &input))
        .await
        .map_err(|err| failed(&err))?;
    let mut scratch = read_state(&state.db, &mut tx, user_id).await?;
    scratch.bind(&session);
    let plan = compose_plan(content, &events, &projection.model, &session, now);
    Ok(Open {
        tx,
        events,
        scratch,
        plan,
    })
}

/// The task of this plan, or `404 unknown_task`.
pub(crate) fn find<'plan>(
    plan: &'plan SessionPlan,
    task_id: &str,
) -> Result<&'plan Task, ApiError> {
    plan.tasks
        .iter()
        .find(|task| task.task_id == task_id)
        .ok_or_else(|| unknown_task(task_id))
}

// --------------------------------------------------------------------------- //
// POST /api/task/{task_id}/serve
// --------------------------------------------------------------------------- //

/// Serve this task's problem, taking one from the D-S5 pool (`api.py:1014-1060`).
///
/// The whole route is ONE transaction. A problem that is already live is handed
/// straight back with a fresh `started_at` and the same `problem_id`.
pub async fn serve(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    Path(task_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let graph = &content.curriculum;
    let (_, now) = now_pair();
    let started_at = unix_seconds(now.micros());

    let Open {
        mut tx,
        mut scratch,
        plan,
        ..
    } = open(&state, content, user_id, now, true).await?;
    let task = find(&plan, &task_id)?;
    let task_type = task.task_type;
    let progress = progress_for(&mut scratch, task, graph).clone();
    if progress.done {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            TASK_COMPLETE,
            "This task is already complete.",
        ));
    }

    // A problem is already live for this task: a reload, or the problem the last
    // answer installed. Re-stamp the clock and hand the SAME one back
    // (`_serve_live`, section 5.6).
    let payload = if let Some(live) = scratch.served.get_mut(&task_id) {
        live.started_at = started_at;
        serve_payload(
            live,
            task.n_problems,
            task_type,
            graph,
            content.cfg.drill.target_secs,
        )
    } else {
        install_next(
            &state,
            content,
            &mut tx,
            user_id,
            task,
            &mut scratch,
            started_at,
        )
        .await?
    };
    write_state(&state.db, &mut tx, user_id, &scratch).await?;
    tx.commit().await.map_err(|err| failed(&err.into()))?;
    Ok(Json(payload))
}

/// Draw the next problem of `task`, install it in the D-S6 row, and give back
/// its client-safe payload.
///
/// It is the ONE place that installs a served problem. `serve` calls it when no
/// problem is live, and the grade path of unit U8 calls it after the attempt
/// commits, so the `next` of a grade reply and a later serve cannot disagree.
///
/// The caller owns the transaction: this writes into `scratch` and into the
/// pool, and it commits nothing.
pub(crate) async fn install_next(
    state: &AppState,
    content: &Content,
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    task: &Task,
    scratch: &mut WebState,
    started_at: f64,
) -> Result<Value, ApiError> {
    let graph = &content.curriculum;
    let task_id = task.task_id.clone();
    let task_type = task.task_type;
    let progress = progress_for(scratch, task, graph).clone();
    let index = next_serve_index(task_type, &progress);

    let target = target_of(task, index, &progress, graph)?;
    let key = target.key();
    let ring = scratch.ring(&target.serve);
    let memory = scratch.memory(&task_id);
    let row = draw(state, tx, user_id, graph, &target, &key, &ring, &memory).await?;
    let solution_sketch = solution_of(state, tx, graph, &target, &key, &row).await?;

    let served = ServedProblem {
        problem_id: Uuid::new_v4().simple().to_string(),
        task_id: task_id.clone(),
        topic: Some(target.record.clone()),
        serve_topic: Some(target.serve.clone()),
        kp: Some(target.kp.clone()),
        answer_kind: answer_kind_of(graph, &target.serve),
        text: row.problem.text.clone(),
        expected: row.expected_answer.clone(),
        solution_sketch,
        started_at,
        hints_given: Vec::new(),
        index,
        rework: None,
    };
    let payload = serve_payload(
        &served,
        task.n_problems,
        task_type,
        graph,
        content.cfg.drill.target_secs,
    );
    scratch.record_served(&target.serve, &task_id, &row.instance_hash);
    scratch.served.insert(task_id.clone(), served);
    if let Some(progress) = scratch.tasks.get_mut(&task_id) {
        progress.served = progress.served.saturating_add(1);
    }
    Ok(payload)
}

/// The answer kind the checker reads for a statement of this topic.
fn answer_kind_of(graph: &Curriculum, topic_id: &str) -> Option<String> {
    let idx = graph.idx_of(topic_id)?;
    let topic = graph.topic(idx)?;
    Some(topic.answer_kind.as_str().to_string())
}

// --------------------------------------------------------------------------- //
// The authored solution sketch (A4, D-M5-3)
// --------------------------------------------------------------------------- //

/// The authored worked solution of the drawn row, or `None`.
///
/// The grade reply of unit U8 reveals it after the attempt commits, and the
/// stock re-solve text of D-M5-3 tells the learner to study it, so a served
/// problem that carries no sketch makes that text point at nothing (M5 review 1,
/// findings F2 and F11).
///
/// The pool row names its own author. An `exemplar` row (A6) names the authored
/// exemplar of the knowledge point by its statement, which
/// [`ExemplarSource::fill`] copies verbatim. A `template` row names the
/// `content_store` document by `content_digest`, and that document's sketch is a
/// statement over the same parameters, so the row's own bindings render it.
///
/// The read costs one indexed `content_store` statement inside the transaction
/// the caller owns, and only for a template row. A `generator` row (A7) names no
/// author, so it takes the `None` branch.
///
/// `None` is the closed answer of every refusal: a row whose digest lost its
/// approval, a document that does not read, an author who wrote no sketch, and a
/// sketch that does not render. The reply then omits `solution`, and the learner
/// keeps the verdict and the expected answer.
async fn solution_of(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    graph: &Curriculum,
    target: &Target,
    key: &str,
    row: &PoolRow,
) -> Result<Option<String>, ApiError> {
    if row.source == Source::Exemplar {
        return Ok(exemplar_sketch(graph, target, &row.problem.text));
    }
    let Some(digest) = row.content_digest.as_deref() else {
        return Ok(None);
    };
    let found = bound(&state.db, approved_template(&mut **tx, key))
        .await
        .map_err(|err| failed(&err))?;
    let Some(found) = found else {
        return Ok(None);
    };
    // C6: the row was drawn from ONE digest, and the approved document may be a
    // later one. The sketch of a different document is not this problem's
    // solution.
    if found.digest != digest {
        return Ok(None);
    }
    Ok(template_sketch(&found.body, &row.problem.bindings, key))
}

/// The authored exemplar whose statement is `text`, and its sketch (A6).
fn exemplar_sketch(graph: &Curriculum, target: &Target, text: &str) -> Option<String> {
    let kp = authored_kp(graph, &target.serve, &target.kp)?;
    kp.exemplars
        .iter()
        .find(|exemplar| exemplar.problem == text)
        .and_then(|exemplar| exemplar.solution_sketch.clone())
}

/// Render the sketch of one template document against the bindings of one row.
fn template_sketch(body: &str, bindings: &BTreeMap<String, String>, key: &str) -> Option<String> {
    let doc = match from_body(body) {
        Ok(doc) => doc,
        Err(reason) => {
            tracing::warn!(
                kp_id = %key,
                error = %reason,
                "serve: the approved template did not read, so the serve carries no solution"
            );
            return None;
        }
    };
    let sketch = doc.solution_sketch?;
    let values: Bindings = bindings
        .iter()
        .map(|(name, text)| (name.clone(), binding_value(text)))
        .collect();
    match render(&sketch, &values) {
        Ok(text) => Some(text),
        Err(reason) => {
            tracing::warn!(
                kp_id = %key,
                error = %reason,
                "serve: the authored solution sketch did not render"
            );
            None
        }
    }
}

/// Read one canonical binding string back into the value the draw bound.
///
/// The pool row keeps every bound value as its canonical string (D6), and the
/// renderer takes values. The reader is the inverse of
/// [`cadus_core::template::Value::canonical_string`]: a spelling that names an
/// exact rational and carries a decimal point is the authored spelling of a
/// decimal domain; every other spelling that names a rational is a number; and a
/// spelling that names no rational is a text choice, such as a multiplication
/// sign. The three cases write the same string back and take the same brackets
/// as the draw took.
fn binding_value(canonical: &str) -> Binding {
    let Some(number) = literal_to_rational(canonical) else {
        return Binding::Text(canonical.to_string());
    };
    if canonical.contains('.') {
        return Binding::Spelled {
            text: canonical.to_string(),
            number,
        };
    }
    Binding::Num(number)
}

/// The authored knowledge point of one topic, or `None`.
fn authored_kp<'graph>(
    graph: &'graph Curriculum,
    topic_id: &str,
    kp_id: &str,
) -> Option<&'graph KnowledgePoint> {
    let idx = graph.idx_of(topic_id)?;
    let kp_idx = graph.kp_idx_of(idx, kp_id)?;
    graph.knowledge_point(idx, kp_idx)
}

/// Take one instance out of the pool, and fall back to the exemplars (A6).
///
/// The order is: pop; on a miss instantiate the authored exemplars in process
/// and write them into the pool; pop again; and if the whole authored list is
/// already claimed, rotate through it. NOTHING here calls a model (A6, T1).
#[allow(clippy::too_many_arguments)]
async fn draw(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    graph: &Curriculum,
    target: &Target,
    key: &str,
    ring: &Ring,
    memory: &TaskMemory,
) -> Result<PoolRow, ApiError> {
    let avoid = Avoid::new(ring, memory);
    let popped = bound(&state.db, pop_with_ring_tx(tx, user_id, key, &avoid))
        .await
        .map_err(|err| failed(&err))?;
    if let Some(claimed) = popped.claimed {
        return Ok(claimed.row);
    }

    let rows = exemplar_rows(graph, target, key)?;
    if !rows.is_empty() {
        bound(&state.db, insert_batch(&mut **tx, user_id, key, &rows))
            .await
            .map_err(|err| failed(&err))?;
        let refilled = bound(&state.db, pop_with_ring_tx(tx, user_id, key, &avoid))
            .await
            .map_err(|err| failed(&err))?;
        if let Some(claimed) = refilled.claimed {
            tracing::warn!(
                user_id = %user_id,
                kp_id = %key,
                rows = rows.len(),
                "serve: the pool was empty, so the A6 exemplar fallback filled it in process"
            );
            return Ok(claimed.row);
        }
    }

    let rotated = bound(&state.db, reclaim_exemplar_tx(tx, user_id, key, &avoid))
        .await
        .map_err(|err| failed(&err))?;
    match rotated {
        Some(row) => {
            tracing::warn!(
                user_id = %user_id,
                kp_id = %key,
                "serve: the A6 exemplar rotation served an instance again; this knowledge point \
                 needs an approved template"
            );
            Ok(row)
        }
        None => Err(no_problem(&target.serve)),
    }
}

/// Instantiate the authored exemplars of the target knowledge point (A6).
///
/// The whole list goes in at once and the D5 ring picks, instead of 1.0's bare
/// `pool[index % len]` (`serving-1.0-spec.md` section 6.1). An exemplar whose
/// answer the checker cannot decide is skipped by
/// [`ExemplarSource::fill`], so one broken exemplar never takes the knowledge
/// point off the air.
fn exemplar_rows(
    graph: &Curriculum,
    target: &Target,
    key: &str,
) -> Result<Vec<NewInstance>, ApiError> {
    let Some(kp) = authored_kp(graph, &target.serve, &target.kp) else {
        return Ok(Vec::new());
    };
    let source = ExemplarSource::new(key, &kp.exemplars);
    if source.is_empty() {
        return Ok(Vec::new());
    }
    // The seed changes nothing for an exemplar list: the rotation is author
    // order and no draw runs.
    match source.fill(key, source.len(), 0) {
        Ok(batch) => Ok(batch
            .instances()
            .iter()
            .map(|instance| NewInstance::from_instance(instance, Source::Exemplar, None, 0))
            .collect()),
        Err(reason) => {
            tracing::warn!(
                kp_id = %key,
                error = %reason,
                "serve: the A6 exemplar fallback built no instance"
            );
            Ok(Vec::new())
        }
    }
}

// --------------------------------------------------------------------------- //
// POST /api/task/{task_id}/teach
// --------------------------------------------------------------------------- //

/// The authored teach page of a lesson's current knowledge point (`api.py:1064-1103`).
///
/// Only a lesson teaches. Every other task type is `409 no_instruction`, and so
/// is a lesson whose knowledge point has no APPROVED teach page: in both cases
/// the server has no worked example to give, and 2.0 never asks a model for one
/// (L4, T1).
///
/// D-O3: one `content_store` read, no state write, and the transaction is read
/// only.
pub async fn teach(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    Path(task_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let graph = &content.curriculum;
    let (_, now) = now_pair();

    let Open {
        mut tx,
        scratch,
        plan,
        ..
    } = open(&state, content, user_id, now, false).await?;
    let task = find(&plan, &task_id)?;
    if task.task_type != TaskType::Lesson {
        return Err(no_instruction());
    }
    let topic = topic_or_refuse(task)?;
    let kp = scratch
        .tasks
        .get(&task_id)
        .and_then(|progress| progress.current_kp.clone())
        .or_else(|| task.start_at_kp.clone())
        .or_else(|| first_kp(graph, &topic))
        .ok_or_else(no_instruction)?;
    let key = kp_key(&topic, &kp);

    let doc = bound(&state.db, approved_document(&mut *tx, &key, KIND_TEACH))
        .await
        .map_err(|err| failed(&err))?;
    tx.rollback().await.map_err(|err| failed(&err.into()))?;

    let Some(doc) = doc else {
        return Err(no_instruction());
    };
    let page: TeachDoc = serde_json::from_value(doc.body).map_err(|reason| {
        tracing::error!(digest = %doc.digest, error = %reason, "teach: the authored page did not read");
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            INTERNAL_ERROR,
            "The authored teach page is not readable.",
        )
    })?;

    Ok(Json(json!({
        "kp": kp,
        "concept": page.concept,
        "worked_example": {
            "problem": page.worked_example.problem,
            "steps": page.worked_example.steps,
        },
    })))
}

// --------------------------------------------------------------------------- //
// POST /api/task/{task_id}/hint
// --------------------------------------------------------------------------- //

/// One rung of the authored hint ladder (`api.py:1800-1865`).
///
/// A quiz reveals nothing until the batch reveal, so a hint inside one is
/// `409 no_hints_in_quiz`. Every other task type is allowed a hint: taking one
/// is recorded on `served.hints_given`, which flags the attempt
/// reference-assisted (H3), and does not force a miss.
///
/// The ladder comes from `content_store` (L5, T1). The reply carries the hint
/// text and nothing else from the served problem: `expected` never leaves the
/// D-S6 row (Hard Rule 1, trap W7).
pub async fn hint(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    Path(task_id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let graph = &content.curriculum;
    let problem_id = body
        .as_ref()
        .and_then(|Json(value)| value.get("problem_id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if problem_id.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            INVALID_REQUEST,
            "hint requires a problem_id.",
        ));
    }
    let (_, now) = now_pair();

    let Open {
        mut tx,
        mut scratch,
        plan,
        ..
    } = open(&state, content, user_id, now, true).await?;
    let task = find(&plan, &task_id)?;
    let task_type = task.task_type;
    if task_type == TaskType::Quiz {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            NO_HINTS_IN_QUIZ,
            "Hints are not available during a quiz.",
        ));
    }
    // The section 4.2 re-check, in its two refusals: a closed task is
    // `409 task_complete` and a superseded id is `404 unknown_problem`.
    let served = scratch.validate(&task_id, &problem_id)?;
    // The ladder belongs to the knowledge point that produced the STATEMENT, so
    // the key comes from `serve_topic`. `topic` is the topic the attempt records
    // against, and for a review that micro-interleaves a component skill the two
    // name different knowledge points (M5 review 1, findings F10 and F16). The
    // reference-lesson escalation below still names the RECORD topic, which is
    // the lesson the review stands for.
    let serving = served.serving_topic().map(ToString::to_string);
    let (topic, kp) = (served.topic.clone(), served.kp.clone());
    let rung = served.hints_given.len();
    let Some(kp) = kp else {
        return Err(no_ladder());
    };
    let Some(topic) = topic else {
        return Err(no_ladder());
    };
    // `serving_topic` gives `topic` when the D-S6 row names no serve topic, so
    // the fallback here never runs and the key is always a real pair.
    let key = kp_key(serving.as_deref().unwrap_or(&topic), &kp);

    let doc = bound(
        &state.db,
        approved_document(&mut *tx, &key, KIND_HINT_LADDER),
    )
    .await
    .map_err(|err| failed(&err))?;
    let Some(doc) = doc else {
        return Err(no_ladder());
    };
    let ladder: HintLadder = serde_json::from_value(doc.body).map_err(|reason| {
        tracing::error!(digest = %doc.digest, error = %reason, "hint: the authored ladder did not read");
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            INTERNAL_ERROR,
            "The authored hint ladder is not readable.",
        )
    })?;
    // A ladder that runs out repeats its last rung. The learner keeps the
    // reference-lesson escalation below, and no model is asked for a new one.
    let Some(text) = ladder
        .hints
        .get(rung.min(ladder.hints.len().saturating_sub(1)))
    else {
        return Err(no_ladder());
    };
    let text = text.clone();

    let Some(served) = scratch.served.get_mut(&task_id) else {
        return Err(no_ladder());
    };
    served.hints_given.push(text.clone());
    let hint_number = served.hints_given.len();
    write_state(&state.db, &mut tx, user_id, &scratch).await?;
    tx.commit().await.map_err(|err| failed(&err.into()))?;

    let mut payload = json!({"hint": text, "hint_number": hint_number});
    // Stuck on a review, or on a multi-step part, after three hints: point the
    // learner at the reference lesson (`api.py:1856-1864`).
    if matches!(task_type, TaskType::Review | TaskType::MultiStep)
        && hint_number >= STUCK_HINT_THRESHOLD
    {
        let name = graph
            .idx_of(&topic)
            .and_then(|idx| graph.topic(idx))
            .map_or_else(|| topic.clone(), |found| found.name.clone());
        if let Some(map) = payload.as_object_mut() {
            map.insert(
                "reference_lesson".to_string(),
                json!({"topic": topic, "name": name}),
            );
        }
    }
    Ok(Json(payload))
}

// --------------------------------------------------------------------------- //
// Shared refusals
// --------------------------------------------------------------------------- //

/// `409 no_open_session`: these three routes all need an open session.
pub(crate) fn no_open_session() -> ApiError {
    ApiError::new(StatusCode::CONFLICT, NO_OPEN_SESSION, "No session is open.")
}

/// `404 unknown_task`: the id is not in this session's plan.
fn unknown_task(task_id: &str) -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        UNKNOWN_TASK,
        format!("No task {task_id:?} is in this session."),
    )
}

/// `409 no_instruction`: there is no worked example for this task.
fn no_instruction() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        NO_INSTRUCTION,
        "Only a lesson with an approved teach page has a worked example.",
    )
}

/// `409 no_hint_ladder`: this knowledge point has no approved ladder.
fn no_ladder() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        NO_HINT_LADDER,
        "This knowledge point has no approved hint ladder.",
    )
}
