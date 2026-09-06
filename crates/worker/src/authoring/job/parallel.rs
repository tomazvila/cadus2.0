//! Bounded parallel knowledge points with a barrier between content kinds.

use super::{
    AuthoringJob, BatchReport, Report, author_one,
    pass::{count, stale_first},
};
use crate::{
    WorkerError,
    authoring::prompt::{AuthoringSpec, Kind},
};
use cadus_store::Db;
use std::{
    future::{Future, poll_fn},
    task::Poll,
};

/// Author one kind with at most `concurrency` active knowledge points.
/// Every started task drains before an error returns, so its paid ledger survives.
/// Duplicate keys are rejected before any model call.
///
/// # Errors
/// Reject concurrency outside 1..=64, duplicate keys, and database failures.
pub async fn run_parallel(
    db: &Db,
    job: &AuthoringJob,
    kind: Kind,
    specs: &[AuthoringSpec],
    concurrency: usize,
) -> Result<BatchReport, WorkerError> {
    if !(1..=64).contains(&concurrency) {
        return Err(WorkerError::Config(
            "author concurrency must be in 1..=64".to_owned(),
        ));
    }
    let mut keys = std::collections::HashSet::new();
    for spec in specs {
        if !keys.insert((&spec.topic_id, &spec.kp_id)) {
            return Err(WorkerError::Config(
                "duplicate knowledge point in author pass".to_owned(),
            ));
        }
    }
    let ordered = stale_first(db, kind, specs).await?;
    let mut report = BatchReport::default();
    for chunk in ordered.chunks(concurrency) {
        let futures = chunk.iter().map(|spec| author_one(db, job, kind, spec));
        for result in wave(futures).await {
            count(&mut report, result?);
        }
    }
    Ok(report)
}

async fn wave<F: Future<Output = Result<Report, WorkerError>>>(
    futures: impl Iterator<Item = F>,
) -> Vec<Result<Report, WorkerError>> {
    let mut pending: Vec<_> = futures.map(|future| Some(Box::pin(future))).collect();
    let mut results = Vec::new();
    poll_fn(|cx| {
        for slot in &mut pending {
            if let Some(future) = slot
                && let Poll::Ready(result) = future.as_mut().poll(cx)
            {
                results.push(result);
                *slot = None;
            }
        }
        if pending.iter().all(Option::is_none) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
    results
}
