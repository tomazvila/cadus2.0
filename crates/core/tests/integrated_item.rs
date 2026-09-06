//! The authored shape of an integrated task, its rules, and its secrecy (D-F10).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cadus_core::integrated::{IntegratedItem, IntegratedSet, check_item, hint, view_of};

/// One complete, valid item: a service window with a staffing lower bound.
pub const WORKFORCE: &str = r#"
id: integrated-workforce-window
title: Staff the Saturday clinic window
course: foundations
topic: measurement-units
component_topics: [measurement-units, expressions-equations]
domain: workforce_capacity
scenario: >
  A clinic opens a 4 hour service window on Saturday. 96 patients are booked.
  One nurse needs 15 minutes for one patient and takes no break in the window.
given:
  - label: Service window
    value: 4 h
  - label: Booked patients
    value: 96
  - label: Time per patient
    value: 15 min
    note: The time holds for every patient; a longer visit changes feasibility.
method:
  prompt: Which method decides the number of nurses?
  options:
    - id: person-minutes
      label: Divide the total person-minutes by the minutes one nurse works.
      correct: true
      why: The window gives every nurse the same minutes, so the ratio is the count.
    - id: patients-per-hour
      label: Divide the patients by the hours of the window.
      correct: false
      why: That gives patients per hour, not a count of nurses.
steps:
  - id: total-person-minutes
    ask:
      prompt: How many person-minutes of work does the window hold?
      answer: "1440"
      unit: person-minutes
      hints:
        - Every patient needs the same time.
        - Multiply the count of patients by the minutes of one patient.
    skills: [unit-rates/kp1]
  - id: minutes-per-nurse
    ask:
      prompt: How many minutes does one nurse work in the window?
      answer: "240"
      unit: min
      hints:
        - The window is stated in hours.
    skills: [metric-unit-conversion/kp1]
final:
  ask:
    prompt: What is the smallest number of nurses that clears the booking?
    answer: "6"
    unit: nurses
    hints:
      - Divide the person-minutes by the minutes of one nurse.
  interpretation: >
    6 nurses clear 1440 person-minutes in the window. 5 nurses leave 240
    person-minutes of work, so the clinic overruns the window.
  skills: [unit-rates/kp1]
"#;

fn item() -> IntegratedItem {
    serde_norway::from_str(WORKFORCE).expect("the fixture parses")
}

#[test]
fn a_complete_item_parses_and_breaks_no_rule() {
    let item = item();
    assert_eq!(item.steps.len(), 2);
    assert_eq!(item.domain.as_str(), "workforce_capacity");
    assert!(item.given[2].note.is_some());
    assert!(check_item(&item).is_empty(), "{:?}", check_item(&item));
}

#[test]
fn the_view_holds_no_answer_and_no_method_flag() {
    let item = item();
    let view = view_of(&item);
    let wire = serde_json::to_string(&view).expect("the view serializes");
    for secret in [
        "1440",
        "240",
        "\"6\"",
        "correct",
        "clear 1440",
        "accept_also",
    ] {
        assert!(!wire.contains(secret), "the view leaked {secret}: {wire}");
    }
    assert_eq!(view.steps.len(), 2);
    assert_eq!(view.steps[0].ask.hints_available, 2);
    assert_eq!(view.method.expect("a method choice").options.len(), 2);
    assert_eq!(
        view.skills,
        ["unit-rates/kp1", "metric-unit-conversion/kp1"]
    );
}

#[test]
fn the_digest_follows_the_statement_the_learner_reads() {
    let item = item();
    let same = item.digest();
    assert_eq!(same.len(), 16);
    assert_eq!(item.digest(), same);
    let mut edited = item.clone();
    edited.scenario.push_str(" The clinic adds one hour.");
    assert_ne!(edited.digest(), same);
    // An edit to a private field is not an edit to the statement.
    let mut private = item.clone();
    private.final_answer.interpretation = "another wording".into();
    assert_eq!(private.digest(), same);
}

#[test]
fn hints_come_one_rung_at_a_time_and_the_answer_is_not_a_rung() {
    let item = item();
    assert_eq!(
        hint(&item, "total-person-minutes", 0),
        Some("Every patient needs the same time.")
    );
    assert!(hint(&item, "total-person-minutes", 2).is_none());
    assert!(hint(&item, "no-such-step", 0).is_none());
    assert!(hint(&item, "final", 0).is_some());
    for step in &item.steps {
        for rung in &step.ask.hints {
            assert!(!rung.contains(&step.ask.answer));
        }
    }
}

#[test]
fn a_broken_item_is_reported_and_never_reaches_the_set() {
    let mut item = item();
    item.steps.truncate(1);
    item.final_answer.ask.answer = "not a number(".into();
    let findings = check_item(&item);
    let codes: Vec<&str> = findings.iter().map(|f| f.code.as_str()).collect();
    assert!(codes.contains(&"integrated_steps"), "{codes:?}");
    assert!(codes.contains(&"integrated_answer"), "{codes:?}");
    let set = IntegratedSet::from_items(vec![item]);
    assert!(set.items().is_empty());
    assert!(set.findings().iter().any(|finding| finding.fatal));
}

#[test]
fn a_method_choice_that_decides_nothing_is_refused() {
    let mut item = item();
    for option in &mut item
        .method
        .as_mut()
        .expect("the fixture offers a method")
        .options
    {
        option.correct = true;
    }
    let codes: Vec<String> = check_item(&item)
        .into_iter()
        .map(|finding| finding.message)
        .collect();
    assert!(
        codes
            .iter()
            .any(|message| message.contains("decides nothing")),
        "{codes:?}"
    );
}

#[test]
fn the_multi_step_dispatch_asks_for_cover_of_the_component_set() {
    let set = IntegratedSet::from_items(vec![item()]);
    assert_eq!(set.items().len(), 1);
    let served = set
        .for_components(&["measurement-units".into(), "expressions-equations".into()])
        .expect("the item covers both components");
    assert_eq!(served.id.as_str(), "integrated-workforce-window");
    assert!(set.for_components(&["measurement-units".into()]).is_some());
    // A component the item does not integrate keeps the old per-component path.
    assert!(
        set.for_components(&["measurement-units".into(), "linear-graphs".into()])
            .is_none()
    );
    assert!(set.for_components(&[]).is_none());
    assert_eq!(set.for_topic("measurement-units").len(), 1);
    assert!(set.get("integrated-workforce-window").is_some());
}

#[test]
fn a_repeated_item_id_drops_the_later_item() {
    let set = IntegratedSet::from_items(vec![item(), item()]);
    assert_eq!(set.items().len(), 1);
    assert!(
        set.findings()
            .iter()
            .any(|finding| finding.code == "integrated_item_id")
    );
}
