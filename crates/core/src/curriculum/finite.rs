//! Trusted reviewed case catalogs for genuinely finite objectives.

use serde::{Deserialize, Serialize};

use super::model::Slug;

/// How one reviewed case may be used in a finite mathematical objective.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FiniteCaseRole {
    /// Eligible ordinary practice that may supply first-exposure evidence.
    PracticeFresh,
    /// Eligible instruction content and excluded from ordinary practice.
    TeachOnly,
    /// Held out from instruction, practice templates, and exemplar fallback.
    ReservedAssessment,
    /// Eligible practice after teaching; every handoff is repeat exposure.
    TaughtRehearsal,
}

impl FiniteCaseRole {
    /// Whether a template may serve this case as practice.
    #[must_use]
    pub const fn is_practice(self) -> bool {
        matches!(self, Self::PracticeFresh | Self::TaughtRehearsal)
    }
}

/// One exact reviewed rendering of a semantic finite case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FiniteCaseVariant {
    /// The exact rendered statement handed to a learner.
    pub problem: String,
    /// The exact output of the reviewed answer writer.
    pub answer: String,
    /// The reviewed answer policy of this rendering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_contract: Option<crate::answer::AnswerContract>,
}

/// One mathematical case in a reviewed finite universe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FiniteObjectiveCase {
    /// Stable semantic identity inside this knowledge point.
    pub id: Slug,
    /// Its instructional role.
    pub role: FiniteCaseRole,
    /// Exact reviewed renderings which all name this same semantic case.
    pub variants: Vec<FiniteCaseVariant>,
}

/// A reviewed complete finite universe owned by curriculum source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FiniteObjectiveDomain {
    /// The policy schema, currently exactly 1.
    pub schema_version: u32,
    /// Immutable evidence identifying the review which established the universe.
    pub review_ref: String,
    /// Every legitimate case and its role.
    pub cases: Vec<FiniteObjectiveCase>,
}

impl FiniteObjectiveDomain {
    /// Validate invariants which serde shape alone cannot express.
    pub fn validate(&self) -> Result<(), String> {
        use std::collections::BTreeSet;

        if self.schema_version != 1 {
            return Err("finite_objective_domain.schema_version must be 1".to_owned());
        }
        if self.review_ref.trim().is_empty() {
            return Err("finite_objective_domain.review_ref must not be empty".to_owned());
        }
        if self.cases.is_empty() {
            return Err("finite_objective_domain.cases must not be empty".to_owned());
        }
        let mut ids = BTreeSet::new();
        let mut variants = BTreeSet::new();
        for case in &self.cases {
            Self::validate_case(case, &mut ids, &mut variants)?;
        }
        if !self.cases.iter().any(|case| case.role.is_practice()) {
            return Err(
                "finite_objective_domain needs a practice_fresh or taught_rehearsal case"
                    .to_owned(),
            );
        }
        Ok(())
    }

    fn validate_case(
        case: &FiniteObjectiveCase,
        ids: &mut std::collections::BTreeSet<String>,
        variants: &mut std::collections::BTreeSet<String>,
    ) -> Result<(), String> {
        if !ids.insert(case.id.as_str().to_owned()) {
            return Err(format!(
                "finite case id {:?} is duplicated",
                case.id.as_str()
            ));
        }
        if case.variants.is_empty() {
            return Err(format!(
                "finite case {:?} has no reviewed variant",
                case.id.as_str()
            ));
        }
        for variant in &case.variants {
            Self::validate_variant(case, variant, variants)?;
        }
        Ok(())
    }

    fn validate_variant(
        case: &FiniteObjectiveCase,
        variant: &FiniteCaseVariant,
        variants: &mut std::collections::BTreeSet<String>,
    ) -> Result<(), String> {
        if variant.problem.trim().is_empty() || variant.answer.trim().is_empty() {
            return Err(format!(
                "finite case {:?} has a blank problem or answer",
                case.id.as_str()
            ));
        }
        if !variants.insert(variant.problem.trim().to_owned()) {
            return Err("one finite problem belongs to more than one semantic case or reviewed answer/contract variant".to_owned());
        }
        if let Some(contract) = &variant.answer_contract {
            contract.validate().map_err(|reason| reason.reason)?;
            contract
                .validate_expected(&variant.answer)
                .map(|_| ())
                .map_err(|reason| reason.reason.to_owned())
        } else {
            crate::answer::canonical_form(&variant.answer)
                .map(|_| ())
                .map_err(|reason| reason.reason.to_owned())
        }
    }

    /// Stable source fingerprint, scoped to the serving key.
    pub fn fingerprint(&self, kp_id: &str) -> Result<String, String> {
        let mut cases: Vec<&FiniteObjectiveCase> = self.cases.iter().collect();
        cases.sort_by_key(|case| case.id.as_str());
        let mut canonical_cases = Vec::with_capacity(cases.len());
        for case in cases {
            let mut variants = Vec::with_capacity(case.variants.len());
            for variant in &case.variants {
                let contract = serde_json::to_value(&variant.answer_contract)
                    .map_err(|error| format!("finite policy does not serialize: {error}"))?;
                variants.push(serde_json::json!({
                    "answer": variant.answer,
                    "answer_contract": contract,
                    "problem": variant.problem,
                }));
            }
            variants.sort_by_key(serde_json::Value::to_string);
            canonical_cases.push(serde_json::json!({
                "id": case.id.as_str(),
                "role": case.role,
                "variants": variants,
            }));
        }
        let canonical = serde_json::json!({
            "cases": canonical_cases,
            "kp_id": kp_id,
            "review_ref": self.review_ref,
            "schema_version": self.schema_version,
        });
        serde_json::to_vec(&canonical)
            .map(|bytes| super::dump::sha256_hex(&bytes))
            .map_err(|error| format!("finite policy does not serialize: {error}"))
    }

    /// The case whose exact reviewed variant matches this instance.
    #[must_use]
    pub fn case_for(
        &self,
        problem: &str,
        answer: &str,
        answer_contract: Option<&crate::answer::AnswerContract>,
    ) -> Option<&FiniteObjectiveCase> {
        self.cases.iter().find(|case| {
            case.variants.iter().any(|variant| {
                variant.problem == problem
                    && variant.answer == answer
                    && variant.answer_contract.as_ref() == answer_contract
            })
        })
    }
}
