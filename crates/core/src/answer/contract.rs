//! Per-item acceptance rules (D-F1, C4, D6).

mod assignment;
mod evaluate;
mod form;
mod list;
mod notation;
mod power;
mod radical;
mod relation;
mod scientific;
mod setup;
mod structured;
mod union;

use serde::{Deserialize, Serialize};

use super::{Canon, MAX_ANSWER_CHARS, Quantity, Undecidable, canonical_form};
use structured::{label_value, multipart_values, tolerance_value, validate_shape};

pub use evaluate::check_contract;
pub use form::NumericForm;

/// A reviewed item's answer policy. Absence retains the historical policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", try_from = "ContractDoc")]
pub enum AnswerContract {
    /// Exact mathematical equivalence.
    Exact,
    /// An exact value written with the authored assignment target.
    RequiredAssignment,
    /// The exact value rounded half-to-even to the authored decimal count.
    Approx { decimals: u8 },
    /// An inclusive absolute error bound, as an exact positive rational.
    #[serde(rename = "approx")]
    Tolerance { tolerance: String },
    /// A measured value; equivalent units of the same quantity are accepted.
    Unit { quantity: Quantity, unit: String },
    /// An integer quotient and a nonnegative integer remainder.
    QuotientRemainder {
        /// An optional positive divisor bounds the remainder.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        divisor: Option<u64>,
    },
    /// An ordered tuple of two to four real numeric coordinates.
    Coordinates { arity: u8 },
    /// An unordered set; order and repeated members have no effect.
    Set,
    /// An authored numeric notation requirement.
    RequiredForm { form: NumericForm },
    /// A bounded homogeneous list; unordered lists retain repeated members.
    List {
        ordered: bool,
        member: Box<AnswerContract>,
    },
    /// An exact union of rational intervals over one named unknown.
    InequalityUnion,
    /// An exact inequality union written in the same notation as the authored answer.
    RequiredInequalityNotation,
    /// An exact numeric value written as one power of the authored literal base.
    RequiredSinglePower,
    /// An exact value written in normalized scientific notation.
    RequiredNormalizedScientificNotation,
    /// An exact reduced rational times one simplified square root.
    RequiredSimplestRadical,
    /// A ratio of two positive integers written in lowest terms as `a:b`.
    ReducedRatio,
    /// A strictly ascending list of exact numbers joined by `<`.
    AscendingChain,
    /// A polynomial equality or inequality, compared after exact normalization.
    PolynomialRelation,
    /// An equality or inequality that preserves the authored unsolved operation structure.
    RelationSetup,
    /// A closed choice vocabulary, with explicit aliases per option.
    Label { options: Vec<Vec<String>> },
    /// Named parts, each with its own deterministic policy.
    Multipart { parts: Vec<AnswerPart> },
    /// The item has no deterministic assessment.
    None,
}

/// One named part of an answer, written as `name = value`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerPart {
    pub name: String,
    pub contract: AnswerContract,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ContractDoc {
    Exact {},
    RequiredAssignment {},
    Approx {
        decimals: Option<u8>,
        tolerance: Option<String>,
    },
    Unit {
        quantity: Quantity,
        unit: String,
    },
    QuotientRemainder {
        divisor: Option<u64>,
    },
    Coordinates {
        arity: u8,
    },
    Set {},
    RequiredForm {
        form: NumericForm,
    },
    List {
        ordered: bool,
        member: Box<AnswerContract>,
    },
    InequalityUnion {},
    RequiredInequalityNotation {},
    RequiredSinglePower {},
    RequiredNormalizedScientificNotation {},
    RequiredSimplestRadical {},
    ReducedRatio {},
    AscendingChain {},
    PolynomialRelation {},
    RelationSetup {},
    Label {
        options: Vec<Vec<String>>,
    },
    Multipart {
        parts: Vec<AnswerPart>,
    },
    None {},
}

impl TryFrom<ContractDoc> for AnswerContract {
    type Error = Undecidable;

    fn try_from(doc: ContractDoc) -> Result<Self, Self::Error> {
        let contract = match doc {
            ContractDoc::Exact {} => Self::Exact,
            ContractDoc::Approx {
                decimals: Some(decimals),
                tolerance: None,
            } => Self::Approx { decimals },
            ContractDoc::Approx {
                decimals: None,
                tolerance: Some(tolerance),
            } => Self::Tolerance { tolerance },
            ContractDoc::Approx { .. } => {
                return Err(Undecidable::new(
                    "choose exactly one approximate answer policy",
                ));
            }
            ContractDoc::Unit { quantity, unit } => Self::Unit { quantity, unit },
            ContractDoc::QuotientRemainder { divisor } => Self::QuotientRemainder { divisor },
            ContractDoc::Coordinates { arity } => Self::Coordinates { arity },
            ContractDoc::Set {} => Self::Set,
            ContractDoc::RequiredForm { form } => Self::RequiredForm { form },
            ContractDoc::List { ordered, member } => Self::List { ordered, member },
            ContractDoc::InequalityUnion {} => Self::InequalityUnion,
            document @ (ContractDoc::RequiredAssignment {}
            | ContractDoc::RequiredInequalityNotation {}
            | ContractDoc::RequiredSinglePower {}
            | ContractDoc::RequiredNormalizedScientificNotation {}
            | ContractDoc::RequiredSimplestRadical {}) => required_syntax_contract(document),
            ContractDoc::ReducedRatio {} => Self::ReducedRatio,
            ContractDoc::AscendingChain {} => Self::AscendingChain,
            ContractDoc::PolynomialRelation {} => Self::PolynomialRelation,
            ContractDoc::RelationSetup {} => Self::RelationSetup,
            ContractDoc::Label { options } => Self::Label { options },
            ContractDoc::Multipart { parts } => Self::Multipart { parts },
            ContractDoc::None {} => Self::None,
        };
        contract.validate()?;
        Ok(contract)
    }
}

fn required_syntax_contract(document: ContractDoc) -> AnswerContract {
    match document {
        ContractDoc::RequiredAssignment {} => AnswerContract::RequiredAssignment,
        ContractDoc::RequiredInequalityNotation {} => AnswerContract::RequiredInequalityNotation,
        ContractDoc::RequiredSinglePower {} => AnswerContract::RequiredSinglePower,
        ContractDoc::RequiredNormalizedScientificNotation {} => {
            AnswerContract::RequiredNormalizedScientificNotation
        }
        ContractDoc::RequiredSimplestRadical {} => AnswerContract::RequiredSimplestRadical,
        _ => unreachable!("caller supplies a required syntax contract"),
    }
}

impl AnswerContract {
    /// Refuse a policy outside the bounded implementation.
    pub fn validate(&self) -> Result<(), Undecidable> {
        match self {
            Self::Approx { decimals } if *decimals > 18 => Err(Undecidable::new(
                "the answer contract supports at most 18 decimal places",
            )),
            Self::Tolerance { tolerance } => tolerance_value(tolerance).map(|_| ()),
            Self::Coordinates { arity } if !(2..=4).contains(arity) => Err(Undecidable::new(
                "coordinates require two to four dimensions",
            )),
            Self::QuotientRemainder { divisor: Some(0) } => Err(Undecidable::new(
                "a quotient contract requires a positive divisor",
            )),
            Self::Unit { quantity, unit } => match super::unit::lookup(unit) {
                Some(found) if found.quantity == *quantity => Ok(()),
                _ => Err(Undecidable::new(
                    "the contract unit does not match its quantity",
                )),
            },
            Self::List { ordered, member } => list::validate(*ordered, member),
            Self::Label { options } => structured::validate_labels(options),
            Self::Multipart { parts } => structured::validate_parts(parts),
            _ => Ok(()),
        }
    }

    /// Validate the authored answer before the item enters the serve pool.
    pub fn validate_expected(&self, expected: &str) -> Result<Canon, Undecidable> {
        self.validate()?;
        bounded(expected)?;
        match self {
            Self::None => Err(Undecidable::new(
                "the item has no deterministic answer contract",
            )),
            Self::Label { options } => label_value(options, expected).ok_or_else(|| {
                Undecidable::new("the authored answer is outside the choice vocabulary")
            }),
            Self::Multipart { parts } => multipart_values(parts, expected),
            Self::List { ordered, member } => list::expected(*ordered, member, expected),
            Self::InequalityUnion | Self::RequiredInequalityNotation => union::read(expected),
            contract @ (Self::RequiredAssignment
            | Self::RequiredSinglePower
            | Self::RequiredNormalizedScientificNotation
            | Self::RequiredSimplestRadical) => required_syntax_expected(contract, expected),
            Self::ReducedRatio => notation::reduced_ratio(expected),
            Self::AscendingChain => notation::ascending_chain(expected),
            Self::PolynomialRelation => relation::read(expected),
            Self::RelationSetup => setup::read(expected),
            Self::RequiredForm { form } if !form::accepts(*form, expected) => Err(
                Undecidable::new("the authored answer does not match its required form"),
            ),
            _ => {
                let value = canonical_form(expected)?;
                if validate_shape(self, &value) {
                    Ok(value)
                } else {
                    Err(Undecidable::new(
                        "the authored answer does not match its contract shape",
                    ))
                }
            }
        }
    }
}

fn required_syntax_expected(
    contract: &AnswerContract,
    expected: &str,
) -> Result<Canon, Undecidable> {
    match contract {
        AnswerContract::RequiredAssignment => assignment::expected(expected),
        AnswerContract::RequiredSinglePower => power::expected(expected),
        AnswerContract::RequiredNormalizedScientificNotation => scientific::expected(expected),
        AnswerContract::RequiredSimplestRadical => radical::expected(expected),
        _ => unreachable!("caller supplies a required syntax contract"),
    }
}

fn bounded(text: &str) -> Result<(), Undecidable> {
    if text.chars().count() > MAX_ANSWER_CHARS {
        Err(Undecidable::new("the answer is longer than the input cap"))
    } else {
        Ok(())
    }
}
