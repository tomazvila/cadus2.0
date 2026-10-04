//! Exact arithmetic for explicit answer policies.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Signed;

use super::function::FunctionSpec;
use super::structured::{label_value, named_parts, ordered_parts, tolerance_value, validate_shape};
use super::{AnswerContract, AnswerPart, Canon, Undecidable, bounded, canonical_form};
use crate::answer::{Outcome, Rounding, Verdict, rounds_to, same_answer};

/// Decide the authored policy with exact arithmetic and bounded input.
#[must_use]
pub fn check_contract(expected: &str, learner: &str, contract: AnswerContract) -> Outcome {
    match contract.validate_expected(expected) {
        Ok(value) => grade(&value, expected, learner, &contract),
        Err(reason) => Outcome::Undecidable(reason),
    }
}

/// Note 115 (owner defect): a comma-separated list key compares as a multiset
/// unless `ordered: true`. The served row of the defect carried no contract, so
/// the key reached the `Exact` path and the bare comma list graded as one
/// ordered expression. The rule lives here: when the authored answer is a bare
/// comma list (or the same list in braces, which carry no meaning), the learner
/// side splits into members and the members match as a multiset. Spacing, a
/// trailing period, braces and the word `and` as a separator carry no meaning;
/// a parenthesized or bracketed learner side keeps the ordered tuple reading of
/// the 1.0 spec, and every other pair falls through unchanged.
fn bare_list_multiset(expected: &Canon, text: &str, learner: &str) -> Option<Outcome> {
    // The Oxford comma of a spoken list ("1, 2, and 3") is the same separator.
    let expected_text = unbraced(text).replace(", and ", " and ");
    let expected_members = super::list::values_grouped(&expected_text).ok()?;
    if expected_members.len() < 2 || !matches!(expected, Canon::Tuple(_) | Canon::Set(_)) {
        return None;
    }
    let learner_side = learner.trim();
    if learner_side.starts_with('(') || learner_side.starts_with('[') {
        return None;
    }
    let learner_text = unbraced(learner_side).replace(", and ", " and ");
    let learner_members = super::list::values_grouped(&learner_text)
        .ok()?
        .into_iter()
        .map(|member| member.strip_suffix('.').unwrap_or(member))
        .collect::<Vec<_>>();
    let mut unused = vec![true; learner_members.len()];
    let mut undecidable = None;
    for expected_member in expected_members {
        let expected_member = expected_member.strip_suffix('.').unwrap_or(expected_member);
        let mut matched = false;
        for (at, learner_member) in learner_members.iter().enumerate() {
            if !unused[at] {
                continue;
            }
            match check_contract(expected_member, learner_member, AnswerContract::Exact) {
                Outcome::Decided(verdict) if verdict.correct => {
                    unused[at] = false;
                    matched = true;
                    break;
                }
                Outcome::Undecidable(reason) => {
                    undecidable.get_or_insert(reason);
                }
                Outcome::Decided(_) => {}
            }
        }
        if !matched {
            return Some(match undecidable {
                Some(reason) => Outcome::Undecidable(reason),
                None => decided(false),
            });
        }
    }
    // A multiset and not a sub-multiset: an extra learner member is wrong. A
    // member that no exact value can grade at all keeps its refusal.
    let mut extra = false;
    for (at, learner_member) in learner_members.iter().enumerate() {
        if unused[at] {
            extra = true;
            if let Outcome::Undecidable(reason) =
                check_contract("0", learner_member, AnswerContract::Exact)
            {
                return Some(Outcome::Undecidable(reason));
            }
        }
    }
    Some(decided(!extra))
}

/// The spelling of one side without the outermost brace pair.
fn unbraced(text: &str) -> &str {
    let trimmed = text.trim();
    trimmed
        .strip_prefix('{')
        .and_then(|inner| inner.strip_suffix('}'))
        .unwrap_or(trimmed)
        .trim()
}

fn grade(expected: &Canon, text: &str, learner: &str, contract: &AnswerContract) -> Outcome {
    if let Err(reason) = bounded(learner) {
        return Outcome::Undecidable(reason);
    }
    if learner.trim().is_empty() {
        return decided(false);
    }
    if let Some(outcome) = structured_contract(expected, text, learner, contract) {
        return outcome;
    }
    if matches!(contract, AnswerContract::Exact)
        && let Some(outcome) = bare_list_multiset(expected, text, learner)
    {
        return outcome;
    }
    let required_form = !matches!(contract, AnswerContract::RequiredForm { form } if !super::form::accepts(*form, learner));
    let learner = match canonical_form(learner) {
        Ok(value) => value,
        Err(reason) => return Outcome::Undecidable(reason),
    };
    if !required_form || !validate_shape(contract, &learner) {
        return decided(false);
    }
    match contract {
        AnswerContract::Approx { decimals } => approximate(expected, &learner, *decimals),
        AnswerContract::Tolerance { tolerance } => {
            absolute_tolerance(expected, &learner, tolerance)
        }
        AnswerContract::Exact
        | AnswerContract::Unit { .. }
        | AnswerContract::QuotientRemainder { .. }
        | AnswerContract::Coordinates { .. }
        | AnswerContract::Set
        | AnswerContract::RequiredForm { .. } => decided(same_answer(expected, &learner)),
        AnswerContract::RequiredAssignment
        | AnswerContract::Label { .. }
        | AnswerContract::Multipart { .. }
        | AnswerContract::List { .. }
        | AnswerContract::Matrix { .. }
        | AnswerContract::InequalityUnion
        | AnswerContract::RequiredInequalityNotation
        | AnswerContract::RequiredSinglePower
        | AnswerContract::RequiredNormalizedScientificNotation
        | AnswerContract::RequiredSimplestRadical
        | AnswerContract::ReducedRatio
        | AnswerContract::AscendingChain
        | AnswerContract::PolynomialRelation
        | AnswerContract::RelationSetup
        | AnswerContract::Function { .. }
        | AnswerContract::None => {
            unreachable!("the structured contracts decide before the learner answer canonicalizes")
        }
    }
}

/// Grade contracts whose learner text has a dedicated parser.
fn structured_contract(
    expected: &Canon,
    text: &str,
    learner: &str,
    contract: &AnswerContract,
) -> Option<Outcome> {
    let outcome = match contract {
        AnswerContract::RequiredAssignment => required_assignment(text, learner),
        AnswerContract::Label { options } => {
            decided(label_value(options, learner).as_ref() == Some(expected))
        }
        AnswerContract::Multipart { parts } => multipart(parts, text, learner),
        AnswerContract::List { ordered, member } => {
            super::list::grade(*ordered, member, text, learner)
        }
        AnswerContract::InequalityUnion => match super::union::read(learner) {
            Ok(value) => decided(super::union::equivalent(expected, &value)),
            Err(reason) => Outcome::Undecidable(reason),
        },
        AnswerContract::RequiredInequalityNotation => {
            match (
                super::union::read_with_notation(text),
                super::union::read_with_notation(learner),
            ) {
                (Ok((_, expected_notation)), Ok((value, learner_notation))) => decided(
                    expected_notation == learner_notation
                        && super::union::equivalent(expected, &value),
                ),
                (Err(reason), _) | (_, Err(reason)) => Outcome::Undecidable(reason),
            }
        }
        contract @ (AnswerContract::RequiredSinglePower
        | AnswerContract::RequiredNormalizedScientificNotation
        | AnswerContract::RequiredSimplestRadical) => {
            required_expression_form(contract, text, learner)
        }
        AnswerContract::ReducedRatio => parsed_or_recognized(
            super::notation::reduced_ratio(learner),
            expected,
            super::notation::recognizes_ratio(learner),
        ),
        AnswerContract::AscendingChain => parsed_or_recognized(
            super::notation::ascending_chain(learner),
            expected,
            super::notation::recognizes_chain(learner),
        ),
        AnswerContract::Matrix { rows, cols } => matrix(expected, *rows, *cols, learner),
        AnswerContract::PolynomialRelation => parsed(super::relation::read(learner), expected),
        AnswerContract::RelationSetup => parsed(super::setup::read(learner), expected),
        AnswerContract::Function {
            vars,
            up_to_constant,
            domain,
        } => FunctionSpec::new(vars, *up_to_constant, domain)
            .map_or_else(Outcome::Undecidable, |spec| {
                super::function::check(&spec, text, learner)
            }),
        AnswerContract::Exact
        | AnswerContract::Approx { .. }
        | AnswerContract::Tolerance { .. }
        | AnswerContract::Unit { .. }
        | AnswerContract::QuotientRemainder { .. }
        | AnswerContract::Coordinates { .. }
        | AnswerContract::Set
        | AnswerContract::RequiredForm { .. }
        | AnswerContract::None => return None,
    };
    Some(outcome)
}

/// Grade a matrix: the shape decides wrong, the entries compare exactly.
fn matrix(expected: &Canon, rows: u8, cols: u8, learner: &str) -> Outcome {
    let parsed = match super::structured::matrix_rows(learner) {
        Ok(parsed) => parsed,
        Err(reason) => return Outcome::Undecidable(reason),
    };
    let shaped = parsed.len() == usize::from(rows)
        && parsed.iter().all(|row| row.len() == usize::from(cols));
    if !shaped {
        return decided(false);
    }
    match super::structured::matrix_value(rows, cols, learner) {
        Ok(value) => decided(same_answer(expected, &value)),
        Err(reason) => Outcome::Undecidable(reason),
    }
}

fn required_assignment(expected: &str, learner: &str) -> Outcome {
    match super::assignment::equivalent(expected, learner) {
        Ok(correct) => decided(correct),
        Err(reason) => Outcome::Undecidable(reason),
    }
}

fn required_expression_form(contract: &AnswerContract, expected: &str, learner: &str) -> Outcome {
    let result = match contract {
        AnswerContract::RequiredSinglePower => super::power::equivalent(expected, learner),
        AnswerContract::RequiredNormalizedScientificNotation => {
            super::scientific::equivalent(expected, learner)
        }
        AnswerContract::RequiredSimplestRadical => super::radical::equivalent(expected, learner),
        _ => unreachable!("caller supplies a required expression contract"),
    };
    match result {
        Ok(correct) => decided(correct),
        Err(reason) => Outcome::Undecidable(reason),
    }
}

fn parsed(value: Result<Canon, Undecidable>, expected: &Canon) -> Outcome {
    match value {
        Ok(value) => decided(&value == expected),
        Err(reason) => Outcome::Undecidable(reason),
    }
}

fn parsed_or_recognized(
    value: Result<Canon, Undecidable>,
    expected: &Canon,
    recognized: bool,
) -> Outcome {
    match value {
        Ok(value) => decided(&value == expected),
        Err(_) if recognized => decided(false),
        Err(reason) => Outcome::Undecidable(reason),
    }
}

fn multipart(parts: &[AnswerPart], expected: &str, learner: &str) -> Outcome {
    let Some(expected) = named_parts(parts, expected) else {
        return refused_parts();
    };
    // Unnamed parts are read by position. A guessed position that puts a value
    // the part cannot read is a wrong answer, as an unreadable text was before.
    let (learner, by_position) = match named_parts(parts, learner) {
        Some(named) => (named, false),
        None => match ordered_parts(parts, learner) {
            Some(ordered) => (ordered, true),
            None => return decided(false),
        },
    };
    let mut correct = true;
    for ((part, expected), learner) in parts.iter().zip(expected).zip(learner) {
        match check_contract(expected, learner, part.contract.clone()) {
            Outcome::Decided(verdict) => correct &= verdict.correct,
            _ if by_position => return decided(false),
            undecidable => return undecidable,
        }
    }
    decided(correct)
}

fn refused_parts() -> Outcome {
    Outcome::Undecidable(Undecidable::new(
        "each named answer part must occur exactly once",
    ))
}

fn decided(correct: bool) -> Outcome {
    Outcome::Decided(Verdict {
        correct,
        notation: false,
    })
}

fn absolute_tolerance(expected: &Canon, learner: &Canon, text: &str) -> Outcome {
    let (Canon::Rational(expected), Canon::Rational(learner)) = (expected, learner) else {
        return decided(false);
    };
    match tolerance_value(text) {
        Ok(tolerance) => decided((expected - learner).abs() <= tolerance),
        Err(reason) => Outcome::Undecidable(reason),
    }
}

fn approximate(expected: &Canon, learner: &Canon, decimals: u8) -> Outcome {
    let Canon::Rational(value) = learner else {
        return decided(false);
    };
    let scale = u32::from(decimals);
    let grid = BigRational::from_integer(BigInt::from(10_u32).pow(scale));
    // Reject extra digits before the integral comparison in the round helper.
    if !(value * grid).is_integer() {
        return decided(false);
    }
    match rounds_to(expected, value, scale) {
        Rounding::Same => decided(true),
        Rounding::Different => decided(false),
        Rounding::NotANumber => {
            Outcome::Undecidable(Undecidable::new("the answer contract requires a number"))
        }
        Rounding::Refused(reason) => Outcome::Undecidable(Undecidable::new(reason)),
    }
}

#[cfg(test)]
mod tests {
    use super::super::NumericForm;
    use super::*;
    use crate::answer::Quantity;

    /// Every `Grades` contract decides one authored answer and refuses its
    /// mutation (a `+1` component, a changed label, or a changed form).
    #[test]
    fn every_grading_contract_decides_one_correct_and_one_mutated_answer() {
        let cases: Vec<(AnswerContract, &str, &str, &str)> = vec![
            (AnswerContract::Exact, "1/2", "0.5", "1/3"),
            (AnswerContract::RequiredAssignment, "y = 3", "y=3", "y = 4"),
            (
                AnswerContract::Approx { decimals: 2 },
                "sqrt(2)",
                "1.41",
                "1.42",
            ),
            (
                AnswerContract::Tolerance {
                    tolerance: "1/100".into(),
                },
                "1",
                "1.01",
                "1.1",
            ),
            (
                AnswerContract::Unit {
                    quantity: Quantity::Volume,
                    unit: "L".into(),
                },
                "4.2 L",
                "4200 ml",
                "4.3 L",
            ),
            (
                AnswerContract::QuotientRemainder { divisor: Some(3) },
                "9 R2",
                "9 remainder 2",
                "(9, 3)",
            ),
            (
                AnswerContract::Coordinates { arity: 2 },
                "(1,2)",
                "(1, 2)",
                "(2, 1)",
            ),
            (
                AnswerContract::Matrix { rows: 2, cols: 2 },
                "[[1,2],[3,4]]",
                "[1,2;3,4]",
                "[[1,2],[4,3]]",
            ),
            (AnswerContract::Set, "{2,4,6}", "{6,2,4}", "{2,4}"),
            (
                AnswerContract::RequiredForm {
                    form: NumericForm::ReducedFraction,
                },
                "1/2",
                "\\frac{1}{2}",
                "2/4",
            ),
            (
                AnswerContract::List {
                    ordered: true,
                    member: Box::new(AnswerContract::Exact),
                },
                "1, 2, 3",
                "1, 2, 3",
                "1, 3, 2",
            ),
            (
                AnswerContract::InequalityUnion,
                "x < 2 or x > 3",
                "3 < x or 2 > x",
                "x < 2",
            ),
            (
                AnswerContract::RequiredInequalityNotation,
                "x < 2 or x > 3",
                "2 > x or 3 < x",
                "x <= 2 or x > 3",
            ),
            (AnswerContract::RequiredSinglePower, "2^(3)", "2^(3)", "8"),
            (
                AnswerContract::RequiredNormalizedScientificNotation,
                "6 x 10^7",
                "6 x 10^7",
                "60000000",
            ),
            (
                AnswerContract::RequiredSimplestRadical,
                "2*sqrt(3)",
                "2*sqrt(3)",
                "3*sqrt(2)",
            ),
            (AnswerContract::ReducedRatio, "3:4", "3:4", "3:5"),
            (
                AnswerContract::AscendingChain,
                "-3 < 0 < 2",
                "-6/2 < 0 < 4/2",
                "-3 < 2 < 0",
            ),
            (
                AnswerContract::PolynomialRelation,
                "x^2 - 1 = 0",
                "0 = x^2 - 1",
                "x^2 - 2 = 0",
            ),
            (
                AnswerContract::RelationSetup,
                "2x + 3 = 11",
                "3 + 2x = 11",
                "2x + 4 = 11",
            ),
            (
                AnswerContract::Label {
                    options: vec![
                        vec!["yes".into(), "true".into()],
                        vec!["no".into(), "false".into()],
                    ],
                },
                "yes",
                " TRUE ",
                "no",
            ),
            (
                AnswerContract::Multipart {
                    parts: vec![
                        crate::answer::AnswerPart {
                            name: "x".into(),
                            contract: AnswerContract::Exact,
                        },
                        crate::answer::AnswerPart {
                            name: "estimate".into(),
                            contract: AnswerContract::Approx { decimals: 2 },
                        },
                    ],
                },
                "x = 2; estimate = 1/3",
                "estimate=0.33; x=4/2",
                "x=2; estimate=0.34",
            ),
        ];
        let function = AnswerContract::Function {
            vars: vec!["x".into()],
            up_to_constant: true,
            domain: std::collections::BTreeMap::new(),
        };
        let mut cases = cases;
        cases.push((function, "y = x^2/2 + C", "x*x/2 + 7", "2*(x^2/2) + x"));
        for (contract, expected, correct, mutated) in cases {
            assert!(
                super::super::triage_verdict(&contract) == super::super::TriageVerdict::Grades,
                "{contract:?} must grade"
            );
            grades(expected, correct, &contract, true);
            grades(expected, mutated, &contract, false);
        }
    }

    #[test]
    fn matrix_grades_dimensions_entries_and_refuses_unparseable_input() {
        let policy = AnswerContract::Matrix { rows: 2, cols: 2 };
        for learner in ["[[1, 2], [3, 4]]", "[[1.0,2],[3,4]]", "[1,2;3,4]"] {
            grades("[[1,2],[3,4]]", learner, &policy, true);
        }
        // Wrong dimensions are decided wrong, never undecidable.
        for learner in ["[[1,2],[3,4],[5,6]]", "[[1,2,3],[4,5,6]]", "[[1,2]]"] {
            grades("[[1,2],[3,4]]", learner, &policy, false);
        }
        grades("[[1,2],[3,4]]", "[[1,2],[3,5]]", &policy, false);
        // A malformed matrix or a non-numeric entry gives no verdict, and the
        // refusal names the expected shape.
        for learner in ["[1,2;3", "1,2;3,4", "[[1,2],[3,4]"] {
            assert!(
                matches!(
                    check_contract("[[1,2],[3,4]]", learner, policy.clone()),
                    Outcome::Undecidable(reason) if reason.reason.contains("matrix")
                ),
                "{learner:?} must refuse with a matrix reason"
            );
        }
        assert!(matches!(
            check_contract("[[1,2],[3,4]]", "[[1,2],[x,4]]", policy.clone()),
            Outcome::Undecidable(_)
        ));
        assert!(
            serde_json::to_string(&policy).unwrap() == r#"{"kind":"matrix","rows":2,"cols":2}"#
        );
        assert!(
            serde_json::from_str::<AnswerContract>(r#"{"kind":"matrix","rows":0,"cols":2}"#)
                .is_err()
        );
        assert!(policy.validate_expected("[[1,2],[3]]").is_err());
    }

    fn grades(expected: &str, learner: &str, contract: &AnswerContract, correct: bool) {
        assert!(
            matches!(
                check_contract(expected, learner, contract.clone()),
                Outcome::Decided(verdict) if verdict.correct == correct
            ),
            "{contract:?}: {expected:?} vs {learner:?}"
        );
    }
}
