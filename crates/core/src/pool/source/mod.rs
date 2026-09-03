//! The pool sources: the A7 seam, the template source, and the exemplar source.
//!
//! # Why a trait
//!
//! A7 keeps the door open for a future generator without a redesign. It states
//! the seam: "Problems reach the learner only through a serving pool, and the
//! pool is source-agnostic. Sources at launch: template instantiation (A1) and
//! exemplar rotation (A6). A future LLM generator is a third source behind the
//! same trait — a new module, not a redesign."
//!
//! [`ProblemSource`] is that trait. It has exactly two implementations here:
//! [`TemplateSource`] and [`ExemplarSource`]. [`super::Source::Generator`] is the
//! third wire value of the `serving_pool.source` column
//! (`migrations/0005_content.sql`); no code writes it in M4, and a later
//! milestone adds the implementation behind this same trait.
//!
//! # What a source does, and what it never does
//!
//! A source fills. It takes a knowledge point, a count, and a recorded seed, and
//! it returns instances. It never reads a clock, never opens a socket, and never
//! calls a model (T1, R3). Every source runs in the worker refill job (D-O4), off
//! the request path, so a miss inside it costs no learner latency (L1).
//!
//! # The batch is distinct by instance hash
//!
//! `serving_pool` carries `UNIQUE (user_id, kp_id, instance_hash)`, so a batch
//! with two equal digests loses a row to a conflict. Both sources therefore drop
//! a repeated digest inside the batch before they return it. The count a source
//! returns is at most `n`, and it is less than `n` when the source ran out of
//! distinct instances.
//!
//! # Every instance is checked again, one by one
//!
//! [`TemplateSource::fill`] runs the per-instance rules of the gate
//! ([`check_instance`](crate::template::check_instance)) on EVERY candidate before
//! the candidate joins the batch. The gate samples a large space from a constant
//! seed and the fill draws from the batch seed, so the two sets differ and an
//! unchecked corner would otherwise reach a learner (C4). A refused candidate is
//! skipped and counted: [`Batch::refusals`] names every one of them, and the
//! refill job flags a knowledge point whose refusal rate is above
//! [`REFUSAL_FLAG_PERCENT`].

mod exemplar;
mod template;

pub use exemplar::{ExemplarRefusal, ExemplarSource};
pub use template::TemplateSource;

use crate::template::{Bindings, Instance, InstantiateError};

use super::Source;

/// The count of candidate streams one [`TemplateSource::fill`] call walks.
///
/// Above [`EXHAUSTIVE_SPACE_LIMIT`] declared tuples a candidate stream holds
/// [`RESAMPLE_ATTEMPTS`](crate::template::RESAMPLE_ATTEMPTS) independent draws,
/// so the whole fill draws at most
/// `FILL_ROUNDS * RESAMPLE_ATTEMPTS` tuples: 192. The bound turns a template
/// whose distinct instances run out into a short batch, and not into a loop.
///
/// At or under the limit one stream already holds every satisfying tuple, so the
/// fill stops after the first round.
pub const FILL_ROUNDS: usize = 8;

/// The refusal rate that flags a knowledge point, in whole percent.
///
/// A refusal is a gate defect: the document passed the gate, so every instance of
/// it must pass the per-instance rules too. One refusal above the exhaustive
/// limit is a corner the gate's sample missed; many refusals say the document is
/// wrong for its knowledge point. The refill job logs the rate and flags the pair
/// above this number (M4 review round 1, ruling on findings #1, #2, and #15).
pub const REFUSAL_FLAG_PERCENT: u64 = 10;

/// One candidate the fill refused, and the rule that refused it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// The drawn tuple.
    pub bindings: Bindings,
    /// The rendered statement, when the candidate rendered at all.
    pub text: Option<String>,
    /// The short name of the rule that refused it, for example `envelope-sign`.
    pub code: String,
    /// The reason, in the words the gate writes.
    pub message: String,
}

/// What one [`ProblemSource::fill`] call produced.
///
/// The batch carries the instances that passed every rule AND the candidates the
/// per-instance re-check refused. A caller that ignores the refusals still sees
/// only checked instances, because the refused ones are not in [`Batch::instances`].
///
/// The type dereferences to the instance slice, so a caller reads it as a slice
/// of [`Instance`] and the anti-repeat rule takes it directly.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Batch {
    /// The instances that passed every per-instance rule.
    instances: Vec<Instance>,
    /// The candidates the re-check refused, in draw order.
    refusals: Vec<Refused>,
}

impl Batch {
    /// Build a batch from its two halves.
    #[must_use]
    pub const fn new(instances: Vec<Instance>, refusals: Vec<Refused>) -> Self {
        Self {
            instances,
            refusals,
        }
    }

    /// The instances that passed every per-instance rule.
    #[must_use]
    pub fn instances(&self) -> &[Instance] {
        &self.instances
    }

    /// The candidates the per-instance re-check refused.
    #[must_use]
    pub fn refusals(&self) -> &[Refused] {
        &self.refusals
    }

    /// The count of candidates the re-check read.
    ///
    /// The sum of the instances and the refusals. A repeated statement with the
    /// same answer is read once. A repeated statement with a DIFFERENT answer is
    /// a refusal, so it is read again and counted again (M4 review 2, finding 1).
    #[must_use]
    pub fn checked(&self) -> usize {
        self.instances.len().saturating_add(self.refusals.len())
    }

    /// The refusal rate of the batch, in whole percent, rounded down.
    ///
    /// A batch that read no candidate has a rate of 0.
    #[must_use]
    pub fn refusal_percent(&self) -> u64 {
        let checked = self.checked();
        if checked == 0 {
            return 0;
        }
        let refused = u64::try_from(self.refusals.len()).unwrap_or(u64::MAX);
        let checked = u64::try_from(checked).unwrap_or(u64::MAX);
        refused.saturating_mul(100) / checked
    }

    /// Whether the refusal rate is above [`REFUSAL_FLAG_PERCENT`].
    #[must_use]
    pub fn is_flagged(&self) -> bool {
        self.refusal_percent() > REFUSAL_FLAG_PERCENT
    }
}

impl std::ops::Deref for Batch {
    type Target = [Instance];

    fn deref(&self) -> &Self::Target {
        &self.instances
    }
}

/// A fill the source refuses.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FillError {
    /// The caller asked for a knowledge point this source does not fill.
    #[error("this source fills knowledge point {have} and the caller asked for {want}")]
    UnknownKp {
        /// The knowledge point the source fills.
        have: String,
        /// The knowledge point the caller asked for.
        want: String,
    },
    /// The document did not compile, or a constraint did not decide.
    #[error("{0}")]
    Instantiate(#[from] InstantiateError),
    /// No tuple of the declared domains satisfies the constraints.
    ///
    /// The gate of U2 refuses such a document, so a stored template never
    /// reaches this. A hand-written document does.
    #[error("no tuple of the declared domains satisfies the constraints")]
    NoSatisfyingTuple,
    /// Every candidate the source built was refused.
    ///
    /// The message is the 1.0 message of `problem_templates.py:405-409`, quoted
    /// by specification section 5.4, step 5.
    #[error(
        "no instance of this template produced a usable answer — it should not have passed the gate, and it must not be served"
    )]
    NoValidInstance {
        /// The refusal of the last candidate the source tried.
        ///
        /// The text is the message of the instantiator or of the per-instance
        /// re-check, whichever refused that candidate.
        reason: String,
    },
    /// No exemplar of the knowledge point has an answer the checker decides.
    #[error("no exemplar of this knowledge point has an answer the checker can decide")]
    NoExemplar,
}

/// A source of verified problem instances (A7).
///
/// The trait is the whole seam. A pool row records [`ProblemSource::source`] in
/// its `source` column, so the pedagogical effect of each source is measurable
/// before a new one earns more budget (A7).
pub trait ProblemSource {
    /// The wire tag the pool row records.
    fn source(&self) -> Source;

    /// The knowledge point this source fills.
    fn kp_id(&self) -> &str;

    /// The content digest of the approved document, when the source has one.
    ///
    /// A template source names the `content_store` row it instantiates. An
    /// exemplar source has no content row, so it returns `None` and the pool row
    /// keeps a NULL `content_digest`.
    fn content_digest(&self) -> Option<&str> {
        None
    }

    /// Fill at most `n` distinct instances of `kp` from the recorded `seed`.
    ///
    /// [`Batch::instances`] is at most `n` long, every digest inside it is
    /// distinct, and every instance passed the per-instance rules of the gate.
    /// [`Batch::refusals`] names every candidate the re-check refused.
    ///
    /// # Errors
    ///
    /// Returns [`FillError::UnknownKp`] when `kp` is not the knowledge point the
    /// source fills, and the refusal of the source when it built no instance.
    fn fill(&self, kp: &str, n: usize, seed: u64) -> Result<Batch, FillError>;
}

/// Refuse a fill whose knowledge point is not the one the source holds.
fn same_kp(have: &str, want: &str) -> Result<(), FillError> {
    if have == want {
        return Ok(());
    }
    Err(FillError::UnknownKp {
        have: have.to_string(),
        want: want.to_string(),
    })
}
