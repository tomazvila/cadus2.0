//! M6 R7: the gate of the `diagnosis` document, and the match the grade path runs.
//!
//! Requirements: A4, A2, C6, C4. Spec
//! `docs/reference/authoring-and-spa-1.0-spec.md` section 2.2 step 1 and row R7
//! of section 7; `docs/reference/web-service-1.0-spec.md` sections 5.3 and 6.2.
//!
//! The first acceptance check of row R7 lands here: an `error_tag` outside the
//! vocabulary is dropped at the gate
//! ([`an_error_tag_outside_the_vocabulary_is_dropped_at_the_gate`]).
//!
//! Every expected value is a LITERAL: a literal rejection sentence, a literal
//! rejection code, a literal stored body, a literal tag. Nothing here reads a
//! constant back from the code under test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use cadus_core::curriculum::{AnswerKind, Exemplar};
use cadus_core::template::{
    DiagnosisDoc, Distractor, GateSpec, Rejection, gate_diagnosis_body, match_answer,
    read_distractors, to_diagnosis_body,
};
use serde_json::json;

// --------------------------------------------------------------------------- //
// The literals of this file
// --------------------------------------------------------------------------- //

/// The tag the vocabulary holds.
const KNOWN_TAG: &str = "arithmetic-slip";

/// The tag the vocabulary does not hold. 1.0 drops it (`prompts.py:1133-1142`).
const UNKNOWN_TAG: &str = "carelessness";

/// The note of the distractor the gate keeps.
const KEPT_NOTE: &str = "You added the whole parts and dropped the half.";

/// The note of the distractor the vocabulary filter drops.
const DROPPED_NOTE: &str = "You subtracted where the problem adds.";

/// The body the gate writes for [`good_body`], character for character.
///
/// The three server-side fields lead it, in the order the document declares
/// them, and the dropped distractor is gone.
const STORED_BODY: &str = r#"{"v":1,"topic_id":"addition","answer_kind":"numeric","distractors":[{"answer":"13","error_tag":"arithmetic-slip","note":"You added the whole parts and dropped the half."}]}"#;

/// The vocabulary of spec section 5.3, as the authoring gate reads it.
fn vocabulary() -> Vec<String> {
    [
        "sign-error",
        "arithmetic-slip",
        "algebra-slip",
        "wrong-method",
        "formula-recall",
        "misread-problem",
        "incomplete",
        "notation",
        "units",
        "timing-unreliable",
        "blowoff",
    ]
    .iter()
    .map(|tag| (*tag).to_owned())
    .collect()
}

/// The knowledge point every test gates for. Its authored answer is `13.5`.
fn exemplars() -> Vec<Exemplar> {
    vec![Exemplar {
        answer_contract: None,
        problem: "Compute $8 + 5.5$.".to_owned(),
        answer: "13.5".to_owned(),
        solution_sketch: None,
    }]
}

/// One diagnosis body, as `authoring::job::assemble` writes it.
fn body(distractors: serde_json::Value) -> String {
    json!({
        "distractors": distractors,
        "v": 1,
        "topic_id": "addition",
        "answer_kind": "numeric",
    })
    .to_string()
}

/// The body of the acceptance check: one known tag, one tag outside the
/// vocabulary.
fn good_body() -> String {
    body(json!([
        {"answer": "13", "error_tag": KNOWN_TAG, "note": KEPT_NOTE},
        {"answer": "2.5", "error_tag": UNKNOWN_TAG, "note": DROPPED_NOTE},
    ]))
}

/// Gate one body and give the rejection it earns.
fn refusal(body: &str) -> Rejection {
    let exemplars = exemplars();
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &exemplars,
        finite: None,
    };
    gate_diagnosis_body(body, &spec, &vocabulary()).expect_err("the gate refuses this document")
}

/// Gate one body and give the document it accepts, with the dropped tags.
fn accepted(body: &str) -> (DiagnosisDoc, Vec<String>) {
    let exemplars = exemplars();
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &exemplars,
        finite: None,
    };
    gate_diagnosis_body(body, &spec, &vocabulary()).expect("the gate accepts this document")
}

// --------------------------------------------------------------------------- //
// Acceptance: an error_tag outside the vocabulary is dropped at the gate
// --------------------------------------------------------------------------- //

/// Row R7, first acceptance check. The list reaches the gate with two
/// distractors; the stored document holds the one whose tag the vocabulary of
/// spec section 5.3 names, and the gate reports the tag it dropped.
#[test]
fn an_error_tag_outside_the_vocabulary_is_dropped_at_the_gate() {
    let (doc, dropped) = accepted(&good_body());

    assert_eq!(dropped, vec!["carelessness".to_owned()]);
    assert_eq!(doc.distractors.len(), 1, "the gate keeps one distractor");
    assert_eq!(doc.distractors[0].answer, "13");
    assert_eq!(doc.distractors[0].error_tag, "arithmetic-slip");
    assert_eq!(
        doc.distractors[0].note.as_deref(),
        Some("You added the whole parts and dropped the half.")
    );
    // The stored body is what the digest covers (C6), so it is pinned whole.
    assert_eq!(to_diagnosis_body(&doc).unwrap(), STORED_BODY);
}

/// A list whose every tag is outside the vocabulary diagnoses nothing, so the
/// gate refuses it and names the vocabulary. The message is the next attempt's
/// instruction (spec section 2.2, step 3).
#[test]
fn a_list_the_filter_empties_is_refused_with_the_vocabulary_named() {
    let rejection = refusal(&body(json!([
        {"answer": "13", "error_tag": UNKNOWN_TAG, "note": KEPT_NOTE},
    ])));

    assert_eq!(rejection.code, "distractor-vocabulary");
    assert_eq!(
        rejection.message,
        "no distractor carries an error_tag from the vocabulary ['sign-error', \
'arithmetic-slip', 'algebra-slip', 'wrong-method', 'formula-recall', 'misread-problem', \
'incomplete', 'notation', 'units', 'timing-unreliable', 'blowoff'] — a tag outside it is \
dropped, so the list diagnoses nothing"
    );
}

// --------------------------------------------------------------------------- //
// The rest of the gate
// --------------------------------------------------------------------------- //

/// A distractor that answers what the exemplar answers names no mistake.
#[test]
fn a_distractor_that_is_an_exemplars_right_answer_is_refused() {
    let rejection = refusal(&body(json!([
        {"answer": "13.50", "error_tag": KNOWN_TAG, "note": KEPT_NOTE},
    ])));

    assert_eq!(rejection.code, "distractor");
    assert_eq!(
        rejection.message,
        "distractor 0 answers '13.50', which is the right answer of exemplar 0 — a distractor \
names a mistake"
    );
}

/// Two distractors that name one answer make the second one dead: the match
/// takes the first. The gate refuses the pair instead of storing dead prose.
#[test]
fn two_distractors_that_name_one_answer_are_refused() {
    let rejection = refusal(&body(json!([
        {"answer": "13", "error_tag": KNOWN_TAG, "note": KEPT_NOTE},
        {"answer": "13.0", "error_tag": "sign-error", "note": DROPPED_NOTE},
    ])));

    assert_eq!(rejection.code, "distractor");
    assert_eq!(
        rejection.message,
        "distractor 1 answers '13.0', which distractor 0 already names — one wrong answer \
carries one diagnosis"
    );
}

/// The note is the diagnosis. A distractor without one carries a tag the learner
/// never reads.
#[test]
fn a_distractor_with_no_note_is_refused() {
    let rejection = refusal(&body(json!([
        {"answer": "13", "error_tag": KNOWN_TAG, "note": "   "},
    ])));

    assert_eq!(rejection.code, "distractor-note");
    assert_eq!(
        rejection.message,
        "distractor 0 carries no note — the note is the diagnosis the learner reads, and a tag \
alone explains nothing"
    );
}

/// A blank tag is no tag. The filter leaves it for the gate, which names it, so
/// the model reads the sentence about the field it left empty and not the
/// sentence about the vocabulary.
#[test]
fn a_blank_error_tag_reaches_the_gate_and_is_named() {
    let rejection = refusal(&body(json!([
        {"answer": "13", "error_tag": "  ", "note": KEPT_NOTE},
    ])));

    assert_eq!(rejection.code, "distractor");
    assert_eq!(rejection.message, "distractor 0 carries no error_tag");
}

/// An answer the checker cannot read never matches, so the gate refuses it with
/// the reason the parser gives.
#[test]
fn an_answer_outside_the_grammar_is_refused() {
    let rejection = refusal(&body(json!([
        {"answer": "about thirteen", "error_tag": KNOWN_TAG, "note": KEPT_NOTE},
    ])));

    assert_eq!(rejection.code, "distractor");
    assert!(
        rejection.message.starts_with(
            "distractor 0 answers 'about thirteen', which is outside the decidable grammar: "
        ),
        "the message names the answer and the parser's reason: {}",
        rejection.message
    );
}

/// An empty list is refused, and the message names the three fields a distractor
/// carries.
#[test]
fn an_empty_list_is_refused() {
    let rejection = refusal(&body(json!([])));

    assert_eq!(rejection.code, "distractor-missing");
    assert_eq!(
        rejection.message,
        "a diagnosis document needs at least one distractor: the wrong answer a real mistake \
produces, the tag it carries, and the note the learner reads"
    );
}

/// A document of another version is refused: a stored document of one version
/// reads one way, and a bump retires every one of them.
#[test]
fn a_document_of_another_version_is_refused() {
    let raw = json!({
        "v": 2,
        "topic_id": "addition",
        "answer_kind": "numeric",
        "distractors": [{"answer": "13", "error_tag": KNOWN_TAG, "note": KEPT_NOTE}],
    })
    .to_string();
    let rejection = refusal(&raw);

    assert_eq!(rejection.code, "diagnosis-version");
    assert_eq!(
        rejection.message,
        "a diagnosis document of version 2 is not the version this server reads (1)"
    );
}

/// An answer kind the checker never decides makes every match impossible, so the
/// list is refused before it is stored (1.0 `problem_templates.py:852`).
#[test]
fn an_undecidable_answer_kind_is_refused() {
    let raw = json!({
        "v": 1,
        "topic_id": "addition",
        "answer_kind": "proof",
        "distractors": [{"answer": "13", "error_tag": KNOWN_TAG, "note": KEPT_NOTE}],
    })
    .to_string();
    let rejection = refusal(&raw);

    assert_eq!(rejection.code, "kind");
    assert_eq!(
        rejection.message,
        "answer kind proof is not symbolically decidable"
    );
}

/// An invented field is a rejection, not a silently dropped instruction.
#[test]
fn an_unknown_field_is_refused() {
    let raw = json!({
        "v": 1,
        "topic_id": "addition",
        "answer_kind": "numeric",
        "confidence": 0.9,
        "distractors": [{"answer": "13", "error_tag": KNOWN_TAG, "note": KEPT_NOTE}],
    })
    .to_string();
    let rejection = refusal(&raw);

    assert_eq!(rejection.code, "diagnosis-body");
    assert!(
        rejection
            .message
            .starts_with("the distractor list does not read as a diagnosis document: "),
        "the message names the read that failed: {}",
        rejection.message
    );
}

// --------------------------------------------------------------------------- //
// The match the grade path runs (spec section 6.2)
// --------------------------------------------------------------------------- //

/// The stored document of the acceptance check answers the learner who writes
/// `13.0`: the checker decides the form, so one authored answer names every
/// spelling of the same mistake.
#[test]
fn the_stored_document_matches_the_learners_spelling_of_that_answer() {
    let (doc, _) = accepted(&good_body());

    let hit = match_answer(&doc.distractors, "13.0", AnswerKind::Numeric, &vocabulary())
        .expect("the stored distractor names 13.0");

    assert_eq!(hit.error_tags, vec!["arithmetic-slip".to_owned()]);
    assert_eq!(
        hit.prose.as_deref(),
        Some("You added the whole parts and dropped the half.")
    );
    assert_eq!(
        match_answer(&doc.distractors, "99", AnswerKind::Numeric, &vocabulary()),
        None,
        "an answer no distractor names is no hit"
    );
}

/// The filter runs again where the tag is READ, so a hand-written row with a tag
/// outside the vocabulary still reaches the learner as prose, with no tag.
#[test]
fn a_read_time_tag_outside_the_vocabulary_is_dropped_and_the_prose_stands() {
    let distractors = vec![
        Distractor {
            answer: "2.5".to_owned(),
            error_tag: UNKNOWN_TAG.to_owned(),
            note: Some(DROPPED_NOTE.to_owned()),
        },
        Distractor {
            answer: "8".to_owned(),
            error_tag: UNKNOWN_TAG.to_owned(),
            note: None,
        },
    ];

    let hit = match_answer(&distractors, "2.5", AnswerKind::Numeric, &vocabulary())
        .expect("the prose alone is still a diagnosis");
    assert_eq!(hit.error_tags, Vec::<String>::new());
    assert_eq!(
        hit.prose.as_deref(),
        Some("You subtracted where the problem adds.")
    );

    assert_eq!(
        match_answer(&distractors, "8", AnswerKind::Numeric, &vocabulary()),
        None,
        "neither a tag nor prose survives, so it is not a hit"
    );
}

/// The reader takes `distractors` and ignores the rest, so it reads a dedicated
/// diagnosis row and a template document stored under the same kind.
#[test]
fn the_reader_takes_the_list_and_ignores_the_rest() {
    let full = read_distractors(&json!({
        "v": 1,
        "statement": "Compute ${a} + {b}$.",
        "distractors": [{"answer": "13", "error_tag": KNOWN_TAG, "note": KEPT_NOTE}],
    }));
    assert_eq!(full.len(), 1);
    assert_eq!(full[0].answer, "13");

    assert_eq!(read_distractors(&json!({"v": 1})).len(), 0);
    assert_eq!(read_distractors(&json!("nonsense")).len(), 0);
    assert_eq!(read_distractors(&json!({"distractors": 7})).len(), 0);
}
