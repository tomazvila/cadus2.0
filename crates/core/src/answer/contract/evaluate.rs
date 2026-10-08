//! Exact arithmetic for explicit answer policies.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Signed;

use super::function::FunctionSpec;
use super::structured::{
    label_value, named_key_parts, named_parts, ordered_parts, tolerance_value, validate_shape,
};
use super::{AnswerContract, AnswerPart, Canon, Undecidable, bounded, canonical_form};
use crate::answer::rounding::rounds_to_constant;
use crate::answer::{Outcome, Rounding, Verdict, same_answer};

/// Decide the authored policy with exact arithmetic and bounded input.
#[must_use]
pub fn check_contract(expected: &str, learner: &str, contract: AnswerContract) -> Outcome {
    let learner = &crate::answer::latex::prepare(learner);
    if matches!(
        contract,
        AnswerContract::QuotientRemainder { .. } | AnswerContract::PolynomialDivision { .. }
    ) {
        // Only a division item reads `3r-10` as a quotient and a remainder.
        return crate::answer::parse::with_negative_remainder(|| {
            check_read_contract(expected, learner, contract)
        });
    }
    check_read_contract(expected, learner, contract)
}

fn check_read_contract(expected: &str, learner: &str, contract: AnswerContract) -> Outcome {
    let outcome = match contract.validate_expected(expected) {
        Ok(value) => grade(&value, expected, learner, &contract),
        Err(reason) => super::named_key::rescue(expected, learner, &contract)
            .or_else(|| super::collections::word_list(expected, learner, &contract))
            .unwrap_or(Outcome::Undecidable(reason)),
    };
    super::spoken::rescue(expected, learner, &contract, outcome)
}

/// The notation hint of a wrong answer to a `mixed_number` item, or `None`.
///
/// The hint is for the learner who writes `4 * 2/5` (which means 4 times 2/5)
/// or the right value in another form (`22/5`, `4.4`, `4 4/10`). The verdict of
/// [`check_contract`] stays wrong.
#[must_use]
pub fn format_hint(expected: &str, learner: &str, contract: &AnswerContract) -> Option<String> {
    super::mixed::hint(expected, learner, contract)
}

/// Note 115 (owner defect): a comma-separated list key compares as a multiset
/// unless `ordered: true`. The served row of the defect carried no contract, so
/// the key reached the `Exact` path and the bare comma list graded as one
/// ordered expression. The rule lives here: when the authored answer is a bare
/// comma list (or the same list in braces, which carry no meaning), the learner
/// side splits into members and the members match as a multiset. Spacing, a
/// trailing period, braces and the words `and` and `or` as separators carry no
/// meaning (the solutions of an equation read `x = -5 or x = 4`);
/// one `±` stands for its two members (`x = -3 ± 2√2`). A learner side wrapped
/// whole in parentheses or brackets keeps the ordered tuple reading of the 1.0
/// spec; `(3+√5)/2, (3-√5)/2` is a bare list whose first member opens with a
/// parenthesis. Every other pair falls through unchanged.
fn bare_list_multiset(expected: &Canon, text: &str, learner: &str) -> Option<Outcome> {
    // The Oxford comma of a spoken list ("1, 2, and 3") is the same separator.
    // A disjunction key ("x = 7 or x = -7") keeps its own set reading.
    if text.contains(" or ") {
        return None;
    }
    let expected_text = unbraced(text).replace(", and ", " and ");
    let expected_members = super::list::values_grouped(&expected_text).ok()?;
    let learner_side = learner.trim();
    if wrapped(learner_side) {
        return None;
    }
    let learner_text = unbraced(learner_side)
        .replace(", and ", " and ")
        .replace(", or ", ", ")
        .replace(" or ", ", ");
    let learner_members = super::list::values_grouped(&learner_text)
        .ok()?
        .into_iter()
        .map(|member| member.strip_suffix('.').unwrap_or(member))
        .flat_map(plus_minus)
        .collect::<Vec<_>>();
    let learner_members: Vec<&str> = learner_members.iter().map(String::as_str).collect();
    if expected_members.len() == 1 {
        return repeated_root(expected, text, &learner_members);
    }
    if !matches!(expected, Canon::Tuple(_) | Canon::Set(_)) {
        return None;
    }
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

/// Whether one parenthesis or bracket pair encloses the whole of `text`.
fn wrapped(text: &str) -> bool {
    let close = match text.chars().next() {
        Some('(') => ')',
        Some('[') => ']',
        _ => return false,
    };
    if !text.ends_with(close) {
        return false;
    }
    let mut depth = 0_usize;
    for (at, ch) in text.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return at + ch.len_utf8() == text.len();
                }
            }
            _ => {}
        }
    }
    false
}

/// A double root written twice ("3, 3", "x = 3 or x = 3") against a one-value
/// key: correct when every member, two or more, is the key's value. Any other
/// list keeps its plain reading.
fn repeated_root(expected: &Canon, text: &str, members: &[&str]) -> Option<Outcome> {
    if members.len() < 2 || !matches!(expected, Canon::Rational(_) | Canon::Radical(_)) {
        return None;
    }
    members
        .iter()
        .all(|member| {
            matches!(
                check_contract(text, member, AnswerContract::Exact),
                Outcome::Decided(verdict) if verdict.correct
            )
        })
        .then(|| decided(true))
}

/// One `±` (also `+/-`, `\pm`, or `+-` after `=` or at the start) written as
/// its two members: `-3 ± 2√2` reads `-3 + 2√2, -3 - 2√2`, `x = ±4` reads
/// `x = 4, x = -4`. Any other member is returned unchanged, so two signs or
/// none keep their plain reading.
fn plus_minus(member: &str) -> Vec<String> {
    let text = spelled_plus_minus(member);
    let text = text.as_str();
    let mut found = None;
    for sign in ["±", "+/-", "\\pm"] {
        let count = text.matches(sign).count();
        if count > 1 || (count == 1 && found.is_some()) {
            return vec![text.to_owned()];
        }
        if count == 1 {
            found = Some(sign);
        }
    }
    let Some((before, after)) = found.and_then(|sign| text.split_once(sign)) else {
        return vec![text.to_owned()];
    };
    if after.contains(',') || before.contains(',') || after.trim().is_empty() {
        return vec![text.to_owned()];
    }
    let lead = before.trim_end();
    if lead.is_empty() || lead.ends_with('=') {
        vec![format!("{before}{after}"), format!("{before}-{after}")]
    } else {
        vec![format!("{before}+{after}"), format!("{before}-{after}")]
    }
}

/// `+-` written for `±` where a sign starts the value: at the start of the
/// member or right after `=` ("x = +-4"). `3 +- 4` keeps its plain reading.
fn spelled_plus_minus(member: &str) -> String {
    let trimmed = member.trim_start();
    if let Some(rest) = trimmed.strip_prefix("+-") {
        return format!("±{rest}");
    }
    if let Some((name, value)) = member.split_once('=')
        && let Some(rest) = value.trim_start().strip_prefix("+-")
    {
        return format!("{name}= ±{rest}");
    }
    member.to_owned()
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

/// A learner who names every part (`y = 3, x = 2`) is read by name: a point
/// takes its parts in the order of the names, and a bare list key takes them in
/// the order of the key, so a swap of the values is a different answer.
fn named_order(text: &str, learner: &str, contract: &AnswerContract) -> Option<Outcome> {
    use super::prefix::named_parts;
    if text.contains('=') {
        return None;
    }
    let named = match named_parts(learner)? {
        Ok(parts) => parts,
        Err(reason) => {
            return matches!(contract, AnswerContract::Coordinates { .. })
                .then_some(Outcome::Undecidable(reason));
        }
    };
    let parts: Vec<String> = named.iter().map(|(_, value)| value.clone()).collect();
    match contract {
        AnswerContract::Coordinates { .. } => {
            let tuple = format!("({})", parts.join(", "));
            match check_contract(text, &tuple, contract.clone()) {
                outcome @ Outcome::Decided(_) => Some(outcome),
                Outcome::Undecidable(_) => None,
            }
        }
        AnswerContract::Exact => {
            let key = text.trim();
            if key.starts_with(['(', '[', '{']) || key.contains(" or ") {
                return None;
            }
            let members = super::list::values_grouped(key).ok()?;
            if members.len() != parts.len() || members.len() < 2 {
                return None;
            }
            let mut all = true;
            for (member, part) in members.iter().zip(&parts) {
                match check_contract(member, part, AnswerContract::Exact) {
                    Outcome::Decided(verdict) => all &= verdict.correct,
                    Outcome::Undecidable(_) => return None,
                }
            }
            Some(decided(all && !super::prefix::bounds_reversed(&named)))
        }
        _ => None,
    }
}

fn grade(expected: &Canon, text: &str, learner: &str, contract: &AnswerContract) -> Outcome {
    if let Some(outcome) = named_order(text, learner, contract) {
        return outcome;
    }
    let outcome = grade_natural(expected, text, learner, contract);
    if matches!(outcome, Outcome::Decided(Verdict { correct: true, .. })) {
        return outcome;
    }
    if let Some(rescued) = super::collections::rescue(text, learner, contract) {
        return rescued;
    }
    if matches!(contract, AnswerContract::Exact) {
        if let Some(rescued) = interval_spelling(text, learner) {
            return rescued;
        }
        if let Some(same) = super::nested::same_value(text, learner) {
            return decided(same);
        }
    }
    super::named_key::rescue(text, learner, contract)
        .or_else(|| named_value(text, learner, contract))
        .or_else(|| capital_e(text, learner, contract, &outcome))
        .or_else(|| equation_for_value(text, learner, contract))
        .unwrap_or(outcome)
}

/// A learner equation whose left side is an expression (`x + 1 = 3`,
/// `x^2 = 3`) against a key that is a value: the equation is no label and a
/// value, so the answer has no verdict.
fn equation_for_value(text: &str, learner: &str, contract: &AnswerContract) -> Option<Outcome> {
    if !matches!(contract, AnswerContract::Exact)
        || text.contains(['=', '<', '>', ','])
        || learner.matches('=').count() != 1
        || learner.contains(['<', '>', ',', '≤', '≥', '≠', '!'])
        || !learner.chars().any(|c| c.is_alphabetic())
    {
        return None;
    }
    Some(Outcome::Undecidable(Undecidable::new(
        "the answer is an equation and the authored answer is a value",
    )))
}

/// A learner who writes `E^x` for `e^x`. The capital `E` is a variable name, so
/// the answer is wrong as written, but the lower case `e` would be right: the
/// answer has no verdict.
fn capital_e(
    text: &str,
    learner: &str,
    contract: &AnswerContract,
    outcome: &Outcome,
) -> Option<Outcome> {
    if !matches!(outcome, Outcome::Decided(Verdict { correct: false, .. }))
        || !matches!(
            contract,
            AnswerContract::Exact | AnswerContract::Function { .. }
        )
        || text.contains('E')
        || learner.trim() == "E"
        || !learner.contains('E')
    {
        return None;
    }
    match check_contract(text, &learner.replace('E', "e"), contract.clone()) {
        Outcome::Decided(verdict) if verdict.correct => Some(Outcome::Undecidable(
            Undecidable::new("the capital E is a variable name; the constant is written e"),
        )),
        _ => None,
    }
}

/// An inequality key against an interval or a union spelling of it: key
/// `y > 0` takes `(0, ∞)` and `y is greater than 0`; key `-1 <= y <= 1` takes
/// `[-1, 1]`. Both sides read as exact unions of intervals.
fn interval_spelling(text: &str, learner: &str) -> Option<Outcome> {
    if !text.contains(['<', '>', '≠']) && !text.contains("!=") {
        return None;
    }
    let key = super::union::read(text).ok()?;
    let learner = super::union::read(learner).ok()?;
    Some(decided(super::union::equivalent(&key, &learner)))
}

/// Grade the text after a name the learner wrote in front of the value
/// (`d = 13 km`, `f'(x) = 2x`, `x = 2, y = 3`). The key must hold no `=`, and
/// only the contracts that grade a value or a formula read the rest.
fn named_value(text: &str, learner: &str, contract: &AnswerContract) -> Option<Outcome> {
    use super::prefix::{strip_name, strip_named_list};
    if text.contains('=')
        || !matches!(
            contract,
            AnswerContract::Exact
                | AnswerContract::Approx { .. }
                | AnswerContract::Tolerance { .. }
                | AnswerContract::Unit { .. }
                | AnswerContract::Coordinates { .. }
                | AnswerContract::RequiredForm { .. }
                | AnswerContract::Function { .. }
                | AnswerContract::Property { .. }
                | AnswerContract::List { .. }
                | AnswerContract::RequiredSinglePower
                | AnswerContract::RequiredNormalizedScientificNotation
                | AnswerContract::RequiredSimplestRadical
        )
    {
        return None;
    }
    let rest = if matches!(
        contract,
        AnswerContract::Coordinates { .. } | AnswerContract::List { .. }
    ) {
        strip_named_list(learner).or_else(|| strip_name(learner))?
    } else {
        strip_name(learner)?
    };
    let rest = if matches!(contract, AnswerContract::Coordinates { .. })
        && rest.contains(',')
        && !rest.starts_with('(')
    {
        format!("({rest})")
    } else {
        rest
    };
    match check_contract(text, &rest, contract.clone()) {
        outcome @ Outcome::Decided(_) => Some(outcome),
        Outcome::Undecidable(_) => None,
    }
}

fn grade_natural(
    expected: &Canon,
    text: &str,
    learner: &str,
    contract: &AnswerContract,
) -> Outcome {
    let strict = grade_strict(expected, text, learner, contract);
    // "3.2x10^5", "3.2e5" and "7.2×10⁻⁴" for a numeric key in scientific form.
    if !matches!(strict, Outcome::Decided(Verdict { correct: true, .. }))
        && matches!(expected, Canon::Rational(_))
        && matches!(
            contract,
            AnswerContract::Exact | AnswerContract::RequiredNormalizedScientificNotation
        )
        && let Some(spelled) = crate::answer::natural::scientific(learner)
    {
        let outcome = grade_strict(expected, text, &spelled, contract);
        if matches!(outcome, Outcome::Decided(_)) {
            return outcome;
        }
    }
    // An approximate answer takes the marks of an estimate: "about 8.94", "≈ 8.94".
    if matches!(
        contract,
        AnswerContract::Approx { .. } | AnswerContract::Tolerance { .. }
    ) && !matches!(strict, Outcome::Decided(Verdict { correct: true, .. }))
        && let Some(bare) = crate::answer::natural::strip_estimate_mark(learner)
    {
        let outcome = grade_strict(expected, text, bare, contract);
        if matches!(outcome, Outcome::Decided(_)) {
            return outcome;
        }
    }
    // The `exact` contract also reads a percent key against its displayed
    // number. Unit contracts own every measured-value reading.
    if !matches!(contract, AnswerContract::Exact) {
        return strict;
    }
    crate::answer::natural::rescue(text, learner, strict, |text, learner| {
        check_contract(text, learner, AnswerContract::Exact)
    })
    .unwrap_or(strict)
}

fn grade_strict(expected: &Canon, text: &str, learner: &str, contract: &AnswerContract) -> Outcome {
    if let Err(reason) = bounded(learner) {
        return Outcome::Undecidable(reason);
    }
    if learner.trim().is_empty() {
        return decided(false);
    }
    if matches!(contract, AnswerContract::Exact)
        && let Some(outcome) = super::phrase::grade(text, learner)
    {
        return outcome;
    }
    if let AnswerContract::Unit {
        unit,
        allow_omitted,
        form,
        ..
    } = contract
    {
        let phrased = crate::answer::natural::unit_phrases(learner);
        // Without an allowed omitted unit, `100°` does not say Celsius or Fahrenheit.
        let degrees =
            crate::answer::natural::temperature_degrees(&phrased, unit).filter(|_| *allow_omitted);
        let learner = degrees.as_deref().unwrap_or(phrased.as_str());
        let cents = ends_in_cents(learner);
        if form.is_some_and(|form| !form.holds(crate::answer::unit::magnitude(learner))) {
            return decided(false);
        }
        match super::super::check::canonical_quantity_in_contract_context(learner) {
            Ok(Some(value)) => {
                let value = super::structured::unit_compatible_value(contract, value);
                let value = cents_as_dollars(contract, cents, value);
                return decided(validate_shape(contract, &value) && same_answer(expected, &value));
            }
            Err(reason) => return Outcome::Undecidable(reason),
            Ok(None) => {}
        }
        let parsed = match canonical_form(learner) {
            Ok(value) => value,
            Err(reason) => return Outcome::Undecidable(reason),
        };
        if matches!(parsed, Canon::Rational(_) | Canon::Radical(_)) {
            if !allow_omitted {
                return decided(false);
            }
            return match canonical_form(&format!("{learner} {unit}")) {
                Ok(value) => {
                    let value = super::structured::unit_compatible_value(contract, value);
                    decided(validate_shape(contract, &value) && same_answer(expected, &value))
                }
                Err(reason) => Outcome::Undecidable(reason),
            };
        }
        return Outcome::Undecidable(Undecidable::new("a unit outside the table"));
    }
    if let Some(outcome) = structured_contract(expected, text, learner, contract) {
        return outcome;
    }
    if matches!(contract, AnswerContract::Exact)
        && let Some(outcome) = bare_list_multiset(expected, text, learner)
    {
        return outcome;
    }
    let spoken;
    let learner = match super::phrase::spoken_quotient(learner) {
        Some(written) if matches!(contract, AnswerContract::QuotientRemainder { .. }) => {
            spoken = written;
            spoken.as_str()
        }
        _ => learner,
    };
    let braced;
    let learner = if matches!(contract, AnswerContract::Set) {
        // A bare comma list `2, 5` is a set written without its braces, and a
        // member that stands twice is a wrong set.
        braced = bare_set(learner);
        if repeats_a_member(&braced) {
            return decided(false);
        }
        braced.as_str()
    } else {
        learner
    };
    let required_form = !matches!(contract, AnswerContract::RequiredForm { form } if !super::form::accepts(*form, learner, text));
    let same_quantity =
        matches!(contract, AnswerContract::Exact) && same_glued_quantity(text, learner);
    let learner = match canonical_form(learner) {
        Ok(value) => value,
        Err(reason) => return Outcome::Undecidable(reason),
    };
    if matches!(contract, AnswerContract::Approx { .. })
        && matches!(learner, Canon::Quantity { .. })
    {
        // The contract names no unit, so a unit the learner adds is neither right
        // nor wrong here.
        return Outcome::Undecidable(Undecidable::new("the answer contract names no unit"));
    }
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
        | AnswerContract::RequiredForm { .. } => {
            decided(same_quantity || same_answer(expected, &learner))
        }
        AnswerContract::RequiredAssignment
        | AnswerContract::Label { .. }
        | AnswerContract::Multipart { .. }
        | AnswerContract::List { .. }
        | AnswerContract::Matrix { .. }
        | AnswerContract::ScalarMultiple { .. }
        | AnswerContract::InequalityUnion
        | AnswerContract::RequiredInequalityNotation
        | AnswerContract::RequiredSinglePower
        | AnswerContract::RequiredNormalizedScientificNotation
        | AnswerContract::RequiredSimplestRadical
        | AnswerContract::ReducedRatio
        | AnswerContract::AscendingChain
        | AnswerContract::OrderedWord
        | AnswerContract::PolynomialRelation
        | AnswerContract::MonicPolynomialRelation
        | AnswerContract::PolynomialDivision { .. }
        | AnswerContract::RelationSetup
        | AnswerContract::Function { .. }
        | AnswerContract::FunctionForm { .. }
        | AnswerContract::Property { .. }
        | AnswerContract::None
        | AnswerContract::Written => {
            unreachable!("the structured contracts decide before the learner answer canonicalizes")
        }
    }
}

/// The learner text of a `set` answer with braces: a bare comma list gets them.
fn bare_set(learner: &str) -> String {
    let text = learner.trim().trim_end_matches('.').trim();
    let braced = text.starts_with('{') || text.starts_with("\\{") || text.starts_with("$");
    if braced || wrapped(text) || !text.contains(',') {
        learner.to_owned()
    } else {
        format!("{{{text}}}")
    }
}

/// Whether a set answer lists one member twice.
fn repeats_a_member(text: &str) -> bool {
    let Ok(crate::answer::Ast::Set(items)) =
        crate::answer::parse(&crate::answer::normalize(text).source)
    else {
        return false;
    };
    let mut seen = Vec::new();
    for item in &items {
        let Ok(value) = crate::answer::canon(item) else {
            return false;
        };
        if seen.contains(&value) {
            return true;
        }
        seen.push(value);
    }
    false
}

/// Whether the text names its unit with the word `cent` or `cents`.
fn ends_in_cents(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    lower.ends_with("cents") || lower.ends_with("cent")
}

/// Under a dollar contract, an amount in cents is an amount of dollars: the
/// unit table holds `cents` as 1/100 of a euro, the other currency.
fn cents_as_dollars(contract: &AnswerContract, cents: bool, value: Canon) -> Canon {
    match (contract, value) {
        (
            AnswerContract::Unit {
                quantity: super::Quantity::Dollar,
                ..
            },
            Canon::Quantity {
                quantity: super::Quantity::Euro,
                value,
            },
        ) if cents => Canon::Quantity {
            quantity: super::Quantity::Dollar,
            value,
        },
        (_, value) => value,
    }
}

/// Whether the key and the learner text are one number with one unit, where one
/// writes the one-letter unit glued (`3m`) and the other with a space (`3 m`).
fn same_glued_quantity(key: &str, learner: &str) -> bool {
    use super::super::check::canonical_quantity_in_contract_context as quantity;
    matches!(
        (quantity(key), quantity(learner)),
        (Ok(Some(left)), Ok(Some(right))) if left == right
    )
}

/// Grade contracts whose learner text has a dedicated parser.
fn structured_contract(
    expected: &Canon,
    text: &str,
    learner: &str,
    contract: &AnswerContract,
) -> Option<Outcome> {
    let outcome = match contract {
        AnswerContract::RequiredForm { form } if form.is_line() => {
            if !super::form::accepts(*form, learner, text) {
                return Some(decided(false));
            }
            parsed(
                super::relation::read(&super::lowest::function_label_as_y(learner)),
                expected,
            )
        }
        AnswerContract::RequiredAssignment => required_assignment(text, learner),
        AnswerContract::Label { options } => {
            let value = label_value(options, learner);
            if value.is_none()
                && super::sentence::split_contrast(learner).is_none()
                && super::sentence::contradicts_opening(options, learner)
            {
                Outcome::Undecidable(Undecidable::new(
                    "the answer opens with a choice word and then denies it",
                ))
            } else {
                decided(value.as_ref() == Some(expected))
            }
        }
        AnswerContract::Multipart { parts } => multipart(parts, text, learner),
        AnswerContract::List { ordered, member } => {
            super::list::grade(*ordered, member, text, learner)
        }
        AnswerContract::InequalityUnion => match super::union::read(learner) {
            Ok(value) if super::union::equivalent(expected, &value) => decided(true),
            Ok(value) if super::union::adds_zero_bound(expected, &value) => Outcome::Undecidable(
                Undecidable::new("the answer adds the lower bound 0, which the key does not state"),
            ),
            Ok(_) => decided(false),
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
            super::notation::learner_ratio(learner),
            expected,
            super::notation::recognizes_ratio(learner),
        ),
        AnswerContract::AscendingChain => parsed_or_recognized(
            super::notation::ascending_chain(learner),
            expected,
            super::notation::recognizes_chain(learner),
        ),
        AnswerContract::OrderedWord => {
            parsed(super::word::read(learner).map(Canon::Label), expected)
        }
        AnswerContract::Matrix { rows, cols } => matrix(expected, *rows, *cols, learner, false),
        AnswerContract::ScalarMultiple { rows, cols } => {
            matrix(expected, *rows, *cols, learner, true)
        }
        AnswerContract::PolynomialDivision { divisor } => {
            match super::division::equivalent(divisor, text, learner) {
                Ok(correct) => decided(correct),
                Err(reason) => Outcome::Undecidable(reason),
            }
        }
        AnswerContract::PolynomialRelation => super::relation::grade_learner(learner, expected),
        AnswerContract::MonicPolynomialRelation => {
            if super::relation::is_monic(learner) {
                parsed(super::relation::read(learner), expected)
            } else {
                match super::relation::read(learner) {
                    Ok(_) => decided(false),
                    Err(reason) => Outcome::Undecidable(reason),
                }
            }
        }
        AnswerContract::RelationSetup => {
            parsed(super::setup::read(&super::union::spoken(learner)), expected)
        }
        AnswerContract::Function {
            vars,
            up_to_constant,
            domain,
        } => FunctionSpec::new(vars, *up_to_constant, domain)
            .map_or_else(Outcome::Undecidable, |spec| {
                super::function::check(&spec, text, learner)
            }),
        AnswerContract::FunctionForm { vars, domain } => FunctionSpec::new(vars, false, domain)
            .map_or_else(Outcome::Undecidable, |spec| {
                super::shape::check(&spec, text, learner)
            }),
        AnswerContract::Property { check, args } => super::property::grade(*check, args, learner),
        AnswerContract::Exact
        | AnswerContract::Approx { .. }
        | AnswerContract::Tolerance { .. }
        | AnswerContract::Unit { .. }
        | AnswerContract::QuotientRemainder { .. }
        | AnswerContract::Coordinates { .. }
        | AnswerContract::Set
        | AnswerContract::RequiredForm { .. }
        | AnswerContract::None
        | AnswerContract::Written => return None,
    };
    Some(outcome)
}

/// Grade a matrix: the shape decides wrong, the entries compare exactly.
fn matrix(expected: &Canon, rows: u8, cols: u8, learner: &str, scaled: bool) -> Outcome {
    let column;
    let learner = match flat_column(learner, rows, cols).filter(|_| scaled) {
        Some(written) => {
            column = written;
            column.as_str()
        }
        None => learner,
    };
    let spelled = super::structured::matrix_spelling(learner);
    let learner = spelled.as_str();
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
        Ok(value) if scaled => decided(super::structured::scalar_multiple(expected, &value)),
        Ok(value) => decided(same_answer(expected, &value)),
        Err(reason) => Outcome::Undecidable(reason),
    }
}

/// A column of `rows` entries written as one flat list: `[1,2]` reads as
/// `[[1],[2]]`. `None` for any other text.
fn flat_column(text: &str, rows: u8, cols: u8) -> Option<String> {
    let inner = text.trim().strip_prefix('[')?.strip_suffix(']')?;
    if cols != 1 || rows < 2 || inner.contains(['[', ']', '(', ')', '{', '}']) {
        return None;
    }
    let entries: Vec<&str> = inner.split(',').map(str::trim).collect();
    (entries.len() == usize::from(rows) && entries.iter().all(|entry| !entry.is_empty())).then(
        || {
            let rows: Vec<String> = entries.iter().map(|entry| format!("[{entry}]")).collect();
            format!("[{}]", rows.join(","))
        },
    )
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
    let strict = multipart_strict(parts, expected, learner);
    if matches!(strict, Outcome::Decided(Verdict { correct: true, .. })) {
        return strict;
    }
    let natural = named_key_parts(parts, expected)
        .is_some_and(|keys| super::multipart_natural::correct(parts, &keys, learner));
    if natural { decided(true) } else { strict }
}

fn multipart_strict(parts: &[AnswerPart], expected: &str, learner: &str) -> Outcome {
    let Some(expected) = named_key_parts(parts, expected) else {
        return refused_parts();
    };
    // Unnamed parts are read by position. A guessed position that puts a value
    // the part cannot read is a wrong answer, as an unreadable text was before.
    let (learner, by_position) = match named_parts(parts, learner) {
        Some(named) => (named, false),
        None => match ordered_parts(parts, learner)
            .or_else(|| super::multipart_natural::tuple(parts, learner))
        {
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

/// Whether a learner decimal with more digits than the contract asks for is a
/// correct, more precise spelling of the key.
///
/// A key that is itself a rounding (`2.81` with two decimals) is the value of
/// the learner rounded to the same count of digits. A key that is the exact
/// value (`sqrt(2)`, `1/3`) takes the learner decimal only as its exact rounding
/// at the learner's own digits. At most six extra digits count.
fn more_precise(expected: &Canon, value: &BigRational, scale: u32) -> bool {
    let ten = BigRational::from_integer(BigInt::from(10_u32));
    let mut own = scale;
    let mut grid = ten.pow(i32::try_from(scale).unwrap_or(0));
    while !(value * &grid).is_integer() {
        own += 1;
        grid *= &ten;
        if own > scale + 6 {
            return false;
        }
    }
    if matches!(rounds_to_constant(expected, value, own), Rounding::Same) {
        return true;
    }
    let Canon::Rational(key) = expected else {
        return false;
    };
    let base = ten.pow(i32::try_from(scale).unwrap_or(0));
    let half = BigRational::new(1.into(), 2.into());
    let roundings = |number: &BigRational| -> Vec<BigRational> {
        let scaled = number * &base;
        if (scaled.clone() - scaled.floor()) == half {
            // A tie at the last digit: both roundings count.
            vec![scaled.floor(), scaled.ceil()]
        } else {
            vec![BigRational::from_integer(scaled.round().to_integer())]
        }
    };
    let key_scaled = key * &base;
    let key_tie = (key_scaled.clone() - key_scaled.floor()) == half;
    if !key_scaled.is_integer() && !key_tie {
        return false;
    }
    let wanted = roundings(key);
    roundings(value)
        .iter()
        .any(|rounded| wanted.contains(rounded))
}

/// Whether an exact learner value (`sqrt(80)`, `1/3`, `pi`) is the key within
/// its digits: the key is the rounding of the learner value.
fn exact_within_key(expected: &Canon, learner: &Canon, scale: u32) -> bool {
    if expected == learner {
        return true;
    }
    let Canon::Rational(key) = expected else {
        return false;
    };
    let base = BigRational::from_integer(BigInt::from(10_u32).pow(scale));
    (key * &base).is_integer() && matches!(rounds_to_constant(learner, key, scale), Rounding::Same)
}

/// Whether the key sits exactly halfway between two decimals of the grid and the
/// learner wrote the other one. `1.005` to two places is `1.00` by the
/// half-to-even rule and `1.01` by the half-up rule, and both are roundings.
fn is_tie_neighbor(expected: &Canon, value: &BigRational, scale: u32) -> bool {
    let Canon::Rational(key) = expected else {
        return false;
    };
    let grid = BigRational::from_integer(BigInt::from(10_u32).pow(scale));
    let scaled = key * &grid;
    let half = BigRational::new(1.into(), 2.into());
    if (scaled.clone() - scaled.floor()) != half {
        return false;
    }
    let learner = value * &grid;
    learner == scaled.floor() || learner == scaled.ceil()
}

fn approximate(expected: &Canon, learner: &Canon, decimals: u8) -> Outcome {
    let scale = u32::from(decimals);
    let Canon::Rational(value) = learner else {
        return decided(exact_within_key(expected, learner, scale));
    };
    let grid = BigRational::from_integer(BigInt::from(10_u32).pow(scale));
    // Extra digits are right only when the learner wrote a MORE precise value of
    // the authored answer (key 2.81, two decimals: `2.807` is correct).
    if !(value * &grid).is_integer() {
        return decided(
            more_precise(expected, value, scale) || exact_within_key(expected, learner, scale),
        );
    }
    match rounds_to_constant(expected, value, scale) {
        Rounding::Same => decided(true),
        Rounding::Different => decided(is_tie_neighbor(expected, value, scale)),
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
                    allow_omitted: false,
                    form: None,
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
        // A property item: a different valid example is correct.
        cases.push((
            AnswerContract::Property {
                check: crate::answer::PropertyCheck::DivisorCount,
                args: [("n".to_owned(), crate::answer::PropertyArg::Integer(3))]
                    .into_iter()
                    .collect(),
            },
            "9",
            "25",
            "10",
        ));
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
