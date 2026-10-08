//! Per-item acceptance rules (D-F1, C4, D6).

mod assignment;
mod collections;
mod division;
mod evaluate;
mod form;
pub mod function;
mod list;
mod logform;
mod lowest;
mod mixed;
mod multipart_natural;
mod named_key;
mod nested;
mod notation;
mod phrase;
mod power;
mod prefix;
pub mod property;
mod radical;
mod relation;
mod scientific;
mod sentence;
mod setup;
mod shape;
mod spoken;
mod structured;
mod triage;
mod union;
mod vector_line;
mod wholes;
mod word;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Canon, MAX_ANSWER_CHARS, Quantity, Undecidable, canonical_form};
use structured::{label_value, multipart_values, tolerance_value, validate_shape};

pub use evaluate::{check_contract, format_hint};
pub use form::NumericForm;
pub use property::{PropertyArg, PropertyArgs, PropertyCheck};
pub use triage::{TriageVerdict, triage_verdict};
pub use wholes::rewrite as whole_number_list;
pub use word::mutants as word_mutants;

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
    Unit {
        quantity: Quantity,
        unit: String,
        /// A bare number may use the unit named by the question. Defaults to false.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        allow_omitted: bool,
        /// A required form of the magnitude, read without its unit. `None`
        /// accepts every equal value.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        form: Option<UnitForm>,
    },
    /// An integer quotient and a nonnegative integer remainder.
    QuotientRemainder {
        /// An optional positive divisor bounds the remainder.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        divisor: Option<u64>,
    },
    /// A polynomial quotient and remainder (`x + 2 remainder 3`); the stored
    /// divisor also accepts the mixed form `x + 2 + 3/(x + 1)`.
    PolynomialDivision { divisor: String },
    /// An ordered tuple of two to four real numeric coordinates.
    Coordinates { arity: u8 },
    /// A grid of exact rational entries, compared entry by entry.
    Matrix {
        /// The count of rows of the grid.
        rows: u8,
        /// The count of columns of the grid.
        cols: u8,
    },
    /// A grid of exact rational entries equal to the key times one nonzero
    /// rational: any scalar multiple of an eigenvector or of a direction.
    ScalarMultiple {
        /// The count of rows of the grid.
        rows: u8,
        /// The count of columns of the grid.
        cols: u8,
    },
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
    /// A product whose factors do not commute (a group word, a matrix product,
    /// a product of cycles, a matrix size), compared factor by factor in order.
    OrderedWord,
    /// A polynomial equality or inequality, compared after exact normalization.
    PolynomialRelation,
    /// A polynomial relation whose leading coefficient is 1 as the learner wrote it.
    MonicPolynomialRelation,
    /// An equality or inequality that preserves the authored unsolved operation structure.
    RelationSetup,
    /// A closed choice vocabulary, with explicit aliases per option.
    Label { options: Vec<Vec<String>> },
    /// Named parts, each with its own deterministic policy.
    Multipart { parts: Vec<AnswerPart> },
    /// A formula, equal to the key if the two agree at eight fixed sample points.
    ///
    /// This is the one variant that compares `f64` values (see `function`).
    Function {
        /// One to three variable names.
        vars: Vec<String>,
        /// If true, a difference that is one constant is correct (antiderivatives).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        up_to_constant: bool,
        /// The sample interval of a variable, as two exact rationals `[low, high]`.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        domain: BTreeMap<String, (String, String)>,
    },
    /// A formula equal to the key at the sample points of `function`, which also
    /// keeps the outer operation of the key (a product stays a product). For the
    /// items "write as a product", "write as a sum", "write with one function".
    FunctionForm {
        /// One to three variable names.
        vars: Vec<String>,
        /// The sample interval of a variable, as two exact rationals `[low, high]`.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        domain: BTreeMap<String, (String, String)>,
    },
    /// Any learner object with a named property: an example-generation task.
    ///
    /// The authored answer is one valid example, shown in the worked solution;
    /// the verdict tests the learner's own object against the predicate (see
    /// `property`), so a different valid example is correct.
    Property {
        /// The named predicate.
        check: PropertyCheck,
        /// The authored arguments of the predicate.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        args: PropertyArgs,
    },
    /// The item has no deterministic assessment.
    None,
    /// The item asks for a sentence. The authored answer is the reference
    /// sentence; the model grader compares the learner's sentence with it,
    /// so the contract gives no deterministic verdict.
    Written,
}

/// The form a measured answer's magnitude must take (spec section 8.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitForm {
    /// A reduced rational times one simplified square root, with a rational
    /// denominator: `2√3 m`, not `√12 m` or `6/√3 m`.
    SimplestRadical,
    /// A rational plus simplest radicals with distinct radicands: `30 + 10√3`.
    SimplestRadicalSum,
    /// A fraction in lowest terms: `1/6 L`, not `2/12 L`.
    ReducedFraction,
}

impl UnitForm {
    /// Whether the magnitude text (the answer without its unit) has this form.
    /// A magnitude the reader cannot read is left to the value check.
    #[must_use]
    pub fn holds(self, magnitude: &str) -> bool {
        match self {
            Self::SimplestRadical => radical::simplest(magnitude).unwrap_or(true),
            Self::SimplestRadicalSum => radical::simplest_sum(magnitude).unwrap_or(true),
            Self::ReducedFraction => {
                form::accepts(NumericForm::ReducedFraction, magnitude, magnitude)
                    || !magnitude.contains('/')
            }
        }
    }
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
        #[serde(default)]
        allow_omitted: bool,
        #[serde(default)]
        form: Option<UnitForm>,
    },
    QuotientRemainder {
        divisor: Option<u64>,
    },
    PolynomialDivision {
        divisor: String,
    },
    Coordinates {
        arity: u8,
    },
    Matrix {
        rows: u8,
        cols: u8,
    },
    ScalarMultiple {
        rows: u8,
        cols: u8,
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
    OrderedWord {},
    PolynomialRelation {},
    MonicPolynomialRelation {},
    RelationSetup {},
    Label {
        options: Vec<Vec<String>>,
    },
    Multipart {
        parts: Vec<AnswerPart>,
    },
    Function {
        vars: Vec<String>,
        #[serde(default)]
        up_to_constant: bool,
        #[serde(default)]
        domain: BTreeMap<String, (String, String)>,
    },
    FunctionForm {
        vars: Vec<String>,
        #[serde(default)]
        domain: BTreeMap<String, (String, String)>,
    },
    Property {
        check: PropertyCheck,
        #[serde(default)]
        args: PropertyArgs,
    },
    None {},
    Written {},
}

impl TryFrom<ContractDoc> for AnswerContract {
    type Error = Undecidable;

    fn try_from(doc: ContractDoc) -> Result<Self, Self::Error> {
        let contract = match doc {
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
            ContractDoc::Unit {
                quantity,
                unit,
                allow_omitted,
                form,
            } => Self::Unit {
                quantity,
                unit,
                allow_omitted,
                form,
            },
            ContractDoc::QuotientRemainder { divisor } => Self::QuotientRemainder { divisor },
            ContractDoc::PolynomialDivision { divisor } => Self::PolynomialDivision { divisor },
            ContractDoc::Coordinates { arity } => Self::Coordinates { arity },
            ContractDoc::Matrix { rows, cols } => Self::Matrix { rows, cols },
            ContractDoc::ScalarMultiple { rows, cols } => Self::ScalarMultiple { rows, cols },
            ContractDoc::RequiredForm { form } => Self::RequiredForm { form },
            ContractDoc::List { ordered, member } => Self::List { ordered, member },
            ContractDoc::Label { options } => Self::Label { options },
            ContractDoc::Multipart { parts } => Self::Multipart { parts },
            ContractDoc::Function {
                vars,
                up_to_constant,
                domain,
            } => Self::Function {
                vars,
                up_to_constant,
                domain,
            },
            ContractDoc::FunctionForm { vars, domain } => Self::FunctionForm { vars, domain },
            ContractDoc::Property { check, args } => Self::Property { check, args },
            document => fieldless_contract(document),
        };
        contract.validate()?;
        Ok(contract)
    }
}

/// The contract of a document that has no field.
fn fieldless_contract(document: ContractDoc) -> AnswerContract {
    match document {
        ContractDoc::Exact {} => AnswerContract::Exact,
        ContractDoc::Set {} => AnswerContract::Set,
        ContractDoc::InequalityUnion {} => AnswerContract::InequalityUnion,
        ContractDoc::ReducedRatio {} => AnswerContract::ReducedRatio,
        ContractDoc::AscendingChain {} => AnswerContract::AscendingChain,
        ContractDoc::OrderedWord {} => AnswerContract::OrderedWord,
        ContractDoc::PolynomialRelation {} => AnswerContract::PolynomialRelation,
        ContractDoc::MonicPolynomialRelation {} => AnswerContract::MonicPolynomialRelation,
        ContractDoc::RelationSetup {} => AnswerContract::RelationSetup,
        ContractDoc::RequiredAssignment {} => AnswerContract::RequiredAssignment,
        ContractDoc::RequiredInequalityNotation {} => AnswerContract::RequiredInequalityNotation,
        ContractDoc::RequiredSinglePower {} => AnswerContract::RequiredSinglePower,
        ContractDoc::RequiredNormalizedScientificNotation {} => {
            AnswerContract::RequiredNormalizedScientificNotation
        }
        ContractDoc::RequiredSimplestRadical {} => AnswerContract::RequiredSimplestRadical,
        ContractDoc::Written {} => AnswerContract::Written,
        // `None {}` is the one document that stays. A document that gets no arm
        // here gives no verdict, which is the safe result.
        _ => AnswerContract::None,
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
            Self::Matrix { rows, cols } | Self::ScalarMultiple { rows, cols }
                if *rows == 0 || *cols == 0 || u32::from(*rows) * u32::from(*cols) > 64 =>
            {
                Err(Undecidable::new(
                    "a matrix requires one to 64 entries in at least one row and column",
                ))
            }
            Self::Unit { quantity, unit, .. } => match super::unit::lookup(unit) {
                Some(found) if found.quantity == *quantity => Ok(()),
                _ => Err(Undecidable::new(
                    "the contract unit does not match its quantity",
                )),
            },
            other => other.validate_structure(),
        }
    }

    /// The rules of the contracts that hold a list, names, or variables.
    fn validate_structure(&self) -> Result<(), Undecidable> {
        match self {
            Self::List { ordered, member } => list::validate(*ordered, member),
            Self::Label { options } => structured::validate_labels(options),
            Self::Multipart { parts } => structured::validate_parts(parts),
            Self::Function {
                vars,
                up_to_constant,
                domain,
            } => function::FunctionSpec::new(vars, *up_to_constant, domain).map(|_| ()),
            Self::FunctionForm { vars, domain } => {
                function::FunctionSpec::new(vars, false, domain).map(|_| ())
            }
            Self::Property { check, args } => property::validate(*check, args),
            _ => Ok(()),
        }
    }

    /// For a `property` contract, the property in one line of plain words.
    ///
    /// The model prompts read it: the stored answer of such an item is one
    /// example, and any object with this property is correct.
    #[must_use]
    pub fn property_description(&self) -> Option<String> {
        match self {
            Self::Property { check, args } => Some(property::describe(*check, args)),
            _ => None,
        }
    }

    /// For a `property` contract, wrong-answer candidates near the example.
    ///
    /// The key self-checks take the first one that does not grade correct.
    #[must_use]
    pub fn property_near_misses(&self, example: &str) -> Option<Vec<String>> {
        match self {
            Self::Property { check, args } => Some(property::near_misses(*check, args, example)),
            _ => None,
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
            Self::Written => Err(Undecidable::new(
                "a written item has no deterministic answer contract",
            )),
            Self::Label { options } => label_value(options, expected).ok_or_else(|| {
                Undecidable::new("the authored answer is outside the choice vocabulary")
            }),
            Self::Multipart { parts } => multipart_values(parts, expected),
            Self::List { ordered, member } => list::expected(*ordered, member, expected),
            Self::InequalityUnion | Self::RequiredInequalityNotation => union::read(expected),
            Self::Matrix { rows, cols } | Self::ScalarMultiple { rows, cols } => {
                structured::matrix_value(*rows, *cols, expected)
            }
            Self::Set => collections::set_key(expected),
            contract @ (Self::RequiredAssignment
            | Self::RequiredSinglePower
            | Self::RequiredNormalizedScientificNotation
            | Self::RequiredSimplestRadical) => required_syntax_expected(contract, expected),
            Self::ReducedRatio | Self::AscendingChain => notation::expected(self, expected),
            Self::OrderedWord => word::expected(expected),
            Self::PolynomialRelation => relation::read(expected),
            Self::MonicPolynomialRelation => {
                if relation::is_monic(expected) {
                    relation::read(expected)
                } else {
                    Err(Undecidable::new("the authored relation is not monic"))
                }
            }
            Self::RelationSetup => setup::read(expected),
            Self::Function {
                vars,
                up_to_constant,
                domain,
            } => function::FunctionSpec::new(vars, *up_to_constant, domain)
                .and_then(|spec| function::expected(&spec, expected)),
            Self::FunctionForm { vars, domain } => function::FunctionSpec::new(vars, false, domain)
                .and_then(|spec| function::expected(&spec, expected)),
            Self::PolynomialDivision { divisor } => division::expected(divisor, expected),
            Self::Property { check, args } => property::expected(*check, args, expected),
            Self::Unit {
                unit,
                allow_omitted,
                form,
                ..
            } => {
                if form.is_some_and(|form| !form.holds(super::unit::magnitude(expected))) {
                    return Err(Undecidable::new(
                        "the authored answer does not match its required form",
                    ));
                }
                let explicit = super::check::canonical_quantity_in_contract_context(expected)?;
                let value = match explicit {
                    Some(value) => value,
                    None => {
                        let parsed = canonical_form(expected)?;
                        if *allow_omitted
                            && matches!(parsed, Canon::Rational(_) | Canon::Radical(_))
                        {
                            canonical_form(&format!("{expected} {unit}"))?
                        } else {
                            parsed
                        }
                    }
                };
                let value = structured::unit_compatible_value(self, value);
                if validate_shape(self, &value) {
                    Ok(value)
                } else {
                    Err(Undecidable::new(
                        "the authored answer does not match its contract shape",
                    ))
                }
            }
            Self::RequiredForm { form } if form.is_line() => {
                if form::accepts(*form, expected, expected) {
                    relation::read(expected)
                } else {
                    Err(Undecidable::new(
                        "the authored answer does not match its required form",
                    ))
                }
            }
            Self::RequiredForm { form } if !form::accepts(*form, expected, expected) => Err(
                Undecidable::new("the authored answer does not match its required form"),
            ),
            Self::Exact if phrase::class(expected).is_some() => {
                phrase::expected(expected).ok_or_else(|| Undecidable::new("not a phrase"))
            }
            _ => {
                let value = match canonical_form(expected) {
                    Ok(value) => value,
                    // `x ≠ 4` is the line without the point 4.
                    Err(_) if matches!(self, Self::Exact) && union::excludes(expected) => {
                        union::read(expected)?
                    }
                    Err(reason) => return Err(reason),
                };
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
