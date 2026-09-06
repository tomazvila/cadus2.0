//! What the client is allowed to see before it answers (Hard Rule 1).
//!
//! The view is built by naming every field it emits. An authored answer, an
//! `accept_also` form, the `correct` flag of a method option, and the final
//! interpretation have no field here, so none of them can reach a browser by
//! accident. The hint ladder is served one rung at a time by
//! [`hint`](super::hint), never as a list.

use serde::{Deserialize, Serialize};

use super::model::{Field, Given, IntegratedItem};

/// One field of the task, without its answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldView {
    /// The question.
    pub prompt: String,
    /// The unit the learner writes, when the author names one.
    pub unit: Option<String>,
    /// How many hints the ladder holds. The text stays on the server.
    pub hints_available: usize,
}

impl FieldView {
    /// The client-safe half of `field`.
    fn of(field: &Field) -> Self {
        Self {
            prompt: field.prompt.clone(),
            unit: field.unit.clone(),
            hints_available: field.hints.len(),
        }
    }
}

/// One step of the task, without its answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepView {
    /// The step id the answer submission names.
    pub id: String,
    /// The question and the hint count.
    pub ask: FieldView,
}

/// One offered method, without the flag that says whether it works.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodOptionView {
    /// The id the learner submits.
    pub id: String,
    /// The method, in words.
    pub label: String,
}

/// The method choice, without the answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodView {
    /// The question that asks for the method.
    pub prompt: String,
    /// The offered methods, in author order.
    pub options: Vec<MethodOptionView>,
}

/// One integrated task as ONE served problem (D-F10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegratedView {
    /// The item id the answer submission names.
    pub item_id: String,
    /// The digest of the served statement (D-F9).
    pub item_digest: String,
    /// The title.
    pub title: String,
    /// The topic the outcome records against.
    pub topic: String,
    /// The application area.
    pub domain: String,
    /// The scenario, in words.
    pub scenario: String,
    /// The quantities and the constraints.
    pub given: Vec<Given>,
    /// The method choice, when the item offers one.
    pub method: Option<MethodView>,
    /// The intermediate steps.
    pub steps: Vec<StepView>,
    /// The final question.
    pub final_ask: FieldView,
    /// The knowledge points the task exercises, for the skill panel.
    pub skills: Vec<String>,
}

/// Build the client-safe view of `item`.
#[must_use]
pub fn view_of(item: &IntegratedItem) -> IntegratedView {
    IntegratedView {
        item_id: item.id.as_str().to_owned(),
        item_digest: item.digest(),
        title: item.title.clone(),
        topic: item.topic.as_str().to_owned(),
        domain: item.domain.as_str().to_owned(),
        scenario: item.scenario.clone(),
        given: item.given.clone(),
        method: item.method.as_ref().map(|method| MethodView {
            prompt: method.prompt.clone(),
            options: method
                .options
                .iter()
                .map(|option| MethodOptionView {
                    id: option.id.as_str().to_owned(),
                    label: option.label.clone(),
                })
                .collect(),
        }),
        steps: item
            .steps
            .iter()
            .map(|step| StepView {
                id: step.id.as_str().to_owned(),
                ask: FieldView::of(&step.ask),
            })
            .collect(),
        final_ask: FieldView::of(&item.final_answer.ask),
        skills: item
            .skills()
            .iter()
            .map(|skill| skill.as_str().to_owned())
            .collect(),
    }
}
