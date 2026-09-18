//! The pure grade (spec section 5): the tier, the assisted rule, and the clock.

use cadus_core::answer::{Ast, Canon, canonical_form, parse};

use super::*;

/// Grade one submission with no model call at all (A3, spec section 5.1).
///
/// The tiers are the D-M5-2 ruling:
///
/// - correct: `nearly_perfect`, the NEUTRAL tier. `perfect` is a bonus for
///   method the checker never reads, so it is never awarded, and every tier
///   below is a penalty for a flaw the checker never observes.
/// - a decided miss: `nearly_passable` (XP ×0.3, FIRe q 0.4), below the pass
///   threshold of 0.7, so a wrong answer is never priced as a pass.
/// - a blank: `poor`, which is the tier of 1.0's `blank_answer_grade`.
///
/// An answer the checker refuses ([`Outcome::Undecidable`]: the input cap, an
/// exit from the grammar, or a kind no checker decides) is UNGRADED (D-F2). It is
/// not a miss: the outcome carries the refusal reason, the fold ignores the
/// attempt, and the tier stays `nearly_passable` on the row while no grade reads
/// it. It is never a model verdict and never a pass.
#[must_use]
pub fn deterministic_grade(expected: &str, answer: &str, kind: AnswerKind) -> Grade {
    grade_outcome(expected, answer, check(expected, answer, kind))
}

/// Grade the policy captured in the served item (D-F1, C2).
#[must_use]
pub fn grade_item(
    expected: &cadus_core::pool::PoolAnswer,
    answer: &str,
    kind: AnswerKind,
) -> Grade {
    if kind == AnswerKind::Proof {
        return ungraded_grade(PROOF_UNGRADED);
    }
    let Some(contract) = expected.answer_contract.clone() else {
        return deterministic_grade(&expected.answer, answer, kind);
    };
    grade_outcome(
        &expected.answer,
        answer,
        cadus_core::answer::check_contract(&expected.answer, answer, contract),
    )
}

/// Grade a served item with source-scoped input representation support.
///
/// Equivalent factor-pair input is checked against the captured answer contract.
/// The caller retains the original submission for learner history.
pub fn grade_served_item(served: &ServedProblem, answer: &str, kind: AnswerKind) -> Grade {
    let normalized = super::answer_format::factor_pairs(served, answer, kind);
    grade_item(
        &served.expected,
        normalized.as_deref().unwrap_or(answer),
        kind,
    )
}

fn grade_outcome(expected: &str, answer: &str, outcome: Outcome) -> Grade {
    if answer.trim().is_empty() && matches!(outcome, Outcome::Decided(_)) {
        return Grade {
            correct: false,
            outcome: AttemptOutcome::Incorrect,
            work_quality: WorkQuality::Poor,
            error_tags: vec![TAG_BLANK_ANSWER.to_string()],
        };
    }
    match outcome {
        Outcome::Decided(verdict) if verdict.correct => Grade {
            correct: true,
            outcome: AttemptOutcome::Correct,
            work_quality: WorkQuality::NearlyPerfect,
            error_tags: if verdict.notation {
                vec![TAG_NOTATION.to_string()]
            } else {
                Vec::new()
            },
        },
        Outcome::Decided(_) => Grade {
            correct: false,
            outcome: AttemptOutcome::Incorrect,
            work_quality: WorkQuality::NearlyPassable,
            error_tags: Vec::new(),
        },
        Outcome::Undecidable(refusal) => {
            ungraded_grade(&format_guidance(expected, answer, refusal.reason))
        }
    }
}

/// The learner-facing wording of a refusal.
///
/// A refusal reason names the production that fired, which tells the learner
/// nothing about the FORM to type (the ISSUE-2 ruling: the refusal must teach
/// the format, not the maths). Every reason a learner answer can reach maps to
/// a prompt that names the expected format; the unit refusals, the trailing-text
/// refusal, and the name refusal (a word like `units` beside the value,
/// ISSUE-13) were the first ones, and C5 covered the rest. No arm ever names
/// the expected value itself (Hard Rule 1).
fn format_guidance(expected: &str, answer: &str, reason: &str) -> String {
    match reason {
        "a unit is missing" => missing_unit_guidance(expected),
        "a name that is not a function or variable" => {
            "I could not read a word in that answer. Enter just the number or expression \
             \u{2014} remove any words, such as 'units'."
                .to_string()
        }
        "a unit on the learner side only" => {
            "The expected answer here is a bare number, so enter the value alone, without a unit."
                .to_string()
        }
        "a unit inside an expression" => {
            "A unit cannot sit inside an expression. Enter the unit only on the final answer, by itself."
                .to_string()
        }
        "trailing text after the answer" | "a disjunction branch has trailing text" => {
            if answer.contains('=') && !expected.contains('=') {
                "Enter only the final expression, without an equals sign. Put your steps in Show working."
                    .to_string()
            } else {
                "I could not read the whole answer. Enter only the requested final answer; put your steps in Show working."
                    .to_string()
            }
        }
        // C5: the rest of the runtime refusals, each with the format to type.
        "the answer is longer than the input cap" => format!(
            "The answer is too long \u{2014} keep it under {MAX_ANSWER_CHARS} characters."
        ),
        "the answer kind is not decidable" => {
            "Enter the final answer as one number or expression, for example 12. Put the steps in Show working."
                .to_string()
        }
        "a character outside the grammar"
        | "the answer ends where a value belongs"
        | "a symbol where a value belongs"
        | "the answer is empty" => unreadable_guidance(expected),
        "a function name with no argument" => {
            "Write the function with its value in brackets, for example sin(30).".to_string()
        }
        "a root with no argument" => {
            "Write the root with its value in brackets, for example sqrt(2).".to_string()
        }
        "a number with two points" | "a point with no digit after it" => {
            "Write the decimal with one point and a digit on each side of it, for example 1.5."
                .to_string()
        }
        "two numbers stand side by side" => {
            "I could not read two numbers side by side. Write one value \u{2014} a fraction as a/b, or a product with a times sign, for example 3*4."
                .to_string()
        }
        "two percent signs on one number" => {
            "Write the percent sign once, for example 25%.".to_string()
        }
        "a unit outside the table" => {
            "I do not know that unit. Write the unit the standard way, for example cm, kg, or s."
                .to_string()
        }
        "an interval that has no two ends" => {
            "Write the interval with both of its ends, for example [1, 5].".to_string()
        }
        "an inequality between two variables" | "an inequality with no bare variable" => {
            "Write the inequality with one unknown and one number, for example x < 3.".to_string()
        }
        "a division by zero" | "a fraction with a zero denominator"
        | "a quotient with a zero divisor" => {
            "A fraction cannot have zero in the denominator. Write the fraction as a/b with a nonzero b."
                .to_string()
        }
        "arithmetic on a collection" => {
            "Enter the final value itself \u{2014} one number, for example 12 \u{2014} rather than arithmetic on a list or set."
                .to_string()
        }
        "arithmetic on a labeled value" => {
            "Enter the value alone, without a label like x =. Put any working in Show working."
                .to_string()
        }
        "arithmetic on a quantity" | "a quantity whose value is not a number" => {
            "Do the arithmetic in plain numbers, and put the unit only on the final answer, for example 42 rather than 40 + 2 cm."
                .to_string()
        }
        "a zero base with a non-positive exponent" => {
            "A power needs a nonzero base, for example 2^3.".to_string()
        }
        "a set against a list" | "a set against a tuple" => set_shape_guidance(expected),
        "each named answer part must occur exactly once" => multipart_guidance(expected),
        "a matrix must be written as bracketed rows of entries"
        | "a matrix row requires comma-separated entries"
        | "a matrix entry must be an exact rational or decimal"
        | "a matrix answer requires the expected count of rows and entries per row" => {
            "Write the answer as a matrix \u{2014} bracketed rows of entries inside one pair of brackets, for example [[1,2],[3,4]]."
                .to_string()
        }
        "a list requires one to 32 complete members" => {
            "Write the list as values separated by commas, for example 1, 2, 3.".to_string()
        }
        "a reduced ratio requires two coprime positive integers separated by one colon" => {
            "Write the ratio as two whole numbers with a colon, for example 3:4, in lowest terms."
                .to_string()
        }
        "an ascending chain requires two to 16 strictly increasing rational values" => {
            "Write the values in increasing order joined by <, for example -3 < 0 < 2."
                .to_string()
        }
        "an inequality union requires one unknown and at most 16 rational intervals" => {
            "Write the answer for one unknown, with the pieces joined by or, for example x < 2 or x > 3."
                .to_string()
        }
        "a required single power needs one reduced numeric literal base" => {
            "Write the answer as one power of a whole number, for example 2^3.".to_string()
        }
        "normalized scientific notation needs one decimal coefficient with magnitude in [1, 10) times 10 to an integer power" => {
            "Write the answer in scientific notation \u{2014} a number from 1 up to 10 times a power of 10, for example 6.2 x 10^7."
                .to_string()
        }
        "a required simplest radical needs a reduced rational times one squarefree integer root" => {
            "Write the answer as a whole or fractional number times one simplified square root, for example 2*sqrt(3)."
                .to_string()
        }
        "a polynomial relation requires two polynomial expressions and one comparison" => {
            "Write one polynomial expression on each side of =, <, or >, for example x^2 - 1 = 0."
                .to_string()
        }
        "a relation setup requires one symbolic left side and one exact numeric right side" => {
            "Write the unknown on the left and one number on the right, for example 2x + 3 = 11."
                .to_string()
        }
        "the answer contract requires a number" => {
            "Enter the answer as one number, for example 1.5.".to_string()
        }
        other => bucket_guidance(expected, other),
    }
}

/// The wording of the reasons that carry no arm of their own.
///
/// Three buckets cover the remainder. An answer that outruns a bound of the
/// grammar reads as too much to mark; a reason that names an authored or
/// contract fault is honest that the problem itself may be at fault; whatever
/// is left gets the same unreadable-answer prompt as a stray character.
fn bucket_guidance(expected: &str, reason: &str) -> String {
    if reason.contains("bound")
        || reason.contains("deeply")
        || reason.contains("tower")
        || reason.contains("exceeds")
        || reason.contains("past the")
        || reason.contains("outside the")
    {
        return "That answer is more than the checker can read at once. Simplify it, and enter one short final value \u{2014} a number or a compact power, for example 2^8."
            .to_string();
    }
    if reason.contains("authored")
        || reason.contains("contract")
        || reason.contains("tolerance")
        || reason.contains("choice")
        || reason.contains("aliases")
        || reason.contains("part names")
        || reason.contains("multipart")
        || reason.contains("label")
        || reason.contains("coordinates require")
        || reason.contains("quotient contract")
        || reason.contains("item has no deterministic")
    {
        return "This answer could not be marked automatically, and the problem itself may be at fault. Re-enter it as one simple value \u{2014} a number or a short expression \u{2014} and if it still reads as not marked, your teacher will look at it."
            .to_string();
    }
    unreadable_guidance(expected)
}

/// The guidance of an answer the grammar could not read at all.
///
/// The authored answer names the format to type: a terminating decimal asks
/// for its decimal form, any other fraction asks for `a/b`, and everything
/// else for plain numbers and the arithmetic symbols. No example ever carries
/// the authored value (Hard Rule 1).
fn unreadable_guidance(expected: &str) -> String {
    let reduced = match canonical_form(expected) {
        Ok(Canon::Rational(value)) if !value.is_integer() => Some(value.to_string()),
        _ => None,
    };
    if let Some(reduced) = reduced {
        return if terminating_decimal(&reduced) {
            "I could not read that. Write the answer as a decimal, for example 1.5.".to_string()
        } else {
            "I could not read that. Write the fraction as a/b \u{2014} two whole numbers with a slash between them, for example 3/4."
                .to_string()
        };
    }
    "I could not read that. Enter one value with plain numbers and the symbols + \u{2212} * / ( ) ^, for example (3 + 4)/2."
        .to_string()
}

/// Whether a reduced fraction in its `a/b` text form has a terminating decimal
/// spelling: a denominator of only 2s and 5s is a power of ten in disguise, so
/// the value has one exact decimal form; any other prime keeps the value in
/// its fraction form. A denominator past `u128` is far beyond the grammar's
/// size bound, and reads as non-terminating.
fn terminating_decimal(reduced: &str) -> bool {
    let Some((_, denominator)) = reduced.split_once('/') else {
        return true; // an integer is its own decimal
    };
    let Ok(mut rest) = denominator.parse::<u128>() else {
        return false;
    };
    for prime in [2, 5] {
        while rest % prime == 0 {
            rest /= prime;
        }
    }
    rest == 1
}

/// The guidance of a shape gap between a set and an ordered collection.
///
/// The authored shape owns the wording: the learner wrote the other one, so the
/// prompt names the shape to type, with an example in the same notation.
fn set_shape_guidance(expected: &str) -> String {
    match canonical_form(expected) {
        Ok(Canon::Set(_)) => {
            "The answer is an unordered set, so write it in braces, like {1, 2, 3}.".to_string()
        }
        Ok(Canon::List(_)) => {
            "The answer is an ordered list, so write it in brackets, like [1, 2, 3].".to_string()
        }
        Ok(Canon::Tuple(_)) => {
            "The answer is one ordered group, so write it in parentheses, like (1, 2)."
                .to_string()
        }
        _ => {
            "Write the answer all in one shape \u{2014} braces for a set like {1, 2}, brackets for a list like [1, 2], or parentheses for a pair like (1, 2)."
                .to_string()
        }
    }
}

/// The guidance of a multipart answer whose named parts did not read once each.
///
/// The part NAMES come out of the authored answer; the example values are
/// placeholders, so the reply never names an expected value (Hard Rule 1).
fn multipart_guidance(expected: &str) -> String {
    let names: Vec<&str> = expected
        .split(';')
        .filter_map(|field| field.split_once('=').map(|(name, _)| name.trim()))
        .collect();
    if names.is_empty() {
        return "Answer each named part once, separated by semicolons \u{2014} for example x = 3; y = 4."
            .to_string();
    }
    let example = names
        .iter()
        .map(|name| format!("{name} = \u{2026}"))
        .collect::<Vec<_>>()
        .join("; ");
    format!("Answer each named part once, separated by semicolons \u{2014} for example {example}.")
}

/// The guidance of an answer that left the unit off a measured value.
///
/// The refusal names no unit, so the guidance reads one out of the authored
/// answer and writes an EXAMPLE with a value that is not the answer (a value in
/// the reply never names the expected value, Hard Rule 1).
fn missing_unit_guidance(expected: &str) -> String {
    match expected_unit(expected) {
        Some(unit) => format!(
            "This answer is a measurement, so it needs its unit. Write the value with the unit {unit}, for example {example}.",
            example = unit_example(unit),
        ),
        None => {
            "This answer is a measurement, so it needs its unit. Write the value with its unit."
                .to_string()
        }
    }
}

/// A sample value in `unit` that is not the authored answer.
fn unit_example(unit: &str) -> String {
    match unit {
        // The currency glyph sits in front, the degree mark glues on.
        "$" => "$42".to_string(),
        "°" | "€" => format!("42{unit}"),
        _ => format!("42 {unit}"),
    }
}

/// The unit spelling the authored answer carries, when it is one quantity.
fn expected_unit(expected: &str) -> Option<&'static str> {
    fn walk(tree: &Ast) -> Option<&'static str> {
        match tree {
            Ast::Quantity { unit, .. } => Some(unit),
            Ast::Assign { value, .. } | Ast::Neg(value) | Ast::Sqrt(value) => walk(value),
            Ast::Add(terms)
            | Ast::Mul(terms)
            | Ast::Tuple(terms)
            | Ast::Set(terms)
            | Ast::List(terms)
            | Ast::Func(_, terms) => terms.iter().find_map(walk),
            Ast::Div(a, b) => walk(a).or_else(|| walk(b)),
            Ast::Pow(value, ..) => walk(value),
            Ast::Interval { lo, hi, .. } => walk(lo).or_else(|| walk(hi)),
            Ast::Ineq { bound, .. } => walk(bound),
            Ast::Chain { lo, hi, .. } => walk(lo).or_else(|| walk(hi)),
            _ => None,
        }
    }
    walk(&parse(expected).ok()?)
}

/// The grade of an answer with no deterministic verdict (D-F2).
///
/// `reason` names why in one phrase, and the client shows it. The tier stays
/// `nearly_passable` on the row, and the fold reads neither the tier nor
/// `correct` of an ungraded attempt.
#[must_use]
pub fn ungraded_grade(reason: &str) -> Grade {
    Grade {
        correct: false,
        outcome: AttemptOutcome::Ungraded {
            reason: reason.to_owned(),
        },
        work_quality: WorkQuality::NearlyPassable,
        error_tags: Vec::new(),
    }
}

/// Whether one submission is reference-assisted (H3, spec section 5.4).
///
/// An attempt is assisted when the learner took a hint on this problem or the
/// client flags it (`api.py:1360`).
///
/// A QUIZ attempt is never assisted. 1.0 answers a quiz in `_quiz_answer` and
/// returns from it BEFORE the assisted rule runs (`api.py:1349-1358`), so no
/// quiz answer of 1.0 carries the flag and no quiz answer reaches the H3 reply.
/// The order matters here and not only for parity: the H3 reply names
/// `expected` and `solution`, and a quiz reveals NOTHING before its batch reveal
/// (trap W7). Without this rule a client that sends `"assisted": true` with a
/// correct quiz answer reads the authored answer of every remaining question.
#[must_use]
pub fn reference_assisted(task_type: TaskType, client_flag: bool, hints_given: usize) -> bool {
    if task_type == TaskType::Quiz {
        return false;
    }
    client_flag || hints_given > 0
}

/// The server-measured solve time and its timing tags (`_measure_secs`).
///
/// `expected_time_secs` is `None` when the topic is not in the arena. 1.0 skips
/// the clamp in that case, so this port skips it too: no cap, and no tag.
#[must_use]
pub fn measure_secs(
    started_at: f64,
    now_micros: i64,
    expected_time_secs: Option<i64>,
) -> (i64, Vec<String>) {
    let elapsed = (unix_seconds(now_micros) - started_at).max(0.0).round();
    let Some(expected) = expected_time_secs else {
        return (whole_secs(elapsed), Vec::new());
    };
    let cap = expected.saturating_mul(TIMING_CAP_MULTIPLIER);
    if whole_secs(elapsed) > cap {
        return (cap, vec![TAG_TIMING_UNRELIABLE.to_string()]);
    }
    (whole_secs(elapsed), Vec::new())
}

/// A rounded, non-negative elapsed time as whole seconds.
///
/// The value is bounded BEFORE the cast, so no instant can truncate and the
/// function cannot panic.
fn whole_secs(elapsed: f64) -> i64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "the ceiling only has to be safe, not exact"
    )]
    let ceiling = i64::MAX as f64;
    if elapsed <= 0.0 {
        return 0;
    }
    if elapsed >= ceiling {
        return i64::MAX;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the two guards above bound the value inside the i64 range"
    )]
    let secs = elapsed as i64;
    secs
}

/// Round an XP total to two places, as `service.py` does.
pub(super) fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
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
}
