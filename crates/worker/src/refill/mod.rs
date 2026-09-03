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
//! # A pair whose source runs dry leaves it too
//!
//! A pair can also have a source that fills and inserts NOTHING. An exemplar
//! list of 3 under a target depth of 24 is the common case: `ExemplarSource`
//! returns the same 3 statements on every call, the pool already holds them, and
//! `ON CONFLICT DO NOTHING` writes 0 rows. A template whose whole space is in the
//! pool does the same. Such a pair stays under the target depth forever, sorts
//! first on every tick, and takes a slot the whole deployment needs, which is the
//! starvation above under a different name (M4 review 2, finding #8).
//!
//! The pass therefore counts the CONSECUTIVE fills of a pair that inserted 0
//! rows. At [`EMPTY_FILLS_BEFORE_BACKOFF`] the pair leaves the target list for
//! [`EXHAUSTED_BACKOFF`] and [`RefillState::exhausted_kps`] names its knowledge
//! point, which `cadus_store::pool::operator_flags_with_exhausted` shows as
//! `source_exhausted`. One fill that inserts a row clears both.
//!
//! # A digest that lost its approval is retired first
//!
//! Every pass starts with `cadus_store::pool::retire_unapproved`. An operator who
//! revokes an approval (C6) stops the next fill from using the digest, and the
//! rows the digest already wrote stay unclaimed and keep being served with the
//! answer the operator rejected (M4 review 2, finding #4). The retire claims
//! those rows and writes the pair, the digest, and the new status into the log.
//! It runs BEFORE the target query, so the pair falls under its target depth in
//! the same pass and refills from the source that IS approved.
//!
//! # No model call
//!
//! Nothing here calls a model (T1). The refill draws, renders, and evaluates in
//! process, exactly as the gate does.

mod state;
mod target;

use std::time::{Duration, Instant};

use cadus_core::curriculum::{Curriculum, KnowledgePoint};
use cadus_core::pool::{Source, split_kp_key};
use cadus_store::Db;
use sqlx::types::Uuid;

pub use state::RefillState;

use crate::WorkerError;
use target::{Filled, refill_target};

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

/// How long a pair whose source ran dry stays out of the target list.
///
/// The pair HAS a source and the source works; it simply produces no statement
/// the pool does not already hold. The cure is authored content, not a retry, so
/// the period is four times [`REFILL_BACKOFF`]. Sixty minutes is the M4 review 2
/// ruling on finding #8.
pub const EXHAUSTED_BACKOFF: Duration = Duration::from_secs(60 * 60);

/// The count of consecutive zero-insert fills that exhausts a pair.
///
/// One zero-insert fill is normal: two ticks inside one second draw the same
/// tuples from a template with a small space. Two in a row say the source has no
/// more to give (M4 review 2, finding #8).
pub const EMPTY_FILLS_BEFORE_BACKOFF: u32 = 2;

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
        let (topic_id, kp_id) = split_kp_key(kp_key)?;
        let topic = self
            .curriculum
            .topics()
            .iter()
            .find(|topic| topic.id.as_str() == topic_id)?;
        let kp = topic
            .knowledge_points
            .iter()
            .find(|kp| kp.id.as_str() == kp_id)?;
        Some((kp, AnswerKindOf(topic.answer_kind)))
    }
}

/// The answer kind of the topic a knowledge point belongs to.
///
/// The wrapper keeps the refill from carrying a `Topic` reference it does not
/// otherwise need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnswerKindOf(pub cadus_core::curriculum::AnswerKind);

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
    /// The count of pairs this pass put on the [`EXHAUSTED_BACKOFF`] period.
    ///
    /// The pair filled and inserted 0 rows [`EMPTY_FILLS_BEFORE_BACKOFF`] times
    /// in a row, so its source has no statement the pool does not hold
    /// (finding #8). `operator_flags` shows the knowledge point as
    /// `source_exhausted`.
    pub exhausted: usize,
    /// The count of rows this pass retired because their digest lost approval.
    ///
    /// Each one is a row an operator's revocation (C6) took out of the pool
    /// (finding #4). A count above 0 is news: it says a served answer was wrong.
    pub retired_unapproved: u64,
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
    // Step 0: retire every unclaimed row whose digest lost its approval (C6,
    // finding #4). This runs BEFORE the target query, so a pair the retire
    // emptied falls under its target depth in this same pass and refills from
    // the source that IS approved.
    let retired = cadus_store::pool::retire_unapproved(db.pool()).await?;
    for row in &retired {
        tracing::warn!(
            row_id = %row.id,
            user_id = %row.user_id,
            kp_id = %row.kp_id,
            digest = %row.content_digest,
            status = %row.status,
            "refill: the digest of this pool row is no longer approved; the row is claimed and \
             leaves the pool, and the pair refills from an approved source (C6)"
        );
    }

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
        retired_unapproved: u64::try_from(retired.len()).unwrap_or(u64::MAX),
        ..RefillReport::default()
    };
    for target in &targets {
        match refill_target(db, job, state, target, nonce).await {
            Ok(Filled::NoSource) => {
                report.without_source = report.without_source.saturating_add(1);
                state.starve(target.user_id, &target.kp_id, now, REFILL_BACKOFF);
                let backoff_secs = REFILL_BACKOFF.as_secs();
                tracing::warn!(
                    user_id = %target.user_id,
                    kp_id = %target.kp_id,
                    backoff_secs,
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
                if note_fill(state, job, target, source, inserted, now) {
                    report.exhausted = report.exhausted.saturating_add(1);
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

/// Record one fill in the empty-fill count of its pair, and say whether the
/// pair is exhausted now (finding #8).
///
/// A fill that wrote no row is the second starvation shape: the source works,
/// and it has nothing the pool does not hold. Two in a row take the pair off the
/// list for an hour, so the budget reaches the pairs that CAN grow.
fn note_fill(
    state: &mut RefillState,
    job: &RefillJob<'_>,
    target: &cadus_store::pool::PoolTarget,
    source: Source,
    inserted: u64,
    now: Instant,
) -> bool {
    if inserted > 0 {
        state.note_filled(target.user_id, &target.kp_id);
        return false;
    }
    if state.note_empty_fill(target.user_id, &target.kp_id) < EMPTY_FILLS_BEFORE_BACKOFF {
        return false;
    }
    state.starve(target.user_id, &target.kp_id, now, EXHAUSTED_BACKOFF);
    state.exhaust(target.user_id, &target.kp_id);
    let source = source.as_str();
    let backoff_secs = EXHAUSTED_BACKOFF.as_secs();
    tracing::warn!(
        user_id = %target.user_id,
        kp_id = %target.kp_id,
        source,
        depth = target.depth,
        target_depth = job.cfg.target_depth,
        empty_fills = EMPTY_FILLS_BEFORE_BACKOFF,
        backoff_secs,
        "refill: the source of this pair produced no new statement twice in a \
         row; the pair leaves the target list and operator_flags names it \
         source_exhausted (A6, D-O4)"
    );
    true
}
