//! The A1 source: draw a tuple, render it, and evaluate the answer.

use std::collections::BTreeMap;

use crate::answer::Canon;
use crate::curriculum::model::Exemplar;
use crate::template::gate::py_str;
use crate::template::{
    Bindings, Compiled, EXHAUSTIVE_SPACE_LIMIT, FiniteGateSpec, GateSpec, Instance,
    InstantiateError, TemplateDoc, check_instance, rng_from_seed,
};

use super::super::Source;
use super::{Batch, FILL_ROUNDS, FillError, ProblemSource, Refused, same_kp};

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
    /// Reviewed finite policy for exact per-instance role checks.
    finite: Option<FiniteGateSpec<'doc>>,
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
            finite: None,
        })
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

    /// Attach trusted finite policy from the source knowledge point.
    pub fn with_finite_policy(
        mut self,
        kp_id: &str,
        policy: &'doc crate::curriculum::FiniteObjectiveDomain,
    ) -> Result<Self, String> {
        self.finite = Some(FiniteGateSpec::new(kp_id, policy)?);
        Ok(self)
    }

    /// The [`GateSpec`] the per-instance re-check runs against.
    ///
    /// The answer kind is the document's own. The gate refuses a document whose
    /// `answer_kind` differs from the knowledge point's, so the two agree on
    /// every document that reached the pool.
    #[must_use]
    pub fn gate_spec(&self) -> GateSpec<'doc> {
        GateSpec {
            answer_kind: self.compiled.doc().answer_kind,
            exemplars: self.exemplars,
            finite: self.finite.clone(),
        }
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
        let mut walk = FillWalk::new(self, n);
        let mut rng = rng_from_seed(seed);
        let whole_space = self.walks_whole_space();
        for _round in 0..FILL_ROUNDS {
            let stream: Vec<Bindings> = self.compiled.candidates(&mut rng)?;
            if stream.is_empty() {
                break;
            }
            let full = stream.into_iter().any(|bindings| walk.take(bindings));
            if full || whole_space {
                break;
            }
        }
        walk.finish()
    }
}

/// The state of one [`TemplateSource::fill`] walk over its candidate streams.
struct FillWalk<'walk, 'doc> {
    /// The source the walk fills.
    source: &'walk TemplateSource<'doc>,
    /// The spec the per-instance re-check runs against.
    ///
    /// The envelope reads every authored answer, so the walk reads it once per
    /// batch and not once per instance.
    spec: GateSpec<'doc>,
    /// The count of instances the caller asked for.
    wanted: usize,
    /// The instances that passed every rule.
    out: Vec<Instance>,
    /// The candidates the walk refused, in draw order.
    refusals: Vec<Refused>,
    /// The digest of a statement, and the first answer the batch computed for it.
    ///
    /// `serving_pool` keys a row by the digest, so a second tuple that renders
    /// the same statement with a DIFFERENT answer must not become the row a
    /// learner reads (C4, M4 review 2, finding 1).
    seen: BTreeMap<String, (String, Canon)>,
    /// The message of the last refusal.
    last: Option<String>,
}

impl<'walk, 'doc> FillWalk<'walk, 'doc> {
    /// Start a walk that stops after `wanted` instances.
    fn new(source: &'walk TemplateSource<'doc>, wanted: usize) -> Self {
        Self {
            source,
            spec: source.gate_spec(),
            wanted,
            out: Vec::new(),
            refusals: Vec::new(),
            seen: BTreeMap::new(),
            last: None,
        }
    }

    /// Read one candidate tuple. The result is `true` when the batch is full.
    ///
    /// 1.0 skips a candidate its `_build` refuses and keeps walking
    /// (`problem_templates.py:399-403`). A refused candidate is a gate defect,
    /// and the batch reports it only when NO candidate survived.
    fn take(&mut self, bindings: Bindings) -> bool {
        match self.source.compiled.instantiate(bindings.clone()) {
            Err(error) => {
                self.refuse(bindings, None, "instantiation", error.to_string());
                false
            }
            Ok(instance) => self.take_instance(bindings, instance),
        }
    }

    /// Read one rendered instance. The result is `true` when the batch is full.
    ///
    /// One statement is decided once. A digest the walk already read with the
    /// SAME answer is neither served again nor counted again; the same digest
    /// with another answer is refused and counted.
    fn take_instance(&mut self, bindings: Bindings, instance: Instance) -> bool {
        match self.seen.get(&instance.instance_hash) {
            Some((_, first)) if *first == instance.canon => return false,
            Some((first, _)) => {
                let message = collision_message(&instance.text, first, &instance.answer);
                self.refuse(
                    bindings,
                    Some(instance.text),
                    "statement-collision",
                    message,
                );
                return false;
            }
            None => {
                self.seen.insert(
                    instance.instance_hash.clone(),
                    (instance.answer.clone(), instance.canon.clone()),
                );
            }
        }
        if let Err(rejection) = check_instance(self.source.doc(), &self.spec, &instance) {
            self.refuse(
                bindings,
                Some(instance.text),
                rejection.code,
                rejection.message,
            );
            return false;
        }
        self.out.push(instance);
        self.out.len() >= self.wanted
    }

    /// Record one refused candidate.
    fn refuse(&mut self, bindings: Bindings, text: Option<String>, code: &str, message: String) {
        self.last = Some(message.clone());
        self.refusals.push(Refused {
            bindings,
            text,
            code: code.to_string(),
            message,
        });
    }

    /// The batch, or the refusal of the walk when no instance survived.
    fn finish(self) -> Result<Batch, FillError> {
        if !self.out.is_empty() {
            return Ok(Batch::new(self.out, self.refusals));
        }
        match self.last {
            Some(reason) => Err(FillError::NoValidInstance { reason }),
            None => Err(FillError::NoSatisfyingTuple),
        }
    }
}

/// The refusal one statement with two answers earns (C4).
///
/// The gate refuses the same defect on the tuples it walked, and it names the
/// count of colliding tuples because it holds every one of them. The fill meets
/// its tuples one at a time, so its message names the two answers it holds and
/// no count (M4 review 2, finding 1).
fn collision_message(text: &str, first: &str, other: &str) -> String {
    format!(
        "statement {} already answers {} and this tuple answers {} — one statement carries one answer",
        py_str(text),
        py_str(first),
        py_str(other)
    )
}
