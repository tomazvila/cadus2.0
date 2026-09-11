//! One `(user, kp)` pair: the source, the gate re-run, the batch, the insert.

use cadus_core::curriculum::{Exemplar, KnowledgePoint};
use cadus_core::pool::{
    Batch, ExemplarSource, ProblemSource, REFUSAL_FLAG_PERCENT, Source, TemplateSource,
};
use cadus_core::template::{GateSpec, TemplateDoc, from_body, gate};
use cadus_store::Db;
use cadus_store::content::CurrentContext;
use cadus_store::pool::{GenerationContext, NewInstance, PoolTarget};

use super::{
    AnswerKindOf, RefillJob, RefillState, batch_seed,
    finite::{fill_finite, new_instance},
};
use crate::WorkerError;

/// What one target gave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Filled {
    /// The pair took rows from the named source.
    Rows {
        source: Source,
        inserted: u64,
        refused: u64,
        flagged: bool,
    },
    /// The finite case set already exists and rotates after first exposure.
    RotationalComplete,
    /// The pair has no approved template and no exemplar.
    NoSource,
}

/// The refill error of one source that did not fill.
fn refill_error(err: impl std::fmt::Display) -> WorkerError {
    WorkerError::Refill(err.to_string())
}

/// Fill one `(user, kp)` pair up to the target depth.
///
/// The target list holds the pairs UNDER the target depth alone
/// (`cadus_store::pool::refill_targets_skipping`), so `need` is 1 or more.
pub(super) async fn refill_target(
    db: &Db,
    job: &RefillJob<'_>,
    state: &mut RefillState,
    target: &PoolTarget,
    nonce: u64,
) -> Result<Filled, WorkerError> {
    let need = usize::try_from(job.cfg.target_depth.saturating_sub(target.depth)).unwrap_or(0);
    let seed = batch_seed(job.cfg.base_seed, target.user_id, &target.kp_id, nonce);
    let known = job.knowledge_point(&target.kp_id);

    let finite = known.and_then(|(kp, _)| kp.finite_objective_domain.as_ref());
    let policy_digest = finite
        .map(|policy| policy.fingerprint(&target.kp_id))
        .transpose()
        .map_err(refill_error)?;
    let generation_context = GenerationContext {
        curriculum_digest: cadus_core::curriculum::review_context_digest(job.curriculum)
            .map_err(refill_error)?,
        review_engine_digest: cadus_core::review_engine::DIGEST.to_owned(),
    };

    // Step 1: only the newest approval bound to the current trusted policy.
    if let Some(approved) = cadus_store::pool::approved_template_current(
        db.pool(),
        &target.kp_id,
        CurrentContext {
            policy_digest: policy_digest.as_deref(),
            curriculum_digest: &generation_context.curriculum_digest,
            review_engine_digest: &generation_context.review_engine_digest,
        },
    )
    .await?
    {
        let cache_key = policy_digest.as_ref().map_or_else(
            || approved.digest.clone(),
            |policy| format!("{}:{policy}", approved.digest),
        );
        match template_instances(
            &cache_key,
            &approved.digest,
            &approved.body,
            state,
            &target.kp_id,
            known,
        ) {
            Ok(Some(doc)) => {
                return fill_from_template(
                    db,
                    target,
                    &approved.digest,
                    &doc,
                    known,
                    need,
                    seed,
                    &approved.generation_context,
                )
                .await;
            }
            Ok(None) => {}
            Err(reason) if finite.is_some() => {
                tracing::warn!(
                    kp_id = %target.kp_id,
                    digest = %approved.digest,
                    reason = %reason,
                    "refill: the current-policy finite template is not servable"
                );
            }
            Err(reason) => {
                tracing::warn!(
                    kp_id = %target.kp_id,
                    digest = %approved.digest,
                    reason = %reason,
                    "refill: the approved template is not servable; falling back to exemplars (A6)"
                );
            }
        }
    }

    // A finite objective never falls back to a partial exemplar subset. Its
    // reviewed current-policy template is the only complete practice source.
    if finite.is_some() {
        return Ok(Filled::NoSource);
    }

    // Step 2: the A6 exemplar fallback. No model call, no synchronous
    // generation: the rotation is the authored exemplar list.
    let Some((kp, _)) = known else {
        return Ok(Filled::NoSource);
    };
    fill_from_exemplars(db, target, kp, need, seed).await
}

/// Draw one batch from the approved template and write it (A1, C4).
///
/// The source carries the authored exemplars, so the per-instance re-check
/// applies the A6 envelope to every instance it draws. A knowledge point the
/// curriculum does not name has none, and the envelope rule then stays silent.
#[allow(clippy::too_many_arguments)]
async fn fill_from_template(
    db: &Db,
    target: &PoolTarget,
    digest: &str,
    doc: &TemplateDoc,
    known: Option<(&KnowledgePoint, AnswerKindOf)>,
    need: usize,
    seed: u64,
    generation_context: &GenerationContext,
) -> Result<Filled, WorkerError> {
    let exemplars: &[Exemplar] = known.map_or(&[], |(kp, _)| kp.exemplars.as_slice());
    let source = TemplateSource::new(target.kp_id.clone(), doc)
        .map_err(refill_error)?
        .with_digest(digest)
        .with_exemplars(exemplars);
    if let Some(policy) = known.and_then(|(kp, _)| kp.finite_objective_domain.as_ref()) {
        return fill_finite(db, target, digest, source, policy, seed, generation_context).await;
    }
    let batch = source
        .fill(&target.kp_id, need, seed)
        .map_err(refill_error)?;
    write_batch(
        db,
        target,
        &batch,
        Source::Template,
        Some(digest),
        seed,
        Some(generation_context),
    )
    .await
}

/// Draw one batch from the authored exemplars and write it (A6).
async fn fill_from_exemplars(
    db: &Db,
    target: &PoolTarget,
    kp: &KnowledgePoint,
    need: usize,
    seed: u64,
) -> Result<Filled, WorkerError> {
    let source = ExemplarSource::new(target.kp_id.clone(), kp.exemplars.as_slice());
    if !source.covers_ring() {
        let exemplars = source.len();
        tracing::info!(
            kp_id = %target.kp_id,
            exemplars,
            "refill: the exemplar count is under the anti-repeat ring (A6)"
        );
    }
    // The source fills the knowledge point it was built for, so the one refusal
    // it gives is `FillError::NoExemplar`: no exemplar has an answer the checker
    // decides. That pair has no source.
    let Ok(batch) = source.fill(&target.kp_id, need, seed) else {
        return Ok(Filled::NoSource);
    };
    write_batch(db, target, &batch, Source::Exemplar, None, seed, None).await
}

/// Log the refusals of one batch, write the rest, and count both.
async fn write_batch(
    db: &Db,
    target: &PoolTarget,
    batch: &Batch,
    source: Source,
    digest: Option<&str>,
    seed: u64,
    generation_context: Option<&GenerationContext>,
) -> Result<Filled, WorkerError> {
    let flagged = report_refusals(&target.kp_id, digest, batch);
    let refused = u64::try_from(batch.refusals().len()).unwrap_or(u64::MAX);
    let inserted = insert(
        db,
        target,
        batch.instances(),
        source,
        digest,
        seed,
        generation_context,
    )
    .await?;
    Ok(Filled::Rows {
        source,
        inserted,
        refused,
        flagged,
    })
}

/// Log every refusal of one batch, and say whether the pair is flagged (C4).
///
/// A refusal means the gate accepted a document one of whose instances breaks a
/// per-instance rule. That is a content defect, so every one of them reaches the
/// log with the rule that refused it.
pub(super) fn report_refusals(kp_id: &str, digest: Option<&str>, batch: &Batch) -> bool {
    let digest = digest.unwrap_or("");
    for refused in batch.refusals() {
        tracing::warn!(
            kp_id = %kp_id,
            digest,
            code = %refused.code,
            reason = %refused.message,
            "refill: the per-instance check refused an instance; it is not in the pool (C4)"
        );
    }
    let flagged = batch.is_flagged();
    if flagged {
        let refused = batch.refusals().len();
        let checked = batch.checked();
        let percent = batch.refusal_percent();
        tracing::warn!(
            kp_id = %kp_id,
            digest,
            refused,
            checked,
            percent,
            limit = REFUSAL_FLAG_PERCENT,
            "refill: the refusal rate of this knowledge point is above the limit (C4, C6)"
        );
    }
    flagged
}

/// Read an approved body and put its digest through the gate once.
///
/// `Ok(Some(doc))` means the document is servable. `Ok(None)` means a cached
/// refusal. `Err(reason)` means this call read the refusal.
fn template_instances(
    cache_key: &str,
    digest: &str,
    body: &str,
    state: &mut RefillState,
    kp_key: &str,
    known: Option<(&KnowledgePoint, AnswerKindOf)>,
) -> Result<Option<TemplateDoc>, String> {
    if state.is_refused(cache_key) {
        return Ok(None);
    }

    let doc = match from_body(body) {
        Ok(doc) => doc,
        Err(err) => {
            let reason = format!("the body did not read: {err}");
            state.refuse(cache_key, &reason);
            return Err(reason);
        }
    };

    if state.is_accepted(cache_key) {
        return Ok(Some(doc));
    }

    // The gate needs the topic's answer kind and the knowledge point's exemplars.
    // A serving key the loaded curriculum does not name has neither, so the
    // document runs on its C6 approval alone.
    let Some((kp, answer_kind)) = known else {
        tracing::info!(
            kp_id = %kp_key,
            digest = %digest,
            "refill: the curriculum does not name this knowledge point; the gate did not run again"
        );
        state.accept(cache_key);
        return Ok(Some(doc));
    };

    let base = GateSpec::new(answer_kind.0, &kp.exemplars);
    let spec = match kp.finite_objective_domain.as_ref() {
        Some(policy) => base.with_finite(kp_key, policy)?,
        None => base,
    };
    match gate(&doc, &spec) {
        Ok(_) => {
            state.accept(cache_key);
            Ok(Some(doc))
        }
        Err(rejection) => {
            let reason = format!("[{}] {}", rejection.code, rejection.message);
            state.refuse(cache_key, &reason);
            Err(reason)
        }
    }
}

/// Write one batch into the pool.
async fn insert(
    db: &Db,
    target: &PoolTarget,
    instances: &[cadus_core::template::Instance],
    source: Source,
    digest: Option<&str>,
    seed: u64,
    generation_context: Option<&GenerationContext>,
) -> Result<u64, WorkerError> {
    if instances.is_empty() {
        return Ok(0);
    }
    let rows: Vec<NewInstance> = instances
        .iter()
        .map(|instance| {
            let row = new_instance(instance, source, digest, seed);
            generation_context.map_or(row.clone(), |context| row.with_generation_context(context))
        })
        .collect();
    let inserted =
        cadus_store::pool::insert_batch_for_user(db.pool(), target.user_id, &target.kp_id, &rows)
            .await?;
    Ok(inserted)
}

#[cfg(test)]
mod tests {
    use super::insert;
    use cadus_core::curriculum::Exemplar;
    use cadus_core::pool::{ExemplarSource, ProblemSource, Source};
    use cadus_store::Db;
    use cadus_store::pool::PoolTarget;
    use cadus_store::test_support::TestDb;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::types::Uuid;

    /// The serving key of the pair under test.
    const KP: &str = "perfect-squares/kp1";

    /// The pair of this learner, with no unclaimed row.
    fn target(user_id: Uuid) -> PoolTarget {
        PoolTarget {
            user_id,
            kp_id: KP.to_owned(),
            depth: 0,
        }
    }

    /// One batch of one exemplar instance.
    fn one_instance() -> cadus_core::pool::Batch {
        let exemplars = [Exemplar {
            answer_contract: None,
            problem: "Compute $7^2$.".to_owned(),
            answer: "49".to_owned(),
            solution_sketch: None,
        }];
        ExemplarSource::new(KP, &exemplars)
            .fill(KP, 1, 0)
            .expect("the exemplar fills")
    }

    /// A `Db` over a lazy pool that points at a closed port.
    fn closed_port() -> Db {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgresql://nobody@127.0.0.1:1/nodb")
            .expect("a lazy pool needs no server");
        Db::new(pool, 100)
    }

    /// An empty batch writes no row and opens no connection.
    ///
    /// The pool below is lazy and points at a closed port, so a write that
    /// reached the database would fail here.
    #[tokio::test]
    async fn an_empty_batch_writes_nothing() {
        let inserted = insert(
            &closed_port(),
            &target(Uuid::nil()),
            &[],
            Source::Exemplar,
            None,
            0,
            None,
        )
        .await
        .expect("an empty batch is not an error");
        assert_eq!(inserted, 0);
    }

    /// A batch with one instance writes one row, and the write fails when the
    /// database does not answer.
    #[tokio::test]
    async fn a_batch_writes_its_rows_or_fails_with_the_store() {
        let batch = one_instance();
        let err = insert(
            &closed_port(),
            &target(Uuid::nil()),
            batch.instances(),
            Source::Exemplar,
            None,
            0,
            None,
        )
        .await
        .expect_err("a closed port answers no write");
        assert!(
            err.to_string().starts_with("store error: "),
            "the error names the store: {err}"
        );

        TestDb::with(|db| async move {
            let user = db.seed_user("insert@example.test").await;
            let db_handle = Db::new(db.admin.clone(), 100);
            let inserted = insert(
                &db_handle,
                &target(user),
                one_instance().instances(),
                Source::Exemplar,
                None,
                0,
                None,
            )
            .await
            .expect("the batch writes");
            assert_eq!(inserted, 1);
        })
        .await;
    }
}
