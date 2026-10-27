//! Unit tests of the quiz round.

use super::*;
use crate::fire::testing::{T_US, graph, ladder, learned, topic};

#[test]
fn a_quiz_round_takes_its_length_and_course() {
    let cfg = Config::default();
    let ids_a: Vec<String> = (0..6).map(|index| format!("a{index}")).collect();
    let ids_b: Vec<String> = (0..6).map(|index| format!("b{index}")).collect();
    let tree = ladder(&[
        ("a", &[], ids_a.iter().map(|id| topic(id, &[])).collect()),
        ("b", &[], ids_b.iter().map(|id| topic(id, &[])).collect()),
    ]);
    let states: BTreeMap<String, TopicState> = ids_a
        .iter()
        .chain(ids_b.iter())
        .map(|id| (id.clone(), learned(0.8)))
        .collect();
    let draw = |length: usize, course: Option<&str>| {
        let mut sampler = SeededSampler::new(3);
        quiz_composer_for(
            &states,
            &tree,
            &cfg,
            T_US,
            &mut sampler,
            None,
            &QuizRequest { length, course },
        )
    };
    assert_eq!(draw(5, None).questions.len(), 5);
    let in_course = draw(10, Some("a"));
    assert_eq!(in_course.questions.len(), 6);
    assert!(in_course.questions.iter().all(|q| ids_a.contains(&q.topic)));
    assert_eq!(draw(20, None).questions.len(), 12);
    let mut sampler = SeededSampler::new(3);
    let default = quiz_composer(&states, &tree, &cfg, T_US, &mut sampler, None);
    assert_eq!(default.questions.len(), 8);
    assert_eq!(draw(5, None).topics(), draw(5, None).topics());
}

#[test]
fn the_quiz_draws_its_strata_and_reads_its_cadence() {
    let cfg = Config::default();
    let ids: Vec<String> = (0..9).map(|index| format!("t{index}")).collect();
    let tree = graph(ids.iter().map(|id| topic(id, &[])).collect());
    let states: BTreeMap<String, TopicState> =
        ids.iter().map(|id| (id.clone(), learned(0.8))).collect();
    let learned_at: BTreeMap<String, i64> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
            (
                id.clone(),
                T_US - i64::try_from(index).expect("small") * 4 * DAY_US,
            )
        })
        .collect();
    let mut sampler = SeededSampler::new(7);
    let plan = quiz_composer(&states, &tree, &cfg, T_US, &mut sampler, Some(&learned_at));
    assert_eq!(plan.questions.len(), 8);
    assert_eq!(plan.topics().len(), 8);
    assert_eq!(plan.total_time_budget_secs(), 8 * 45);
    let recent = plan
        .questions
        .iter()
        .filter(|q| q.stratum == "recent")
        .count();
    assert_eq!(recent, 4);
    let by_rep = quiz_composer(&states, &tree, &cfg, T_US, &mut sampler, None);
    assert!(by_rep.questions.iter().all(|q| q.stratum != "recent"));
    assert_eq!(sampler.sample(&ids, 20).len(), 9);

    assert_eq!(quiz_difficulty_target(1), QUIZ_DIFFICULTY_TARGETS[1]);
    let quiet = QuizState {
        last_at: utc_date(T_US),
        xp_since: 0,
        retake_pending: false,
    };
    assert!(!quiz_is_due(Some(&quiet), &states, &tree, &cfg, T_US, None));
    assert!(quiz_is_due(
        Some(&quiet),
        &states,
        &tree,
        &cfg,
        T_US + 9 * DAY_US,
        None
    ));
    let days = [utc_date(T_US + DAY_US).expect("a date")];
    assert!(!quiz_is_due(
        Some(&quiet),
        &states,
        &tree,
        &cfg,
        T_US + 9 * DAY_US,
        Some(&days)
    ));
    assert!(quiz_is_due(None, &states, &tree, &cfg, T_US, None));
    let retake = QuizState {
        retake_pending: true,
        ..quiet
    };
    assert_eq!(
        quiz_retake_available_at(Some(&retake)),
        utc_date(T_US + DAY_US)
    );
    assert!(!quiz_is_due(
        Some(&retake),
        &states,
        &tree,
        &cfg,
        T_US,
        None
    ));
    assert!(!quiz_is_due(
        Some(&retake),
        &states,
        &tree,
        &cfg,
        i64::MAX,
        None
    ));
    assert_eq!(quiz_budget(&tree, "ghost"), 0);
}
