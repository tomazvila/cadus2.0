//! Serve the exact approved practice cases of a trusted finite objective.

use super::*;
use cadus_core::pool::{ProblemSource, TemplateSource};
use cadus_core::template::{GateSpec, TemplateDoc, gate};
use cadus_store::pool::{
    FiniteDraw, FiniteEligibility, NewFiniteInstance, approved_template_current,
    finite_pool_complete_tx, insert_current_finite_tx, pop_finite_tx,
};

/// Draw an approved finite case, preserving prior-handoff provenance.
pub(super) async fn draw(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    key: &str,
    spec: &GateSpec<'_>,
    avoid: &Avoid<'_>,
    generation_context: &cadus_store::pool::GenerationContext,
) -> Result<(PoolRow, bool), ApiError> {
    let finite = spec.finite.as_ref().ok_or_else(|| no_problem(key))?;
    let approved = store(
        state,
        approved_template_current(
            &mut **tx,
            key,
            cadus_store::content::CurrentContext {
                policy_digest: Some(finite.policy_fingerprint()),
                curriculum_digest: &generation_context.curriculum_digest,
                review_engine_digest: &generation_context.review_engine_digest,
            },
        ),
    )
    .await?
    .ok_or_else(|| no_problem(key))?;
    let doc = from_body(&approved.body).map_err(|error| refused(key, error))?;
    let verified = gate(&doc, spec).map_err(|error| refused(key, error))?;
    let hashes: Vec<_> = verified
        .finite_cases
        .iter()
        .map(|case| case.instance_hash.clone())
        .collect();
    let ids: Vec<_> = verified
        .finite_cases
        .iter()
        .map(|case| case.case_id.clone())
        .collect();
    let eligibility = FiniteEligibility {
        content_digest: &approved.digest,
        policy_digest: finite.policy_fingerprint(),
        generation_context,
        allowed_instance_hashes: &hashes,
        allowed_case_ids: &ids,
    };
    let complete = store(
        state,
        finite_pool_complete_tx(tx, user_id, key, &eligibility),
    )
    .await?;
    if !complete {
        let rows = instances(
            key,
            &approved.digest,
            &doc,
            spec,
            ids.len(),
            &approved.generation_context,
        )?;
        store(
            state,
            insert_current_finite_tx(tx, user_id, key, &eligibility, &rows),
        )
        .await?
        .ok_or_else(|| no_problem(key))?;
    }
    for (mode, repeated) in [(FiniteDraw::Unclaimed, false), (FiniteDraw::Repeat, true)] {
        let popped = store(
            state,
            pop_finite_tx(tx, user_id, key, &eligibility, avoid, mode),
        )
        .await?;
        if let Some(claimed) = popped.claimed {
            return Ok((claimed.row, repeated));
        }
    }
    Err(no_problem(key))
}

fn instances(
    key: &str,
    digest: &str,
    doc: &TemplateDoc,
    spec: &GateSpec<'_>,
    count: usize,
    generation_context: &cadus_store::pool::GenerationContext,
) -> Result<Vec<NewFiniteInstance>, ApiError> {
    let finite = spec.finite.as_ref().ok_or_else(|| no_problem(key))?;
    let source = TemplateSource::new(key, doc)
        .map_err(|error| refused(key, error))?
        .with_digest(digest)
        .with_exemplars(spec.exemplars)
        .with_finite_policy(key, finite.policy)
        .map_err(|error| refused(key, error))?;
    let batch = source
        .fill(key, count, 0)
        .map_err(|error| refused(key, error))?;
    if batch.instances().len() != count || !batch.refusals().is_empty() {
        return Err(refused(
            key,
            "the finite source did not produce every verified case",
        ));
    }
    batch
        .instances()
        .iter()
        .map(|instance| {
            let case = finite
                .match_practice_instance(instance)
                .map_err(|error| refused(key, error))?;
            Ok(NewFiniteInstance {
                case_id: case.case_id,
                instance: NewInstance::from_instance(
                    instance,
                    Source::Template,
                    Some(digest.to_owned()),
                    0,
                )
                .with_generation_context(generation_context),
            })
        })
        .collect()
}

fn refused(key: &str, reason: impl std::fmt::Display) -> ApiError {
    tracing::warn!(kp_id = key, %reason, "finite approved content refused before handoff");
    no_problem(key)
}
