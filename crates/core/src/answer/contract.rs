//! Per-item acceptance rules (D-F1, C4, D6).

use num_bigint::BigInt;
use num_rational::BigRational;
use serde::{Deserialize, Serialize};

use super::{Canon, MAX_ANSWER_CHARS, Outcome, Rounding, Undecidable, Verdict};
use super::{canonical_form, rounds_to, same_answer};

/// A reviewed item's answer policy. Absence retains the historical policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", try_from = "ContractDoc")]
pub enum AnswerContract {
    /// Exact mathematical equivalence, with no learner-selected approximation.
    Exact,
    /// The exact value rounded half-to-even to the authored decimal count.
    Approx { decimals: u8 },
    /// The item has no deterministic assessment.
    None,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ContractDoc {
    Exact {},
    Approx { decimals: u8 },
    None {},
}

impl TryFrom<ContractDoc> for AnswerContract {
    type Error = Undecidable;

    fn try_from(doc: ContractDoc) -> Result<Self, Self::Error> {
        let contract = match doc {
            ContractDoc::Exact {} => Self::Exact,
            ContractDoc::Approx { decimals } => Self::Approx { decimals },
            ContractDoc::None {} => Self::None,
        };
        contract.validate()?;
        Ok(contract)
    }
}

impl AnswerContract {
    /// Refuse a policy outside the bounded implementation.
    pub fn validate(self) -> Result<(), Undecidable> {
        if matches!(self, Self::Approx { decimals } if decimals > 18) {
            return Err(Undecidable::new(
                "the answer contract supports at most 18 decimal places",
            ));
        }
        Ok(())
    }

    /// Validate the authored answer before the item enters the serve pool.
    pub fn validate_expected(self, expected: &str) -> Result<Canon, Undecidable> {
        self.validate()?;
        if self == Self::None {
            return Err(Undecidable::new(
                "the item has no deterministic answer contract",
            ));
        }
        if expected.chars().count() > MAX_ANSWER_CHARS {
            return Err(Undecidable::new("the answer is longer than the input cap"));
        }
        let value = canonical_form(expected)?;
        if matches!(self, Self::Approx { .. })
            && !matches!(value, Canon::Rational(_) | Canon::Radical(_))
        {
            return Err(Undecidable::new(
                "an approximate answer contract requires a supported number",
            ));
        }
        Ok(value)
    }
}

/// Decide the authored policy with exact arithmetic and bounded input.
#[must_use]
pub fn check_contract(expected: &str, learner: &str, contract: AnswerContract) -> Outcome {
    let expected = match contract.validate_expected(expected) {
        Ok(value) => value,
        Err(reason) => return Outcome::Undecidable(reason),
    };
    if learner.chars().count() > MAX_ANSWER_CHARS {
        return Outcome::Undecidable(Undecidable::new("the answer is longer than the input cap"));
    }
    if learner.trim().is_empty() {
        return decided(false);
    }
    let learner = match canonical_form(learner) {
        Ok(value) => value,
        Err(reason) => return Outcome::Undecidable(reason),
    };
    match contract {
        AnswerContract::Exact => decided(same_answer(&expected, &learner)),
        AnswerContract::Approx { decimals } => approximate(&expected, &learner, decimals),
        AnswerContract::None => Outcome::Undecidable(Undecidable::new(
            "the item has no deterministic answer contract",
        )),
    }
}

fn decided(correct: bool) -> Outcome {
    Outcome::Decided(Verdict {
        correct,
        notation: false,
    })
}

fn approximate(expected: &Canon, learner: &Canon, decimals: u8) -> Outcome {
    let Canon::Rational(value) = learner else {
        return decided(false);
    };
    let scale = u32::from(decimals);
    let grid = BigRational::from_integer(BigInt::from(10_u32).pow(scale));
    // The rounding helper compares integral scaled values. Reject extra digits
    // before that comparison so truncation cannot admit a nearby wrong value.
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
