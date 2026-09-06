//! What an authored integrated task must satisfy before a learner sees it.
//!
//! The rules run at load time, in the same spirit as the curriculum checker: a
//! fatal finding drops the item from the served set, an advisory finding is
//! reported and keeps the item. Nothing here approves content: approval stays
//! with the human review path.

use crate::curriculum::{Curriculum, Finding, Slug};

use super::model::{Field, IntegratedItem, MethodChoice};

/// The smallest number of intermediate steps of an integrated task.
///
/// One step and a final answer is a two-part drill, not integration. D-F10 asks
/// for meaningful intermediate reasoning, so an item carries at least two.
pub const MIN_STEPS: usize = 2;

/// The smallest number of offered methods, when the author offers a choice.
pub const MIN_METHOD_OPTIONS: usize = 2;

/// Report every rule the item breaks.
///
/// The answer of every field is validated with its own contract, so a served
/// item can always be graded: an unparsable authored answer never reaches the
/// serve path.
#[must_use]
pub fn check_item(item: &IntegratedItem) -> Vec<Finding> {
    let mut findings = Vec::new();
    let id = item.id.as_str();
    if item.title.trim().is_empty() {
        findings.push(fatal(id, "integrated_title", "the item has no title"));
    }
    if item.scenario.trim().is_empty() {
        findings.push(fatal(id, "integrated_scenario", "the item has no scenario"));
    }
    if item.given.is_empty() {
        findings.push(fatal(
            id,
            "integrated_given",
            "the scenario states no quantity or constraint",
        ));
    }
    if item.component_topics.is_empty() {
        findings.push(fatal(
            id,
            "integrated_components",
            "the item names no component topic to integrate",
        ));
    }
    check_steps(item, &mut findings);
    if let Some(method) = &item.method {
        check_method(id, method, &mut findings);
    }
    check_field(id, "final", &item.final_answer.ask, &mut findings);
    if item.final_answer.interpretation.trim().is_empty() {
        findings.push(fatal(
            id,
            "integrated_interpretation",
            "the final answer states no interpretation",
        ));
    }
    if item.skills().is_empty() {
        findings.push(fatal(
            id,
            "integrated_skills",
            "no step names the knowledge point it exercises",
        ));
    }
    findings
}

/// Check the step list: the count, the ids, the fields, and the attribution.
fn check_steps(item: &IntegratedItem, findings: &mut Vec<Finding>) {
    let id = item.id.as_str();
    if item.steps.len() < MIN_STEPS {
        findings.push(fatal(
            id,
            "integrated_steps",
            format!(
                "the item has {} intermediate steps; {MIN_STEPS} is the minimum",
                item.steps.len()
            ),
        ));
    }
    let mut seen: Vec<&Slug> = Vec::new();
    for step in &item.steps {
        let step_id = step.id.as_str();
        if seen.contains(&&step.id) {
            findings.push(fatal(
                id,
                "integrated_step_id",
                format!("two steps share the id {step_id}"),
            ));
        }
        seen.push(&step.id);
        check_field(id, step_id, &step.ask, findings);
        if step.skills.is_empty() {
            findings.push(advisory(
                id,
                "integrated_step_skills",
                format!("step {step_id} names no knowledge point"),
            ));
        }
    }
}

/// Check one prompt-and-answer field.
fn check_field(id: &str, field_id: &str, field: &Field, findings: &mut Vec<Finding>) {
    if field.prompt.trim().is_empty() {
        findings.push(fatal(
            id,
            "integrated_prompt",
            format!("{field_id} has no prompt"),
        ));
    }
    for (index, answer) in std::iter::once(&field.answer)
        .chain(field.accept_also.iter())
        .enumerate()
    {
        if let Err(reason) = field.contract.validate_expected(answer) {
            findings.push(fatal(
                id,
                "integrated_answer",
                format!(
                    "{field_id} answer {index} ({answer}) fails its contract: {}",
                    reason.reason
                ),
            ));
        }
    }
    if field.hints.is_empty() {
        findings.push(advisory(
            id,
            "integrated_hints",
            format!("{field_id} has no hint ladder to fade"),
        ));
    }
    if field.hints.iter().any(|hint| hint.trim().is_empty()) {
        findings.push(fatal(
            id,
            "integrated_hints",
            format!("{field_id} has an empty hint"),
        ));
    }
    if field.hints.iter().any(|hint| hint.contains(&field.answer)) {
        findings.push(advisory(
            id,
            "integrated_hint_leak",
            format!("a hint of {field_id} holds the answer text"),
        ));
    }
}

/// Check the method choice: enough options, unique ids, and a real decision.
fn check_method(id: &str, method: &MethodChoice, findings: &mut Vec<Finding>) {
    if method.prompt.trim().is_empty() {
        findings.push(fatal(
            id,
            "integrated_method",
            "the method choice has no prompt",
        ));
    }
    if method.options.len() < MIN_METHOD_OPTIONS {
        findings.push(fatal(
            id,
            "integrated_method",
            format!("the method choice offers {} options", method.options.len()),
        ));
    }
    let mut seen: Vec<&Slug> = Vec::new();
    for option in &method.options {
        if seen.contains(&&option.id) {
            findings.push(fatal(
                id,
                "integrated_method_id",
                format!("two method options share the id {}", option.id),
            ));
        }
        seen.push(&option.id);
    }
    if !method.options.iter().any(|option| option.correct) {
        findings.push(fatal(
            id,
            "integrated_method",
            "no offered method solves the scenario",
        ));
    }
    if method.options.iter().all(|option| option.correct) {
        findings.push(fatal(
            id,
            "integrated_method",
            "every offered method is correct, so the choice decides nothing",
        ));
    }
}

/// Report every id of the item the curriculum arena does not hold.
///
/// The arena is the authority on topics and knowledge points, so the check runs
/// against the loaded graph and not against a second list.
#[must_use]
pub fn check_against(item: &IntegratedItem, graph: &Curriculum) -> Vec<Finding> {
    let id = item.id.as_str();
    let mut findings = Vec::new();
    if graph.idx_of(item.topic.as_str()).is_none() {
        findings.push(fatal(
            id,
            "integrated_topic",
            format!("no topic {} in the curriculum", item.topic),
        ));
    }
    for component in &item.component_topics {
        if graph.idx_of(component.as_str()).is_none() {
            findings.push(fatal(
                id,
                "integrated_component",
                format!("no component topic {component} in the curriculum"),
            ));
        }
    }
    for skill in item.skills() {
        if !holds_kp(graph, skill.as_str()) {
            findings.push(fatal(
                id,
                "integrated_skill",
                format!("no knowledge point {skill} in the curriculum"),
            ));
        }
    }
    findings
}

/// True when some topic of the arena holds the knowledge point `kp`.
fn holds_kp(graph: &Curriculum, kp: &str) -> bool {
    graph.topics().iter().any(|topic| {
        topic
            .knowledge_points
            .iter()
            .any(|point| point.id.as_str() == kp)
    })
}

/// A finding that drops the item from the served set.
fn fatal(id: &str, code: &str, message: impl AsRef<str>) -> Finding {
    Finding::new(code, format!("{id}: {}", message.as_ref()))
}

/// A finding that is reported and keeps the item.
fn advisory(id: &str, code: &str, message: impl AsRef<str>) -> Finding {
    Finding::advisory(code, format!("{id}: {}", message.as_ref()))
}
