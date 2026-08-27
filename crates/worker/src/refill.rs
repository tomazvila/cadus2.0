//! The pool refill job (D-O4, A6, A7).
//!
//! # Why the refill is here and not on the serve path
//!
//! D-O4 puts the redraw in the worker. The serve pops at most
//! [`POP_CANDIDATES`](cadus_core::pool::POP_CANDIDATES) rows and serves one; it
//! never draws, because a draw that misses the anti-repeat ring would cost the
//! learner latency inside the L1 budget. Here a miss costs nothing.
//!
//! # What one refill does
//!
//! ```text
//!   refill_targets(target_depth)          -- every (user, kp) below the depth
//!            │
//!            ▼   for each target
//!   approved_template(kp)  --found-->  TemplateSource  (source = 'template')
//!            │ none
//!            ▼
//!   curriculum exemplars   --found-->  ExemplarSource  (source = 'exemplar', A6)
//!            │
//!            ▼
//!   ProblemSource::fill(kp, need, seed) ──▶ insert_batch (ON CONFLICT DO NOTHING)
//! ```
//!
//! # The recorded seed
//!
//! [`batch_seed`] derives the seed of one batch from the base seed of the
//! deployment, the learner, the serving key, and the nonce of the tick. The seed
//! goes into `serving_pool.problem`, so a reviewer reproduces the whole batch
//! from the row.
//!
//! The nonce is the UTC microsecond clock
//! ([`batch_nonce`](crate::batch_nonce)), so a second refill of one pair draws a
//! different batch and the pool grows past the tuples it already holds. A
//! process-local tick counter did that inside one process and failed across
//! processes: a restart began the count at zero, redrew the batches the pool
//! already held, and inserted no row for the whole window the old process had
//! walked (finding #10).
//!
//! # The gate runs again, and every instance runs again
//!
//! 1.0 re-runs its whole check set at serve time (`problem_templates.py:427-467`),
//! and the comment at `:432-437` records why: the exemplar envelope once ran in
//! the authoring gate only, and a 10,000-instance subtraction template was
//! accepted on 22 of 60 seeds with 55 negative instances. 2.0 keeps that property
//! and moves the cost off the request path. Two checks run, and both are needed:
//!
//! - The DOCUMENT goes through [`gate`] once per digest, and the verdict is
//!   cached in [`RefillState`]. The gate is a pure function of the document, so
//!   one verdict per content address is exact (C6).
//! - Every INSTANCE goes through the per-instance rules again inside
//!   [`ProblemSource::fill`]. The document verdict says nothing about the tuples
//!   the batch draws: above the exhaustive limit the gate reads a sample from a
//!   constant seed, and the fill draws from the batch seed. A refused instance is
//!   skipped and counted, and a pair whose refusal rate is above
//!   [`REFUSAL_FLAG_PERCENT`] is logged and flagged (C4).
//!
//! A knowledge point the loaded curriculum does not name has no exemplars, so its
//! envelope rule stays silent. Such a document is filled on its C6 approval alone,
//! and the fact is logged.
//!
//! # A pair that cannot fill leaves the target list
//!
//! A `(user, kp)` pair with no approved template and no decidable exemplar can
//! never gain a row, and it sits at depth 0 forever. The target query orders by
//! ascending depth, so such a pair took the head of every tick and the budget
//! never reached a pair that CAN fill. The pass therefore holds a backoff map
//! keyed by the pair: a starved pair leaves the target list for
//! [`REFILL_BACKOFF`], and `operator_flags` shows it with `needs_template`.
//!
//! # No model call
//!
//! Nothing here calls a model (T1). The refill draws, renders, and evaluates in
//! process, exactly as the gate does.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use cadus_core::curriculum::{Curriculum, Exemplar, KnowledgePoint};
use cadus_core::pool::{
    Batch, ExemplarSource, FillError, PoolAnswer, PoolProblem, ProblemSource, REFUSAL_FLAG_PERCENT,
    Source, TemplateSource, split_kp_key,
};
use cadus_core::template::{GateSpec, TemplateDoc, from_body, gate};
use cadus_store::Db;
use cadus_store::pool::{NewInstance, PoolTarget};
use sqlx::types::Uuid;

use crate::WorkerError;

/// The unclaimed depth one `(user, kp)` pair keeps.
///
/// The serve pops at most 8 rows and the D5 ring holds 20 digests, so a depth of
/// 24 keeps three full pops of unblocked candidates ahead of the learner.
pub const DEFAULT_TARGET_DEPTH: i64 = 24;

/// The count of pairs one refill call works on.
///
/// The bound keeps one tick short, so the loop stays open to SIGTERM and the
/// refill of a large deployment spreads over ticks instead of blocking one.
pub const DEFAULT_TARGETS_PER_TICK: i64 = 32;

/// The base seed of a deployment that sets none.
pub const DEFAULT_BASE_SEED: u64 = 0;

/// How long a pair with no fillable source stays out of the target list.
///
/// The pair is retried after this period, because the operator can approve a
/// template or author an exemplar at any time. Fifteen minutes is the M4 review
/// ruling on finding #3.
pub const REFILL_BACKOFF: Duration = Duration::from_secs(15 * 60);

/// The configuration of the refill job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefillConfig {
    /// The unclaimed depth a `(user, kp)` pair keeps.
    pub target_depth: i64,
    /// The count of pairs one call works on.
    pub targets_per_tick: i64,
    /// The base seed of the deployment.
    pub base_seed: u64,
}

impl Default for RefillConfig {
    fn default() -> Self {
        Self {
            target_depth: DEFAULT_TARGET_DEPTH,
            targets_per_tick: DEFAULT_TARGETS_PER_TICK,
            base_seed: DEFAULT_BASE_SEED,
        }
    }
}

/// The refill job: its configuration and the curriculum it reads exemplars from.
///
/// The curriculum is required. The worker refuses to start without one (the M4
/// review ruling on findings #5 and #6), so the A6 exemplar fallback and the
/// gate's knowledge-point half are always available here.
#[derive(Debug, Clone, Copy)]
pub struct RefillJob<'arena> {
    /// The configuration.
    pub cfg: RefillConfig,
    /// The loaded curriculum, for the A6 exemplar fallback and for the gate.
    pub curriculum: &'arena Curriculum,
}

impl<'arena> RefillJob<'arena> {
    /// Build a job over a curriculum with the default configuration.
    #[must_use]
    pub fn new(curriculum: &'arena Curriculum) -> Self {
        Self {
            cfg: RefillConfig::default(),
            curriculum,
        }
    }

    /// Replace the configuration.
    #[must_use]
    pub const fn with_config(mut self, cfg: RefillConfig) -> Self {
        self.cfg = cfg;
        self
    }

    /// The knowledge point of one serving key, when the curriculum names it.
    #[must_use]
    pub fn knowledge_point(&self, kp_key: &str) -> Option<(&'arena KnowledgePoint, AnswerKindOf)> {
        let curriculum = self.curriculum;
        let (topic_id, kp_id) = split_kp_key(kp_key)?;
        let topic_idx = curriculum.idx_of(topic_id)?;
        let topic = curriculum.topic(topic_idx)?;
        let kp_idx = curriculum.kp_idx_of(topic_idx, kp_id)?;
        let kp = curriculum.knowledge_point(topic_idx, kp_idx)?;
        Some((kp, AnswerKindOf(topic.answer_kind)))
    }
}

/// The answer kind of the topic a knowledge point belongs to.
///
/// The wrapper keeps the refill from carrying a `Topic` reference it does not
/// otherwise need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnswerKindOf(pub cadus_core::curriculum::AnswerKind);

/// The gate verdicts one worker process remembers.
///
/// The gate walks up to
/// [`GATE_SAMPLES`](cadus_core::template::GATE_SAMPLES) instances, so it is far
/// too heavy to run once per refill. The verdict belongs to the document, and the
/// document is content-addressed (C6), so one verdict per digest is exact.
#[derive(Debug, Default)]
pub struct RefillState {
    /// Digests the gate accepted.
    accepted: HashSet<String>,
    /// Digests the gate refused, with the reason it wrote.
    refused: HashMap<String, String>,
    /// Pairs with no fillable source, and the instant each one is tried again.
    starved: HashMap<(Uuid, String), Instant>,
}

impl RefillState {
    /// Build an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The count of digests the cache holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.accepted.len().saturating_add(self.refused.len())
    }

    /// Whether the cache holds no verdict.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.accepted.is_empty() && self.refused.is_empty()
    }

    /// The refusal reason of a digest the gate rejected.
    #[must_use]
    pub fn refusal(&self, digest: &str) -> Option<&str> {
        self.refused.get(digest).map(String::as_str)
    }

    /// The count of pairs the backoff map holds.
    #[must_use]
    pub fn starved_len(&self) -> usize {
        self.starved.len()
    }

    /// Whether one pair is out of the target list at `now`.
    #[must_use]
    pub fn is_starved(&self, user_id: Uuid, kp_id: &str, now: Instant) -> bool {
        self.starved
            .get(&(user_id, kp_id.to_string()))
            .is_some_and(|until| *until > now)
    }

    /// Put one pair out of the target list for [`REFILL_BACKOFF`].
    fn starve(&mut self, user_id: Uuid, kp_id: &str, now: Instant) {
        let until = now.checked_add(REFILL_BACKOFF).unwrap_or(now);
        self.starved.insert((user_id, kp_id.to_string()), until);
    }

    /// Drop every backoff entry whose period ended, and list the ones that hold.
    fn active_starved(&mut self, now: Instant) -> Vec<(Uuid, String)> {
        self.starved.retain(|_, until| *until > now);
        let mut pairs: Vec<(Uuid, String)> = self.starved.keys().cloned().collect();
        pairs.sort();
        pairs
    }
}

/// What one refill call did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RefillReport {
    /// The count of pairs the call looked at.
    pub targets: usize,
    /// The count of rows the call inserted.
    pub inserted: u64,
    /// The count of rows that came from an approved template (A1).
    pub from_template: u64,
    /// The count of rows that came from an exemplar (A6).
    pub from_exemplar: u64,
    /// The count of pairs with no approved template and no exemplar.
    ///
    /// Each one leaves the target list for [`REFILL_BACKOFF`].
    pub without_source: usize,
    /// The count of pairs whose fill or insert failed.
    pub failed: usize,
    /// The count of instances the per-instance re-check refused (C4).
    pub refused_instances: u64,
    /// The count of pairs whose refusal rate is above [`REFUSAL_FLAG_PERCENT`].
    pub flagged_refusals: usize,
    /// The count of pairs the backoff map held out of this pass.
    pub skipped_starved: usize,
}

/// What one target gave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filled {
    /// The pair reached its depth already.
    Full,
    /// The pair took rows from the named source.
    Rows {
        source: Source,
        inserted: u64,
        refused: u64,
        flagged: bool,
    },
    /// The pair has no approved template and no exemplar.
    NoSource,
}

/// The 64-bit mixing step of SplitMix64.
///
/// The function is the published SplitMix64 finalizer. It is written out here,
/// and not taken from a crate, because the worker takes no new dependency for
/// four lines of arithmetic (R4).
const fn splitmix64(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The FNV-1a offset basis.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

/// The FNV-1a prime.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Fold bytes into an FNV-1a digest.
fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// The seed of one refill batch.
///
/// The value is `splitmix64(base ^ fnv1a(user_id bytes ++ kp_id bytes) ^ nonce *
/// 0x9E3779B97F4A7C15)`. Three properties matter:
///
/// - It is a pure function. The same four inputs give the same batch forever, so
///   a reviewer reproduces a served instance from the recorded seed.
/// - It reads no clock and no entropy source, so a test drives it with a literal
///   nonce.
/// - A change of any input changes the seed, so two learners of one knowledge
///   point get different batches and two ticks of one learner get different
///   batches.
#[must_use]
pub fn batch_seed(base: u64, user_id: Uuid, kp_id: &str, nonce: u64) -> u64 {
    let mut hash = fnv1a(FNV_OFFSET, user_id.as_bytes());
    hash = fnv1a(hash, kp_id.as_bytes());
    splitmix64(base ^ hash ^ nonce.wrapping_mul(0x9E37_79B9_7F4A_7C15))
}

/// Run one refill pass over every pair below the target depth (D-O4).
///
/// A failure of one pair does not stop the pass: the report counts it and the log
/// names it. Only a failure of the target query itself returns an error, because
/// that failure says the database is gone.
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when the target query fails.
pub async fn refill_once(
    db: &Db,
    job: &RefillJob<'_>,
    state: &mut RefillState,
    nonce: u64,
) -> Result<RefillReport, WorkerError> {
    refill_once_at(db, job, state, nonce, Instant::now()).await
}

/// [`refill_once`] at a given instant.
///
/// The instant drives the backoff map alone. A test passes a literal instant and
/// gets a pass that reads no clock.
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when the target query fails.
pub async fn refill_once_at(
    db: &Db,
    job: &RefillJob<'_>,
    state: &mut RefillState,
    nonce: u64,
    now: Instant,
) -> Result<RefillReport, WorkerError> {
    // A pair with no fillable source leaves the target list. The database
    // excludes it, so the per-tick budget goes to pairs that can fill.
    let starved = state.active_starved(now);
    let targets = cadus_store::pool::refill_targets_skipping(
        db.pool(),
        job.cfg.target_depth,
        job.cfg.targets_per_tick,
        &starved,
    )
    .await?;

    let mut report = RefillReport {
        targets: targets.len(),
        skipped_starved: starved.len(),
        ..RefillReport::default()
    };

    for target in &targets {
        match refill_target(db, job, state, target, nonce).await {
            Ok(Filled::Full) => {}
            Ok(Filled::NoSource) => {
                report.without_source = report.without_source.saturating_add(1);
                state.starve(target.user_id, &target.kp_id, now);
                tracing::warn!(
                    user_id = %target.user_id,
                    kp_id = %target.kp_id,
                    backoff_secs = REFILL_BACKOFF.as_secs(),
                    "refill: the knowledge point has no approved template and no exemplar; \
                     the pair leaves the target list and operator_flags names it (A6)"
                );
            }
            Ok(Filled::Rows {
                source,
                inserted,
                refused,
                flagged,
            }) => {
                report.inserted = report.inserted.saturating_add(inserted);
                report.refused_instances = report.refused_instances.saturating_add(refused);
                if flagged {
                    report.flagged_refusals = report.flagged_refusals.saturating_add(1);
                }
                match source {
                    Source::Exemplar => {
                        report.from_exemplar = report.from_exemplar.saturating_add(inserted);
                    }
                    Source::Template | Source::Generator => {
                        report.from_template = report.from_template.saturating_add(inserted);
                    }
                }
            }
            Err(err) => {
                report.failed = report.failed.saturating_add(1);
                tracing::warn!(
                    kp_id = %target.kp_id,
                    error = %err,
                    "refill: the knowledge point did not fill"
                );
            }
        }
    }

    Ok(report)
}

/// Log every refusal of one batch, and say whether the pair is flagged (C4).
///
/// A refusal means the gate accepted a document one of whose instances breaks a
/// per-instance rule. That is a content defect, so every one of them reaches the
/// log with the rule that refused it.
fn report_refusals(kp_id: &str, digest: Option<&str>, batch: &Batch) -> bool {
    for refused in batch.refusals() {
        tracing::warn!(
            kp_id = %kp_id,
            digest = digest.unwrap_or(""),
            code = %refused.code,
            reason = %refused.message,
            "refill: the per-instance check refused an instance; it is not in the pool (C4)"
        );
    }
    let flagged = batch.is_flagged();
    if flagged {
        tracing::warn!(
            kp_id = %kp_id,
            digest = digest.unwrap_or(""),
            refused = batch.refusals().len(),
            checked = batch.checked(),
            percent = batch.refusal_percent(),
            limit = REFUSAL_FLAG_PERCENT,
            "refill: the refusal rate of this knowledge point is above the limit (C4, C6)"
        );
    }
    flagged
}

/// Fill one `(user, kp)` pair up to the target depth.
async fn refill_target(
    db: &Db,
    job: &RefillJob<'_>,
    state: &mut RefillState,
    target: &PoolTarget,
    nonce: u64,
) -> Result<Filled, WorkerError> {
    let want = job.cfg.target_depth.saturating_sub(target.depth);
    let Ok(need) = usize::try_from(want) else {
        return Ok(Filled::Full);
    };
    if need == 0 {
        return Ok(Filled::Full);
    }

    let seed = batch_seed(job.cfg.base_seed, target.user_id, &target.kp_id, nonce);
    let known = job.knowledge_point(&target.kp_id);

    // Step 1: an approved template (A1). C6 binds the approval to the digest, so
    // `approved_template` never returns a pending body.
    if let Some(approved) = cadus_store::pool::approved_template(db.pool(), &target.kp_id).await? {
        match template_instances(
            &approved.digest,
            &approved.body,
            state,
            &target.kp_id,
            known,
        ) {
            Ok(Some(doc)) => {
                // The source carries the authored exemplars, so the per-instance
                // re-check applies the A6 envelope to every instance it draws
                // (C4). A knowledge point the curriculum does not name has none,
                // and the envelope rule then stays silent.
                let exemplars: &[Exemplar] = known.map_or(&[], |(kp, _)| kp.exemplars.as_slice());
                let source = TemplateSource::new(target.kp_id.clone(), &doc)
                    .map_err(|err| WorkerError::Refill(err.to_string()))?
                    .with_digest(approved.digest.clone())
                    .with_exemplars(exemplars);
                let batch = source
                    .fill(&target.kp_id, need, seed)
                    .map_err(|err: FillError| WorkerError::Refill(err.to_string()))?;
                let flagged = report_refusals(&target.kp_id, Some(&approved.digest), &batch);
                let refused = u64::try_from(batch.refusals().len()).unwrap_or(u64::MAX);
                let inserted = insert(
                    db,
                    target,
                    batch.instances(),
                    Source::Template,
                    Some(&approved.digest),
                    seed,
                )
                .await?;
                return Ok(Filled::Rows {
                    source: Source::Template,
                    inserted,
                    refused,
                    flagged,
                });
            }
            // The gate refused the approved document, or the body did not read.
            // The knowledge point falls back to its exemplars (A6) instead of
            // going off the air.
            Ok(None) => {}
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

    // Step 2: the A6 exemplar fallback. No model call, no synchronous
    // generation: the rotation is the authored exemplar list.
    let Some((kp, _)) = known else {
        return Ok(Filled::NoSource);
    };
    let exemplars: &[Exemplar] = &kp.exemplars;
    if exemplars.is_empty() {
        return Ok(Filled::NoSource);
    }
    let source = ExemplarSource::new(target.kp_id.clone(), exemplars);
    if !source.covers_ring() {
        tracing::info!(
            kp_id = %target.kp_id,
            exemplars = source.len(),
            "refill: the exemplar count is under the anti-repeat ring (A6)"
        );
    }
    let batch = match source.fill(&target.kp_id, need, seed) {
        Ok(batch) => batch,
        Err(FillError::NoExemplar) => return Ok(Filled::NoSource),
        Err(err) => return Err(WorkerError::Refill(err.to_string())),
    };
    let flagged = report_refusals(&target.kp_id, None, &batch);
    let refused = u64::try_from(batch.refusals().len()).unwrap_or(u64::MAX);
    let inserted = insert(db, target, batch.instances(), Source::Exemplar, None, seed).await?;
    Ok(Filled::Rows {
        source: Source::Exemplar,
        inserted,
        refused,
        flagged,
    })
}

/// Read an approved body and put its digest through the gate once.
///
/// `Ok(Some(doc))` means the document is servable. `Ok(None)` means a cached
/// refusal. `Err(reason)` means this call read the refusal.
fn template_instances(
    digest: &str,
    body: &str,
    state: &mut RefillState,
    kp_key: &str,
    known: Option<(&KnowledgePoint, AnswerKindOf)>,
) -> Result<Option<TemplateDoc>, String> {
    if state.refused.contains_key(digest) {
        return Ok(None);
    }

    let doc = match from_body(body) {
        Ok(doc) => doc,
        Err(err) => {
            let reason = format!("the body did not read: {err}");
            state.refused.insert(digest.to_string(), reason.clone());
            return Err(reason);
        }
    };

    if state.accepted.contains(digest) {
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
        state.accepted.insert(digest.to_string());
        return Ok(Some(doc));
    };

    let spec = GateSpec {
        answer_kind: answer_kind.0,
        exemplars: &kp.exemplars,
    };
    match gate(&doc, &spec) {
        Ok(_) => {
            state.accepted.insert(digest.to_string());
            Ok(Some(doc))
        }
        Err(rejection) => {
            let reason = format!("[{}] {}", rejection.code, rejection.message);
            state.refused.insert(digest.to_string(), reason.clone());
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
) -> Result<u64, WorkerError> {
    if instances.is_empty() {
        return Ok(0);
    }
    let rows: Vec<NewInstance> = instances
        .iter()
        .map(|instance| NewInstance {
            source,
            content_digest: digest.map(str::to_string),
            problem: PoolProblem::from_instance(instance, seed),
            expected_answer: PoolAnswer::from_instance(instance),
            instance_hash: instance.instance_hash.clone(),
        })
        .collect();
    let inserted =
        cadus_store::pool::insert_batch_for_user(db.pool(), target.user_id, &target.kp_id, &rows)
            .await?;
    Ok(inserted)
}
