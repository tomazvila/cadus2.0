//! Part 5 of the `template_gate` tests. The header of `template_gate_1.rs` names the sources.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::gate::*;

/// The four corners of `a` and `b` over 1..12, with the product of each pair.
const PRODUCT_SAMPLES: &str = r#"[{"params": {"a": 1, "b": 1}, "expected": "1"},
    {"params": {"a": 1, "b": 12}, "expected": "12"},
    {"params": {"a": 12, "b": 1}, "expected": "12"},
    {"params": {"a": 12, "b": 12}, "expected": "144"}]"#;

/// The same four corners, with the sum of each pair.
const SUM_SAMPLES: &str = r#"[{"params": {"a": 1, "b": 1}, "expected": "2"},
    {"params": {"a": 1, "b": 12}, "expected": "13"},
    {"params": {"a": 12, "b": 1}, "expected": "13"},
    {"params": {"a": 12, "b": 12}, "expected": "24"}]"#;

/// M4 review 2, finding 1: one statement carries one answer (C4).
///
/// Every parameter of this document appears in the statement, so the
/// hidden-parameter rule of review 1 finding 4 passes it. The statement writes
/// the two numbers next to each other, so `a = 1, b = 12` and `a = 11, b = 2`
/// render the same text `$112$` and the same digest, and the product of the two
/// tuples is 12 and 22. `serving_pool` keys a row by that digest, so the pool
/// keeps ONE of the two answers and a learner who reads the other one is graded
/// wrong.
#[test]
fn one_statement_that_two_tuples_answer_differently_is_refused() {
    let rejection = reject_squares(&adjacent(
        "Multiply",
        "product",
        r#""a * b""#,
        PRODUCT_SAMPLES,
    ));
    assert_eq!(rejection.code, "statement-collision");
    assert_eq!(
        rejection.message,
        "statement 'A code is made by writing one number next to another: $112$. Multiply the two numbers that were written. What is the product?' renders from 2 tuples with different answers"
    );
}

/// The rule reads the ANSWERS, and never the count of digests.
///
/// The same statement shape with the sum of the two numbers renders `$112$`
/// from the same two tuples, and both of them answer 13. One statement, one
/// answer: the gate accepts it and the pool keeps one row for the two tuples.
#[test]
fn one_statement_that_two_tuples_answer_alike_is_accepted() {
    let verified = accept(
        &adjacent("Add", "sum", r#""a + b""#, SUM_SAMPLES),
        AnswerKind::Numeric,
        &["49", "81"],
    );
    assert_eq!(verified.space, SpaceSize::Exact(144));
    assert_eq!(verified.instances_checked, 144);
    assert!(verified.exhaustive);
}

#[test]
fn a_pythagorean_triple_template_is_accepted_above_the_limit() {
    let triples = |samples: &str| -> String {
        body_with(&[
            (
                "statement",
                r#""A right triangle has legs ${a}$ and ${b}$, and a hypotenuse of ${c}$. What is the perimeter?""#,
            ),
            (
                "params",
                r#"{"a": {"kind": "int", "low": 1, "high": 50},
                    "b": {"kind": "int", "low": 1, "high": 50},
                    "c": {"kind": "int", "low": 1, "high": 50}}"#,
            ),
            (
                "constraints",
                r#"[{"op": "eq",
                     "left": {"add": [{"mul": ["a", "a"]}, {"mul": ["b", "b"]}]},
                     "right": {"mul": ["c", "c"]}}]"#,
            ),
            ("answer_expr", r#""a + b + c""#),
            ("solution_sketch", r#""Add the three side lengths.""#),
            ("hints", r#"["Which three lengths make the way around?"]"#),
            ("samples", samples),
        ])
    };
    let verified = accept_numeric(&triples(
        r#"[{"params": {"a": 3, "b": 4, "c": 5}, "expected": "12"},
                {"params": {"a": 4, "b": 3, "c": 5}, "expected": "12"},
                {"params": {"a": 40, "b": 9, "c": 41}, "expected": "90"},
                {"params": {"a": 9, "b": 40, "c": 41}, "expected": "90"},
                {"params": {"a": 14, "b": 48, "c": 50}, "expected": "112"}]"#,
    ));
    assert_eq!(verified.space.count(), PYTHAGOREAN_FOUND);
    assert_eq!(verified.instances_checked, PYTHAGOREAN_FOUND);
    assert!(!verified.exhaustive);
    // The count is a floor of the hand-worked 40 and it clears the floor of 12.
    assert!(verified.space.count() < PYTHAGOREAN_TUPLES);
    assert!(verified.space.count() >= MIN_SPACE_SIZE);
    assert_eq!(
        verified.space,
        SpaceSize::Estimated {
            estimate: 35,
            samples: 262_144,
            hits: 90,
        }
    );
}

/// M4 review 2, findings 3 and 9: a constrained choice axis is approvable.
///
/// `n` in 1..12 over the divisor list [2,3,4,5,6,8,10,12] with `d divides n` and
/// `n != d` admits 12 tuples: d=2 takes n in {4,6,8,10,12}, d=3 takes {6,9,12},
/// d=4 takes {8,12}, d=5 takes {10}, and d=6 takes {12}. The constraints admit
/// no tuple at d=8, d=10, or d=12, because the only multiple of each of them in
/// 1..12 is the divisor itself.
///
/// The rule read the DECLARED choice list, so it asked for a worked sample at
/// d=8, and the sample-constraint rule refused exactly that sample. No sample
/// list cleared both rules and the document was unapprovable.
#[test]
fn a_constrained_choice_axis_is_covered_by_its_reachable_values() {
    let division = |samples: &str| -> String {
        body_with(&[
            ("statement", r#""Divide ${n}$ by ${d}$.""#),
            (
                "params",
                r#"{"n": {"kind": "int", "low": 1, "high": 12},
                    "d": {"kind": "choice", "values": [2, 3, 4, 5, 6, 8, 10, 12]}}"#,
            ),
            (
                "constraints",
                r#"[{"op": "divides", "left": "d", "right": "n"},
                    {"op": "ne", "left": "n", "right": "d"}]"#,
            ),
            ("answer_expr", r#""n / d""#),
            (
                "solution_sketch",
                r#""Share ${n}$ into ${d}$ equal groups.""#,
            ),
            ("hints", r#"["How many groups do you need?"]"#),
            ("samples", samples),
        ])
    };
    let every_reachable = r#"[{"params": {"n": 4, "d": 2}, "expected": "2"},
        {"params": {"n": 6, "d": 3}, "expected": "2"},
        {"params": {"n": 8, "d": 4}, "expected": "2"},
        {"params": {"n": 10, "d": 5}, "expected": "2"},
        {"params": {"n": 12, "d": 6}, "expected": "2"}]"#;
    let verified = accept_numeric(&division(every_reachable));
    assert_eq!(verified.space, SpaceSize::Exact(12));
    assert_eq!(verified.instances_checked, 12);

    // A reachable choice with no worked sample is still refused: drop the sample
    // that binds d=5, and d=5 is the value the gate names.
    let rejection = reject_numeric(&division(
        r#"[{"params": {"n": 4, "d": 2}, "expected": "2"},
                {"params": {"n": 6, "d": 3}, "expected": "2"},
                {"params": {"n": 8, "d": 4}, "expected": "2"},
                {"params": {"n": 12, "d": 6}, "expected": "2"}]"#,
    ));
    assert_eq!(rejection.code, "choice-coverage");
    assert_eq!(
        rejection.message,
        "no worked sample uses d=['5'] — every choice must appear in a sample, or the expression is unverified for it"
    );
}

/// M4 review 2, finding 10: an unconstrained axis reads its DECLARED ends.
///
/// `a` in 1..10000 with no constraint puts the document above the exhaustive
/// limit. Every declared value lies in a satisfying tuple, so both declared ends
/// are reachable, and the gate asks for a worked sample at 1 and at 10000. The
/// sampled ends of review 1 finding 16 asked for a worked sample at 2, a number
/// that appears nowhere in the document: it was the smallest value the draws of
/// `GATE_SEED` happened to hit.
#[test]
fn an_unconstrained_axis_reads_its_declared_ends_above_the_limit() {
    let squares = |samples: &str| -> String {
        body_with(&[
            (
                "params",
                r#"{"a": {"kind": "int", "low": 1, "high": 10000}}"#,
            ),
            ("samples", samples),
        ])
    };
    let verified = accept(
        &squares(
            r#"[{"params": {"a": 1}, "expected": "1"},
                {"params": {"a": 10000}, "expected": "100000000"}]"#,
        ),
        AnswerKind::Numeric,
        &["49", "81"],
    );
    assert!(!verified.exhaustive);
    assert_eq!(verified.instances_checked, 4_096);

    // The declared low end is the one the rule names, and a sample at 2 does not
    // cover it. The gate's own comment names `a = 1` of this template as the
    // degenerate instance that matters.
    let rejection = reject(
        &squares(
            r#"[{"params": {"a": 2}, "expected": "4"},
                {"params": {"a": 10000}, "expected": "100000000"}]"#,
        ),
        AnswerKind::Numeric,
        &["49", "81"],
    );
    assert_eq!(rejection.code, "edge-coverage");
    assert_eq!(
        rejection.message,
        "no worked sample uses the low end of a (1) — the edges are where an expression stops being right"
    );
}

/// M4 review 2, finding 10: a sample at a declared end covers that end.
///
/// `a > b` names both axes, so the ends of `a` come from the satisfying sample
/// and the sample above the limit misses the declared high end 10000. A worked
/// sample AT 10000 is inside the constraints and it verifies the true edge, so
/// the rule takes it. Without that exemption the author must run the gate and
/// copy the seed's own maximum back into the document.
#[test]
fn a_sample_at_a_declared_end_covers_a_constrained_axis() {
    let banded = |samples: &str| -> String {
        difference_body(
            (1, 10000),
            (1, 2),
            r#"[{"op": "gt", "left": "a", "right": "b"}]"#,
            samples,
        )
    };
    let verified = accept_numeric(&banded(
        r#"[{"params": {"a": 5, "b": 1}, "expected": "4"},
                {"params": {"a": 5, "b": 2}, "expected": "3"},
                {"params": {"a": 10000, "b": 2}, "expected": "9998"}]"#,
    ));
    assert!(!verified.exhaustive);

    // The exemption reads a DECLARED end and nothing else: a sample below the
    // sampled high end is still refused, and the gate names the end it read.
    let rejection = reject_numeric(&banded(
        r#"[{"params": {"a": 5, "b": 1}, "expected": "4"},
                {"params": {"a": 5, "b": 2}, "expected": "3"},
                {"params": {"a": 9990, "b": 2}, "expected": "9988"}]"#,
    ));
    assert_eq!(rejection.code, "edge-coverage");
}

/// M4 review 2, findings 2 and 5: the floor holds above the exhaustive limit.
///
/// The reviewer's document: `a` and `b` in 1..1000 with `a = b` and `202`
/// divides `a`, which admits four tuples — a = b = 202, 404, 606, and 808.
/// 1,000,000 declared tuples put it above the limit, and the deleted estimator
/// scaled its one hit in 4,096 draws to a space of 244, cleared the floor of 12,
/// and stored 244 in a body a human then approved. The pool of that knowledge
/// point can hold four rows, and the D5 ring holds twenty digests, so every
/// serve after the fourth is a ring hit forever.
///
/// The walk counts what it found and the floor refuses the document. The
/// constraints are sparse — four tuples in a million — so the 262,144 draws of
/// the budget find one of the four, and the gate names that one. Four is under
/// the floor of twelve as well, so the verdict holds for the true count too.
#[test]
fn a_space_under_the_floor_above_the_limit_is_refused() {
    let rejection = reject_numeric(&body_with(&[
        ("statement", r#""Compute ${a} \\times {b}$.""#),
        (
            "params",
            r#"{"a": {"kind": "int", "low": 1, "high": 1000},
                    "b": {"kind": "int", "low": 1, "high": 1000}}"#,
        ),
        (
            "constraints",
            r#"[{"op": "eq", "left": "a", "right": "b"},
                    {"op": "divides", "left": {"lit": 202}, "right": "a"}]"#,
        ),
        ("answer_expr", r#""a * b""#),
        ("solution_sketch", r#""Multiply ${a}$ by ${b}$.""#),
        ("hints", r#"["Which two numbers do you multiply?"]"#),
        (
            "samples",
            r#"[{"params": {"a": 202, "b": 202}, "expected": "40804"},
                    {"params": {"a": 808, "b": 808}, "expected": "652864"}]"#,
        ),
    ]));
    assert_eq!(rejection.code, "space-floor");
    assert_eq!(
        rejection.message,
        "the declared domains produce only 1 distinct problem(s); at least 12 are needed for randomized values and for avoidance of a recently-served problem to mean anything (Hard Rule 4)"
    );
}
