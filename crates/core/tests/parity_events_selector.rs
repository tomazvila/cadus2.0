//! U5 acceptance, part 3: the 1.0 `compose_session` plans of the 10 seeded
//! states, and the live oracle that re-derives them (R5, D4).
//!
//! `tests/fixtures/events/selector_1_0.json` holds the 1.0 plan of each seeded
//! state (`scripts/oracle/dump_selector_1_0.py`). No expectation calls the code
//! under test. A plan whose Rust rows differ from the 1.0 rows is a RUST BUG.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;

use cadus_core::event::{Event, TaskType, Timestamp};
use cadus_core::projector::PROJECTOR_VERSION;
use cadus_core::selector::{SeededSampler, SessionContext, compose_session};
use common::events::{cfg, stream, tree};
use common::parity::{SelectorIndex, TaskRow, fold_in, oracle_index, selector_index};

/// The `config_hash` of the default config (spec section 9).
const CONFIG_HASH: &str = "797575e985c12149";

/// The task cap `dump_selector_1_0.py` composed with.
///
/// It is 40, above the longest recorded plan, so the comparison reaches the quiz,
/// multi-step, and drill sections of `compose_session`. At the session cap of 8
/// every seeded plan is already full of remediation, review, and lesson tasks
/// (M3 review round 1, finding #13).
const SELECTOR_N: usize = 40;

/// The longest plan any seeded state records, which is below [`SELECTOR_N`].
///
/// A plan AT the cap would mean the cap truncates it again, so the whole-plan
/// comparison would end early once more.
const SELECTOR_LONGEST_PLAN: usize = 14;

/// The number of seeded selector states.
const SELECTOR_SEEDS: usize = 10;

// --------------------------------------------------------------------------- //
// The selector oracle (U4's 1.0 `compose_session` reference)
// --------------------------------------------------------------------------- //

/// The last `enrolled` course of a stream, which the oracle passed as `course_id`.
fn enrolled_course(events: &[Event]) -> Option<String> {
    let mut course = None;
    for event in events {
        if let Event::Enrolled(body) = event {
            course = Some(body.course.as_str().to_owned());
        }
    }
    course
}

/// The 2.0 task row in the shape `selector_1_0.json` records.
fn task_rows(plan_tasks: &[cadus_core::selector::Task]) -> Vec<TaskRow> {
    plan_tasks
        .iter()
        .map(|task| {
            // Trap T11: the quiz sample is a documented non-parity, so a quiz row
            // carries no topic and no size on either side, and compares by presence
            // only. `topic`, `n_problems`, and `time_budget_secs` all read the
            // sampled questions.
            let quiz = task.task_type == TaskType::Quiz;
            let topic = if quiz { None } else { task.topic.clone() };
            let n_problems = if quiz { None } else { task.n_problems };
            let budget = if quiz { None } else { task.time_budget_secs };
            (
                task.task_type.as_str().to_owned(),
                topic,
                task.is_remediation,
                task.nearly_due,
                n_problems,
                budget,
            )
        })
        .collect()
}

#[test]
fn the_selector_index_holds_the_pinned_metadata() {
    let index = selector_index();
    assert_eq!(index.oracle, "scripts/oracle/dump_selector_1_0.py");
    assert_eq!(index.n, SELECTOR_N);
    assert_eq!(index.config_hash, CONFIG_HASH);
    assert_eq!(index.projector_version, PROJECTOR_VERSION);
    assert_eq!(index.states.len(), SELECTOR_SEEDS);
    // Every section of `compose_session` reaches the comparison. Without the drill
    // and the multi-step rows the cap truncated the plan (finding #13).
    let kinds: BTreeSet<&str> = index
        .states
        .iter()
        .flat_map(|state| state.tasks.iter().map(|task| task.0.as_str()))
        .collect();
    for kind in ["review", "lesson", "quiz", "multi-step", "drill"] {
        assert!(
            kinds.contains(kind),
            "no seeded state records a `{kind}` task"
        );
    }
    for (position, state) in index.states.iter().enumerate() {
        assert_eq!(state.seed, position + 1);
        assert_eq!(state.stream, format!("stream_{}.jsonl", state.seed));
        assert_eq!(state.session_id, format!("s{}", state.seed));
        // A plan AT the cap is a truncated plan, and a truncated plan hides
        // whatever `compose_session` appends last (finding #13).
        assert!(
            state.tasks.len() < SELECTOR_N,
            "{}: the plan fills the cap, so the comparison stops early",
            state.stream
        );
        assert!(
            state.tasks.len() <= SELECTOR_LONGEST_PLAN,
            "{}: a plan is longer than the recorded longest plan",
            state.stream
        );
    }
}

#[test]
fn compose_session_reproduces_the_1_0_plan_of_every_seeded_state() {
    let cfg = cfg();
    for state in &selector_index().states {
        let events = stream(&state.stream);
        let model = fold_in(&events, None);
        let course = enrolled_course(&events);
        assert_eq!(
            course.as_deref(),
            state.course_id.as_deref(),
            "{}: the enrolled course differs from the oracle's",
            state.stream
        );

        // The oracle composed at the last event's timestamp, which is the fold's
        // own `t_ref`. Both sides read that instant off the same committed stream.
        let t_us = events
            .iter()
            .map(|event| event.ts().micros())
            .max()
            .expect("the stream is not empty");
        assert_eq!(
            Timestamp::from_micros(t_us).to_wire_string().unwrap(),
            state.t,
            "{}: the composition instant differs from the oracle's",
            state.stream
        );

        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let mut sampler = SeededSampler::new(state.seed as u64);
        let ctx = SessionContext::default()
            .with_session_id(&state.session_id)
            .with_course(course.as_deref())
            .with_pending_remediation(&model.pending_remediation)
            .with_quiz_state(Some(&model.quiz))
            .with_limit(Some(SELECTOR_N));
        let plan = compose_session(&model.topics, tree(), cfg, t_us, &mut sampler, &ctx);

        let actual = task_rows(&plan.tasks);
        assert_eq!(
            actual, state.tasks,
            "RUST BUG: {} (seed {}) composes a different session than 1.0.\n  \
             rust {actual:?}\n  1.0  {:?}\n\
             Do NOT change the oracle or the fixture. Fix the port.",
            state.stream, state.seed, state.tasks
        );
    }
}

#[test]
fn the_live_oracle_reproduces_the_committed_selector_plans() {
    let Ok(python) = std::env::var("CADUS_ORACLE_PYTHON") else {
        eprintln!("skipped: CADUS_ORACLE_PYTHON is not set");
        return;
    };
    let text = oracle_index(
        &python,
        "scripts/oracle/dump_selector_1_0.py",
        "selector_live.json",
    );
    let live: SelectorIndex = serde_json::from_str(&text).expect("the live index parses");
    let committed = selector_index();
    assert_eq!(live.states.len(), committed.states.len());
    for (fresh, old) in live.states.iter().zip(committed.states.iter()) {
        assert_eq!(fresh.stream, old.stream);
        assert_eq!(
            fresh.tasks, old.tasks,
            "{}: the live 1.0 selector moved away from the committed plan",
            old.stream
        );
    }
}
