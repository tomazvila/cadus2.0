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

use cadus_core::answer::canonical_form;
use cadus_core::template::{Instance, SpaceSize, gate_body};
use common::gate::*;

/// One hand-built instance of the base document at `a`.
fn instance(a: i64, text: &str, answer: &str) -> Instance {
    Instance {
        bindings: bind(&[("a", a)]),
        text: text.to_string(),
        answer: answer.to_string(),
        canon: canonical_form("16").expect("16 canonicalizes"),
        instance_hash: "digest".to_string(),
    }
}

/// The per-instance rejection of one hand-built instance under the squares point.
fn refuse_instance(instance: &Instance) -> Rejection {
    let doc = doc_of(&body_with(&[]));
    let pool = exemplars(&["49", "81"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &pool,
    };
    check_instance(&doc, &spec, instance).expect_err("the instance is refused")
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
    let verified = accept(
        &body_with(&[
            ("statement", r#""Compute ${a}^{{2}}$ (part {_b})""#),
            (
                "params",
                r#"{"a": {"kind": "int", "low": 1, "high": 12}, "_b": {"kind": "int", "low": 1, "high": 2}}"#,
            ),
            (
                "samples",
                r#"[{"params": {"a": 1, "_b": 1}, "expected": "1"},
                    {"params": {"a": 12, "_b": 2}, "expected": "144"},
                    {"params": {"a": 1, "_b": 2}, "expected": "1"},
                    {"params": {"a": 12, "_b": 1}, "expected": "144"}]"#,
            ),
        ]),
        AnswerKind::Numeric,
        &["49", "81"],
    );
    assert_eq!(verified.space, SpaceSize::Exact(24));
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
        r#"[{"params": {"a": 2, "b": 1}, "expected": "1"},
            {"params": {"a": 6, "b": 5}, "expected": "1"},
            {"params": {"a": 6, "b": 1}, "expected": "5"}]"#,
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
    let verified = accept(
        &body_with(&[
            (
                "params",
                r#"{"a": {"kind": "int", "low": 1, "high": 12}, "b": {"kind": "int", "low": 1, "high": 2}}"#,
            ),
            (
                "distractors",
                r#"[{"answer": "a*2", "error_tag": "doubled", "note": "You doubled {a} and forgot {b}."},
                    {"answer": "1/(a-1)", "error_tag": "slip"}]"#,
            ),
            (
                "samples",
                r#"[{"params": {"a": 1, "b": 1}, "expected": "1"},
                    {"params": {"a": 12, "b": 2}, "expected": "144"},
                    {"params": {"a": 1, "b": 2}, "expected": "1"},
                    {"params": {"a": 12, "b": 1}, "expected": "144"}]"#,
            ),
        ]),
        AnswerKind::Numeric,
        &["49", "81"],
    );
    assert_eq!(verified.space, SpaceSize::Exact(24));
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
    let rejection = refuse_instance(&instance(4, "Compute ${a}$.", "16"));
    assert_eq!(rejection.code, "placeholder-left");
    assert_eq!(
        rejection.message,
        "a rendered problem still contains a placeholder"
    );
    let rejection = refuse_instance(&instance(4, "Compute $4^{2}$.", ""));
    assert_eq!(rejection.code, "empty-answer");
    assert_eq!(
        rejection.message,
        "instantiation for {'a': 4} produced no answer"
    );
    let rejection = refuse_instance(&instance(4, "Compute $4^{2}$.", "oo"));
    assert_eq!(rejection.code, "not-a-number");
    assert_eq!(
        rejection.message,
        "instance {'a': 4} answers 'oo', which is not a number (['oo'])"
    );
    let rejection = refuse_instance(&instance(4, "Compute $4^{2}$.", "1.50"));
    assert_eq!(rejection.code, "decimal-answer");
    assert_eq!(
        rejection.message,
        "instance {'a': 4} answers '1.50', which is a decimal with a trailing zero run — 2.0 answers hold exact values only (D6)"
    );
    let rejection = refuse_instance(&instance(4, "Compute $4^{2}$.", "2*q"));
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
    let shown = instance(4, "The square is 16", "16");
    assert_eq!(check_instance(&doc, &spec, &shown), Ok(()));
    let hidden = instance(4, "Compute $4^{2}$.", "16");
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

#[test]
fn the_body_read_reports_every_shape_it_owns() {
    let pool = exemplars(&["49"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &pool,
    };
    let read = |body: &str| gate_body(body, &spec).expect_err("the body is refused");
    let refusal = read("not json");
    assert_eq!(refusal.code, "body");
    assert!(
        refusal
            .message
            .starts_with("the template body is not JSON: "),
        "{}",
        refusal.message
    );
    let choice = "a choice domain needs a non-empty 'values' list";
    assert_eq!(
        read(r#"{"params": {"a": {"kind": "choice"}}}"#).message,
        choice
    );
    assert_eq!(
        read(r#"{"params": {"a": {"kind": "choice", "values": []}}}"#).message,
        choice
    );
    assert_eq!(
        read(r#"{"samples": [{"params": {}, "expected": 1.5}]}"#).message,
        "a sample needs 'params' and a scalar 'expected'"
    );
    let generic = [
        r#"{"samples": [{"params": {}, "expected": 18446744073709551615}]}"#,
        r#"{"samples": [{"params": {"a": 1}, "expected": 1}]}"#,
        r#"{"params": 1}"#,
        r#"{"params": {"a": {"kind": "rational"}}}"#,
        r#"{"v": 1}"#,
    ];
    for body in generic {
        let refusal = read(body);
        assert!(
            refusal
                .message
                .starts_with("the template body does not read: "),
            "{body}: {}",
            refusal.message
        );
    }
}

#[test]
fn a_stated_estimate_with_the_right_count_is_still_not_the_gates_count() {
    let rejection = reject_squares(&body_with(&[(
        "space_size",
        r#"{"estimate": 12, "samples": 1, "hits": 1}"#,
    )]));
    assert_eq!(rejection.code, "space-size");
    assert_eq!(
        rejection.message,
        "space_size states 12 and the gate counts 12 — the gate fills space_size, not the author"
    );
}

/// The base document with an operator choice `op` beside `a`.
fn with_operator(samples: &str, constraints: &str) -> String {
    body_with(&[
        ("statement", r#""Compute ${a} {op} 1$.""#),
        (
            "params",
            r#"{"a": {"kind": "int", "low": 1, "high": 12}, "op": {"kind": "choice", "values": ["+", "-"]}}"#,
        ),
        ("constraints", constraints),
        ("answer_expr", r#""a + 1""#),
        ("solution_sketch", r#""Add one to ${a}$.""#),
        ("samples", samples),
    ])
}

#[test]
fn a_text_choice_is_quoted_in_the_messages_that_name_it() {
    let rejection = reject_squares(&with_operator(
        r#"[{"params": {"a": 1, "op": "*"}, "expected": "2"},
            {"params": {"a": 12, "op": "-"}, "expected": "13"}]"#,
        "",
    ));
    assert_eq!(rejection.code, "sample-domain");
    assert_eq!(
        rejection.message,
        "sample 0 binds op='*', which its own domain cannot produce — a sample outside the domain verifies nothing"
    );
    let rejection = reject_squares(&with_operator(
        r#"[{"params": {"a": 1, "op": "+"}, "expected": "3"}]"#,
        "",
    ));
    assert_eq!(rejection.code, "sample-agreement");
    assert_eq!(
        rejection.message,
        "answer_expr gives '2' for {'a': 1, 'op': '+'} but the sample claims '3' — the expression does not compute the stated answer"
    );
    let rejection = reject_squares(&with_operator(
        r#"[{"params": {"a": 1, "op": "+"}, "expected": "2"}]"#,
        r#"[{"op": "eq", "left": "op", "right": {"lit": 1}}]"#,
    ));
    assert_eq!(rejection.code, "domain-size");
    assert_eq!(
        rejection.message,
        "the declared domains do not count: a constraint term names \"op\", which is bound to the text \"+\" and not to a number"
    );
}

#[test]
fn a_stray_brace_snippet_quotes_its_escapes() {
    let rejection = reject_squares(&body_with(&[("statement", r#""Compute {a} }'\n\r\t x""#)]));
    assert_eq!(rejection.code, "unescaped-brace");
    assert_eq!(
        rejection.message,
        "text has an unescaped brace at index 12 ('}\\'\\n\\r\\t x') — literal LaTeX braces must be doubled"
    );
}

#[test]
fn a_sample_the_sampled_walk_did_not_meet_can_still_break_a_constraint() {
    // `a` in 0..9999 and `b` in 1..2 is 20,000 tuples, so the walk samples.
    // `mod(b, a)` errors at a = 0 alone, and the seeded walk never draws it.
    let rejection = reject_numeric(&addition_body(
        (0, 9999),
        (1, 2),
        r#"[{"op": "ne", "left": {"mod": ["b", "a"]}, "right": {"lit": 5}}]"#,
        r#"[{"params": {"a": 0, "b": 1}, "expected": "1"},
            {"params": {"a": 9999, "b": 2}, "expected": "10001"},
            {"params": {"a": 0, "b": 2}, "expected": "2"}]"#,
    ));
    assert_eq!(rejection.code, "constraint-parameter");
    assert_eq!(rejection.message, "a mod term needs a non-zero right term");
}

#[test]
fn a_sample_the_expression_cannot_evaluate_and_a_distractor_the_grammar_refuses() {
    let rejection = reject_squares(&body_with(&[
        ("answer_expr", r#""1/(a-1)""#),
        (
            "samples",
            r#"[{"params": {"a": 1}, "expected": "1"},
                {"params": {"a": 12}, "expected": "1/11"}]"#,
        ),
    ]));
    assert_eq!(rejection.code, "sample-eval");
    assert_eq!(
        rejection.message,
        "answer_expr failed on sample {'a': 1}: answer_expr divides by zero"
    );
    let rejection = reject_squares(&body_with(&[(
        "distractors",
        r#"[{"answer": "a", "error_tag": " "}]"#,
    )]));
    assert_eq!(rejection.code, "distractor");
    assert_eq!(rejection.message, "distractor 0 carries no error_tag");
    let rejection = reject_squares(&body_with(&[(
        "distractors",
        r#"[{"answer": "a +", "error_tag": "slip"}]"#,
    )]));
    assert_eq!(rejection.code, "distractor");
    assert_eq!(
        rejection.message,
        "distractor 0 answers 'a +', which is outside the decidable grammar: the answer ends where a value belongs"
    );
}

#[test]
fn the_body_read_checks_every_choice_value() {
    let pool = exemplars(&["49"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &pool,
    };
    let read = |body: &str| gate_body(body, &spec).expect_err("the body is refused");
    assert_eq!(
        read(r#"{"params": {"a": {"kind": "choice", "values": [1.5]}}}"#).message,
        "choice values must be strings or integers"
    );
    let refusal = read(r#"{"params": {"a": {"kind": "choice", "values": ["x", 1]}}}"#);
    assert!(
        refusal
            .message
            .starts_with("the template body does not read: "),
        "{}",
        refusal.message
    );
}
