//! The readiness audit of D-F5: what one knowledge point needs before the
//! service teaches it, practices it, and assesses it.
//!
//! # Why the module exists
//!
//! The ground audit of 2026-09-06 records two facts. Finding (h): the deployed
//! `content_store` holds zero rows, so no knowledge point has an approved teach
//! page. Finding (j): the SPA falls through to practice when the teach route
//! refuses, so the learner practices a skill the service never taught. The
//! selector plans a lesson from the curriculum alone and learns neither fact.
//!
//! This module is the missing check. It reads the curriculum and a
//! [`ContentIndex`] — the approved documents of the store — and answers, per
//! knowledge point, which of the seven readiness conditions hold.
//!
//! # The two halves, and why they are separate
//!
//! [`ReadinessIndex`] holds the CURRICULUM half: which exemplars the answer
//! grammar decides, which one the audit holds out of practice, whether every
//! practice exemplar carries a solution sketch, whether the topic needs a
//! visual, and the prerequisite topics. The half costs one pass over every
//! exemplar answer, so a process builds it ONCE, at boot, beside the arena.
//!
//! [`ReadinessIndex::resolve`] adds the STORE half — the approved documents and
//! the approved template count — and gives a [`ReadinessSet`]. That step is map
//! lookups only, so a request path pays for it.
//!
//! # Purity
//!
//! Nothing here opens a socket, reads a clock, or calls a model (R3). The store
//! reaches this module through [`ContentIndex`] and through nothing else.

mod facts;
mod prereq;
mod report;
mod resolve;
mod visual;

use std::collections::BTreeMap;
use std::fmt;

pub use facts::{KpFacts, ReadinessIndex};
pub use prereq::{CoverageCounts, DiagnosticState, FloorEvidence, PrereqCoverage, TopicCoverage};
pub use report::{CourseReport, ReadinessReport, TopicReport};
pub use resolve::{ReadinessGate, ReadinessSet};
pub use visual::{VISUAL_WORDS, visual_needed};

/// The `content_store.kind` of a problem template (A1).
///
/// [`crate::instruction`] names the two instruction kinds; the template kind
/// belongs to the pool, and the readiness audit reads all three.
pub const KIND_TEMPLATE: &str = "template";

/// The decidable items one knowledge point needs before it is practicable.
///
/// Three distinct items make a rotation the anti-repeat ring can turn. Fewer
/// than three serves one statement again and again inside one lesson.
pub const PRACTICE_MINIMUM: usize = 3;

/// The decidable exemplars a knowledge point needs before the audit holds one
/// out for assessment.
///
/// The held-out item is the LAST decidable exemplar by author index. An author
/// who adds a fourth exemplar therefore moves the held-out item, and the three
/// before it stay in practice.
pub const HELD_OUT_MINIMUM: usize = 3;

/// The approved documents and templates of one knowledge point.
///
/// The store implements it. The core never names a table, a column, or a
/// connection: this trait is the whole surface between the audit and the
/// database (R3).
pub trait ContentIndex {
    /// Whether an APPROVED document of this kind stands for the serving key.
    ///
    /// `kp_key` is the key [`crate::pool::kp_key`] writes: `"<topic>/<kp>"`.
    /// `kind` is one of [`crate::instruction::KIND_TEACH`],
    /// [`crate::instruction::KIND_HINT_LADDER`], and [`KIND_TEMPLATE`].
    fn has_approved(&self, kp_key: &str, kind: &str) -> bool;

    /// The count of APPROVED templates of the serving key.
    fn approved_templates(&self, kp_key: &str) -> usize;
}

/// A content index that holds nothing: the fresh deployment of finding (h).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EmptyContent;

impl ContentIndex for EmptyContent {
    fn has_approved(&self, _kp_key: &str, _kind: &str) -> bool {
        false
    }

    fn approved_templates(&self, _kp_key: &str) -> usize {
        0
    }
}

/// A content index over a map, for a test and for the worker report.
///
/// The key of `documents` is the pair `(kp_key, kind)`, and the value is the
/// count of approved documents of that pair.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MapContent {
    /// Serving key and kind to the count of approved documents.
    pub documents: BTreeMap<(String, String), usize>,
}

impl MapContent {
    /// An index over the counted pairs.
    #[must_use]
    pub fn new(documents: BTreeMap<(String, String), usize>) -> Self {
        Self { documents }
    }

    /// Record `count` approved documents of one pair.
    pub fn insert(&mut self, kp_key: impl Into<String>, kind: impl Into<String>, count: usize) {
        self.documents.insert((kp_key.into(), kind.into()), count);
    }

    /// The count of approved documents of one pair.
    #[must_use]
    pub fn count(&self, kp_key: &str, kind: &str) -> usize {
        self.documents
            .get(&(kp_key.to_owned(), kind.to_owned()))
            .copied()
            .unwrap_or(0)
    }
}

impl ContentIndex for MapContent {
    fn has_approved(&self, kp_key: &str, kind: &str) -> bool {
        self.count(kp_key, kind) > 0
    }

    fn approved_templates(&self, kp_key: &str) -> usize {
        self.count(kp_key, KIND_TEMPLATE)
    }
}

/// One unmet readiness condition.
///
/// The order of the variants is the order the report prints, and it is the
/// order an author fixes them in: instruction first, then practice, then
/// assessment, then the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Blocker {
    /// No approved teach page.
    Teachable,
    /// Fewer than [`PRACTICE_MINIMUM`] decidable items stay in practice.
    Practicable,
    /// No decidable item is held out of practice.
    Assessable,
    /// No approved hint ladder.
    Hints,
    /// One practice exemplar carries no solution sketch.
    Solutions,
    /// One prerequisite topic has no practicable knowledge point.
    Prerequisites,
    /// The topic needs a visual and no visual exists.
    Visual,
}

/// The wire values of [`Blocker`], in declaration order.
pub const BLOCKERS: [&str; 7] = [
    "teachable",
    "practicable",
    "assessable",
    "hints",
    "solutions",
    "prerequisites",
    "visual",
];

impl Blocker {
    /// The wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Teachable => "teachable",
            Self::Practicable => "practicable",
            Self::Assessable => "assessable",
            Self::Hints => "hints",
            Self::Solutions => "solutions",
            Self::Prerequisites => "prerequisites",
            Self::Visual => "visual",
        }
    }

    /// Every blocker, in declaration order.
    #[must_use]
    pub const fn every() -> [Self; 7] {
        [
            Self::Teachable,
            Self::Practicable,
            Self::Assessable,
            Self::Hints,
            Self::Solutions,
            Self::Prerequisites,
            Self::Visual,
        ]
    }
}

impl fmt::Display for Blocker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The readiness of one knowledge point (D-F5).
///
/// Every field is a fact about ONE knowledge point at ONE moment. Nothing here
/// is a policy: [`crate::selector`] reads the three serve conditions and the
/// worker report prints all seven.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Readiness {
    /// The serving key `"<topic>/<kp>"`.
    pub kp_key: String,
    /// An approved teach page exists (L4).
    pub teachable: bool,
    /// At least [`PRACTICE_MINIMUM`] decidable items stay in practice.
    pub practicable: bool,
    /// At least one decidable item stays out of practice for assessment.
    pub assessable: bool,
    /// An approved hint ladder exists (L5).
    pub hints: bool,
    /// Every practice exemplar carries a solution sketch.
    pub solutions: bool,
    /// Every prerequisite topic has at least one practicable knowledge point.
    pub prerequisites_ok: bool,
    /// The topic id or its text names a visual (see [`visual_needed`]).
    pub visual_needed: bool,
    /// A visual exists: the author wrote at least one
    /// [`crate::visual::VisualSpec`] that passes its own check (unit f9).
    pub visual_present: bool,
    /// The authored visuals the check refuses. Each one counts as absent.
    pub broken_visuals: usize,
    /// The decidable exemplars of the knowledge point.
    pub decidable_exemplars: usize,
    /// The authored exemplars of the knowledge point (84 b). The teach-only
    /// difference (`authored - decidable_exemplars`) serves as self-check rows.
    pub authored_exemplars: usize,
    /// The approved templates of the knowledge point.
    pub approved_templates: usize,
    /// The decidable items practice draws from: the decidable exemplars less
    /// the held-out one, plus the approved templates.
    pub practice_items: usize,
}

impl Readiness {
    /// Whether a LESSON serves this knowledge point (D-F5, note 101 b).
    ///
    /// A lesson teaches, practices, and then assesses, so it needs all three —
    /// EXCEPT that a short practice pool (`PRACTICE_MINIMUM` unmet) and a missing
    /// held-out item are practice/assessment pool rules: a knowledge point with
    /// decidable exemplars serves those exemplars in the lesson. The staging
    /// probes of 2026-09-24 (note 103 b) locked every topic whose KPs each carry
    /// exactly two exemplars — the plan dropped the topic and the walk stopped.
    #[must_use]
    pub fn serves_lesson(&self) -> bool {
        let serves_from_exemplars = self.decidable_exemplars > 0;
        self.teachable
            && (self.practicable || serves_from_exemplars)
            && (self.assessable || serves_from_exemplars)
    }

    /// Whether a REVIEW or a QUIZ serves this knowledge point (D-F5).
    ///
    /// Both revisit a skill the learner already met, so neither needs the teach
    /// page or the held-out item.
    #[must_use]
    pub const fn serves_review(&self) -> bool {
        self.practicable
    }

    /// The unmet conditions, in [`Blocker`] order.
    #[must_use]
    pub fn blockers(&self) -> Vec<Blocker> {
        let mut out = Vec::new();
        if !self.teachable {
            out.push(Blocker::Teachable);
        }
        if !self.practicable {
            out.push(Blocker::Practicable);
        }
        if !self.assessable {
            out.push(Blocker::Assessable);
        }
        if !self.hints {
            out.push(Blocker::Hints);
        }
        if !self.solutions {
            out.push(Blocker::Solutions);
        }
        if !self.prerequisites_ok {
            out.push(Blocker::Prerequisites);
        }
        if self.visual_needed && !self.visual_present {
            out.push(Blocker::Visual);
        }
        out
    }

    /// The blockers that stop a LESSON, in [`Blocker`] order.
    ///
    /// The list is what the session plan carries and what the SPA prints. It
    /// names the three serve conditions and nothing else: a missing hint ladder
    /// costs the learner a hint, and it never withholds the lesson.
    #[must_use]
    pub fn lesson_blockers(&self) -> Vec<Blocker> {
        let serves_from_exemplars = self.decidable_exemplars > 0;
        self.blockers()
            .into_iter()
            .filter(|blocker| match blocker {
                // A short practice pool and a missing held-out item never stop a
                // knowledge point that has decidable exemplars (note 101 b: the
                // exemplars serve first; the plan gate and the serve skip read
                // the same rule).
                Blocker::Practicable | Blocker::Assessable if serves_from_exemplars => false,
                blocker => {
                    matches!(
                        blocker,
                        Blocker::Teachable | Blocker::Practicable | Blocker::Assessable
                    )
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod lesson_gate_tests {
    use super::{Blocker, Readiness};

    fn row(teachable: bool, practicable: bool, assessable: bool, decidable: usize) -> Readiness {
        Readiness {
            kp_key: "test/kp1".to_owned(),
            teachable,
            practicable,
            assessable,
            hints: true,
            solutions: true,
            prerequisites_ok: true,
            visual_needed: false,
            visual_present: false,
            broken_visuals: 0,
            decidable_exemplars: decidable,
            authored_exemplars: decidable,
            approved_templates: 0,
            practice_items: decidable,
        }
    }

    /// A readiness row of the shape the staging probe of 2026-09-24 (note 103 b)
    /// locked: two decidable exemplars, one fact short of `PRACTICE_MINIMUM`, no
    /// held-out item. The lesson serves it; the plan keeps the topic.
    fn two_exemplars() -> Readiness {
        row(true, false, false, 2)
    }

    #[test]
    fn two_decidable_exemplars_serve_the_lesson_despite_the_short_pool() {
        let row = two_exemplars();
        assert!(row.serves_lesson(), "exemplars first (note 101 b)");
        assert!(row.lesson_blockers().is_empty());
    }

    #[test]
    fn a_point_with_no_decidable_exemplar_keeps_the_pool_blockers() {
        let row = row(true, false, false, 0);
        assert!(!row.serves_lesson());
        assert_eq!(
            row.lesson_blockers(),
            vec![Blocker::Practicable, Blocker::Assessable]
        );
    }

    #[test]
    fn an_unteachable_point_stays_blocked_whatever_its_pool() {
        let row = row(false, true, true, 2);
        assert!(!row.serves_lesson());
        assert_eq!(row.lesson_blockers(), vec![Blocker::Teachable]);
    }
}
