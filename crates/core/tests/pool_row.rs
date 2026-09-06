//! M4 U4: the pool row documents and the serving key (D-S5, D6).
//!
//! Every expected value here is a LITERAL JSON text, a literal key, or a literal
//! message. Nothing is read back from the code under test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::BTreeMap;

use cadus_core::answer::canonical_form;
use cadus_core::pool::{
    KP_KEY_SEPARATOR, POOL_ROW_VERSION, PoolAnswer, PoolProblem, kp_key, split_kp_key,
};
use cadus_core::template::{Bindings, Instance, Scalar, Value};

/// The document version of M4 is 1.
#[test]
fn the_document_version_is_one() {
    assert_eq!(POOL_ROW_VERSION, 1);
    assert_eq!(KP_KEY_SEPARATOR, '/');
}

// --------------------------------------------------------------------------
// The serving key
// --------------------------------------------------------------------------

/// A knowledge-point id is unique inside its topic only, so the key qualifies it.
#[test]
fn the_serving_key_qualifies_the_knowledge_point_with_its_topic() {
    assert_eq!(kp_key("perfect-squares", "kp1"), "perfect-squares/kp1");
    assert_eq!(kp_key("adding-two-digits", "kp3"), "adding-two-digits/kp3");
    assert_eq!(
        split_kp_key("perfect-squares/kp1"),
        Some(("perfect-squares", "kp1"))
    );
}

/// The split takes the first separator and refuses an empty half.
#[test]
fn a_key_without_two_halves_does_not_split() {
    assert_eq!(split_kp_key("kp1"), None, "no separator");
    assert_eq!(split_kp_key("/kp1"), None, "no topic");
    assert_eq!(split_kp_key("perfect-squares/"), None, "no knowledge point");
    assert_eq!(split_kp_key(""), None);
    assert_eq!(
        split_kp_key("topic/kp/extra"),
        Some(("topic", "kp/extra")),
        "the split takes the FIRST separator"
    );
}

// --------------------------------------------------------------------------
// The problem document
// --------------------------------------------------------------------------

/// The written document is this exact text.
#[test]
fn the_problem_document_writes_the_literal_body() {
    let doc = PoolProblem {
        v: 1,
        text: "Compute $7^{2}$.".to_string(),
        bindings: [("a".to_string(), "7".to_string())].into_iter().collect(),
        seed: 42,
    };
    assert_eq!(
        doc.to_body().unwrap(),
        r#"{"v":1,"text":"Compute $7^{2}$.","bindings":{"a":"7"},"seed":42}"#
    );
    assert_eq!(
        PoolProblem::from_body(&doc.to_body().unwrap()).unwrap(),
        doc,
        "the document round-trips"
    );
}

/// An exemplar binds no parameter, so the field leaves the document.
#[test]
fn an_empty_binding_map_leaves_the_document() {
    let doc = PoolProblem {
        v: 1,
        text: "Compute $1 + 1$.".to_string(),
        bindings: BTreeMap::new(),
        seed: 0,
    };
    assert_eq!(
        doc.to_body().unwrap(),
        r#"{"v":1,"text":"Compute $1 + 1$.","seed":0}"#
    );
    assert_eq!(
        PoolProblem::from_body(&doc.to_body().unwrap()).unwrap(),
        doc
    );
}

/// A field the document does not declare is a refusal.
#[test]
fn an_unknown_field_is_refused() {
    let err = PoolProblem::from_body(r#"{"v":1,"text":"x","seed":0,"answer":"9"}"#)
        .expect_err("an unknown field is refused");
    assert!(
        err.message.contains("unknown field `answer`"),
        "the message names the field, it read {}",
        err.message
    );
}

/// A version this build does not know is a refusal.
#[test]
fn an_unknown_version_is_refused() {
    let err =
        PoolProblem::from_body(r#"{"v":2,"text":"x","seed":0}"#).expect_err("version 2 is refused");
    assert_eq!(err.message, "pool row version 2 is not 1");
    assert_eq!(err.to_string(), "pool row version 2 is not 1");

    let err = PoolAnswer::from_body(r#"{"v":9,"answer":"49"}"#).expect_err("version 9 is refused");
    assert_eq!(err.message, "pool row version 9 is not 1");
}

// --------------------------------------------------------------------------
// The answer document
// --------------------------------------------------------------------------

/// The answer document is the answer string and its version, and nothing else.
#[test]
fn the_answer_document_writes_the_literal_body() {
    let doc = PoolAnswer {
        answer_contract: None,
        v: 1,
        answer: "49".to_string(),
    };
    assert_eq!(doc.to_body().unwrap(), r#"{"v":1,"answer":"49"}"#);
    assert_eq!(
        PoolAnswer::from_body(r#"{"v":1,"answer":"49"}"#).unwrap(),
        doc
    );
}

// --------------------------------------------------------------------------
// From an instance (D6: no float spelling)
// --------------------------------------------------------------------------

/// Build one instance by hand: two bound values and one rendered statement.
fn instance() -> Instance {
    let mut bindings: Bindings = Bindings::new();
    bindings.insert("a".to_string(), Scalar::Int(7).value());
    bindings.insert("op".to_string(), Value::Text("\\times".to_string()));
    Instance {
        answer_contract: None,
        bindings,
        text: "Compute $7 \\times 7$.".to_string(),
        answer: "49".to_string(),
        canon: canonical_form("49").expect("49 canonicalizes"),
        instance_hash: "0123456789ab".to_string(),
    }
}

/// Every bound value reaches the document as its exact canonical string.
#[test]
fn an_instance_writes_its_bindings_as_canonical_strings() {
    let doc = PoolProblem::from_instance(&instance(), 7_821_167_185_356_633_737);
    assert_eq!(
        doc.to_body().unwrap(),
        r#"{"v":1,"text":"Compute $7 \\times 7$.","bindings":{"a":"7","op":"\\times"},"seed":7821167185356633737}"#
    );

    let answer = PoolAnswer::from_instance(&instance());
    assert_eq!(answer.to_body().unwrap(), r#"{"v":1,"answer":"49"}"#);
}

/// A backslash choice value survives the round trip.
///
/// 1.0 trap 6: `\times` as a choice value broke the replacement side of
/// `re.sub` (`cadus_web/sympy_check.py:299-303`).
#[test]
fn a_backslash_choice_value_round_trips() {
    let doc = PoolProblem::from_instance(&instance(), 0);
    let read = PoolProblem::from_body(&doc.to_body().unwrap()).unwrap();
    assert_eq!(read.bindings.get("op").map(String::as_str), Some("\\times"));
    assert_eq!(read.text, "Compute $7 \\times 7$.");
}

/// A text that is no JSON document is refused with the reader's own words, for
/// both documents.
#[test]
fn a_text_that_is_no_document_is_refused() {
    let err = PoolAnswer::from_body("nope").expect_err("no JSON");
    assert_eq!(err.message, "expected ident at line 1 column 2");
    let err = PoolProblem::from_body("").expect_err("no JSON");
    assert_eq!(err.message, "EOF while parsing a value at line 1 column 0");
}
