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
            &QuizRequest {
                length,
                course,
                prereqs: false,
            },
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

use crate::curriculum::{AnswerKind, ExamWeights, Exemplar};
use crate::fire::testing::{knowledge_point, ladder_weighted};

const WEIGHTS: ExamWeights = ExamWeights {
    numbers: 15,
    models: 50,
    geometry: 20,
    data: 15,
};

#[test]
fn the_largest_remainder_fills_each_area() {
    assert_eq!(area_quotas(20, &WEIGHTS), [3, 10, 4, 3]);
    assert_eq!(area_quotas(10, &WEIGHTS), [2, 5, 2, 1]);
    // 5 items: the floors give 0, 2, 1, 0. The two items left go to the
    // largest remainders, which are the tied 15 percent areas.
    assert_eq!(area_quotas(5, &WEIGHTS), [1, 2, 1, 1]);
    assert_eq!(area_quotas(0, &WEIGHTS), [0, 0, 0, 0]);
    assert_eq!(largest_area(&WEIGHTS), 1);
}

/// Four checkpoints per area but models (twelve), each with a skill below it
/// in the course `s`. The skills carry no area.
fn exam_tree() -> (Curriculum, BTreeMap<String, TopicState>) {
    let layout: [(&str, usize); 4] = [("numbers", 4), ("models", 12), ("geometry", 5), ("data", 4)];
    let mut skills = Vec::new();
    let mut checkpoints = Vec::new();
    for (area, count) in layout {
        for number in 0..count {
            let skill = format!("sk-{area}-{number}");
            let id = format!("cp-{area}-{number}");
            skills.push(topic(&skill, &[]));
            let mut checkpoint = topic(&id, &[(skill.as_str(), 0.5, true)]);
            checkpoint.area = Some(area.to_owned());
            checkpoints.push(checkpoint);
        }
    }
    let states: BTreeMap<String, TopicState> = skills
        .iter()
        .chain(checkpoints.iter())
        .map(|item| (item.id.as_str().to_owned(), learned(0.8)))
        .collect();
    let tree = ladder_weighted(
        &[("s", &[], skills), ("v", &[], checkpoints)],
        Some(WEIGHTS),
    );
    (tree, states)
}

fn count_areas(tree: &Curriculum, plan: &QuizPlan) -> [usize; 4] {
    let map = area_map(tree, "v");
    let mut counts = [0_usize; 4];
    for question in &plan.questions {
        counts[map[&question.topic]] += 1;
    }
    counts
}

#[test]
fn a_skill_inherits_the_area_of_its_checkpoint() {
    let (tree, _) = exam_tree();
    let map = area_map(&tree, "v");
    assert_eq!(map["sk-models-3"], 1);
    assert_eq!(map["sk-data-0"], 3);
    assert_eq!(map["cp-geometry-2"], 2);
    assert_eq!(map.len(), 50);
}

#[test]
fn a_twenty_item_round_asks_three_ten_four_three_items_by_area() {
    let (tree, states) = exam_tree();
    let cfg = Config::default();
    for seed in 0..6 {
        let mut sampler = SeededSampler::new(seed);
        let plan = quiz_composer_for(
            &states,
            &tree,
            &cfg,
            T_US,
            &mut sampler,
            None,
            &QuizRequest {
                length: 20,
                course: Some("v"),
                prereqs: true,
            },
        );
        assert_eq!(plan.questions.len(), 20);
        assert_eq!(count_areas(&tree, &plan), [3, 10, 4, 3]);
        let mut seen: Vec<&str> = plan.topics();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 20, "no topic repeats");
    }
}

#[test]
fn a_short_area_backfills_from_the_other_areas() {
    let (tree, mut states) = exam_tree();
    // Only one numbers topic keeps a review history.
    states.retain(|id, _| !id.contains("numbers") || id == "cp-numbers-0");
    let mut sampler = SeededSampler::new(1);
    let plan = quiz_composer_for(
        &states,
        &tree,
        &Config::default(),
        T_US,
        &mut sampler,
        None,
        &QuizRequest {
            length: 20,
            course: Some("v"),
            prereqs: true,
        },
    );
    assert_eq!(plan.questions.len(), 20);
    assert_eq!(count_areas(&tree, &plan)[0], 1);
}

#[test]
fn the_ancestor_scope_adds_the_skills_of_other_courses() {
    let (tree, states) = exam_tree();
    let draw = |prereqs: bool| {
        let mut sampler = SeededSampler::new(4);
        quiz_composer_for(
            &states,
            &tree,
            &Config::default(),
            T_US,
            &mut sampler,
            None,
            &QuizRequest {
                length: 20,
                course: Some("v"),
                prereqs,
            },
        )
    };
    assert!(
        draw(false)
            .questions
            .iter()
            .all(|q| q.topic.starts_with("cp-"))
    );
    assert!(
        draw(true)
            .questions
            .iter()
            .any(|q| q.topic.starts_with("sk-"))
    );
    // The ancestor scope adds no topic that has no review history.
    let mut sampler = SeededSampler::new(4);
    let none: BTreeMap<String, TopicState> = BTreeMap::new();
    let plan = quiz_composer_for(
        &none,
        &tree,
        &Config::default(),
        T_US,
        &mut sampler,
        None,
        &QuizRequest {
            length: 20,
            course: Some("v"),
            prereqs: true,
        },
    );
    assert!(plan.questions.is_empty());
}

#[test]
fn a_plain_course_scope_keeps_the_strata_without_weights() {
    // The same states and seed give the same plan when the course has no weights.
    let ids: Vec<String> = (0..12).map(|index| format!("t{index}")).collect();
    let tree = ladder_weighted(
        &[("a", &[], ids.iter().map(|id| topic(id, &[])).collect())],
        None,
    );
    let states: BTreeMap<String, TopicState> =
        ids.iter().map(|id| (id.clone(), learned(0.8))).collect();
    let draw = |prereqs: bool| {
        let mut sampler = SeededSampler::new(9);
        quiz_composer_for(
            &states,
            &tree,
            &Config::default(),
            T_US,
            &mut sampler,
            None,
            &QuizRequest {
                length: 10,
                course: Some("a"),
                prereqs,
            },
        )
        .topics()
        .iter()
        .map(|id| (*id).to_owned())
        .collect::<Vec<_>>()
    };
    assert_eq!(draw(false), draw(true));
    assert_eq!(draw(false).len(), 10);
}

fn exemplar(problem: &str, written: bool) -> Exemplar {
    Exemplar {
        problem: problem.to_owned(),
        answer_contract: written.then_some(crate::answer::AnswerContract::Written),
        answer: "1".to_owned(),
        solution_sketch: None,
        visual: None,
        points: Some(1),
    }
}

/// A set topic `set` with the pools `sizes`; a pool item named `w..` is written.
fn set_tree(sizes: [usize; 4], weighted: bool) -> Curriculum {
    set_tree_with(sizes, weighted, 2)
}

/// Like [`set_tree`]; the items of pool four from position `written_from` are written.
fn set_tree_with(sizes: [usize; 4], weighted: bool, written_from: usize) -> Curriculum {
    let mut set = topic("set", &[]);
    set.answer_kind = AnswerKind::MultiStep;
    set.knowledge_points = sizes
        .iter()
        .enumerate()
        .map(|(position, size)| {
            let mut point = knowledge_point(&format!("kp{}", position + 1), &[]);
            point.exemplars = (0..*size)
                .map(|number| {
                    exemplar(
                        &format!("q{position}-{number}"),
                        position == 3 && number >= written_from,
                    )
                })
                .collect();
            point
        })
        .collect();
    ladder_weighted(&[("v", &[], vec![set])], weighted.then_some(WEIGHTS))
}

#[test]
fn a_paper_takes_ten_five_three_two_items_in_authored_order() {
    let tree = set_tree([10, 6, 6, 9], true);
    let form = paper_form(&tree, "set").expect("a set makes a paper");
    assert_eq!(form.len(), 20);
    assert_eq!(PAPER_FORM.iter().sum::<usize>(), 20);
    assert_eq!(
        form[0],
        PaperItem {
            kp: "kp1".into(),
            index: 0
        }
    );
    assert_eq!(
        form[9],
        PaperItem {
            kp: "kp1".into(),
            index: 9
        }
    );
    assert_eq!(
        form[10],
        PaperItem {
            kp: "kp2".into(),
            index: 0
        }
    );
    assert_eq!(
        form[15],
        PaperItem {
            kp: "kp3".into(),
            index: 0
        }
    );
    assert_eq!(
        form[19],
        PaperItem {
            kp: "kp4".into(),
            index: 1
        }
    );
    assert_eq!(exam_sets(&tree, "v"), ["set"]);
}

#[test]
fn a_paper_serves_a_written_item_last() {
    let tree = set_tree_with([10, 6, 6, 9], true, 0);
    let form = paper_form(&tree, "set").expect("a paper");
    assert_eq!(form.len(), 20);
    // The two written items of pool four stand at the end.
    assert_eq!(
        form[18],
        PaperItem {
            kp: "kp4".into(),
            index: 0
        }
    );
    assert_eq!(
        form[19],
        PaperItem {
            kp: "kp4".into(),
            index: 1
        }
    );
    assert_eq!(
        form[17],
        PaperItem {
            kp: "kp3".into(),
            index: 2
        }
    );
    // A short pool gives a shorter paper.
    let short = set_tree([4, 6, 6, 2], true);
    assert_eq!(paper_form(&short, "set").expect("a paper").len(), 14);
}

#[test]
fn a_paper_refuses_a_topic_that_is_no_exam_set() {
    let plain = set_tree([10, 6, 6, 9], false);
    assert_eq!(paper_form(&plain, "set"), Err(PaperRefusal::NotAnExamSet));
    assert_eq!(paper_form(&plain, "ghost"), Err(PaperRefusal::NotAnExamSet));
    assert!(exam_sets(&plain, "v").is_empty());
    // A topic with one knowledge point only.
    let mut topic_one = topic("one", &[]);
    topic_one.knowledge_points = vec![knowledge_point("kp1", &[])];
    let single = ladder_weighted(&[("v", &[], vec![topic_one])], Some(WEIGHTS));
    assert_eq!(paper_form(&single, "one"), Err(PaperRefusal::TooFewPoints));
    assert!(!PaperRefusal::TooFewPoints.message().is_empty());
}

#[test]
fn points_score_a_plain_round_as_before() {
    let plain: Vec<ItemPoints> = [true, true, false, true]
        .iter()
        .map(|right| item_points(None, None, *right))
        .collect();
    assert!((score_of(&plain) - 0.75).abs() < f64::EPSILON);
    // A multipart item with no points counts 1 and gets no partial credit.
    assert!((item_points(None, Some((2, 3)), false).earned).abs() < f64::EPSILON);
}

#[test]
fn points_weigh_an_item_and_give_partial_credit_by_part() {
    let items = [
        item_points(Some(1), None, true),
        item_points(Some(7), Some((2, 3)), false),
        item_points(Some(3), Some((0, 3)), false),
        item_points(Some(4), Some((3, 3)), true),
    ];
    let sum = sum_points(&items);
    assert!((sum.total - 15.0).abs() < 1e-9);
    assert!((round2(sum.earned) - 9.67).abs() < 1e-9);
    assert!((round2(score_of(&items)) - 0.64).abs() < 1e-9);
    assert!(score_of(&[]).abs() < f64::EPSILON);
}
