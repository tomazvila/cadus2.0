//! Instruction roles in a curriculum-owned finite objective.
use crate::curriculum::{FiniteCaseRole, FiniteObjectiveCase, FiniteObjectiveDomain};
use crate::template::Rejection;

pub(super) fn teach_case<'a>(
    problem: &str,
    policy: Option<&'a FiniteObjectiveDomain>,
) -> Result<Option<&'a FiniteObjectiveCase>, Rejection> {
    let Some(policy) = policy else {
        return Ok(None);
    };
    policy.validate().map_err(|message| Rejection {
        code: "finite-policy",
        message,
    })?;
    // Validation gives each trimmed problem exactly one owner.
    let case = policy
        .cases
        .iter()
        .find(|case| {
            case.variants
                .iter()
                .any(|variant| variant.problem.trim() == problem.trim())
        })
        .ok_or_else(|| Rejection {
            code: "teach-finite-case",
            message: "The worked problem must name an exact reviewed finite teaching case."
                .to_owned(),
        })?;
    if !matches!(
        case.role,
        FiniteCaseRole::TeachOnly | FiniteCaseRole::TaughtRehearsal
    ) {
        return Err(Rejection {
            code: "teach-finite-role",
            message: format!(
                "Finite case {} has role {:?} and cannot be a worked example.",
                case.id, case.role
            ),
        });
    }
    Ok(Some(case))
}

pub(super) fn permitted_collision(case: Option<&FiniteObjectiveCase>, problem: &str) -> bool {
    case.is_some_and(|case| {
        matches!(
            case.role,
            FiniteCaseRole::TeachOnly | FiniteCaseRole::TaughtRehearsal
        ) && case
            .variants
            .iter()
            .any(|variant| variant.problem.trim() == problem.trim())
    })
}
