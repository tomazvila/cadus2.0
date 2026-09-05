//! The refusal sites of the gate, reached one by one (A2, C4, V2).
//!
//! Every expected message is a literal, in the words the author reads.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_core::template::SpaceSize;
use common::gate::*;

/// Accept the base document with `overrides`, over `a` in 1..12 and `b` in 1..2, with the four corner samples.
fn accept_over_a_and(b: &str, overrides: &[(&str, &str)]) {
    let params = format!(
        r#"{{"a": {{"kind": "int", "low": 1, "high": 12}}, "{b}": {{"kind": "int", "low": 1, "high": 2}}}}"#
    );
    let samples = format!(
        r#"[{{"params": {{"a": 1, "{b}": 1}}, "expected": "1"}},
            {{"params": {{"a": 12, "{b}": 2}}, "expected": "144"}},
            {{"params": {{"a": 1, "{b}": 2}}, "expected": "1"}},
            {{"params": {{"a": 12, "{b}": 1}}, "expected": "144"}}]"#
    );
    let mut fields: Vec<(&str, &str)> = overrides.to_vec();
    fields.push(("params", &params));
    fields.push(("samples", &samples));
    let verified = accept(&body_with(&fields), AnswerKind::Numeric, &["49", "81"]);
    assert_eq!(verified.space, SpaceSize::Exact(24));
}

#[test]
fn the_document_rows_refuse_a_version_a_kind_and_an_empty_field() {
    let mut old = doc_of(&body_with(&[]));
    old.v = 2;
    let pool = exemplars(&["49"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &pool,
    };
    let rejection = gate(&old, &spec).expect_err("the version is old");
    assert_eq!(rejection.code, "template-version");
    assert_eq!(
        rejection.message,
        "template version 2 is not the current version 1"
    );
    let rejection = reject(&body_with(&[]), AnswerKind::Expression, &["x"]);
    assert_eq!(rejection.code, "answer-kind-mismatch");
    assert_eq!(
        rejection.message,
        "the document declares answer kind numeric and the knowledge point declares expression"
    );
    let rejection = reject_squares(&body_with(&[("statement", r#""  ""#)]));
    assert_eq!(rejection.code, "text-empty");
    assert_eq!(rejection.message, "template text is missing or empty");
    let rejection = reject_squares(&body_with(&[("answer_expr", r#""""#)]));
    assert_eq!(rejection.code, "answer-expr-empty");
    assert_eq!(rejection.message, "answer_expr is missing or empty");
}

#[test]
fn a_parameter_name_is_an_identifier() {
    for name in ["1a", "", "a-b"] {
        let params = format!(r#"{{"{name}": {{"kind": "int", "low": 1, "high": 12}}}}"#);
        let rejection = reject_squares(&body_with(&[("params", &params)]));
        assert_eq!(rejection.code, "parameter-name");
        assert_eq!(
            rejection.message,
            format!("parameter name '{name}' is not an identifier")
        );
    }
    // A name that starts with an underscore is an identifier. The M2 grammar
    // reads no underscore, so the parameter shows in the statement alone.
    accept_over_a_and(
        "_b",
        &[("statement", r#""Compute ${a}^{{2}}$ (part {_b})""#)],
    );
}

#[test]
fn every_domain_shape_the_walk_refuses_earns_its_sentence() {
    let cases = [
        (
            r#"{"kind": "rational", "num": {"low": 1, "high": 3}, "den": {"low": -1, "high": 1}}"#,
            "rational-domain",
            "the denominator range -1..1 of a rational domain holds zero",
        ),
        (
            r#"{"kind": "rational", "num": {"low": 5, "high": 1}, "den": {"low": 1, "high": 2}}"#,
            "rational-domain",
            "rational domain 5..1 over 1..2 is empty",
        ),
        (
            r#"{"kind": "rational", "num": {"low": 1, "high": 200}, "den": {"low": 1, "high": 100}}"#,
            "domain-size",
            "rational domain 1..200 over 1..100 exceeds MAX_DOMAIN_SIZE (10000)",
        ),
        (
            r#"{"kind": "decimal", "low": 1, "high": 9, "scale": 10}"#,
            "decimal-domain",
            "decimal domain scale 10 exceeds MAX_DECIMAL_SCALE (9)",
        ),
        (
            r#"{"kind": "decimal", "low": 9, "high": 2, "scale": 1}"#,
            "decimal-domain",
            "decimal domain 9..2 at scale 1 is empty",
        ),
        (
            r#"{"kind": "decimal", "low": 1, "high": 20000, "scale": 1}"#,
            "domain-size",
            "decimal domain 1..20000 exceeds MAX_DOMAIN_SIZE (10000)",
        ),
        (
            r#"{"kind": "choice", "values": []}"#,
            "choice-domain",
            "a choice domain needs a non-empty 'values' list",
        ),
        (
            r#"{"kind": "int", "low": 1, "high": 20000}"#,
            "domain-size",
            "int domain 1..20000 exceeds MAX_DOMAIN_SIZE",
        ),
    ];
    for (domain, code, message) in cases {
        let params = format!(r#"{{"a": {domain}}}"#);
        let rejection = reject_squares(&body_with(&[("params", &params)]));
        assert_eq!(rejection.code, code, "{domain}");
        assert_eq!(rejection.message, message, "{domain}");
    }
}

#[test]
fn a_constraint_over_a_sub_and_an_abs_term_is_walked() {
    let verified = accept_numeric(&difference_body(
        (1, 6),
        (1, 6),
        r#"[{"op": "gt", "left": {"sub": ["a", "b"]}, "right": {"lit": 0}},
            {"op": "ne", "left": {"abs": "a"}, "right": {"lit": 0}}]"#,
        DIFFERENCE_SAMPLES,
    ));
    assert_eq!(verified.space, SpaceSize::Exact(15));
}

#[test]
fn every_rendered_field_names_its_hole_and_its_brace() {
    let rejection = reject_squares(&body_with(&[("solution_sketch", r#""Square {q}.""#)]));
    assert_eq!(rejection.code, "undeclared-parameter");
    assert_eq!(rejection.message, "text uses undeclared parameters ['q']");
    let rejection = reject_squares(&body_with(&[("hints", r#"["What is {a} }?"]"#)]));
    assert_eq!(rejection.code, "hint-placeholder");
    assert_eq!(
        rejection.message,
        "hint 0 has an unescaped brace at index 12 ('}?') — literal LaTeX braces must be doubled"
    );
    let rejection = reject_squares(&body_with(&[(
        "distractors",
        r#"[{"answer": "a", "error_tag": "slip", "note": "You wrote {q}."}]"#,
    )]));
    assert_eq!(rejection.code, "distractor-placeholder");
    assert_eq!(
        rejection.message,
        "distractor 0 uses undeclared parameters ['q']"
    );
    let rejection = reject_squares(&body_with(&[(
        "distractors",
        r#"[{"answer": "a", "error_tag": "slip", "note": "You wrote }."}]"#,
    )]));
    assert_eq!(rejection.code, "distractor-placeholder");
    assert_eq!(
        rejection.message,
        "distractor 0 has an unescaped brace at index 10 ('}.') — literal LaTeX braces must be doubled"
    );
}

#[test]
fn a_distractor_note_is_a_use_and_a_distractor_the_samples_cannot_evaluate_is_kept() {
    accept_over_a_and(
        "b",
        &[(
            "distractors",
            r#"[{"answer": "a*2", "error_tag": "doubled", "note": "You doubled {a} and forgot {b}."},
                {"answer": "1/(a-1)", "error_tag": "slip"}]"#,
        )],
    );
}

#[test]
fn the_answer_expression_names_of_every_node_kind_are_read() {
    let agreement = [("(a, 2)"), ("{a}"), ("[a, 12]"), ("(a, 12]"), ("pi*a")];
    for expr in agreement {
        let rejection = reject_squares(&body_with(&[("answer_expr", &format!("\"{expr}\""))]));
        assert_eq!(rejection.code, "sample-agreement", "{expr}");
    }
    let unknown = [("x < a + 1", "x"), ("y = a", "y"), ("1 < x < a", "x")];
    for (expr, name) in unknown {
        let rejection = reject_squares(&body_with(&[("answer_expr", &format!("\"{expr}\""))]));
        assert_eq!(rejection.code, "unknown-names", "{expr}");
        assert_eq!(
            rejection.message,
            format!("answer_expr references unknown names ['{name}']")
        );
    }
}

#[test]
fn the_per_instance_rules_on_hand_built_instances() {
    let rejection = refuse_instance(&hand_instance(4, "Compute ${a}$.", "16"));
    assert_eq!(rejection.code, "placeholder-left");
    assert_eq!(
        rejection.message,
        "a rendered problem still contains a placeholder"
    );
    let rejection = refuse_instance(&hand_instance(4, "Compute $4^{2}$.", ""));
    assert_eq!(rejection.code, "empty-answer");
    assert_eq!(
        rejection.message,
        "instantiation for {'a': 4} produced no answer"
    );
    let rejection = refuse_instance(&hand_instance(4, "Compute $4^{2}$.", "oo"));
    assert_eq!(rejection.code, "not-a-number");
    assert_eq!(
        rejection.message,
        "instance {'a': 4} answers 'oo', which is not a number (['oo'])"
    );
    let rejection = refuse_instance(&hand_instance(4, "Compute $4^{2}$.", "1.50"));
    assert_eq!(rejection.code, "decimal-answer");
    assert_eq!(
        rejection.message,
        "instance {'a': 4} answers '1.50', which is a decimal with a trailing zero run — 2.0 answers hold exact values only (D6)"
    );
    let rejection = refuse_instance(&hand_instance(4, "Compute $4^{2}$.", "2*q"));
    assert_eq!(rejection.code, "free-symbol");
    assert_eq!(
        rejection.message,
        "numeric answer '2*q' for {'a': 4} still contains ['q'] — a parameter is undeclared"
    );
    // A statement that ends on the answer shows it already, so no hint gives it away.
    let doc = doc_of(&body_with(&[("hints", r#"["The answer is 16"]"#)]));
    let pool = exemplars(&["49", "81"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &pool,
    };
    let shown = hand_instance(4, "The square is 16", "16");
    assert_eq!(check_instance(&doc, &spec, &shown), Ok(()));
    let hidden = hand_instance(4, "Compute $4^{2}$.", "16");
    let rejection = check_instance(&doc, &spec, &hidden).expect_err("the hint gives it away");
    assert_eq!(rejection.code, "hint-answer");
}

#[test]
fn an_instance_the_samples_never_reach_reports_its_own_refusal() {
    let rejection = reject(
        &body_with(&[
            ("answer_kind", r#""expression""#),
            ("statement", r#""Divide $x$ by ${a} - 6$.""#),
            ("answer_expr", r#""x/(a-6)""#),
            ("solution_sketch", r#""Divide by ${a} - 6$.""#),
            (
                "samples",
                r#"[{"params": {"a": 1}, "expected": "-x/5"},
                    {"params": {"a": 12}, "expected": "x/6"}]"#,
            ),
        ]),
        AnswerKind::Expression,
        &["x/2"],
    );
    assert_eq!(rejection.code, "undecidable-answer");
    assert_eq!(
        rejection.message,
        "instance {'a': 6} answers 'x/0', which the answer checker cannot decide: a quotient with a zero divisor — every instance answer must canonicalize (V2)"
    );
    let rejection = reject(
        &body_with(&[
            ("statement", r#""Divide 1 by ${a} - 6$.""#),
            ("answer_expr", r#""1/(a-6)""#),
            ("solution_sketch", r#""Divide by ${a} - 6$.""#),
            (
                "samples",
                r#"[{"params": {"a": 1}, "expected": "-1/5"},
                    {"params": {"a": 12}, "expected": "1/6"}]"#,
            ),
        ]),
        AnswerKind::Numeric,
        &["-1/2"],
    );
    assert_eq!(rejection.code, "instantiation");
    assert_eq!(
        rejection.message,
        "instantiation failed for {'a': 6}: answer_expr divides by zero"
    );
}

#[test]
fn a_surd_answers_only_the_integrality_rule_of_the_envelope() {
    let root = |exemplar: &str| {
        body_with(&[
            ("statement", r#""Simplify $\\sqrt{{{a}}}$.""#),
            ("answer_expr", r#""sqrt(a)""#),
            (
                "solution_sketch",
                r#""Take the square factor out of ${a}$.""#,
            ),
            ("hints", r#"["Which square divides the radicand?"]"#),
            (
                "samples",
                r#"[{"params": {"a": 1}, "expected": "1"},
                    {"params": {"a": 12}, "expected": "2*sqrt(3)"}]"#,
            ),
            ("topic_id", &format!("\"{exemplar}\"")),
        ])
    };
    let rejection = reject(&root("roots"), AnswerKind::Numeric, &["49"]);
    assert_eq!(rejection.code, "envelope-integral");
    assert_eq!(
        rejection.message,
        "instance {'a': 2} answers 'sqrt(2)', but every authored answer for this knowledge point is a whole number"
    );
    let verified = accept(&root("roots"), AnswerKind::Numeric, &["1/2"]);
    assert_eq!(verified.space, SpaceSize::Exact(12));
}
