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
//! ([`check_instance`](super::recheck::check_instance)) on EVERY candidate before
//! the candidate joins the batch. The gate samples a large space from a constant
//! seed and the fill draws from the batch seed, so the two sets differ and an
//! unchecked corner would otherwise reach a learner (C4). A refused candidate is
//! skipped and counted: [`Batch::refusals`] names every one of them, and the
//! refill job flags a knowledge point whose refusal rate is above
//! [`REFUSAL_FLAG_PERCENT`].

use std::collections::BTreeSet;

use crate::answer::{Undecidable, canonical_form};
use crate::curriculum::model::{Exemplar, KnowledgePoint};
use crate::learner::problem_text_hash;
use crate::template::{
    Bindings, Compiled, EXHAUSTIVE_SPACE_LIMIT, Envelope, GateSpec, Instance, InstantiateError,
    TemplateDoc, exemplar_envelope, rng_from_seed,
};

use super::Source;
use super::recheck;
use super::ring::RING_CAPACITY;

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

    /// Take the instances out of the batch.
    #[must_use]
    pub fn into_instances(self) -> Vec<Instance> {
        self.instances
    }

    /// The candidates the per-instance re-check refused.
    #[must_use]
    pub fn refusals(&self) -> &[Refused] {
        &self.refusals
    }

    /// The count of distinct candidates the re-check read.
    ///
    /// The sum of the instances and the refusals. A repeated statement is read
    /// once, so the number never counts one candidate twice.
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

/// The A1 source: draw a tuple, render it, and evaluate the answer.
///
/// The source holds a [`Compiled`] document, so the answer expression parses
/// once and the domains materialize once (specification section 3.4). A refill
/// batch builds the source once and fills many times.
#[derive(Debug, Clone)]
pub struct TemplateSource<'doc> {
    /// The knowledge point the template serves.
    kp_id: String,
    /// The digest of the `content_store` row, when the caller knows it.
    digest: Option<String>,
    /// The compiled document.
    compiled: Compiled<'doc>,
    /// The authored exemplars of the knowledge point (A6, C4).
    ///
    /// The per-instance re-check reads the envelope of these answers. An empty
    /// list means the knowledge point authored no exemplar, or the loaded
    /// curriculum does not name it; then there is no envelope to read and the
    /// envelope rule is silent, exactly as the gate is silent in that case.
    exemplars: &'doc [Exemplar],
}

impl<'doc> TemplateSource<'doc> {
    /// Compile a document into a source.
    ///
    /// # Errors
    ///
    /// Returns [`InstantiateError`] when the answer expression leaves the
    /// grammar (V2) or when a domain is empty or past its bound.
    pub fn new(kp_id: impl Into<String>, doc: &'doc TemplateDoc) -> Result<Self, InstantiateError> {
        Ok(Self {
            kp_id: kp_id.into(),
            digest: None,
            compiled: Compiled::new(doc)?,
            exemplars: &[],
        })
    }

    /// Build a source from a document that is compiled already.
    #[must_use]
    pub fn from_compiled(kp_id: impl Into<String>, compiled: Compiled<'doc>) -> Self {
        Self {
            kp_id: kp_id.into(),
            digest: None,
            compiled,
            exemplars: &[],
        }
    }

    /// Name the `content_store` digest the pool row records.
    #[must_use]
    pub fn with_digest(mut self, digest: impl Into<String>) -> Self {
        self.digest = Some(digest.into());
        self
    }

    /// Give the source the authored exemplars of its knowledge point (C4).
    ///
    /// The per-instance re-check then applies the exemplar envelope, which is the
    /// rule that catches the template that computes correctly and is not this
    /// knowledge point's problem.
    #[must_use]
    pub const fn with_exemplars(mut self, exemplars: &'doc [Exemplar]) -> Self {
        self.exemplars = exemplars;
        self
    }

    /// The authored exemplars the re-check reads.
    #[must_use]
    pub const fn exemplars(&self) -> &'doc [Exemplar] {
        self.exemplars
    }

    /// The [`GateSpec`] the per-instance re-check runs against.
    ///
    /// The answer kind is the document's own. The gate refuses a document whose
    /// `answer_kind` differs from the knowledge point's, so the two agree on
    /// every document that reached the pool.
    #[must_use]
    pub const fn gate_spec(&self) -> GateSpec<'doc> {
        GateSpec {
            answer_kind: self.compiled.doc().answer_kind,
            exemplars: self.exemplars,
        }
    }

    /// The compiled document.
    #[must_use]
    pub const fn compiled(&self) -> &Compiled<'doc> {
        &self.compiled
    }

    /// The document.
    #[must_use]
    pub const fn doc(&self) -> &'doc TemplateDoc {
        self.compiled.doc()
    }

    /// Whether one candidate stream already holds every satisfying tuple.
    ///
    /// The split is the 1.0 split of `problem_templates.py:353-375`, and
    /// [`EXHAUSTIVE_SPACE_LIMIT`] is its threshold.
    #[must_use]
    pub fn walks_whole_space(&self) -> bool {
        self.compiled.plan().declared_space() <= EXHAUSTIVE_SPACE_LIMIT
    }
}

impl ProblemSource for TemplateSource<'_> {
    fn source(&self) -> Source {
        Source::Template
    }

    fn kp_id(&self) -> &str {
        &self.kp_id
    }

    fn content_digest(&self) -> Option<&str> {
        self.digest.as_deref()
    }

    fn fill(&self, kp: &str, n: usize, seed: u64) -> Result<Batch, FillError> {
        same_kp(&self.kp_id, kp)?;
        if n == 0 {
            return Ok(Batch::default());
        }
        let spec = self.gate_spec();
        // The envelope reads every authored answer, so the fill reads it once
        // per batch and not once per instance.
        let envelope: Option<Envelope> = exemplar_envelope(spec.exemplars);
        let mut rng = rng_from_seed(seed);
        let mut out: Vec<Instance> = Vec::new();
        let mut refusals: Vec<Refused> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut last: Option<String> = None;
        let whole_space = self.walks_whole_space();
        for _round in 0..FILL_ROUNDS {
            let stream: Vec<Bindings> = self.compiled.candidates(&mut rng)?;
            if stream.is_empty() {
                break;
            }
            for bindings in stream {
                match self.compiled.instantiate(bindings.clone()) {
                    // 1.0 skips a candidate its `_build` refuses and keeps
                    // walking (`problem_templates.py:399-403`). A refused
                    // candidate is a gate defect, and the batch reports it only
                    // when NO candidate survived.
                    Err(error) => {
                        let message = error.to_string();
                        last = Some(message.clone());
                        refusals.push(Refused {
                            bindings,
                            text: None,
                            code: "instantiation".to_string(),
                            message,
                        });
                    }
                    Ok(instance) => {
                        // One statement is decided once. A digest the walk
                        // already read is neither served again nor counted
                        // again.
                        if !seen.insert(instance.instance_hash.clone()) {
                            continue;
                        }
                        // FIXM4a: replace with template::check_instance
                        match recheck::check_one(self.doc(), &spec, envelope.as_ref(), &instance) {
                            Err(rejection) => {
                                last = Some(rejection.message.clone());
                                refusals.push(Refused {
                                    bindings,
                                    text: Some(instance.text),
                                    code: rejection.code.to_string(),
                                    message: rejection.message,
                                });
                            }
                            Ok(()) => {
                                out.push(instance);
                                if out.len() >= n {
                                    return Ok(Batch::new(out, refusals));
                                }
                            }
                        }
                    }
                }
            }
            if whole_space {
                break;
            }
        }
        if !out.is_empty() {
            return Ok(Batch::new(out, refusals));
        }
        match last {
            Some(reason) => Err(FillError::NoValidInstance { reason }),
            None => Err(FillError::NoSatisfyingTuple),
        }
    }
}

/// One exemplar the checker cannot decide.
///
/// Specification section 6.1 asks 2.0 to record the fact rather than hide it. The
/// A6 operator flag of U4 reads this list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExemplarRefusal {
    /// The position of the exemplar in author order, from zero.
    pub index: usize,
    /// The authored answer the checker refused.
    pub answer: String,
    /// Why the grammar refused it.
    pub reason: Undecidable,
}

/// The A6 fallback source: the authored exemplars of one knowledge point.
///
/// # What changes from the 1.0 rotation
///
/// 1.0 serves `pool[spec.index % len(pool)]` (`cadus_web/engine.py:436-442`), so
/// the rotation restarts with every task and a knowledge point with one exemplar
/// serves the same problem forever. 2.0 fills the pool with the whole exemplar
/// list at once and lets the D5 ring choose (specification section 6.1). A
/// knowledge point with three exemplars gets a real three-cycle.
///
/// # The anti-repeat limit, recorded and not hidden
///
/// A knowledge point with fewer exemplars than [`RING_CAPACITY`] cannot satisfy
/// anti-repeat: the ring blocks every one of them before it drops the oldest.
/// [`ExemplarSource::covers_ring`] reports that, and the A6 flag of U4 shows it.
#[derive(Debug, Clone)]
pub struct ExemplarSource<'kp> {
    /// The knowledge point the exemplars belong to.
    kp_id: String,
    /// The exemplars, in author order.
    exemplars: &'kp [Exemplar],
}

impl<'kp> ExemplarSource<'kp> {
    /// Build a source over an exemplar list in author order.
    #[must_use]
    pub fn new(kp_id: impl Into<String>, exemplars: &'kp [Exemplar]) -> Self {
        Self {
            kp_id: kp_id.into(),
            exemplars,
        }
    }

    /// Build a source over the exemplars of one knowledge point.
    #[must_use]
    pub fn from_knowledge_point(kp: &'kp KnowledgePoint) -> Self {
        Self {
            kp_id: kp.id.as_str().to_string(),
            exemplars: &kp.exemplars,
        }
    }

    /// The exemplars, in author order.
    #[must_use]
    pub const fn exemplars(&self) -> &'kp [Exemplar] {
        self.exemplars
    }

    /// The count of authored exemplars.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.exemplars.len()
    }

    /// Whether the knowledge point authored no exemplar.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.exemplars.is_empty()
    }

    /// Whether the exemplar count reaches the ring size (A6).
    ///
    /// A `false` result is the A6 flag: the rotation repeats inside the ring
    /// window, and the operator dashboard says so.
    #[must_use]
    pub const fn covers_ring(&self) -> bool {
        self.exemplars.len() >= RING_CAPACITY
    }

    /// Every exemplar whose answer leaves the decidable grammar (V2).
    ///
    /// [`ProblemSource::fill`] skips these, because one broken exemplar must not
    /// take the whole knowledge point off the air. The list names them.
    #[must_use]
    pub fn refusals(&self) -> Vec<ExemplarRefusal> {
        let mut out = Vec::new();
        for (index, exemplar) in self.exemplars.iter().enumerate() {
            if let Err(reason) = canonical_form(&exemplar.answer) {
                out.push(ExemplarRefusal {
                    index,
                    answer: exemplar.answer.clone(),
                    reason,
                });
            }
        }
        out
    }
}

impl ProblemSource for ExemplarSource<'_> {
    fn source(&self) -> Source {
        Source::Exemplar
    }

    fn kp_id(&self) -> &str {
        &self.kp_id
    }

    fn fill(&self, kp: &str, n: usize, seed: u64) -> Result<Batch, FillError> {
        same_kp(&self.kp_id, kp)?;
        // The rotation is author order. No draw runs here, so the seed changes
        // nothing: two refills of one exemplar list give the same batch.
        let _ = seed;
        if n == 0 {
            return Ok(Batch::default());
        }
        let mut out: Vec<Instance> = Vec::new();
        let mut refusals: Vec<Refused> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for exemplar in self.exemplars {
            let canon = match canonical_form(&exemplar.answer) {
                Ok(canon) => canon,
                // An exemplar answer the checker cannot decide is skipped and
                // counted. One broken exemplar must not take the whole knowledge
                // point off the air (specification section 6.1).
                Err(reason) => {
                    refusals.push(Refused {
                        bindings: Bindings::new(),
                        text: Some(exemplar.problem.clone()),
                        code: "undecidable-answer".to_string(),
                        message: reason.reason.to_string(),
                    });
                    continue;
                }
            };
            let text = exemplar.problem.clone();
            let instance_hash = problem_text_hash(&text);
            if !seen.insert(instance_hash.clone()) {
                continue;
            }
            out.push(Instance {
                bindings: Bindings::new(),
                text,
                answer: exemplar.answer.clone(),
                canon,
                instance_hash,
            });
            if out.len() >= n {
                break;
            }
        }
        if out.is_empty() {
            return Err(FillError::NoExemplar);
        }
        Ok(Batch::new(out, refusals))
    }
}
