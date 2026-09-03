//! Part 2 of the `template_gate` tests. The header of `template_gate_1.rs` names the sources.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::gate::*;

#[test]
fn a_choice_domain_is_bounded_because_every_choice_must_be_sampled() {
    // `tests/test_problem_templates.py:540-556`.
    let values: Vec<String> = (0..=MAX_CHOICES)
        .map(|index| format!("\"{index}\""))
        .collect();
    let params = format!(
        r#"{{"a": {{"kind": "int", "low": 1, "high": 9}},
             "op": {{"kind": "choice", "values": [{}]}}}}"#,
        values.join(", ")
    );
    let rejection = reject_squares(&body_with(&[
        ("statement", r#""Compute ${a} + 1 {op}$.""#),
        ("params", params.as_str()),
        ("answer_expr", r#""a + 1""#),
        ("solution_sketch", r#""Add one to ${a}$.""#),
        ("hints", r#"["What is one more?"]"#),
        (
            "samples",
            r#"[{"params": {"a": 1, "op": "0"}, "expected": "2"}]"#,
        ),
    ]));
    assert_eq!(
        rejection.message,
        "a choice domain of 25 exceeds MAX_CHOICES (24); every choice must appear in a worked sample, so use an int domain or split the template"
    );
    assert_eq!(rejection.code, "choice-domain");
}

#[test]
fn a_backslash_in_a_choice_value_passes_the_gate() {
    // `tests/test_problem_templates.py:509-537`: `\times` as a choice value broke
    // the replacement side of the 1.0 regular expression, mid-serve, past the
    // error guard and into a 500. 2.0 renders through a scanner, so the class is
    // gone — and the specification (trap 6) asks for the pin anyway.
    let verified = accept(
        &body_with(&[
            ("statement", r#""Compute ${a} {sym} 2$.""#),
            (
                "params",
                r#"{"a": {"kind": "int", "low": 1, "high": 9},
                    "sym": {"kind": "choice", "values": ["\\times", "\\cdot"]}}"#,
            ),
            ("answer_expr", r#""a * 2""#),
            ("solution_sketch", r#""Double ${a}$.""#),
            ("hints", r#"["What does doubling mean?"]"#),
            (
                "samples",
                r#"[{"params": {"a": 1, "sym": "\\times"}, "expected": "2"},
                    {"params": {"a": 9, "sym": "\\times"}, "expected": "18"},
                    {"params": {"a": 5, "sym": "\\cdot"}, "expected": "10"}]"#,
            ),
        ]),
        AnswerKind::Numeric,
        &["49", "81"],
    );
    assert_eq!(verified.space, SpaceSize::Exact(18));
    assert_eq!(verified.instances_checked, 18);
}

// --------------------------------------------------------------------------
// 4. The pinned constants and the sampled branch
// --------------------------------------------------------------------------
#[test]
fn the_pinned_constants_hold_their_values() {
    // `docs/reference/serving-1.0-spec.md` section 9. 1.0 draws
    // `GATE_SAMPLES = 200`; 2.0 raises the count to the exhaustive limit, so the
    // walked branch and the drawn branch read the same number of instances.
    assert_eq!(GATE_SAMPLES, 4_096);
    assert_eq!(EXHAUSTIVE_SPACE_LIMIT, 4_096);
    assert_eq!(MIN_SPACE_SIZE, 12);
    assert_eq!(MAX_CHOICES, 24);
    assert_eq!(MAX_DOMAIN_SIZE, 10_000);
    assert_eq!(MAX_EXPONENT, 1_000);
    assert_eq!(TEMPLATE_VERSION, 1);
    assert_eq!(GATE_SEED, 0);
}

#[test]
fn a_space_too_large_to_walk_takes_the_sampled_branch() {
    // `tests/test_problem_templates.py:1172-1220`: 200 x 200 = 40,000 instances,
    // well above the walkable limit, and `a - b` goes negative for a < b.
    let rejection = reject_numeric(&subtraction_body(
        (100, 299),
        (100, 299),
        r#"[{"params": {"a": 299, "b": 100}, "expected": "199"},
                    {"params": {"a": 100, "b": 299}, "expected": "-199"},
                    {"params": {"a": 299, "b": 299}, "expected": "0"},
                    {"params": {"a": 100, "b": 100}, "expected": "0"}]"#,
    ));
    assert_eq!(rejection.code, "envelope-sign");
    assert!(
        rejection.message.ends_with(
            ", but every authored answer for this knowledge point is non-negative — narrow the domains so no instance goes below zero"
        ),
        "{}",
        rejection.message
    );
}

#[test]
fn the_sampled_branch_reports_its_found_count_and_its_draw_count() {
    // The same 200 x 200 space with an expression that never leaves the
    // envelope. 40,000 declared tuples and no constraint, so every drawn tuple
    // is a hit. The walk stops at GATE_SAMPLES distinct tuples, so the count it
    // reports is 4,096 of the 40,000: the number is a FLOOR of the satisfying
    // count and never a scaled guess (M4 review 2, findings 2 and 5). It took
    // 4,298 draws to reach 4,096 distinct tuples, and 202 of those draws
    // repeated a tuple the walk already held.
    let verified = accept_numeric(&addition_body(
        (100, 299),
        (100, 299),
        "",
        r#"[{"params": {"a": 100, "b": 100}, "expected": "200"},
                    {"params": {"a": 299, "b": 299}, "expected": "598"},
                    {"params": {"a": 100, "b": 299}, "expected": "399"}]"#,
    ));
    assert_eq!(
        verified.space,
        SpaceSize::Estimated {
            estimate: 4_096,
            samples: 4_298,
            hits: 4_298,
        }
    );
    assert_eq!(verified.instances_checked, 4_096);
    assert!(!verified.exhaustive);
    // Every tuple the instance check read is distinct, so the count of instances
    // and the recorded count are the same number.
    assert_eq!(verified.space.count(), 4_096);
}

// --------------------------------------------------------------------------
// 5. The 2.0 additions
// --------------------------------------------------------------------------
#[test]
fn the_space_is_the_satisfying_count_and_the_ends_come_from_it() {
    // `a > b` over 1..6 twice admits 15 of the 36 tuples, by hand: 5+4+3+2+1.
    // The lowest `a` of a SATISFYING tuple is 2, not the declared 1 — spec trap
    // 11, which says to read the ends off the satisfying set.
    let satisfying = |samples: &str| -> String {
        difference_body(
            (1, 6),
            (1, 6),
            r#"[{"op": "gt", "left": "a", "right": "b"}]"#,
            samples,
        )
    };
    let verified = accept_numeric(&satisfying(
        r#"[{"params": {"a": 2, "b": 1}, "expected": "1"},
                {"params": {"a": 6, "b": 5}, "expected": "1"},
                {"params": {"a": 6, "b": 1}, "expected": "5"}]"#,
    ));
    assert_eq!(verified.space, SpaceSize::Exact(15));
    assert_eq!(verified.instances_checked, 15);

    // Drop the sample at the satisfying low end of `a`, and the gate names 2.
    let rejection = reject_numeric(&satisfying(
        r#"[{"params": {"a": 3, "b": 1}, "expected": "2"},
                {"params": {"a": 6, "b": 5}, "expected": "1"},
                {"params": {"a": 6, "b": 1}, "expected": "5"}]"#,
    ));
    assert_eq!(
        rejection.message,
        "no worked sample uses the low end of a (2) — the edges are where an expression stops being right"
    );
}

#[test]
fn a_sample_outside_the_constraints_verifies_nothing() {
    let rejection = reject_numeric(&difference_body(
        (1, 6),
        (1, 6),
        r#"[{"op": "gt", "left": "a", "right": "b"}]"#,
        r#"[{"params": {"a": 2, "b": 5}, "expected": "-3"},
                    {"params": {"a": 6, "b": 5}, "expected": "1"},
                    {"params": {"a": 6, "b": 1}, "expected": "5"}]"#,
    ));
    assert_eq!(
        rejection.message,
        "sample 0 binds {'a': 2, 'b': 5}, which the gt constraint refuses — a sample outside the constraints verifies nothing"
    );
    assert_eq!(rejection.code, "sample-constraint");
}

#[test]
fn a_constraint_set_with_no_satisfying_tuple_is_refused() {
    let rejection = reject_numeric(&addition_body(
        (1, 12),
        (20, 31),
        r#"[{"op": "eq", "left": "a", "right": "b"}]"#,
        r#"[{"params": {"a": 1, "b": 20}, "expected": "21"}]"#,
    ));
    assert_eq!(
        rejection.message,
        "the constraints refuse every tuple of the declared domains, so the template has no instance to serve"
    );
    assert_eq!(rejection.code, "no-satisfying-tuple");
}

#[test]
fn a_stated_space_size_that_is_not_the_count_is_refused() {
    // `space_size` is the gate's field, never the author's
    // (`problem_templates.py:309`), and the 2.0 digest covers it (spec trap 8).
    let rejection = reject_squares(&body_with(&[("space_size", "20")]));
    assert_eq!(
        rejection.message,
        "space_size states 20 and the gate counts 12 — the gate fills space_size, not the author"
    );
    assert_eq!(rejection.code, "space-size");
}

#[test]
fn a_constraint_over_a_fractional_parameter_is_refused() {
    // Spec section 2.3: reject a template whose constraints read a non-integer
    // parameter under `divides`, `coprime`, `carries`, `mod`, or `digit_sum`.
    let rejection = reject_squares(&body_with(&[
        ("statement", r#""Compute ${a} \\times {r}$.""#),
        (
            "params",
            r#"{"a": {"kind": "int", "low": 1, "high": 12},
                "r": {"kind": "rational", "num": {"low": 1, "high": 3},
                      "den": {"low": 2, "high": 4}}}"#,
        ),
        (
            "constraints",
            r#"[{"op": "divides", "left": "a", "right": "r"}]"#,
        ),
        ("answer_expr", r#""a""#),
        ("solution_sketch", r#""Multiply ${a}$ by ${r}$.""#),
        ("samples", r#"[]"#),
    ]));
    assert_eq!(
        rejection.message,
        "the divides constraint reads whole numbers, and parameter 'r' draws values that are not whole"
    );
    assert_eq!(rejection.code, "constraint-whole");
}

#[test]
fn a_constraint_over_an_undeclared_parameter_is_refused() {
    let rejection = reject_squares(&body_with(&[(
        "constraints",
        r#"[{"op": "gt", "left": "a", "right": "b"}]"#,
    )]));
    assert_eq!(
        rejection.message,
        "a constraint term names undeclared parameter 'b'"
    );
    assert_eq!(rejection.code, "constraint-parameter");
}

#[test]
fn a_template_needs_a_hint_ladder() {
    let rejection = reject_squares(&body_with(&[("hints", "[]")]));
    assert_eq!(
        rejection.message,
        "a template needs at least one hint rung, and a hint may never give the answer away"
    );
    assert_eq!(rejection.code, "hint-missing");
}

#[test]
fn a_hint_that_names_the_answer_is_refused() {
    // Hard Rule 3: a hint is Socratic and never the final step. `a` of 4 answers
    // 16, and the statement `Compute $4^{2}$.` does not carry 16, so the rung
    // hands it over.
    let rejection = reject_squares(&body_with(&[(
        "hints",
        r#"["Remember that four squared is 16."]"#,
    )]));
    assert_eq!(
        rejection.message,
        "hint 0 reads 'Remember that four squared is 16.' for {'a': 4}, which names the answer '16' — a hint is a question, never the final step (Hard Rule 3)"
    );
    assert_eq!(rejection.code, "hint-answer");
}

#[test]
fn a_hint_that_names_an_undeclared_parameter_is_refused() {
    let rejection = reject_squares(&body_with(&[("hints", r#"["Look at {b} first."]"#)]));
    assert_eq!(rejection.message, "hint 0 uses undeclared parameters ['b']");
    assert_eq!(rejection.code, "hint-placeholder");
}

#[test]
fn the_exemplar_envelope_reads_exact_canonical_forms() {
    // Spec trap 12: 1.0 reads every exemplar answer through `float()`, which is
    // a float in a correctness decision. 2.0 reads the exact rational.
    assert_eq!(
        exemplar_envelope(&exemplars(&["49", "81"])),
        Some(Envelope {
            non_negative: true,
            integral: true
        })
    );
    assert_eq!(
        exemplar_envelope(&exemplars(&["1/2", "3/4"])),
        Some(Envelope {
            non_negative: true,
            integral: false
        })
    );
    assert_eq!(
        exemplar_envelope(&exemplars(&["-3", "8"])),
        Some(Envelope {
            non_negative: false,
            integral: true
        })
    );
    // A surd is not a plain number, so there is no envelope to read.
    assert_eq!(exemplar_envelope(&exemplars(&["2*sqrt(2)"])), None);
    // Neither is a knowledge point with no exemplar.
    assert_eq!(exemplar_envelope(&[]), None);
}

#[test]
fn a_whole_number_envelope_refuses_a_fractional_instance() {
    let rejection = reject_numeric(&halving_body(
        r#"[{"params": {"a": 1}, "expected": "1/2"},
                    {"params": {"a": 12}, "expected": "6"}]"#,
    ));
    assert_eq!(
        rejection.message,
        "instance {'a': 1} answers '1/2', but every authored answer for this knowledge point is a whole number"
    );
    assert_eq!(rejection.code, "envelope-integral");

    // With a fractional exemplar there is no integrality rule, and the same
    // document passes.
    let verified = accept(
        &halving_body(
            r#"[{"params": {"a": 1}, "expected": "1/2"},
                    {"params": {"a": 12}, "expected": "6"}]"#,
        ),
        AnswerKind::Numeric,
        &["1/2"],
    );
    assert_eq!(verified.space, SpaceSize::Exact(12));
}
