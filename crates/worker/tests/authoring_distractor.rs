//! M6 R7 acceptance: the distractor authoring pass (A4, A2, C6, T3, T6).
//!
//! Spec: `docs/reference/authoring-and-spa-1.0-spec.md` section 2.2 and row R7
//! of section 7; `docs/reference/web-service-1.0-spec.md` sections 5.3 and 6.2.
//!
//! The first acceptance check of row R7 lands here, at the place the pipeline
//! runs it: an `error_tag` outside the vocabulary is dropped at the gate, so the
//! stored row never carries it
//! ([`an_error_tag_outside_the_vocabulary_never_reaches_the_stored_row`]).
//!
//! Every expected value is a LITERAL: a literal body, a literal digest, a
//! literal rejection sentence, a literal row count. Nothing is read back from
//! the code under test.
//!
//! The endpoint is a fake OpenAI-compatible server (`common::FakeModel`). No
//! test reaches a real provider.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::curriculum::{AnswerKind, Exemplar};
use cadus_core::template::{GateSpec, gate_body};
use cadus_store::test_support::TestDb;
use cadus_worker::authoring::job::{
    NO_ARGUMENTS, Outcome, authoring_vocabulary, verify, verify_diagnosis,
};
use cadus_worker::authoring::prompt::{AuthoringSpec, Kind, tool_schema};
use serde_json::{Value, json};

use common::{
    FakeModel, RETRY_HEADER, assert_one_row, author, content_rows, seed_content,
    squares_spec as template_spec,
};

/// The serving key of the knowledge point under test, `"<topic_id>/<kp_id>"`.
const KP_KEY: &str = "addition/kp1";

/// The tag outside the vocabulary of spec section 5.3.
const UNKNOWN_TAG: &str = "carelessness";

/// The digest of the stored body: `sha256:` and the first 16 hex characters of
/// its SHA-256.
///
/// The body the pass writes leads with the three server-side fields and holds
/// the one distractor the vocabulary names:
///
/// ```json
/// {"v":1,"topic_id":"addition","answer_kind":"numeric","distractors":[{"answer":"13","error_tag":"arithmetic-slip","note":"You added the whole parts and dropped the half."}]}
/// ```
///
/// The key of `content_store` is the knowledge point, the kind AND the body, so
/// the material is `KP_KEY`, one NUL byte, `diagnosis`, one NUL byte, and the
/// body. The digest is computed outside this tree with
///
/// ```sh
/// printf 'addition/kp1\0diagnosis\0%s' '<the body above>' | sha256sum
/// # 69ceac16786901e7c87fa0bbdf503b0b25738289cfe446593fe880c1c93c48bc
/// ```
///
/// It pins the stored bytes, which the `content_store.body` column cannot: the
/// column holds jsonb, and jsonb keeps neither key order nor whitespace.
const STORED_DIGEST: &str = "sha256:69ceac16786901e7";

/// The gate's sentence for a distractor that answers what the exemplar answers.
const RIGHT_ANSWER: &str = "distractor 0 answers '13.5', which is the right answer of exemplar 0 \
— a distractor names a mistake";

/// The closing line of the distractor retry block.
const RETRY_FIX: &str = "Fix that specifically. Do not restate the same list — change the \
answers, the tags, or the notes so the reason no longer applies.";

/// A reply that carries a complete `emit_distractors` call with these arguments.
fn tool_reply(arguments: &Value) -> (u16, String) {
    common::reply(
        "emit_distractors",
        &arguments.to_string(),
        Some(json!({"prompt_tokens": 700, "completion_tokens": 200})),
    )
}

/// The tool arguments of a list that carries one known tag and one tag outside
/// the vocabulary.
fn mixed_tags() -> Value {
    json!({
        "distractors": [
            {"answer": "13", "error_tag": "arithmetic-slip",
             "note": "You added the whole parts and dropped the half."},
            {"answer": "2.5", "error_tag": UNKNOWN_TAG,
             "note": "You subtracted where the problem adds."}
        ]
    })
}

/// A list whose first distractor answers what the exemplar answers. The gate
/// refuses it with [`RIGHT_ANSWER`].
fn names_the_right_answer() -> Value {
    json!({
        "distractors": [
            {"answer": "13.5", "error_tag": "arithmetic-slip",
             "note": "You added the whole parts and dropped the half."}
        ]
    })
}

/// The knowledge point every test authors for. Its exemplar answers `13.5`.
fn spec() -> AuthoringSpec {
    AuthoringSpec {
        kp_id: "kp1".to_owned(),
        kp_name: "Add a whole number and a decimal".to_owned(),
        topic_id: "addition".to_owned(),
        topic_name: "Addition".to_owned(),
        answer_kind: AnswerKind::Numeric,
        difficulty_target: None,
        constraints: None,
        exemplars: vec![Exemplar {
            answer_contract: None,
            problem: "Compute $8 + 5.5$.".to_owned(),
            answer: "13.5".to_owned(),
            solution_sketch: None,
        }],
    }
}

// --------------------------------------------------------------------------- //
// Acceptance: the dropped tag never reaches the row
// --------------------------------------------------------------------------- //

/// Row R7, first acceptance check, at the place the pipeline runs it.
///
/// The model answers two distractors and one of them carries `carelessness`,
/// which the vocabulary of spec section 5.3 does not hold. The gate drops it,
/// the pass stores the other one, and the stored body is pinned whole because
/// the C6 approval binds to its digest.
#[tokio::test]
async fn an_error_tag_outside_the_vocabulary_never_reaches_the_stored_row() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&mixed_tags())]).await;

        let report = author(&db, &fake, Kind::Diagnosis, &spec()).await;

        assert_eq!(report.outcome, Outcome::Stored);
        assert_eq!(report.attempts, 1, "one call authored the list");
        assert_eq!(report.digest.as_deref(), Some(STORED_DIGEST));
        assert_eq!(fake.call_count(), 1);

        // An independent AI reviewer approves the row before it serves. The column holds
        // jsonb, which keeps neither key order nor whitespace, so the row is
        // read as a value and [`STORED_DIGEST`] pins the bytes.
        let row = assert_one_row(&db.admin, KP_KEY, "diagnosis", STORED_DIGEST, "pending", 1).await;
        assert_eq!(
            row.body,
            json!({
                "v": 1,
                "topic_id": "addition",
                "answer_kind": "numeric",
                "distractors": [{
                    "answer": "13",
                    "error_tag": "arithmetic-slip",
                    "note": "You added the whole parts and dropped the half."
                }]
            }),
            "the dropped tag is not in the stored body"
        );
    })
    .await;
}

/// The rejection message is the yield lever (1.0 `problem_templates.py:1386-1393`),
/// and it works for this kind too: a refused list is re-prompted with the gate's
/// own sentence and is rescued on attempt 2.
#[tokio::test]
async fn a_refused_list_is_re_prompted_verbatim_and_rescued_on_attempt_two() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![
            tool_reply(&names_the_right_answer()),
            tool_reply(&mixed_tags()),
        ])
        .await;

        let report = author(&db, &fake, Kind::Diagnosis, &spec()).await;

        assert_eq!(report.outcome, Outcome::Stored);
        assert_eq!(report.attempts, 2);
        assert_eq!(fake.call_count(), 2);

        let retry = format!("{RETRY_HEADER}\n    {RIGHT_ANSWER}\n{RETRY_FIX}");
        assert!(
            fake.user_message(1).contains(&retry),
            "the second call carries the retry block verbatim: {}",
            fake.user_message(1)
        );
        assert!(
            !fake.user_message(0).contains(RETRY_HEADER),
            "the first call carries no retry block"
        );

        // The row records both calls (T3).
        assert_one_row(&db.admin, KP_KEY, "diagnosis", STORED_DIGEST, "pending", 2).await;
    })
    .await;
}

/// The two tools ask for a different `answer`, because the two documents hold a
/// different one.
///
/// A template distractor is an expression over the declared parameters. A
/// `diagnosis` document declares none, so its answer is the wrong answer itself.
/// One description for both kinds sends the distractor author to write a formula
/// over parameters that do not exist, and the gate spends an attempt on it.
#[test]
fn the_distractor_tool_asks_for_a_literal_answer() {
    let of = |kind| {
        tool_schema(kind)["properties"]["distractors"]["items"]["properties"]["answer"]
        ["description"]
        .clone()
    };

    assert_eq!(
        of(Kind::Diagnosis),
        json!(
            "The wrong answer itself, written the way a learner writes it. It is a literal \
answer, not a formula: this knowledge point declares no parameters."
        )
    );
    assert_eq!(
        of(Kind::Template),
        json!(
            "The wrong answer, as an expression over the declared parameters, so the server \
computes it per instance."
        )
    );
}

/// The gate keeps exactly the 11 tags of spec section 5.3, and the grade path
/// keeps every one of them.
///
/// The two filters are the wiring of row R7: the gate drops a tag at authoring
/// time, and `cadus_core::template::match_answer` drops one at read time. A tag
/// the gate keeps and the reader drops would store a diagnosis no learner ever
/// reads, so this test pins the two lists against each other.
#[test]
fn the_gate_keeps_only_tags_the_grade_path_also_keeps() {
    assert_eq!(
        authoring_vocabulary(),
        vec![
            "sign-error".to_owned(),
            "arithmetic-slip".to_owned(),
            "algebra-slip".to_owned(),
            "wrong-method".to_owned(),
            "formula-recall".to_owned(),
            "misread-problem".to_owned(),
            "incomplete".to_owned(),
            "notation".to_owned(),
            "units".to_owned(),
            "timing-unreliable".to_owned(),
            "blowoff".to_owned(),
        ]
    );
    let grade_path = cadus_core::config::default_error_tags();
    for tag in authoring_vocabulary() {
        assert!(
            grade_path.contains(&tag),
            "the grade path drops {tag}, which the gate keeps"
        );
    }
    // 2.0 spells the tag `blank-answer` and 1.0 spells it `blank_answer`
    // (`cadus_web::grade::TAG_BLANK_ANSWER`, spec section 5.3, the trap). The
    // gate keeps neither: the grade path stamps that tag on a blank submission,
    // and a distractor names an answer the learner wrote.
    for spelling in ["blank-answer", "blank_answer"] {
        assert!(
            !authoring_vocabulary().contains(&spelling.to_owned()),
            "{spelling} is server-assigned, so no distractor carries it"
        );
    }
}

/// A knowledge point whose diagnosis list is already approved pays nothing: the
/// bank of this kind is one document (spec section 2.2, "Bank target").
#[tokio::test]
async fn an_approved_list_makes_no_call() {
    TestDb::with(|db| async move {
        seed_content(
            &db.admin,
            "sha256:already0000000a",
            KP_KEY,
            "diagnosis",
            "approved",
        )
        .await;
        let fake = FakeModel::start(vec![tool_reply(&mixed_tags())]).await;

        let report = author(&db, &fake, Kind::Diagnosis, &spec()).await;

        assert_eq!(report.outcome, Outcome::Skipped);
        assert_eq!(report.attempts, 0);
        assert_eq!(fake.call_count(), 0, "a full bank makes zero model calls");
        assert_eq!(content_rows(&db.admin, KP_KEY).await.len(), 1);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The template document: the same drop, in the same place
// --------------------------------------------------------------------------- //

/// A template whose two distractors carry one known tag and one tag outside the
/// vocabulary. Neither note renders a parameter, so the drop takes the
/// distractor and nothing else.
fn mixed_template() -> Value {
    let mut arguments = common::good_arguments();
    arguments["distractors"] = json!([
        {"answer": "2*a", "error_tag": "arithmetic-slip",
         "note": "You doubled the number instead of squaring it."},
        {"answer": "a+2", "error_tag": UNKNOWN_TAG,
         "note": "You added two instead of squaring."}
    ]);
    arguments
}

/// The same template, with the parameter `b` rendered in ONE place: the note of
/// the single distractor. The tag decides whether that note survives.
fn note_holds_the_only_use(tag: &str) -> Value {
    let mut arguments = common::good_arguments();
    arguments["params"] = json!({
        "a": {"kind": "int", "low": 1, "high": 12},
        "b": {"kind": "int", "low": 1, "high": 2}
    });
    arguments["distractors"] = json!([
        {"answer": "2*a", "error_tag": tag,
         "note": "You multiplied by {b} instead of squaring."}
    ]);
    arguments["samples"] = json!([
        {"params": {"a": 1, "b": 1}, "expected": "1"},
        {"params": {"a": 12, "b": 2}, "expected": "144"},
        {"params": {"a": 1, "b": 2}, "expected": "1"},
        {"params": {"a": 12, "b": 1}, "expected": "144"}
    ]);
    arguments
}

/// Row R7, first acceptance check, on the OTHER document that carries
/// distractors: the template. The stored body holds the distractor the
/// vocabulary names, and the gate accepts that body a second time.
///
/// The re-gate is the point. `content_store` keeps the body the digest covers,
/// and the reviewer of unit R5 reads the gate block of that stored body. A
/// stored body the gate refuses shows the reviewer a refusal the authored
/// document never earned.
#[test]
fn a_template_tag_outside_the_vocabulary_is_dropped_and_the_stored_body_re_gates() {
    let spec = template_spec();

    let body = verify(&spec, &mixed_template()).expect("the gate accepts the template");

    let stored: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        stored["distractors"],
        json!([{
            "answer": "2*a",
            "error_tag": "arithmetic-slip",
            "note": "You doubled the number instead of squaring it."
        }]),
        "the dropped tag is not in the stored body: {body}"
    );
    let gate_spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &spec.exemplars,
        finite: None,
    };
    gate_body(&body, &gate_spec).expect("the stored body passes the gate a second time");
}

/// The drop runs BEFORE the gate, and that order is the whole rule.
///
/// One document, one tag apart. A distractor note is a rendered field, so the
/// note is where the parameter `b` does its work. The known tag keeps the note,
/// and the gate accepts. The unknown tag takes the note with the distractor, and
/// the gate then reads the FILTERED document: it names the dead parameter, and
/// the next attempt reads that sentence.
///
/// A drop after the gate stores the second document instead, and the row then
/// holds a body the gate refuses.
#[test]
fn the_template_drop_runs_before_the_gate() {
    let spec = template_spec();

    verify(&spec, &note_holds_the_only_use("arithmetic-slip"))
        .expect("the surviving note renders the parameter");

    let rejection = verify(&spec, &note_holds_the_only_use(UNKNOWN_TAG))
        .expect_err("the drop leaves the parameter dead");

    assert_eq!(rejection.code, "dead-parameter");
    assert_eq!(
        rejection.message,
        "parameters ['b'] are declared but never used"
    );
}

/// A tool call with no arguments object is refused on the template gate and
/// on the diagnosis gate alike, before either gate reads a field.
#[test]
fn arguments_that_are_not_an_object_are_refused_on_both_gates() {
    let spec = template_spec();
    let refusals = [
        verify(&spec, &json!("emit_template")),
        verify_diagnosis(&spec, &json!("emit_distractors")),
    ];
    for refusal in refusals {
        let rejection = refusal.expect_err("a string is not an arguments object");
        assert_eq!(
            (rejection.code, rejection.message),
            ("tool-arguments", NO_ARGUMENTS.to_owned())
        );
    }
}
