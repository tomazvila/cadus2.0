//! Part 2 of the `template_gate_edges` tests. The header of `template_gate_edges_1.rs` names the sources.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_core::template::gate_body;
use common::gate::*;

/// The refusal of the body read, under a numeric point with the exemplar 49.
fn read_refusal(body: &str) -> Rejection {
    let pool = exemplars(&["49"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &pool,
        finite: None,
    };
    gate_body(body, &spec).expect_err("the body is refused")
}

#[test]
fn the_body_read_reports_every_shape_it_owns() {
    let refusal = read_refusal("not json");
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
        read_refusal(r#"{"params": {"a": {"kind": "choice"}}}"#).message,
        choice
    );
    assert_eq!(
        read_refusal(r#"{"params": {"a": {"kind": "choice", "values": []}}}"#).message,
        choice
    );
    assert_eq!(
        read_refusal(r#"{"samples": [{"params": {}, "expected": 1.5}]}"#).message,
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
        let refusal = read_refusal(body);
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
fn an_unclosed_statement_math_delimiter_is_refused() {
    let rejection = reject_squares(&body_with(&[("statement", r#""Compute ${a}""#)]));
    assert_eq!(rejection.code, "math-delimiter");
    assert_eq!(
        rejection.message,
        "text has an unmatched '$' math delimiter"
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
    assert_eq!(
        read_refusal(r#"{"params": {"a": {"kind": "choice", "values": [1.5]}}}"#).message,
        "choice values must be strings or integers"
    );
    let refusal = read_refusal(r#"{"params": {"a": {"kind": "choice", "values": ["x", 1]}}}"#);
    assert!(
        refusal
            .message
            .starts_with("the template body does not read: "),
        "{}",
        refusal.message
    );
}

#[test]
fn the_body_read_takes_a_negative_number_as_an_integer() {
    // Each body fails the typed read for a missing field, so the shape check
    // runs, and a negative number is a whole number in every position.
    let generic = [
        r#"{"samples": [{"params": {"a": 1}, "expected": -4}]}"#,
        r#"{"params": {"a": {"kind": "int", "low": -3, "high": 2}}}"#,
        r#"{"params": {"a": {"kind": "choice", "values": [-1, 2]}}}"#,
    ];
    for body in generic {
        let refusal = read_refusal(body);
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
fn a_choice_domain_of_exactly_the_bound_passes() {
    let mut samples: Vec<String> = (0..MAX_CHOICES)
        .map(|index| format!(r#"{{"params": {{"a": 1, "op": "{index}"}}, "expected": "2"}}"#))
        .collect();
    samples.push(r#"{"params": {"a": 9, "op": "0"}, "expected": "10"}"#.to_string());
    let body = choice_body(MAX_CHOICES, &format!("[{}]", samples.join(", ")));
    let verified = accept(&body, AnswerKind::Numeric, &["49", "81"]);
    assert_eq!(verified.space, SpaceSize::Exact(216));
}

#[test]
fn a_digit_sum_inside_a_plain_comparison_needs_a_whole_parameter() {
    let rejection = reject_squares(&rational_r_body(
        r#"[{"op": "eq", "left": {"digit_sum": "r"}, "right": {"lit": 1}}]"#,
    ));
    assert_eq!(
        rejection.message,
        "the eq constraint reads whole numbers, and parameter 'r' draws values that are not whole"
    );
    assert_eq!(rejection.code, "constraint-whole");
}
