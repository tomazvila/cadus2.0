//! Shared, side-effect-free eligibility check for online and offline authoring.

use cadus_core::curriculum::AnswerKind;
use cadus_core::template::{Rejection, TEMPLATABLE_KINDS};

use crate::authoring::prompt::{AuthoringSpec, Kind};

/// Reject a kind whose production gate cannot accept any candidate.
///
/// Multi-step templates can carry a deterministic pending-item contract, so
/// their eligibility is decided when the document reaches the contract gate.
/// This check is shared by the worker pass and offline document verification.
///
/// # Errors
/// Returns `answer-kind` for an unsupported template or diagnosis kind.
pub fn preflight(kind: Kind, spec: &AuthoringSpec) -> Result<(), Rejection> {
    let gated = matches!(kind, Kind::Template | Kind::Diagnosis);
    let candidate_contract = kind == Kind::Template && spec.answer_kind == AnswerKind::MultiStep;
    if gated && !TEMPLATABLE_KINDS.contains(&spec.answer_kind) && !candidate_contract {
        return Err(Rejection {
            code: "answer-kind",
            message: format!(
                "answer kind {} is not symbolically decidable",
                spec.answer_kind
            ),
        });
    }
    Ok(())
}
