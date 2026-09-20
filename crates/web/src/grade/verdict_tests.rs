//! Unit tests of the pure grade: the clock, the rounding, and the guidance texts.

use super::*;
use cadus_core::answer::AnswerContract;
use cadus_core::pool::PoolAnswer;

/// One Unix microsecond instant, 4,000 seconds after `started_at` 0.
const LATER_US: i64 = 4_000_000_000;

/// A topic outside the arena has no authored solve time, so 1.0 skips the
/// clamp: the whole elapsed time is recorded and no tag is set.
#[test]
fn a_topic_with_no_expected_time_is_never_clamped() {
    assert_eq!(measure_secs(0.0, LATER_US, None), (4_000, Vec::new()));
}

/// A hand-off stamped in the future measures zero, never a negative time.
#[test]
fn a_future_hand_off_measures_zero() {
    assert_eq!(measure_secs(9_000.0, LATER_US, Some(30)), (0, Vec::new()));
}

/// The cast is guarded at both ends: a time past the `i64` range saturates
/// at the maximum, and a time at or below zero is zero.
#[test]
fn whole_secs_is_bounded_at_both_ends() {
    assert_eq!(whole_secs(f64::MAX), i64::MAX);
    assert_eq!(whole_secs(f64::INFINITY), i64::MAX);
    assert_eq!(whole_secs(-1.0), 0);
    assert_eq!(whole_secs(0.0), 0);
    assert_eq!(whole_secs(12.0), 12);
}

/// The XP rounding is two places, half away from zero, as `round()` is.
#[test]
fn round2_keeps_two_places() {
    assert!((round2(1.049) - 1.05).abs() < f64::EPSILON);
    assert!((round2(2.004) - 2.0).abs() < f64::EPSILON);
    assert!((round2(7.0) - 7.0).abs() < f64::EPSILON);
}

// H-1 (ISSUES.md): a unit-contract refusal teaches the format, the way the
// ISSUE-2 ruling made the trailing-text refusal teach it.

/// A unitless answer to a measured value reads the unit out of the authored
/// answer and shows an example that is not the answer.
#[test]
fn a_unitless_answer_names_the_unit_to_write() {
    let grade = deterministic_grade("30°", "30", AnswerKind::Numeric);
    assert_eq!(
        grade.outcome.reason(),
        Some(
            "This answer is a measurement, so it needs its unit. Write the value with the \
             unit °, for example 42°."
        )
    );
    let grade = deterministic_grade("56.5 cm", "56.5", AnswerKind::Numeric);
    assert!(
        grade
            .outcome
            .reason()
            .is_some_and(|reason| reason.contains("unit cm, for example 42 cm."))
    );
}

/// The unit read survives a label, and a currency answers in its own order.
#[test]
fn the_unit_guidance_reads_the_authored_spelling() {
    let grade = deterministic_grade("d = 5 cm", "5", AnswerKind::Numeric);
    assert!(
        grade
            .outcome
            .reason()
            .is_some_and(|reason| reason.contains("unit cm"))
    );
    let grade = deterministic_grade("$5", "5", AnswerKind::Numeric);
    assert!(
        grade
            .outcome
            .reason()
            .is_some_and(|reason| reason.contains("for example $42."))
    );
    let grade = deterministic_grade("5 €", "5", AnswerKind::Numeric);
    assert!(
        grade
            .outcome
            .reason()
            .is_some_and(|reason| reason.contains("for example 42€."))
    );
}

/// A unit the learner alone carried tells them to drop it; a unit inside an
/// expression tells them where a unit may stand.
#[test]
fn the_other_unit_refusals_teach_their_own_form() {
    let grade = deterministic_grade("13.5", "13.5 cm", AnswerKind::Numeric);
    assert_eq!(
        grade.outcome.reason(),
        Some(
            "The expected answer here is a bare number, so enter the value alone, without a unit."
        )
    );
    let grade = deterministic_grade("30°", "sin(30°)", AnswerKind::Expression);
    assert_eq!(
        grade.outcome.reason(),
        Some(
            "A unit cannot sit inside an expression. Enter the unit only on the final answer, by itself."
        )
    );
}

/// The trailing-text refusals keep the guidance the grade path already
/// served, now worded by the same helper.
#[test]
fn the_trailing_text_refusal_keeps_its_two_guidances() {
    let grade = deterministic_grade("13.5", "13.5 = the total", AnswerKind::Numeric);
    assert_eq!(
        grade.outcome.reason(),
        Some(
            "Enter only the final expression, without an equals sign. Put your steps in Show working."
        )
    );
    let grade = deterministic_grade("23", "9 R2 R3", AnswerKind::Numeric);
    assert_eq!(
        grade.outcome.reason(),
        Some(
            "I could not read the whole answer. Enter only the requested final answer; put your steps in Show working."
        )
    );
}

// ISSUE-13 (ISSUES.md): the name refusal speaks human, the way H-1 taught
// the unit refusals to.

/// The exact screen of ISSUE-13: a correct value followed by the word
/// `units` is refused with the format to type, never with the grammar's
/// wording. A trailing word is not auto-accepted: the learner-side unit
/// (`13.5 cm` against `13.5`) is refused with teaching under the same
/// ruling, and C4 keeps the checker from inventing a reading for prose.
#[test]
fn a_word_beside_the_value_teaches_the_format() {
    let grade = deterministic_grade("6/5", "6/5 units", AnswerKind::Numeric);
    assert!(matches!(grade.outcome, AttemptOutcome::Ungraded { .. }));
    assert_eq!(
        grade.outcome.reason(),
        Some(
            "I could not read a word in that answer. Enter just the number or expression \
             \u{2014} remove any words, such as 'units'."
        )
    );
}

/// The mixed-number ruling of ISSUE-13: the natural mixed-number input
/// already reads as ONE value, the whole and the fraction together, so the
/// re-scaled-number-line screen grades `1 1/5` correct against `6/5` with
/// no refusal and no product reading.
#[test]
fn a_mixed_number_input_reads_as_one_value() {
    let grade = deterministic_grade("6/5", "1 1/5", AnswerKind::Numeric);
    assert!(grade.correct);
    assert!(matches!(grade.outcome, AttemptOutcome::Correct));
}

// C5: every remaining Undecidable reason teaches the expected format.

/// The input cap reads as a length limit with the number in it.
#[test]
fn the_input_cap_refusal_names_the_limit() {
    let grade = deterministic_grade("13.5", &"1".repeat(4_001), AnswerKind::Numeric);
    assert!(matches!(grade.outcome, AttemptOutcome::Ungraded { .. }));
    assert_eq!(
        grade.outcome.reason(),
        Some("The answer is too long \u{2014} keep it under 4000 characters.")
    );
}

/// An unparseable answer to a fraction item asks for `a/b`; a decimal
/// answer asks for its decimal form; a whole-number item asks for plain
/// numbers. No example names the authored value (Hard Rule 1).
#[test]
fn an_unreadable_answer_asks_for_the_expected_shape() {
    let grade = deterministic_grade("1/3", "1/3 ???", AnswerKind::Numeric);
    assert!(matches!(grade.outcome, AttemptOutcome::Ungraded { .. }));
    assert!(
        grade
            .outcome
            .reason()
            .is_some_and(|reason| reason.contains("Write the fraction as a/b"))
    );
    // A terminating decimal asks for its decimal form, not for a fraction.
    let grade = deterministic_grade("13.5", "13.5 ???", AnswerKind::Numeric);
    assert!(
        grade
            .outcome
            .reason()
            .is_some_and(|reason| reason.contains("Write the answer as a decimal"))
    );
}

/// A malformed matrix answer names the bracketed-rows shape with a stock
/// example, never the authored entries.
#[test]
fn a_malformed_matrix_teaches_the_grid_shape() {
    let grade = grade_item(
        &PoolAnswer {
            v: 1,
            answer: "[[1,2],[3,4]]".to_string(),
            answer_contract: Some(AnswerContract::Matrix { rows: 2, cols: 2 }),
        },
        "[1,2;3",
        AnswerKind::Numeric,
    );
    assert!(matches!(grade.outcome, AttemptOutcome::Ungraded { .. }));
    assert!(
        grade
            .outcome
            .reason()
            .is_some_and(|reason| reason.contains("[[1,2],[3,4]]"))
    );
}

/// A set read against a list names the authored shape: braces for a set,
/// brackets for a list.
#[test]
fn a_shape_gap_names_the_authored_shape() {
    let grade = deterministic_grade("{1, 2}", "[1, 2]", AnswerKind::Numeric);
    assert_eq!(
        grade.outcome.reason(),
        Some("The answer is an unordered set, so write it in braces, like {1, 2, 3}.")
    );
    let grade = deterministic_grade("1, 2", "{1, 2}", AnswerKind::Numeric);
    // A bare comma pair is one ordered group, so the prompt names the
    // parentheses shape the authored answer carries.
    assert_eq!(
        grade.outcome.reason(),
        Some("The answer is one ordered group, so write it in parentheses, like (1, 2).")
    );
}

/// A zero denominator teaches the `a/b` form, the way the ISSUE-2 ruling
/// taught the trailing-text form.
#[test]
fn a_zero_denominator_teaches_the_fraction_form() {
    let grade = deterministic_grade("1/2", "1/0", AnswerKind::Numeric);
    assert!(matches!(grade.outcome, AttemptOutcome::Ungraded { .. }));
    assert!(
        grade
            .outcome
            .reason()
            .is_some_and(|reason| reason.contains("Write the fraction as a/b"))
    );
}

/// Every reason the grammar and the contracts can emit maps to a
/// learner-facing prompt: never the raw production name, and never empty.
/// The list is the reason strings of `crates/core/src/answer/`.
#[test]
fn every_refusal_reason_reads_human() {
    let reasons = [
        "a character outside the grammar",
        "a decimal past the size bound",
        "a disjunction branch has trailing text",
        "a disjunction exceeds 16 alternatives",
        "a division by zero",
        "a fraction with a zero denominator",
        "a function name with no argument",
        "a list requires one to 32 complete members",
        "a list requires a flat deterministic member contract",
        "a matrix must be written as bracketed rows of entries",
        "a matrix row requires comma-separated entries",
        "a matrix entry must be an exact rational or decimal",
        "a matrix answer requires the expected count of rows and entries per row",
        "a matrix requires one to 64 entries in at least one row and column",
        "an ascending chain requires two to 16 strictly increasing rational values",
        "an inequality between two variables",
        "an inequality union requires one unknown and at most 16 rational intervals",
        "an inequality with no bare variable",
        "an interval that has no two ends",
        "an exponent outside the evaluation bound",
        "an exponent past the size bound",
        "a number past the size bound",
        "a number with two points",
        "a point with no digit after it",
        "a polynomial relation requires two polynomial expressions and one comparison",
        "a quantity whose value is not a number",
        "a quotient with a zero divisor",
        "a quotient contract requires a positive divisor",
        "a radicand past the factoring bound",
        "a reduced ratio requires two coprime positive integers separated by one colon",
        "a relation setup requires one symbolic left side and one exact numeric right side",
        "a required single power needs one reduced numeric literal base",
        "a required simplest radical needs a reduced rational times one squarefree integer root",
        "a root with no argument",
        "a set against a list",
        "a set against a tuple",
        "a symbol where a value belongs",
        "a tower of powers",
        "a unit is missing",
        "a unit inside an expression",
        "a unit on the learner side only",
        "a unit outside the table",
        "a zero base with a non-positive exponent",
        "arithmetic on a collection",
        "arithmetic on a labeled value",
        "arithmetic on a quantity",
        "each named answer part must occur exactly once",
        "normalized scientific notation needs one decimal coefficient with magnitude in [1, 10) times 10 to an integer power",
        "the answer contract requires a number",
        "the answer contract supports at most 18 decimal places",
        "the answer ends where a value belongs",
        "the answer goes past the term bound",
        "the answer goes past the work bound",
        "the answer is empty",
        "the answer is longer than the input cap",
        "the answer kind is not decidable",
        "the answer nests too deeply",
        "the answer contract requires a positive divisor",
        "the authored answer does not match its required form",
        "the authored answer is outside the choice vocabulary",
        "the authored answer does not match its contract shape",
        "the authored answer must be an assignment such as y = 4",
        "the contract unit does not match its quantity",
        "the tolerance must be an exact positive rational of at most 80 characters",
        "a label contract requires one to 32 choices",
        "a choice requires one to eight explicit aliases",
        "choice aliases must be bounded, nonempty, and unique",
        "a multipart answer requires one to 16 parts",
        "part names must be bounded unique identifiers",
        "multipart parts require flat deterministic contracts",
        "an item has no deterministic answer contract",
        "the item has no deterministic answer contract",
        "a required assignment needs the authored target",
        "two numbers stand side by side",
        "two percent signs on one number",
        "trailing text after the answer",
        "a name that is not a function or variable",
    ];
    for reason in reasons {
        let guidance = format_guidance("1/2", "6/5 ???", reason);
        assert!(!guidance.is_empty(), "{reason}: empty guidance");
        assert_ne!(guidance, reason, "{reason}: the raw production leaked");
    }
}

/// Lane B4b: a `function` answer that is a list gets the frozen format text.
#[test]
fn a_function_answer_that_is_not_one_formula_gets_the_frozen_guidance() {
    const TEXT: &str =
        "Enter one formula, for example 3x^2 + 1. Do not enter a list, a set, or an inequality.";
    let reason = "a function answer must be one expression";
    assert_eq!(format_guidance("x^2", "(1, 2)", reason), TEXT);
    let contract: AnswerContract =
        serde_json::from_str(r#"{"kind":"function","vars":["x"]}"#).unwrap();
    let outcome = cadus_core::answer::check_contract("x/sqrt(x^2+9)", "(1, 2)", contract);
    assert!(matches!(
        outcome,
        cadus_core::answer::Outcome::Undecidable(refusal) if format_guidance("x", "(1, 2)", refusal.reason) == TEXT
    ));
}
