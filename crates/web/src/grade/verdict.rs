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
///
/// The answer contract decides before the topic kind. A `proof` item with a
/// contract gets a verdict from the contract. A `proof` item with no contract,
/// or with the contract `none`, stays ungraded with [`PROOF_UNGRADED`].
#[must_use]
pub fn grade_item(
    expected: &cadus_core::pool::PoolAnswer,
    answer: &str,
    kind: AnswerKind,
) -> Grade {
    let contract = match expected.answer_contract.clone() {
        None | Some(cadus_core::answer::AnswerContract::None) if kind == AnswerKind::Proof => {
            return ungraded_grade(PROOF_UNGRADED);
        }
        None => return deterministic_grade(&expected.answer, answer, kind),
        Some(contract) => contract,
    };
    let quantity_answer = matches!(&contract, cadus_core::answer::AnswerContract::Unit { .. });
    let outcome = cadus_core::answer::check_contract(&expected.answer, answer, contract);
    if quantity_answer && matches!(&outcome, Outcome::Undecidable(_)) {
        return ungraded_grade(
            "I could not interpret this quantity reliably. This answer is ungraded.",
        );
    }
    grade_outcome(&expected.answer, answer, outcome)
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
        "a function answer must be one expression" => {
            "Enter one formula, for example 3x^2 + 1. Do not enter a list, a set, or an inequality."
                .to_string()
        }
        "a number too large for this property check" => {
            "That number is too large to check here. Give a smaller example.".to_string()
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
#[path = "verdict_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "grade_item_tests.rs"]
mod grade_item_tests;
