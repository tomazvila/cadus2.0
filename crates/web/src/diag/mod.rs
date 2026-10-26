//! The placement diagnostic: `POST /api/diag/start`, `/api/diag/answer` and
//! `/api/diag/finish` (`api.py:1896-2091`, spec section 2).
//!
//! The three rows sat in the spec table marked `keep` while no unit owned them:
//! M5 built no route, and the M6 placement screen shipped against
//! `SPEC_ROUTES_ABSENT`. This module closes that gap. The pedagogy is
//! `cadus_core::diagnostic`; the work here is the transport, the scratch and the
//! two events.
//!
//! THE VERDICT IS DETERMINISTIC AND THIS ROUTE FAMILY CALLS NO MODEL (R4, L6,
//! spec section 2 "deterministic-only verdict"). `start` therefore drops from the
//! probe set each topic whose probe gives no verdict under
//! `Exemplar::verdict_policy`, so the diagnostic never asks a question it cannot
//! mark. Readiness, the pool and the lint use the same rule. 1.0 sent such a probe to its
//! grading engine; this service asks no model on a request path, so the question
//! is not asked at all.
//!
//! ONE TRANSACTION PER CALL. 1.0 splits `answer` into three phases, because it
//! held the tenant lock across a model round trip. The verdict here is a local
//! `check`, so the scratch save, the fold and the appended event stay in one
//! transaction. A crash then leaves neither a folded balance with no event nor an
//! event with no balance (1.0 finding F-10-1).
//!
//! THE SENTINEL TASK ID. The diagnostic is a single-track task: the served probe
//! lives at `served["diag"]` of the D-S6 document, so a new probe replaces the
//! one before it and a stale `problem_id` answers `404 unknown_problem`.

use axum::http::StatusCode;
use cadus_core::answer::AnswerContract;
use cadus_core::config::Config;
use cadus_core::curriculum::model::Exemplar;
use cadus_core::curriculum::{AnswerKind, Curriculum, Topic};
use cadus_core::diagnostic::{self, DiagState, Next};
use cadus_core::event::Timestamp;
use cadus_core::pool::PoolAnswer;
use cadus_store::state::{load_diag_state, save_diag_state};
use serde_json::{Value, json};
use sqlx::types::Uuid;

use crate::AppState;
use crate::error::ApiError;
use crate::serve::unix_seconds;
use crate::session::{Tx, store, write_state};
use crate::state::{Content, ServedProblem, WebState};

mod overturn;
mod policy_health;
mod routes;

pub use policy_health::{live_policy_map, stale_policy_rows};

pub use overturn::overturns;

pub use routes::{answer, current, finish, start};

/// The code of a diagnostic call made with no diagnostic open.
pub const NO_DIAGNOSTIC: &str = "no_diagnostic";

/// The code of a start with no course to diagnose.
pub const NO_COURSE: &str = "no_course";

/// The task id the served probe hangs on.
const DIAG_TASK_ID: &str = "diag";

/// The text a topic with no exemplar shows. It matches 1.0 character for
/// character, because the placement screen renders whatever it is given.
const NO_EXEMPLAR: &str = "(no exemplar)";

/// The topic record of `topic`, or `None` when the arena does not hold it.
fn topic_record<'a>(graph: &'a Curriculum, topic: &str) -> Option<&'a Topic> {
    graph.idx_of(topic).and_then(|idx| graph.topic(idx))
}

/// Whether the probe of `record` gives a verdict. An explicit supported
/// contract makes a multi-step or `proof` final answer markable. A `proof` topic
/// with no contract is not markable.
fn markable(record: &Topic) -> bool {
    record
        .diagnostic_exemplar
        .as_ref()
        .is_some_and(|item| markable_item(record.answer_kind, item))
}

/// Whether the service can mark `item` of a topic with answer kind `kind`.
/// This is `Exemplar::verdict_policy`, the one rule of the start, readiness,
/// the pool and the lint.
fn markable_item(kind: AnswerKind, item: &Exemplar) -> bool {
    item.verdict_policy(kind).is_ok()
}

/// The exemplar of the second probe of a topic, or `None` when it has none.
///
/// It is the first exemplar, in author order across the knowledge points, that
/// passes four tests. Its text differs from the text of the diagnostic exemplar.
/// It shows no figure, because a probe has no figure. Its contract is not a
/// Label, because a label answer is not pass evidence (owner rule, 2026-10-07).
/// It passes the same marking rule as [`markable`].
fn second_exemplar(record: &Topic) -> Option<&Exemplar> {
    let first = record
        .diagnostic_exemplar
        .as_ref()
        .map(|item| item.problem.trim());
    record
        .knowledge_points
        .iter()
        .flat_map(|kp| kp.exemplars.iter())
        .find(|item| {
            Some(item.problem.trim()) != first
                && item.visual.is_none()
                && !matches!(item.answer_contract, Some(AnswerContract::Label { .. }))
                && markable_item(record.answer_kind, item)
        })
}

/// The `409 no_diagnostic` of a call with no diagnostic in progress.
fn no_diagnostic() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        NO_DIAGNOSTIC,
        "No diagnostic is in progress; POST /api/diag/start.",
    )
}

/// The live problem of one probe of `topic`, dealt at `started_at`.
fn probe_problem(
    record: &Topic,
    exemplar: Option<&Exemplar>,
    topic: &str,
    index: usize,
    started_at: f64,
) -> (ServedProblem, Value) {
    let problem_id = Uuid::new_v4().simple().to_string();
    let text = exemplar.map_or_else(|| NO_EXEMPLAR.to_owned(), |item| item.problem.clone());
    let served = ServedProblem {
        timing_interrupted: false,
        problem_id,
        task_id: DIAG_TASK_ID.to_owned(),
        topic: Some(topic.to_owned()),
        serve_topic: Some(topic.to_owned()),
        kp: None,
        answer_kind: Some(record.answer_kind.as_str().to_owned()),
        text,
        expected: PoolAnswer {
            answer_contract: exemplar.and_then(|item| item.answer_contract.clone()),
            v: cadus_core::pool::POOL_ROW_VERSION,
            answer: exemplar.map(|item| item.answer.clone()).unwrap_or_default(),
        },
        solution_sketch: exemplar.and_then(|item| item.solution_sketch.clone()),
        started_at,
        hints_given: Vec::new(),
        index: index as i64,
        rework: None,
        handoff: None,
    };
    let probe = probe_view(&served);
    (served, probe)
}

/// The `probe` object of one live problem: the topic, the id and the text, and
/// the choices and the contract hint when the problem has them. It shows no
/// answer and no sketch.
pub(super) fn probe_view(served: &ServedProblem) -> Value {
    let topic = served.topic.as_deref().unwrap_or_default();
    let mut probe = json!({
        "topic": topic,
        "problem_id": served.problem_id,
        "text": served.text,
    });
    // A top-level Label contract shows its options; each other probe keeps the
    // three keys of today. The order comes from `problem_id` only (Hard Rule 1).
    let contract = served.expected.answer_contract.as_ref();
    if let Some(choices) = crate::serve::choices::label_choices(contract, &served.problem_id) {
        probe["choices"] = json!(choices);
    }
    if let Some(hint) = crate::serve::choices::contract_hint(contract) {
        probe["answer_contract"] = hint;
    }
    probe
}

/// Serve the next probe, storing it as the live problem.
///
/// The return value is the `probe` object of the spec, or `None` when the probe
/// list is exhausted. A second probe uses [`second_exemplar`]. The caller answers `{"probe": null}` or `{"done": true}`
/// from that, which the screen reads. `next_probe` deals a topic of the arena
/// only, so the record lookup finds it.
fn serve_probe(
    diag: &mut DiagState,
    graph: &Curriculum,
    cfg: &Config,
    scratch: &mut WebState,
    started_at: f64,
) -> Option<Value> {
    let has_second =
        |topic: &str| topic_record(graph, topic).is_some_and(|r| second_exemplar(r).is_some());
    let next = diagnostic::next_question(diag, graph, cfg, has_second)?;
    let (topic, second) = match next {
        Next::First(topic) => (topic, false),
        Next::Second(topic) => (topic, true),
    };
    topic_record(graph, &topic).map(|record| {
        let exemplar = if second {
            second_exemplar(record)
        } else {
            record.diagnostic_exemplar.as_ref()
        };
        let (served, probe) =
            probe_problem(record, exemplar, &topic, diag.question_count(), started_at);
        scratch.served.insert(DIAG_TASK_ID.to_owned(), served);
        probe
    })
}

/// Drop the live probe and deal the next one at `now`.
fn deal_probe(
    diag: &mut DiagState,
    content: &Content,
    scratch: &mut WebState,
    now: Timestamp,
) -> Option<Value> {
    scratch.served.remove(DIAG_TASK_ID);
    let started_at = unix_seconds(now.micros());
    serve_probe(diag, &content.curriculum, &content.cfg, scratch, started_at)
}

/// Read the open diagnostic, or answer `409 no_diagnostic`.
fn open_diagnostic(doc: Option<Value>) -> Result<DiagState, ApiError> {
    let Some(doc) = doc else {
        return Err(no_diagnostic());
    };
    serde_json::from_value(doc).map_err(unreadable_diagnostic)
}

/// The `409 no_diagnostic` of a diagnostic document that did not read.
fn unreadable_diagnostic(err: serde_json::Error) -> ApiError {
    tracing::error!(error = %err, "cadus-web: the diagnostic document did not read");
    no_diagnostic()
}

/// Load the open diagnostic of this tenant, inside the caller's transaction.
async fn load_diagnostic(
    state: &AppState,
    tx: &mut Tx,
    user_id: Uuid,
) -> Result<DiagState, ApiError> {
    let doc = store(state, load_diag_state(tx, user_id)).await?;
    open_diagnostic(doc)
}

/// Write the diagnostic document and the D-S6 document that holds its probe.
/// Every field of the diagnostic is a plain value, so its JSON always writes.
async fn save_diagnostic(
    state: &AppState,
    tx: &mut Tx,
    user_id: Uuid,
    diag: &DiagState,
    scratch: &WebState,
) -> Result<(), ApiError> {
    store(state, save_diag_state(tx, user_id, &json!(diag))).await?;
    write_state(&state.db, tx, user_id, scratch).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `markable` result for a topic of `kind` whose diagnostic exemplar has
    /// the key `answer`. `None` = the test record is not a topic.
    fn marks(kind: &str, answer: &str, contract: Option<&Value>) -> Option<bool> {
        let record = json!({
            "id": "t", "name": "T", "difficulty": 0.5, "answer_kind": kind,
            "expected_time_secs": 60,
            "diagnostic_exemplar": {
                "problem": "Which step has the error?",
                "answer": answer,
                "answer_contract": contract,
            },
        });
        let topic: Option<Topic> = serde_json::from_value(record).ok();
        topic.as_ref().map(markable)
    }

    /// The contract decides before the topic kind: a `proof` topic is markable
    /// only with a contract that accepts its key.
    #[test]
    fn a_proof_topic_is_markable_only_with_a_valid_contract() {
        let steps = json!({ "kind": "label", "options": [["Step 1"], ["Step 2"], ["Step 3"]] });
        assert_eq!(marks("proof", "Step 3", Some(&steps)), Some(true));
        assert_eq!(marks("proof", "Step 9", Some(&steps)), Some(false));
        assert_eq!(marks("proof", "Step 3", None), Some(false));
        assert_eq!(marks("numeric", "7", None), Some(true));
    }

    /// `markable` equals `verdict_policy(..).is_ok()` for 14 shapes of kind,
    /// contract and key.
    #[test]
    fn markable_agrees_with_verdict_policy_on_fourteen_shapes() {
        let exact = json!({ "kind": "exact" });
        let none = json!({ "kind": "none" });
        let written = json!({ "kind": "written" });
        let label = json!({ "kind": "label", "options": [["A"], ["B"]] });
        let shapes: [(&str, &str, Option<&Value>); 14] = [
            ("numeric", "7", None),
            ("numeric", "(((", None),
            ("expression", "x+1", None),
            ("expression", "x+", None),
            ("multi-step", "7", None),
            ("proof", "7", None),
            ("proof", "3", Some(&exact)),
            ("multi-step", "3", Some(&exact)),
            ("numeric", "7", Some(&none)),
            ("proof", "7", Some(&written)),
            ("numeric", "7", Some(&written)),
            ("proof", "A", Some(&label)),
            ("proof", "Z", Some(&label)),
            ("numeric", "(((", Some(&exact)),
        ];
        for (kind, key, contract) in shapes {
            let record = json!({
                "id": "t", "name": "T", "difficulty": 0.5, "answer_kind": kind,
                "expected_time_secs": 60,
                "diagnostic_exemplar": {
                    "problem": "Find it.", "answer": key, "answer_contract": contract,
                },
            });
            let topic: Topic = serde_json::from_value(record).unwrap();
            let probe = topic.diagnostic_exemplar.as_ref().unwrap();
            assert_eq!(
                markable(&topic),
                probe.verdict_policy(topic.answer_kind).is_ok(),
                "{kind} {key} {contract:?}"
            );
        }
    }

    /// A numeric topic with no contract and an unreadable key is not markable.
    #[test]
    fn a_numeric_topic_with_an_unreadable_key_is_not_markable() {
        assert_eq!(marks("numeric", "(((", None), Some(false));
        assert_eq!(marks("numeric", "7", None), Some(true));
    }

    /// The probe object of a topic whose diagnostic exemplar has `contract`.
    fn probe_of(contract: Option<&Value>) -> Value {
        let record = json!({
            "id": "t", "name": "T", "difficulty": 0.5, "answer_kind": "proof",
            "expected_time_secs": 60,
            "diagnostic_exemplar": {
                "problem": "Which step is the first step that is not valid?",
                "answer": "Step 3",
                "answer_contract": contract,
            },
        });
        let topic: Topic = serde_json::from_value(record).unwrap();
        probe_problem(&topic, topic.diagnostic_exemplar.as_ref(), "t", 0, 0.0).1
    }

    /// A Label probe carries `choices` from its `problem_id`; a probe with no
    /// contract keeps the three keys of today.
    #[test]
    fn a_label_probe_carries_the_choices_and_each_other_probe_has_no_key() {
        let steps =
            json!({ "kind": "label", "options": [["Step 1", "1"], ["Step 2"], ["Step 3"]] });
        let probe = probe_of(Some(&steps));
        let contract = serde_json::from_value(steps).unwrap();
        let problem_id = probe["problem_id"].as_str().unwrap();
        let expected = crate::serve::choices::label_choices(Some(&contract), problem_id);
        assert_eq!(probe["choices"], json!(expected.unwrap()));
        assert_eq!(probe["choices"].as_array().unwrap().len(), 3);

        let plain = probe_of(None);
        assert_eq!(plain.as_object().unwrap().len(), 3, "{plain}");
        assert!(plain.get("choices").is_none());

        // A topic with no diagnostic exemplar gives the fixed text and no key.
        let record = json!({
            "id": "t", "name": "T", "difficulty": 0.5, "answer_kind": "numeric",
            "expected_time_secs": 60,
        });
        let bare: Topic = serde_json::from_value(record).unwrap();
        let (served, probe) = probe_problem(&bare, bare.diagnostic_exemplar.as_ref(), "t", 2, 0.0);
        assert_eq!(probe["text"], NO_EXEMPLAR);
        assert_eq!(served.index, 2);
        assert!(probe.get("choices").is_none());
    }
    /// A numeric topic whose diagnostic exemplar says "first" and whose one
    /// knowledge point holds `exemplars`.
    fn with_exemplars(exemplars: &Value) -> Topic {
        let record = json!({
            "id": "t", "name": "T", "difficulty": 0.5, "answer_kind": "numeric",
            "expected_time_secs": 60,
            "diagnostic_exemplar": { "problem": "first", "answer": "7" },
            "knowledge_points": [{ "id": "kp1", "name": "kp1", "exemplars": exemplars }],
        });
        serde_json::from_value(record).unwrap()
    }

    /// The second exemplar is the first one in author order that has another
    /// text, no figure, no Label contract, and a mark rule that accepts it.
    #[test]
    fn the_second_exemplar_skips_each_unfit_item() {
        let label = json!({ "kind": "label", "options": [["A"], ["B"]] });
        let bad = json!({ "kind": "exact" });
        let topic = with_exemplars(&json!([
            { "problem": " first ", "answer": "7" },
            { "problem": "has a figure", "answer": "7", "visual": 0 },
            { "problem": "is a label", "answer": "A", "answer_contract": label },
            { "problem": "cannot be marked", "answer": "(", "answer_contract": bad },
            { "problem": "fits", "answer": "7", "answer_contract": { "kind": "exact" } },
            { "problem": "fits later", "answer": "7" },
        ]));
        let second = second_exemplar(&topic);
        assert_eq!(second.map(|item| item.problem.as_str()), Some("fits"));

        let none = with_exemplars(&json!([
            { "problem": "first", "answer": "7" },
            { "problem": "has a figure", "answer": "7", "visual": 0 },
        ]));
        assert!(second_exemplar(&none).is_none());
        assert!(second_exemplar(&with_exemplars(&json!([]))).is_none());
    }
}
