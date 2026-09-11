//! Exact case matching for reviewed finite mathematical objectives.

use std::collections::{BTreeMap, BTreeSet};

use crate::curriculum::FiniteObjectiveDomain;
use crate::template::document::{Compiled, Instance};

use super::space::Walk;
use super::{FiniteGateSpec, GateSpec, Rejection, VerifiedFiniteCase};

pub(super) fn match_practice_instance(
    finite: &FiniteGateSpec<'_>,
    instance: &Instance,
) -> Result<VerifiedFiniteCase, Rejection> {
    let Some(case) = finite.policy.case_for(
        &instance.text,
        &instance.answer,
        instance.answer_contract.as_ref(),
    ) else {
        return Err(Rejection::new(
            "finite-case-unknown",
            format!(
                "the rendered problem {:?} and its reviewed answer do not name a case in finite policy {}",
                instance.text, finite.policy_fingerprint
            ),
        ));
    };
    if !case.role.is_practice() {
        return Err(Rejection::new(
            "finite-case-role",
            format!(
                "finite case {:?} has role {:?} and is not eligible for ordinary practice",
                case.id.as_str(),
                case.role
            ),
        ));
    }
    Ok(VerifiedFiniteCase {
        case_id: case.id.as_str().to_owned(),
        role: case.role,
        instance_hash: instance.instance_hash.clone(),
    })
}

pub(super) fn check_complete(
    compiled: &Compiled<'_>,
    spec: &GateSpec<'_>,
    walk: &Walk,
) -> Result<Vec<VerifiedFiniteCase>, Rejection> {
    let Some(finite) = spec.finite.as_ref() else {
        return Ok(Vec::new());
    };
    if !walk.exhaustive {
        return Err(Rejection::new(
            "finite-case-exhaustive",
            "a reviewed finite objective must walk every satisfying tuple".to_owned(),
        ));
    }
    let expected: BTreeSet<&str> = finite
        .policy
        .cases
        .iter()
        .filter(|case| case.role.is_practice())
        .map(|case| case.id.as_str())
        .collect();
    let mut found: BTreeMap<String, VerifiedFiniteCase> = BTreeMap::new();
    for bindings in &walk.tuples {
        let instance = compiled.instantiate(bindings.clone()).map_err(|error| {
            Rejection::new(
                "finite-case-instance",
                format!("a finite case did not instantiate: {error}"),
            )
        })?;
        let evidence = match_practice_instance(finite, &instance)?;
        let case_id = evidence.case_id.clone();
        if found.insert(case_id.clone(), evidence).is_some() {
            return Err(Rejection::new(
                "finite-case-duplicate",
                format!(
                    "more than one satisfying tuple renders semantic finite case {:?}",
                    case_id
                ),
            ));
        }
    }
    let observed: BTreeSet<&str> = found.keys().map(String::as_str).collect();
    if observed != expected {
        let missing: Vec<&str> = expected.difference(&observed).copied().collect();
        return Err(Rejection::new(
            "finite-case-missing",
            format!(
                "the template omits reviewed practice case(s) {missing:?} from finite policy {}",
                finite.policy_fingerprint
            ),
        ));
    }
    Ok(found.into_values().collect())
}

pub(super) fn check_policy(policy: &FiniteObjectiveDomain) -> Result<(), Rejection> {
    policy
        .validate()
        .map_err(|message| Rejection::new("finite-policy", message))
}
