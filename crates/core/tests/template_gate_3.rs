//! Part 3 of the `template_gate` tests. The header of `template_gate_1.rs` names the sources.

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
fn a_distractor_that_is_the_right_answer_is_refused() {
    let rejection = reject_squares(&body_with(&[(
        "distractors",
        r#"[{"answer": "a*a", "error_tag": "doubled"}]"#,
    )]));
    assert_eq!(
        rejection.message,
        "distractor 0 answers '1' for {'a': 1}, which is the right answer — a distractor names a mistake"
    );
    assert_eq!(rejection.code, "distractor");
}

// --------------------------------------------------------------------------
// 6. The body read owns the shape rows the typed read enforces
// --------------------------------------------------------------------------
#[test]
fn the_body_read_reports_the_1_0_shape_rejections() {
    let pool = exemplars(&["49", "81"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &pool,
    };
    // The last row reads as a document — an empty list is a list — so the typed
    // read passes it to the gate and the gate refuses it. The message is the
    // same sentence either way, which is the point: one mistake, one wording.
    let cases: [(&str, &str, &str); 7] = [
        (
            r#"{"a": {"kind": "int", "low": 1.5, "high": 12}}"#,
            "an int domain needs integer 'low' and 'high'",
            "body",
        ),
        (
            r#"{"a": {"kind": "choice", "values": [true]}}"#,
            "choice values must be strings or integers",
            "body",
        ),
        (
            r#"{"a": 5}"#,
            "a parameter domain was not an object",
            "body",
        ),
        (
            r#"{"a": {"kind": "weird"}}"#,
            "unknown domain kind 'weird'",
            "body",
        ),
        (
            r#"{"a": {"low": 1, "high": 2}}"#,
            "unknown domain kind None",
            "body",
        ),
        (
            r#"{"a": {"kind": "int", "low": "1", "high": 12}}"#,
            "an int domain needs integer 'low' and 'high'",
            "body",
        ),
        (
            r#"{"a": {"kind": "choice", "values": []}}"#,
            "a choice domain needs a non-empty 'values' list",
            "choice-domain",
        ),
    ];
    for (params, expected, code) in cases {
        let body = body_with(&[("params", params)]);
        let rejection = gate_body(&body, &spec).expect_err("the body does not verify");
        assert_eq!(rejection.message, expected, "params {params}");
        assert_eq!(rejection.code, code, "params {params}");
    }

    for (field, value, expected) in [
        (
            "solution_sketch",
            "5",
            "solution_expr must be a string when present",
        ),
        ("samples", "[5]", "a sample was not an object"),
        (
            "samples",
            r#"[{"params": {"a": 1}}]"#,
            "a sample needs 'params' and a scalar 'expected'",
        ),
    ] {
        let body = body_with(&[(field, value)]);
        let rejection = gate_body(&body, &spec).expect_err("the body does not read");
        assert_eq!(rejection.message, expected, "{field} = {value}");
    }
}

#[test]
fn gate_body_reads_and_verifies_a_well_formed_body() {
    let pool = exemplars(&["49", "81"]);
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &pool,
    };
    let (doc, verified) = gate_body(&body_with(&[]), &spec).expect("the body verifies");
    assert_eq!(doc.answer_expr, "a**2");
    assert_eq!(doc.topic_id, "perfect-squares");
    assert_eq!(verified.space, SpaceSize::Exact(12));
}

// --------------------------------------------------------------------------
// 7. Every rejection substring the 1.0 tests assert
// --------------------------------------------------------------------------
#[test]
fn every_rejection_substring_of_the_specification_appears() {
    // `docs/reference/serving-1.0-spec.md` section 9, the row
    // "Rejection substrings asserted". Each substring is a literal here, and the
    // message it must appear in is produced by a real gate run.
    let messages: Vec<String> = vec![
        reject_squares(&body_with(&[("answer_expr", r#""a*2""#)])).message,
        reject_squares(&body_with(&[("statement", r#""Compute ${b}^{{2}}$.""#)])).message,
        reject_squares(&body_with(&[("statement", r#""Compute ${a}^{2}$.""#)])).message,
        reject_squares(&body_with(&[("samples", "[]")])).message,
        reject_squares(&body_with(&[("params", "{}")])).message,
        reject_squares(&body_with(&[(
            "params",
            r#"{"a": {"kind": "int", "low": 1, "high": 1000000}}"#,
        )]))
        .message,
        reject_squares(&body_with(&[(
            "params",
            r#"{"a": {"kind": "int", "low": 9, "high": 2}}"#,
        )]))
        .message,
        reject_squares(&body_with(&[("answer_expr", r#""a + b""#)])).message,
        reject_squares(&body_with(&[(
            "samples",
            r#"[{"params": {"q": 7}, "expected": "49"}]"#,
        )]))
        .message,
        reject_squares(&body_with(&[("answer_expr", r#""a**99999""#)])).message,
        reject_numeric(&live_rejection_one_body()).message,
        reject_numeric(&halving_body(
            r#"[{"params": {"a": 1}, "expected": "1/2"},
                        {"params": {"a": 12}, "expected": "6"}]"#,
        ))
        .message,
        reject_squares(&five_problems_body()).message,
    ];
    for wanted in [
        "does not compute the stated answer",
        "undeclared parameters",
        "unescaped brace",
        "worked samples",
        "at least one parameter",
        "MAX_DOMAIN_SIZE",
        "is empty",
        "unknown names",
        "template declares",
        "exceeds the evaluation bound",
        "non-negative",
        "whole number",
        "distinct problem",
    ] {
        assert!(
            messages.iter().any(|message| message.contains(wanted)),
            "no rejection carries {wanted:?}"
        );
    }
}

/// M4 review 1, finding 4: the answer never reads a parameter the learner never sees.
///
/// The reviewer's document renders `Compute $3$ squared.` for twelve values of
/// `b` and answers 10, 11, ... 21 for them. The pool keys an instance by the
/// digest of the statement, so it keeps one of the twelve and serves that one
/// answer for every learner who reads the same problem. Nothing downstream sees
/// the defect, because the two halves of one instance agree.
#[test]
fn a_parameter_the_statement_never_shows_is_refused() {
    let rejection = reject_squares(&body_with(&[
        ("statement", r#""Compute ${a}$ squared.""#),
        (
            "params",
            r#"{"a": {"kind": "int", "low": 1, "high": 12},
                "b": {"kind": "int", "low": 1, "high": 12}}"#,
        ),
        ("answer_expr", r#""a**2 + b""#),
        ("solution_sketch", r#""Multiply ${a}$ by itself.""#),
        ("hints", r#"["What does squaring a number mean?"]"#),
        (
            "samples",
            r#"[{"params": {"a": 1, "b": 1}, "expected": "2"},
                {"params": {"a": 1, "b": 12}, "expected": "13"},
                {"params": {"a": 12, "b": 1}, "expected": "145"},
                {"params": {"a": 12, "b": 12}, "expected": "156"}]"#,
        ),
    ]));
    assert_eq!(
        rejection.message,
        "parameter 'b' changes the answer but never appears in the statement"
    );
    assert_eq!(rejection.code, "hidden-parameter");
}

/// A parameter the statement shows and the answer reads is not hidden.
#[test]
fn a_parameter_the_statement_shows_passes_the_hidden_rule() {
    let verified = accept(&body_with(&[]), AnswerKind::Numeric, &["49", "81"]);
    assert_eq!(verified.space, SpaceSize::Exact(12));
}

/// M4 review 1, findings 7 and 12: a spent draw never ends the sampled walk.
///
/// Both documents are the reviewer's. The first is the one whose constraints
/// hold for about one draw in 2,048; the second holds for about one in 450.
/// Before the repair the walk stopped at the first draw that spent its budget,
/// and the gate answered `[no-satisfying-tuple] the constraints refuse every
/// tuple`, which the same gate call contradicts: it counts 20 and 17 tuples.
///
/// The counts are the ones a run of the fixed seed produces, and the seed is a
/// constant, so the numbers hold on every machine.
#[test]
fn a_sparse_constraint_is_walked_past_the_draws_that_spend_their_budget() {
    let sparse = accept_numeric(&addition_body(
        (4096, 8192),
        (1, 10),
        r#"[{"op": "eq", "left": {"mod": ["a", {"lit": 4096}]}, "right": {"lit": 0}}]"#,
        r#"[{"params": {"a": 4096, "b": 1}, "expected": "4097"},
                    {"params": {"a": 8192, "b": 10}, "expected": "8202"},
                    {"params": {"a": 4096, "b": 10}, "expected": "4106"}]"#,
    ));
    // The 20 satisfying tuples are `a` in {4096, 8192} times `b` in 1..10,
    // worked by hand. The walk spends the whole budget and finds every one of
    // them, so the recorded count is the true count and no estimator scales it
    // (M4 review 2, findings 2 and 5). 128 of the 262,144 draws satisfied the
    // constraint, and those 128 draws hold 20 distinct tuples.
    assert_eq!(
        sparse.space,
        SpaceSize::Estimated {
            estimate: 20,
            samples: 262_144,
            hits: 128,
        }
    );
    assert_eq!(sparse.instances_checked, 20);
    assert_eq!(
        sparse.notes,
        vec![
            "the sampled walk found 20 satisfying tuple(s) in 262144 draw(s), and the instance check read those 20"
                .to_string()
        ]
    );

    // 1..90 twice with `a = b` and `5 divides a`: the 18 tuples a = b = 5, 10,
    // ... 90, worked by hand, of 8,100 declared tuples.
    let paired = accept_numeric(&addition_body(
        (1, 90),
        (1, 90),
        r#"[{"op": "eq", "left": "a", "right": "b"},
                    {"op": "divides", "left": {"lit": 5}, "right": "a"}]"#,
        r#"[{"params": {"a": 5, "b": 5}, "expected": "10"},
                    {"params": {"a": 90, "b": 90}, "expected": "180"}]"#,
    ));
    // The walk finds all 18 of them, so the recorded count is the hand-worked
    // count. The deleted estimator scaled 9 hits of 4,096 draws to 17 and was
    // never the count of anything (M4 review 2, findings 2 and 5).
    assert_eq!(
        paired.space,
        SpaceSize::Estimated {
            estimate: 18,
            samples: 262_144,
            hits: 617,
        }
    );
    assert_eq!(paired.instances_checked, 18);
}

/// A sampled walk that finds nothing says what it drew.
///
/// The exhaustive branch keeps the 1.0 sentence, because it read every tuple.
/// The sampled branch read a sample, and the message says so (M4 review 1,
/// finding 12).
#[test]
fn a_sampled_walk_that_finds_no_tuple_names_the_count_it_found() {
    let rejection = reject_numeric(&addition_body(
        (1, 100),
        (200, 299),
        r#"[{"op": "eq", "left": "a", "right": "b"}]"#,
        r#"[{"params": {"a": 1, "b": 200}, "expected": "201"}]"#,
    ));
    assert_eq!(
        rejection.message,
        "the sampled walk drew 262144 tuple(s) of the declared domains and 0 satisfied the constraints, so the template has no instance to serve"
    );
    assert_eq!(rejection.code, "no-satisfying-tuple");
    assert_eq!(GATE_DRAW_BUDGET, 262_144);
}
