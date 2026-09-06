//! The `diagnosis` field of one grade reply (spec section 2.1): the
//! pre-authored lookup, the replay path, and the enqueue.

use cadus_core::config::Config;
use cadus_core::curriculum::AnswerKind;
use cadus_core::pool::kp_key;
use cadus_store::diagnosis::{JobPayload, PAYLOAD_VERSION, enqueue};
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::{Postgres, Transaction};

use super::{KIND_DIAGNOSIS, STATUS_NOT_OFFERED, STATUS_PENDING, STATUS_READY, match_distractor};
use crate::AppState;
use crate::error::ApiError;
use crate::session::json_of;
use crate::session::{bound, failed};
use crate::state::{ServedProblem, WebState};

/// The submission the diagnosis is about.
///
/// It names THIS submission, never the stashed H3 attempt the log records: a
/// failed re-solve records the stashed correct answer with `correct` rewritten,
/// and the mistake to diagnose is the one the learner just made.
pub(crate) struct Miss<'a> {
    /// The `attempt_id` of the row the log holds. It is the enqueue's idempotency key.
    pub attempt_id: &'a str,
    /// The open session, for the T4 per-session knob of unit U10.
    pub session: Option<&'a str>,
    /// The task the attempt belongs to.
    pub task_id: &'a str,
    /// What the learner answered.
    pub answer: &'a str,
    /// The learner's shown work.
    pub work: Option<&'a str>,
    /// The deterministic verdict of THIS submission.
    pub correct: bool,
    /// Whether THIS submission got no deterministic verdict (D-F2, D-F4).
    pub ungraded: bool,
}

/// Everything one diagnosis decision reads about its own grade.
pub(crate) struct Pending<'a> {
    /// The scheduler config, for the section 5.3 vocabulary.
    pub cfg: &'a Config,
    /// The problem the learner answered.
    pub served: &'a ServedProblem,
    /// The answer grammar the checker read.
    pub kind: AnswerKind,
    /// The submission itself.
    pub miss: Miss<'a>,
    /// Whether this call may write. It is false on the replay path: a grade that
    /// appends nothing must leave no job row, so the replay looks the
    /// pre-authored answer up, reads the standing job id out of the D-S6
    /// document, and writes neither.
    pub write: bool,
}

/// Decide the `diagnosis` field of one grade reply (spec section 2.1).
pub(crate) async fn decide(
    state: &AppState,
    tx: &mut Transaction<'static, Postgres>,
    user_id: Uuid,
    scratch: &mut WebState,
    about: &Pending<'_>,
) -> Result<Value, ApiError> {
    let Pending {
        cfg,
        served,
        kind,
        miss,
        write,
    } = about;
    let (kind, write) = (*kind, *write);
    // D-F4: the model-assisted diagnosis runs on a decided MISS and on nothing
    // else. An ungraded attempt has no mistake to diagnose, because the checker
    // named no mistake, so no job is enqueued and no pre-authored answer is read.
    if miss.correct || miss.ungraded || miss.answer.trim().is_empty() {
        return Ok(json!({ "status": STATUS_NOT_OFFERED }));
    }

    // Spec section 6.2: the pre-authored path first. A hit writes no job row.
    //
    // The distractors belong to the knowledge point that produced the STATEMENT,
    // so the key comes from `serve_topic`. For a review that micro-interleaves a
    // component skill, `topic` names the parent the attempt records against and
    // the pair `(topic, kp)` names a knowledge point that never produced this
    // problem (M5 review 1, findings F10 and F16).
    if let (Some(topic), Some(point)) = (served.serving_topic(), served.kp.as_deref()) {
        let key = kp_key(topic, point);
        let doc = bound(
            &state.db,
            cadus_store::content::approved_document(&mut **tx, &key, KIND_DIAGNOSIS),
        )
        .await
        .map_err(|err| failed(&err))?;
        if let Some(hit) =
            doc.and_then(|doc| match_distractor(&doc.body, miss.answer, kind, &cfg.error_tags))
        {
            // T6, spec section 7: the pre-authored hit is the one diagnosis
            // result with NO job row, so the counter is the only record of it.
            state
                .metrics
                .count_diagnosis(crate::metrics::DIAGNOSIS_PREAUTHORED);
            return Ok(json!({
                "status": STATUS_READY,
                "error_tags": hit.error_tags,
                "prose": hit.prose,
            }));
        }
    }

    if !write {
        // The replay path. The job id of the first request stands in the D-S6
        // document, so a retried request names the same job and starts none.
        return Ok(match scratch.pending_diagnoses.get(miss.attempt_id) {
            Some(id) => json!({ "id": id, "status": STATUS_PENDING }),
            None => json!({ "status": STATUS_NOT_OFFERED }),
        });
    }

    let payload = JobPayload {
        v: PAYLOAD_VERSION,
        session: miss.session.map(str::to_string),
        task_id: miss.task_id.to_string(),
        topic: served.topic.clone().unwrap_or_default(),
        kp: served.kp.clone(),
        problem: served.text.clone(),
        expected: served.expected.answer.clone(),
        answer_kind: served.answer_kind.clone().unwrap_or_default(),
        given_answer: miss.answer.to_string(),
        work: miss.work.map(str::to_string),
    };
    // The payload is strings and one integer, so the write cannot refuse.
    let document = json_of(&payload);
    let id = bound(&state.db, enqueue(tx, user_id, miss.attempt_id, &document))
        .await
        .map_err(|err| failed(&err))?;
    scratch
        .pending_diagnoses
        .insert(miss.attempt_id.to_string(), id.to_string());
    Ok(json!({ "id": id.to_string(), "status": STATUS_PENDING }))
}
