//! The triage rule: which contracts the deterministic grader decides.

use super::AnswerContract;

/// Whether a contract grades deterministically or needs a teacher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriageVerdict {
    /// The deterministic grader decides the answer.
    Grades,
    /// The contract claims no deterministic verdict, so a teacher grades it.
    TeachOnly,
}

/// The one triage rule over the whole contract enum, with no fallback.
///
/// Every variant the grader decides is [`TriageVerdict::Grades`]; only
/// [`AnswerContract::None`] is [`TriageVerdict::TeachOnly`] (V2, A3). The match
/// names every variant, so a new contract cannot merge into the pool until it
/// states its own triage.
#[must_use]
pub fn triage_verdict(contract: &AnswerContract) -> TriageVerdict {
    match contract {
        AnswerContract::None => TriageVerdict::TeachOnly,
        AnswerContract::Approx { .. }
        | AnswerContract::AscendingChain
        | AnswerContract::Coordinates { .. }
        | AnswerContract::Exact
        | AnswerContract::Function { .. }
        | AnswerContract::InequalityUnion
        | AnswerContract::Label { .. }
        | AnswerContract::List { .. }
        | AnswerContract::Matrix { .. }
        | AnswerContract::Multipart { .. }
        | AnswerContract::PolynomialRelation
        | AnswerContract::Property { .. }
        | AnswerContract::QuotientRemainder { .. }
        | AnswerContract::ReducedRatio
        | AnswerContract::RelationSetup
        | AnswerContract::RequiredAssignment
        | AnswerContract::RequiredForm { .. }
        | AnswerContract::RequiredInequalityNotation
        | AnswerContract::RequiredNormalizedScientificNotation
        | AnswerContract::RequiredSimplestRadical
        | AnswerContract::RequiredSinglePower
        | AnswerContract::Set
        | AnswerContract::Tolerance { .. }
        | AnswerContract::Unit { .. } => TriageVerdict::Grades,
    }
}

#[cfg(test)]
mod tests {
    use super::super::NumericForm;
    use super::*;
    use crate::answer::Quantity;

    #[test]
    fn triage_names_every_grading_contract_and_only_none_teaches() {
        assert_eq!(
            triage_verdict(&AnswerContract::None),
            TriageVerdict::TeachOnly
        );
        let graded = [
            AnswerContract::Exact,
            AnswerContract::RequiredAssignment,
            AnswerContract::Approx { decimals: 1 },
            AnswerContract::Tolerance {
                tolerance: "1/2".into(),
            },
            AnswerContract::Unit {
                quantity: Quantity::Length,
                unit: "m".into(),
                allow_omitted: false,
                form: None,
            },
            AnswerContract::QuotientRemainder { divisor: None },
            AnswerContract::Coordinates { arity: 2 },
            AnswerContract::Matrix { rows: 1, cols: 1 },
            AnswerContract::Set,
            AnswerContract::RequiredForm {
                form: NumericForm::Integer,
            },
            AnswerContract::List {
                ordered: false,
                member: Box::new(AnswerContract::Exact),
            },
            AnswerContract::InequalityUnion,
            AnswerContract::RequiredInequalityNotation,
            AnswerContract::RequiredSinglePower,
            AnswerContract::RequiredNormalizedScientificNotation,
            AnswerContract::RequiredSimplestRadical,
            AnswerContract::ReducedRatio,
            AnswerContract::AscendingChain,
            AnswerContract::PolynomialRelation,
            AnswerContract::RelationSetup,
            AnswerContract::Label {
                options: vec![vec!["yes".into()]],
            },
            AnswerContract::Multipart {
                parts: vec![crate::answer::AnswerPart {
                    name: "x".into(),
                    contract: AnswerContract::Exact,
                }],
            },
            AnswerContract::Property {
                check: crate::answer::PropertyCheck::Prime,
                args: crate::answer::PropertyArgs::new(),
            },
        ];
        assert_eq!(graded.len(), 23);
        for contract in graded {
            assert_eq!(triage_verdict(&contract), TriageVerdict::Grades);
        }
    }
}
