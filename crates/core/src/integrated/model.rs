//! The authored shape of one integrated task (D-F10).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::answer::AnswerContract;
use crate::curriculum::Slug;

/// The length of [`IntegratedItem::digest`], in hex characters.
pub const ITEM_DIGEST_LEN: usize = 16;

/// The application area an integrated task exercises.
///
/// The set is closed: an author picks one of these six, and a seventh value is a
/// load error. The six are the areas D-F10 names for the Foundations set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    /// Rates, unit conversion, and unit reasoning.
    RatesUnits,
    /// Percentages, ratios, and proportional parts.
    PercentagesRatios,
    /// Algebraic constraints and inequalities over a model.
    AlgebraicConstraints,
    /// Reading a plot, a table, or a chart.
    GraphInterpretation,
    /// Length, area, volume, and scale.
    Geometry,
    /// Person-minutes, service windows, staffing bounds, and feasibility.
    WorkforceCapacity,
}

impl Domain {
    /// The wire value of the domain.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RatesUnits => "rates_units",
            Self::PercentagesRatios => "percentages_ratios",
            Self::AlgebraicConstraints => "algebraic_constraints",
            Self::GraphInterpretation => "graph_interpretation",
            Self::Geometry => "geometry",
            Self::WorkforceCapacity => "workforce_capacity",
        }
    }
}

/// One quantity or constraint the scenario gives the learner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Given {
    /// What the quantity is, in words.
    pub label: String,
    /// The value as the scenario states it, with its unit.
    pub value: String,
    /// An assumption that changes feasibility, when the author states one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One offered method of solution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MethodOption {
    /// The id the learner submits.
    pub id: Slug,
    /// The method, in words.
    pub label: String,
    /// True when the method solves the scenario. Never served to a client.
    #[serde(default)]
    pub correct: bool,
    /// Why the method works, or why it fails. Shown after the grade only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

/// The method choice of an integrated task.
///
/// D-F10 asks for a method the learner chooses. More than one option can carry
/// `correct: true`: an integrated task supports alternate solution paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MethodChoice {
    /// The question that asks for the method.
    pub prompt: String,
    /// The offered methods, in author order.
    pub options: Vec<MethodOption>,
}

/// The answer policy of one field of the task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    /// The question of this field.
    pub prompt: String,
    /// The authored answer. Never served to a client (Hard Rule 1).
    pub answer: String,
    /// The acceptance rule. The default is [`AnswerContract::Exact`].
    #[serde(default = "exact")]
    pub contract: AnswerContract,
    /// The unit the learner writes beside the number, for the prompt only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Other authored answers an alternate solution path reaches.
    #[serde(default)]
    pub accept_also: Vec<String>,
    /// The hint ladder, from the widest hint to the narrowest.
    #[serde(default)]
    pub hints: Vec<String>,
}

/// The default acceptance rule of a field.
const fn exact() -> AnswerContract {
    AnswerContract::Exact
}

/// One intermediate step of the task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// The step id, unique inside the item.
    pub id: Slug,
    /// The question, the answer, and the hint ladder of the step.
    ///
    /// The field is nested, not flattened: `serde` refuses `flatten` beside
    /// `deny_unknown_fields`, and an authored file must not lose a misspelled
    /// key to a silent default.
    pub ask: Field,
    /// The knowledge points this step exercises (D-F9 attribution).
    #[serde(default)]
    pub skills: Vec<Slug>,
}

/// The final answer of the task, with the interpretation it carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Final {
    /// The question, the answer, and the hint ladder of the final answer.
    pub ask: Field,
    /// What the number means in the scenario. Shown after the grade only.
    pub interpretation: String,
    /// The knowledge points the final answer exercises.
    #[serde(default)]
    pub skills: Vec<Slug>,
}

/// One hand-authored integrated task (D-F10).
///
/// The item is ONE problem. The serve route hands over the scenario, the given
/// quantities, the method choice, and every step field together; the grade route
/// grades every step and the final answer in one submission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegratedItem {
    /// The item id, unique across the loaded set.
    pub id: Slug,
    /// The title of the task.
    pub title: String,
    /// The course the task belongs to.
    pub course: Slug,
    /// The topic the outcome records against.
    pub topic: Slug,
    /// The component topics the task integrates. A multi-step task whose
    /// component set is covered by this list serves this item (D-F10).
    pub component_topics: Vec<Slug>,
    /// The application area.
    pub domain: Domain,
    /// The scenario, in words.
    pub scenario: String,
    /// The quantities and the constraints of the scenario.
    pub given: Vec<Given>,
    /// The method choice, when the author offers one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<MethodChoice>,
    /// The intermediate steps, in author order.
    pub steps: Vec<Step>,
    /// The final answer and its interpretation.
    #[serde(rename = "final")]
    pub final_answer: Final,
}

impl IntegratedItem {
    /// The step with this id.
    #[must_use]
    pub fn step(&self, id: &str) -> Option<&Step> {
        self.steps.iter().find(|step| step.id.as_str() == id)
    }

    /// Every knowledge point the item exercises, in first-seen order.
    #[must_use]
    pub fn skills(&self) -> Vec<Slug> {
        let mut skills: Vec<Slug> = Vec::new();
        let steps = self.steps.iter().map(|step| &step.skills);
        for list in steps.chain(std::iter::once(&self.final_answer.skills)) {
            for skill in list {
                if !skills.iter().any(|held| held == skill) {
                    skills.push(skill.clone());
                }
            }
        }
        skills
    }

    /// The stable digest of the served statement (D-F9 `item_digest`).
    ///
    /// It hashes the id, the scenario, and every prompt, so an edit to what the
    /// learner reads gives a new digest and the exposure counts as first again.
    #[must_use]
    pub fn digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.id.as_str().as_bytes());
        hasher.update(b"\n");
        hasher.update(self.scenario.as_bytes());
        for step in &self.steps {
            hasher.update(b"\n");
            hasher.update(step.id.as_str().as_bytes());
            hasher.update(b"\t");
            hasher.update(step.ask.prompt.as_bytes());
        }
        hasher.update(b"\n");
        hasher.update(self.final_answer.ask.prompt.as_bytes());
        let mut hex = String::with_capacity(ITEM_DIGEST_LEN);
        for byte in hasher.finalize().iter().take(ITEM_DIGEST_LEN.div_ceil(2)) {
            hex.push_str(&format!("{byte:02x}"));
        }
        hex.truncate(ITEM_DIGEST_LEN);
        hex
    }

    /// True when this item covers every component topic of `components`.
    ///
    /// The multi-step dispatch of D-F10 asks this question: it serves the item
    /// when one covers the component set, and it keeps the per-component path
    /// when none does.
    #[must_use]
    pub fn covers(&self, components: &[String]) -> bool {
        !components.is_empty()
            && components.iter().all(|wanted| {
                self.component_topics
                    .iter()
                    .any(|held| held.as_str() == wanted)
            })
    }
}
