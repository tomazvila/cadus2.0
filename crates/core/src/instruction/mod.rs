//! The two authored instruction documents: the teach page (L4) and the hint
//! ladder (L5), with the gate that judges each one (A2, C6, R3).
//!
//! Spec: `docs/reference/authoring-and-spa-1.0-spec.md` sections 2.2 and 7 row
//! R6, and `docs/reference/web-service-1.0-spec.md:53` for the teach reply.
//!
//! # Why these documents have a gate at all
//!
//! 1.0 asked a model for a teach page and for every hint on the request path,
//! and its grade cache excluded both on purpose (`grade_cache.py:451-467`). So
//! 1.0 had no document to judge. 2.0 authors both offline, stores them in
//! `content_store`, and serves them with zero model tokens (L4, L5, T1). An
//! authored document that no machine read is a document a learner reads first.
//!
//! # What each gate refuses
//!
//! | Document | Rule | Why |
//! |---|---|---|
//! | teach | `concept` is a non-empty string | a page with no method states nothing |
//! | teach | `worked_example.problem` is a non-empty string | the concept alone is not instruction |
//! | teach | `worked_example.steps` is a list of at least one non-empty step | the worked solution IS the page (spec section 7, row R6) |
//! | teach | the worked problem is not an exemplar | A6 serves the exemplars, so an exemplar worked out hands the learner an answer before the attempt (Hard Rule 1) |
//! | teach | the last step names no answer of ANOTHER served problem | the page works one problem; a step that states a second served answer hands that one away too (Hard Rule 1) |
//! | hint | `hints` holds at least one rung | the hint route serves the rungs and nothing else |
//! | hint | no rung repeats an earlier rung | each rung goes one step past the one before it |
//! | hint | no rung names an answer the knowledge point serves | Hard Rule 3: a hint is a question, never the final step |
//! | both | no unknown field | the serve reader refuses one, so a stored body it cannot read serves nothing |
//!
//! # What "an answer the knowledge point serves" means
//!
//! One ladder serves every instance of one knowledge point, so the gate reads the
//! whole set of answers that knowledge point hands a learner, and not one
//! instance:
//!
//! - every exemplar answer — A6 serves the exemplars when no template is
//!   approved;
//! - every answer of an instance the templates of the knowledge point render
//!   ([`InstructionSpec::instance_answers`], filled by the worker).
//!
//! The template set covers the `approved` templates AND the `pending` ones.
//! `cadus-worker author` writes all four kinds in ONE process, template first,
//! and every kind enters `content_store` as `pending` (C6). A gate that read the
//! approved rows alone therefore judged every ladder of a fresh curriculum
//! against an EMPTY instance set, because the template it belongs beside was
//! minutes old and unreviewed (M6 review 2, finding V1). A pending template is
//! the material the reviewer is about to approve, so its answers gate the ladder
//! and the page authored in the same pass.
//!
//! The set carries no exemption. An earlier build skipped an exemplar whose own
//! problem showed its answer; a rendered instance is a different statement that
//! shows nothing, and the L5 route serves the stored rung with no re-check, so
//! the skip handed the answer to the learner (M6 review, findings F2, F15 and
//! F25). The template gate reads the same rule over the instances it renders
//! (`crate::template::gate`).
//!
//! # The two answer rules are not one rule
//!
//! The hint gate refuses a rung that names ANY served answer: a hint works no
//! problem of its own, so every answer in it is an answer of the learner's
//! problem. The teach page DOES work a problem, and it states that problem's
//! answer in its last step, which is what a worked example is. So the teach rule
//! is the pair rule: the last step names no answer of a served problem OTHER
//! than the one the page works ([`gate_teach`]).
//!
//! # No panic, on any body
//!
//! Every step returns a [`Rejection`]. Nothing here indexes, unwraps, or divides
//! (C4). A body that is not a document at all is a rejection, not a crash.

mod body;
mod finite;
mod hint;
mod teach;

use serde::{Deserialize, Serialize};

use crate::curriculum::Exemplar;
use crate::template::gate::Rejection;
use crate::template::{Compiled, GATE_SEED, from_body, rng_from_seed};

pub use hint::gate_hint_ladder;
pub use teach::{gate_teach, gate_teach_with_policy};

/// The `content_store.kind` of a teach page (L4).
pub const KIND_TEACH: &str = "teach";

/// The `content_store.kind` of a hint ladder (L5).
pub const KIND_HINT_LADDER: &str = "hint_ladder";

/// The instances one template document contributes to the instruction gates.
///
/// The number is the floor the M6 review ruling names for findings F2, F15 and
/// F25. Every draw runs from [`GATE_SEED`], so the answer set of one document is
/// the same set on every pass and a reviewer reproduces the verdict (C6).
pub const INSTANCE_SAMPLES: u32 = 8;

/// One problem the knowledge point serves, with the answer it expects.
///
/// The pair matters, and not the answer alone: the teach gate lets the page
/// state the answer of the problem the page works, and it refuses the answer of
/// every other served problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServedInstance {
    /// The problem statement, as the learner reads it.
    pub problem: String,
    /// The answer that problem expects.
    pub answer: String,
}

/// Every instance one stored template body serves.
///
/// The list holds the instance of every worked sample the document pins and
/// [`INSTANCE_SAMPLES`] drawn instances. A body this build cannot read or cannot
/// compile contributes nothing: it is a row of an older shape, and a gate that
/// refused every document over it would teach the model nothing it can act on.
#[must_use]
pub fn template_instances(body: &str) -> Vec<ServedInstance> {
    let Ok(doc) = from_body(body) else {
        return Vec::new();
    };
    let Ok(compiled) = Compiled::new(&doc) else {
        return Vec::new();
    };
    let mut served: Vec<ServedInstance> = Vec::new();
    let mut keep = |problem: String, answer: String| {
        let instance = ServedInstance { problem, answer };
        if !instance.answer.is_empty() && !served.contains(&instance) {
            served.push(instance);
        }
    };
    for sample in &doc.samples {
        if let Ok(instance) = compiled.instantiate(sample.bindings()) {
            keep(instance.text, instance.answer);
        }
    }
    let mut rng = rng_from_seed(GATE_SEED);
    for _ in 0..INSTANCE_SAMPLES {
        if let Ok(instance) = compiled.draw(&mut rng) {
            keep(instance.text, instance.answer);
        }
    }
    served
}

/// The fields a teach body carries.
pub const TEACH_FIELDS: [&str; 2] = ["concept", "worked_example"];

/// The fields a worked example carries.
pub const WORKED_EXAMPLE_FIELDS: [&str; 2] = ["problem", "steps"];

/// The fields a hint body carries.
pub const HINT_FIELDS: [&str; 1] = ["hints"];

/// One fully worked example (`docs/reference/web-service-1.0-spec.md:53`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkedExample {
    /// The example problem. It is self-contained: it never names the served
    /// practice problem and it never reveals its answer (Hard Rule 1).
    pub problem: String,
    /// The solution steps, in order, ending with the answer.
    pub steps: Vec<String>,
}

/// The body of a `content_store` row of kind `teach` (L4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeachPage {
    /// The method or the rule, in one or two sentences.
    pub concept: String,
    /// The fully worked example.
    pub worked_example: WorkedExample,
}

/// The body of a `content_store` row of kind `hint_ladder` (L5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HintLadder {
    /// The rungs, from the widest nudge to the narrowest.
    pub hints: Vec<String>,
}

/// What one instruction gate knows about the knowledge point it judges for.
#[derive(Debug, Clone)]
pub struct InstructionSpec<'a> {
    /// The authored problems of the knowledge point. A6 serves these when no
    /// template is approved, so their answers are answers a learner sees.
    pub exemplars: &'a [Exemplar],
    /// The instances the templates of this knowledge point render, each one with
    /// its answer. The serve path draws one instance per attempt, so every
    /// answer here is an answer a learner reads.
    ///
    /// The worker fills the list from the `approved` AND the `pending` template
    /// documents of the knowledge point
    /// (`cadus_worker::authoring::job::served_instances`). An empty list is the
    /// honest value for a knowledge point that stores no template at all: A6
    /// serves the exemplars there and nothing else.
    pub instance_answers: Vec<ServedInstance>,
}

impl InstructionSpec<'_> {
    /// Every problem this knowledge point serves, with its answer, in a fixed
    /// order: the exemplars first, then the rendered instances.
    fn served(&self) -> impl Iterator<Item = (&str, &str)> {
        let exemplars = self
            .exemplars
            .iter()
            .map(|exemplar| (exemplar.problem.as_str(), exemplar.answer.as_str()));
        let instances = self
            .instance_answers
            .iter()
            .map(|served| (served.problem.as_str(), served.answer.as_str()));
        exemplars.chain(instances)
    }

    /// Every answer this knowledge point serves, in one list and in a fixed
    /// order: the exemplar answers first, then the instance answers.
    ///
    /// The list holds each answer once, and it holds no empty answer.
    #[must_use]
    pub fn served_answers(&self) -> Vec<&str> {
        let mut answers: Vec<&str> = Vec::new();
        for (_, answer) in self.served() {
            if answer.is_empty() || answers.contains(&answer) {
                continue;
            }
            answers.push(answer);
        }
        answers
    }
}

/// Re-run the gate of one STORED instruction document against the material the
/// knowledge point serves now (C6).
///
/// `kind` is the `content_store.kind` of the row: [`KIND_TEACH`] or
/// [`KIND_HINT_LADDER`]. Every other kind answers [`None`], because this gate
/// judges no other kind.
///
/// # Why a stored document is judged twice
///
/// The gate of an authoring pass reads the material of that pass. A template
/// authored, or approved, AFTER a ladder was stored is material the ladder was
/// never judged against, so a give-away rung no pass read stands
/// `pending` in the review queue. The approve route runs this function over the
/// pending ladders and pages of the knowledge point, and it moves a document
/// this function refuses to `rejected` with the message as the reason (M6 review
/// 2, the FIX2-M6-A ruling, part 3).
///
/// [`None`] means the document still passes. The caller leaves it alone.
#[must_use]
pub fn regate(kind: &str, body: &str, spec: &InstructionSpec<'_>) -> Option<Rejection> {
    regate_with_policy(kind, body, spec, None)
}

/// Recheck instruction against the currently loaded reviewed finite policy.
#[must_use]
pub fn regate_with_policy(
    kind: &str,
    body: &str,
    spec: &InstructionSpec<'_>,
    policy: Option<&crate::curriculum::FiniteObjectiveDomain>,
) -> Option<Rejection> {
    match kind {
        KIND_TEACH => gate_teach_with_policy(body, spec, policy).err(),
        KIND_HINT_LADDER => gate_hint_ladder(body, spec).err(),
        _ => None,
    }
}
