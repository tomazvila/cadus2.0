//! The A6 fallback source: the authored exemplars of one knowledge point.

use std::collections::BTreeSet;

use crate::answer::{Undecidable, canonical_form};
use crate::curriculum::model::{Exemplar, KnowledgePoint};
use crate::learner::problem_text_hash;
use crate::template::{Bindings, Instance};

use super::super::Source;
use super::super::ring::RING_CAPACITY;
use super::{Batch, FillError, ProblemSource, Refused, same_kp};

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
        // The exemplar list is authored text, and one entry is one statement, so
        // the digest alone decides a repeat here.
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
