//! Part 4 of the `template_gate` tests. The header of `template_gate_1.rs` names the sources.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::gate::*;

/// M4 review 1, finding 16: above the limit the ends come from the satisfying sample.
///
/// The A1 flagship shape — `a > b` over 1..100 twice — was unapprovable: the
/// gate read the DECLARED low end of `a`, which is 1, and asked for a worked
/// sample there; every such sample breaks `a > b`, and the sample-constraint
/// rule then refused it. The satisfying sample of the fixed seed holds `a` from
/// 2 to 100 and `b` from 1 to 98, and two samples at those ends pass.
#[test]
fn above_the_limit_the_axis_ends_come_from_the_satisfying_sample() {
    let flagship = |samples: &str| -> String {
        difference_body(
            (1, 100),
            (1, 100),
            r#"[{"op": "gt", "left": "a", "right": "b"}]"#,
            samples,
        )
    };
    let verified = accept_numeric(&flagship(
        r#"[{"params": {"a": 2, "b": 1}, "expected": "1"},
                {"params": {"a": 100, "b": 98}, "expected": "2"}]"#,
    ));
    // 4,950 tuples of the 10,000 satisfy `a > b`, by hand: 99 + 98 + ... + 1.
    // The walk stops at GATE_SAMPLES distinct tuples, so it records 4,096 of
    // them: 17,666 draws, 8,662 of which the constraint accepted.
    assert_eq!(
        verified.space,
        SpaceSize::Estimated {
            estimate: 4_096,
            samples: 17_666,
            hits: 8_662,
        }
    );
    assert_eq!(verified.instances_checked, 4_096);
    assert!(!verified.exhaustive);
    // One corner of the two is outside `a > b`, so the crossed-corner rule is
    // skipped and the reason is recorded (finding 22).
    assert_eq!(
        verified.notes,
        vec![
            "the crossed-corner rule is skipped for a and b: the constraints admit no tuple at a=2 with b=98"
                .to_string()
        ]
    );

    // The gate names the satisfying end, and never the declared end 1.
    let rejection = reject_numeric(&flagship(
        r#"[{"params": {"a": 3, "b": 1}, "expected": "2"},
                {"params": {"a": 100, "b": 98}, "expected": "2"}]"#,
    ));
    assert_eq!(
        rejection.message,
        "no worked sample uses the low end of a (2) — the edges are where an expression stops being right"
    );
    assert_eq!(rejection.code, "edge-coverage");
}

/// M4 review 1, finding 22: a band constraint is approvable with edge samples.
///
/// `b < a < b + 3` over 1..10 twice admits 17 tuples, worked by hand: 9 tuples
/// with a difference of 1 and 8 with a difference of 2. Neither crossed corner —
/// `a = 2` with `b = 9`, nor `a = 10` with `b = 1` — satisfies the band, so the
/// crossed-corner rule says nothing, and the gate records that instead of asking
/// for a sample its own sample-constraint rule refuses.
#[test]
fn a_band_constrained_template_is_approvable_and_the_skip_is_recorded() {
    let verified = accept_numeric(&difference_body(
        (1, 10),
        (1, 10),
        r#"[{"op": "gt", "left": "a", "right": "b"},
                    {"op": "lt", "left": "a", "right": {"add": ["b", {"lit": 3}]}}]"#,
        r#"[{"params": {"a": 2, "b": 1}, "expected": "1"},
                    {"params": {"a": 10, "b": 9}, "expected": "1"},
                    {"params": {"a": 3, "b": 1}, "expected": "2"},
                    {"params": {"a": 10, "b": 8}, "expected": "2"}]"#,
    ));
    assert_eq!(verified.space, SpaceSize::Exact(17));
    assert_eq!(verified.instances_checked, 17);
    assert_eq!(
        verified.notes,
        vec![
            "the crossed-corner rule is skipped for a and b: the constraints admit no tuple at a=2 with b=9, nor at a=10 with b=1"
                .to_string()
        ]
    );
}

/// M4 review 1, finding 9: a decimal parameter reaches the learner as a decimal.
///
/// The choice values are decimals, and the statement shows them as the author
/// wrote them. The answer is exact, because the value behind the spelling is the
/// rational 1/5.
#[test]
fn a_decimal_choice_serves_the_spelling_its_author_wrote() {
    let verified = accept(&decimal_choice_body(), AnswerKind::Numeric, &["2/5"]);
    assert_eq!(verified.space, SpaceSize::Exact(16));

    let doc = doc_of(&decimal_choice_body());
    let compiled = Compiled::new(&doc).expect("the template compiles");
    let mut bindings = bind(&[("n", 3)]);
    bindings.insert(
        "d".to_string(),
        cadus_core::template::Scalar::Text("0.2".to_string()).value(),
    );
    let instance = compiled
        .instantiate(bindings)
        .expect("the tuple instantiates");
    assert_eq!(instance.text, "Compute $0.2 \\times 3$.");
    assert_eq!(instance.answer, "3/5");
}

/// A decimal domain draws decimals, and the gate reads its ends.
#[test]
fn a_decimal_domain_serves_decimals() {
    let verified = accept(
        &decimal_product_body(
            r#"{"kind": "decimal", "low": 1, "high": 9, "scale": 1}"#,
            r#"[{"params": {"d": "0.1", "n": 2}, "expected": "1/5"},
                    {"params": {"d": "0.9", "n": 9}, "expected": "81/10"},
                    {"params": {"d": "0.1", "n": 9}, "expected": "9/10"},
                    {"params": {"d": "0.9", "n": 2}, "expected": "9/5"}]"#,
        ),
        AnswerKind::Numeric,
        &["2/5"],
    );
    assert_eq!(verified.space, SpaceSize::Exact(72));
    assert_eq!(verified.instances_checked, 72);
}

/// M4 review 1, findings 13 and 17: an expression answer keeps its brackets.
///
/// No test instantiated an expression template before, so the two bracket rules
/// of the answer writer were unverified. `a*(x + 1)` needs the brackets around
/// the sum, and `a*(x**2)**3` needs them around the inner power: without them
/// the answer leaves the decidable grammar and every instance is refused.
#[test]
fn an_expression_answer_brackets_a_sum_and_a_nested_power() {
    let sum_body = body_with(&[
        ("topic_id", r#""distribute-over-a-sum""#),
        ("answer_kind", r#""expression""#),
        ("statement", r#""Expand ${a}(x + 1)$.""#),
        ("params", r#"{"a": {"kind": "int", "low": 2, "high": 13}}"#),
        ("answer_expr", r#""a*(x + 1)""#),
        ("solution_sketch", r#""Multiply each term by ${a}$.""#),
        ("hints", r#"["What does each term multiply?"]"#),
        (
            "samples",
            r#"[{"params": {"a": 2}, "expected": "2*x + 2"},
                {"params": {"a": 13}, "expected": "13*x + 13"}]"#,
        ),
    ]);
    let verified = accept(&sum_body, AnswerKind::Expression, &[]);
    assert_eq!(verified.space, SpaceSize::Exact(12));
    let sum_doc = doc_of(&sum_body);
    let compiled = Compiled::new(&sum_doc).expect("the template compiles");
    let instance = compiled
        .instantiate(bind(&[("a", 7)]))
        .expect("a = 7 instantiates");
    assert_eq!(instance.text, "Expand $7(x + 1)$.");
    assert_eq!(instance.answer, "7*(1 + x)");

    let power_body = body_with(&[
        ("topic_id", r#""powers-of-powers""#),
        ("answer_kind", r#""expression""#),
        (
            "statement",
            r#""Simplify ${a}\\left(x^{{2}}\\right)^{{3}}$.""#,
        ),
        ("params", r#"{"a": {"kind": "int", "low": 2, "high": 14}}"#),
        ("answer_expr", r#""a*(x**2)**3""#),
        ("solution_sketch", r#""Multiply the two exponents.""#),
        ("hints", r#"["What happens to the exponents?"]"#),
        (
            "samples",
            r#"[{"params": {"a": 2}, "expected": "2*x**6"},
                {"params": {"a": 14}, "expected": "14*x**6"}]"#,
        ),
    ]);
    let verified = accept(&power_body, AnswerKind::Expression, &[]);
    assert_eq!(verified.space, SpaceSize::Exact(13));
    let power_doc = doc_of(&power_body);
    let compiled = Compiled::new(&power_doc).expect("the template compiles");
    let instance = compiled
        .instantiate(bind(&[("a", 2)]))
        .expect("a = 2 instantiates");
    assert_eq!(instance.text, "Simplify $2\\left(x^{2}\\right)^{3}$.");
    assert_eq!(instance.answer, "2*(x**2)**3");
}

/// M4 review 1, finding 14: the exact root reads the denominator too.
///
/// `sqrt(4/3)` is not 2, and `sqrt(4/9)` is 2/3. Every earlier sqrt case bound a
/// whole number, so the denominator half of the perfect-square test never ran.
#[test]
fn the_exact_square_root_reads_a_radicand_that_is_not_whole() {
    let body = body_with(&[
        ("topic_id", r#""square-roots-of-fractions""#),
        ("statement", r#""Compute $\\sqrt{{4/{a}}}$.""#),
        ("params", r#"{"a": {"kind": "int", "low": 1, "high": 12}}"#),
        ("answer_expr", r#""sqrt(4/a)""#),
        (
            "solution_sketch",
            r#""Take the root of the top and the bottom.""#,
        ),
        ("hints", r#"["What is the square root of the top?"]"#),
        (
            "samples",
            r#"[{"params": {"a": 1}, "expected": "2"},
                {"params": {"a": 12}, "expected": "sqrt(1/3)"}]"#,
        ),
    ]);
    let verified = accept(&body, AnswerKind::Numeric, &[]);
    assert_eq!(verified.space, SpaceSize::Exact(12));
    let doc = doc_of(&body);
    let compiled = Compiled::new(&doc).expect("the template compiles");
    for (value, wanted) in [(1, "2"), (3, "sqrt(4/3)"), (9, "2/3"), (12, "sqrt(1/3)")] {
        let instance = compiled
            .instantiate(bind(&[("a", value)]))
            .expect("the tuple instantiates");
        assert_eq!(instance.answer, wanted, "sqrt(4/{value})");
    }
}

/// The per-instance check the refill runs is the per-instance check of the gate.
///
/// The refill of D-O4 draws tuples the gate never saw, so it runs this on every
/// instance before the instance enters the pool (M4 review 1, findings 1, 2, and
/// 15). The document is the reviewer's live rejection 1: `a - b` over 59..70 and
/// 63..63 answers -4 for the first tuple, and every authored answer of the
/// knowledge point is non-negative.
#[test]
fn the_per_instance_check_refuses_one_instance_the_gate_refuses() {
    let body = subtraction_body(
        (59, 70),
        (63, 63),
        r#"[{"params": {"a": 59, "b": 63}, "expected": "-4"},
                {"params": {"a": 70, "b": 63}, "expected": "7"}]"#,
    );
    let doc = doc_of(&body);
    let pool = exemplars(&["25"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &pool,
        finite: None,
    };
    let compiled = Compiled::new(&doc).expect("the template compiles");

    let refused = compiled
        .instantiate(bind(&[("a", 59), ("b", 63)]))
        .expect("the tuple instantiates");
    assert_eq!(refused.answer, "-4");
    let rejection = check_instance(&doc, &spec, &refused).expect_err("the envelope refuses it");
    assert_eq!(
        rejection.message,
        "instance {'a': 59, 'b': 63} answers '-4', but every authored answer for this knowledge point is non-negative — narrow the domains so no instance goes below zero"
    );
    assert_eq!(rejection.code, "envelope-sign");

    let served = compiled
        .instantiate(bind(&[("a", 70), ("b", 63)]))
        .expect("the tuple instantiates");
    assert_eq!(served.answer, "7");
    assert!(check_instance(&doc, &spec, &served).is_ok());
}

/// The per-instance check reads the hint rule and the canonical round trip.
#[test]
fn the_per_instance_check_reads_the_hint_rule() {
    let body = body_with(&[(
        "hints",
        r#"["Remember that four squared is 16.", "What does squaring mean?"]"#,
    )]);
    let doc = doc_of(&body);
    let pool = exemplars(&["49", "81"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &pool,
        finite: None,
    };
    let instance = squares_instance(&doc, 4);
    assert_eq!(instance.answer, "16");
    let rejection = check_instance(&doc, &spec, &instance).expect_err("the hint gives the answer");
    assert_eq!(rejection.code, "hint-answer");
    assert_eq!(
        rejection.message,
        "hint 0 reads 'Remember that four squared is 16.' for {'a': 4}, which names the answer '16' — a hint is a question, never the final step (Hard Rule 3)"
    );
}

/// The per-instance check reads the canonical round trip (V2).
///
/// The refill stores the answer string and the pool row carries it. A string the
/// M2 checker cannot decide grades nothing, and a string that decides to another
/// value grades every correct learner wrong, so the check reads the string back
/// and compares it with the canonical form the instance carries.
#[test]
fn the_per_instance_check_reads_the_answer_string_back() {
    let doc = doc_of(&body_with(&[]));
    let pool = exemplars(&["49", "81"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &pool,
        finite: None,
    };
    let instance = squares_instance(&doc, 5);
    assert_eq!(instance.answer, "25");
    assert!(check_instance(&doc, &spec, &instance).is_ok());

    let mut wrong = instance.clone();
    wrong.answer = "26".to_string();
    let rejection = check_instance(&doc, &spec, &wrong).expect_err("26 is not the canonical form");
    assert_eq!(rejection.code, "canonical-mismatch");
    assert_eq!(
        rejection.message,
        "instance {'a': 5} answers '26', which does not read back as the canonical form the instance carries — the answer and its canonical form must agree (V2)"
    );

    let mut undecidable = instance;
    undecidable.answer = "1 +".to_string();
    let rejection =
        check_instance(&doc, &spec, &undecidable).expect_err("the checker cannot decide it");
    assert_eq!(rejection.code, "undecidable-answer");
    assert!(
        rejection.message.starts_with(
            "instance {'a': 5} answers '1 +', which the answer checker cannot decide: "
        ),
        "{}",
        rejection.message
    );
}
