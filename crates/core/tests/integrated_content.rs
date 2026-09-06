//! The hand-authored Foundations integrated set loads, checks, and stays secret
//! (D-F10, f18).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;

use cadus_core::curriculum::load_curriculum;
use cadus_core::integrated::{IntegratedSet, Submission, grade, view_of};

use common::paths::curriculum_root;

fn authored() -> IntegratedSet {
    IntegratedSet::load(&curriculum_root())
}

#[test]
fn every_authored_item_loads_with_no_fatal_finding() {
    let set = authored();
    let fatal: Vec<&str> = set
        .findings()
        .iter()
        .filter(|finding| finding.fatal)
        .map(|finding| finding.message.as_str())
        .collect();
    assert!(fatal.is_empty(), "{fatal:?}");
    assert!(
        set.items().len() >= 6,
        "the Foundations set covers six areas: {}",
        set.items().len()
    );
}

#[test]
fn the_set_covers_every_area_the_plan_names() {
    let set = authored();
    let domains: BTreeSet<&str> = set
        .items()
        .iter()
        .map(|item| item.domain.as_str())
        .collect();
    for area in [
        "rates_units",
        "percentages_ratios",
        "algebraic_constraints",
        "graph_interpretation",
        "geometry",
        "workforce_capacity",
    ] {
        assert!(domains.contains(area), "no authored item for {area}");
    }
    // The workforce set is more than one example: person-minutes, a service
    // window, a staffing lower bound, and an assumption that changes it.
    let workforce = set
        .items()
        .iter()
        .filter(|item| item.domain.as_str() == "workforce_capacity")
        .count();
    assert!(workforce >= 2, "the workforce set holds {workforce} items");
}

#[test]
fn every_authored_id_stands_in_the_curriculum() {
    let (graph, _) = load_curriculum(&curriculum_root()).expect("the curriculum loads");
    let set = authored();
    let findings: Vec<String> = set
        .check_curriculum(&graph)
        .into_iter()
        .map(|finding| finding.message)
        .collect();
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn every_authored_item_is_a_scenario_and_not_a_component_drill() {
    let set = authored();
    for item in set.items() {
        let id = item.id.as_str();
        assert!(item.steps.len() >= 2, "{id} has too few steps");
        assert!(item.given.len() >= 3, "{id} states too few quantities");
        assert!(
            item.scenario.split_whitespace().count() >= 20,
            "{id} has no real scenario"
        );
        assert!(
            item.given.iter().any(|given| given.note.is_some()),
            "{id} names no assumption"
        );
        assert!(item.method.is_some(), "{id} asks for no method choice");
        assert!(
            item.component_topics.len() >= 2,
            "{id} integrates one topic only"
        );
        assert!(
            !item.final_answer.interpretation.trim().is_empty(),
            "{id} interprets nothing"
        );
    }
}

#[test]
fn no_authored_answer_reaches_the_served_view() {
    let set = authored();
    for item in set.items() {
        let wire = serde_json::to_string(&view_of(item)).expect("the view serializes");
        let mut secrets: Vec<&str> = item
            .steps
            .iter()
            .map(|step| step.ask.answer.as_str())
            .collect();
        secrets.push(item.final_answer.ask.answer.as_str());
        for secret in secrets {
            // A number of the scenario may repeat in a prompt, so the check is
            // on the private text and on the shape: no answer field is emitted.
            assert!(
                !wire.contains(&format!("\"answer\":\"{secret}\"")),
                "{} leaked an answer field",
                item.id
            );
        }
        assert!(
            !wire.contains("accept_also"),
            "{} leaked alternates",
            item.id
        );
        let reading = item
            .final_answer
            .interpretation
            .split_whitespace()
            .take(6)
            .collect::<Vec<&str>>()
            .join(" ");
        assert!(!wire.contains(&reading), "{} leaked the reading", item.id);
        assert!(
            !wire.contains("\"correct\""),
            "{} leaked a method flag",
            item.id
        );
        assert!(
            !wire.contains("\"why\""),
            "{} leaked a method reason",
            item.id
        );
    }
}

#[test]
fn the_authored_answers_grade_as_correct() {
    let set = authored();
    for item in set.items() {
        let submission = Submission {
            method: item.method.as_ref().and_then(|method| {
                method
                    .options
                    .iter()
                    .find(|option| option.correct)
                    .map(|option| option.id.as_str().to_owned())
            }),
            steps: item
                .steps
                .iter()
                .map(|step| cadus_core::integrated::FieldResponse {
                    id: step.id.as_str().to_owned(),
                    answer: step.ask.answer.clone(),
                    hints_used: 0,
                })
                .collect(),
            final_answer: cadus_core::integrated::FieldResponse {
                id: "final".into(),
                answer: item.final_answer.ask.answer.clone(),
                hints_used: 0,
            },
            reasoning: None,
        };
        let result = grade(item, &submission);
        let id = item.id.as_str();
        assert!(result.solved, "{id} does not grade its own final answer");
        assert!(!result.ungraded, "{id} holds an answer the checker refuses");
        assert_eq!(result.correct_steps, result.total_steps, "{id} steps");
        assert!(
            result.method.as_ref().is_none_or(|method| method.correct),
            "{id} method"
        );
        assert!(!result.skills_credited.is_empty(), "{id} credits no skill");
    }
}
