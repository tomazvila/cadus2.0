//! Part 3 of the `template_gate_edges` tests. The header of `template_gate_edges_1.rs` names the sources.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::gate::*;

/// The instance check of the base document with one hint rung, at `a` of 4.
fn check_hint(hint: &str) -> Result<(), Rejection> {
    let doc = doc_of(&body_with(&[("hints", &format!("[\"{hint}\"]"))]));
    let pool = exemplars(&["49", "81"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &pool,
    };
    check_instance(&doc, &spec, &hand_instance(4, "Compute $4^{2}$.", "16"))
}

/// The answer `16` stands in a hint only as a token of its own.
///
/// A digit, a letter, or an underscore beside the run makes it part of a longer
/// word, and a point or a slash breaks the run only when a digit stands on the
/// far side of it.
#[test]
fn a_hint_names_the_answer_only_when_it_stands_free() {
    assert_eq!(check_hint("Is it near 160?"), Ok(()));
    assert_eq!(check_hint("Try x16 first."), Ok(()));
    assert_eq!(check_hint("Is it 16.5?"), Ok(()));
    assert_eq!(check_hint("Is it 1/16?"), Ok(()));
    let rejection = check_hint("Is 16 5 or more?").expect_err("the run stands free");
    assert_eq!(rejection.code, "hint-answer");
    assert_eq!(
        rejection.message,
        "hint 0 reads 'Is 16 5 or more?' for {'a': 4}, which names the answer '16' — a hint is a question, never the final step (Hard Rule 3)"
    );
}

/// A trailing zero run needs all three: a fraction part, digits only, and a
/// final zero. An answer that lacks one of them fails a later rule instead.
#[test]
fn a_decimal_without_a_trailing_zero_run_reaches_the_later_rules() {
    let rejection = refuse_instance(&hand_instance(4, "Compute $4^{2}$.", "1.5"));
    assert_eq!(rejection.code, "canonical-mismatch");
    assert_eq!(
        rejection.message,
        "instance {'a': 4} answers '1.5', which does not read back as the canonical form the instance carries — the answer and its canonical form must agree (V2)"
    );
    let rejection = refuse_instance(&hand_instance(4, "Compute $4^{2}$.", "1.x0"));
    assert_eq!(rejection.code, "free-symbol");
    assert_eq!(
        rejection.message,
        "numeric answer '1.x0' for {'a': 4} still contains ['x0'] — a parameter is undeclared"
    );
}
