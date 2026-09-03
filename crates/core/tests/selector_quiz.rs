//! Selector part 2: the quiz strata, the quiz cadence and the quiz boundaries.
//!
//! The selector of spec section 6, pinned against the ORDER assertions of the
//! 1.0 suite (`tests/test_selector.py`, `tests/test_gap_fill.py`).
//!
//! Every expected value here is a LITERAL taken from the 1.0 test that pins it.
//! Nothing is re-derived from the code under test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;

use cadus_core::curriculum::model::Topic;
use cadus_core::learner::{QuizState, TopicState};
use cadus_core::selector::{
    DIFFICULTY_TARGET, QUIZ_RECENT_DAYS, QUIZ_RETAKE_DELAY_DAYS, quiz_budget, quiz_composer,
    quiz_difficulty_target, quiz_is_due, quiz_retake_available_at, utc_date,
};
use common::selector::{cfg, graph_of, id_set, learned, sampler, ten_learned, topic};
use common::{T_US, days};

// --------------------------------------------------------------------------- //
// Quiz strata and cadence (test_selector.py:296-327, 656-739)
// --------------------------------------------------------------------------- //

#[test]
fn quiz_composer_strata_and_time_budget() {
    let recent_ids = ["counting", "addition", "subtraction", "multiplication"];
    let mid_ids = ["division", "place-value"];
    let old_ids = ["fraction-basics", "equivalent-fractions"];
    let all: Vec<&str> = recent_ids
        .iter()
        .chain(mid_ids.iter())
        .chain(old_ids.iter())
        .copied()
        .collect();
    let graph = graph_of(
        all.iter()
            .map(|id| topic(id).build())
            .collect::<Vec<Topic>>(),
        &[],
    );
    let states: BTreeMap<String, TopicState> = all
        .iter()
        .map(|id| ((*id).to_owned(), learned(0.8)))
        .collect();
    let mut learned_at: BTreeMap<String, i64> = BTreeMap::new();
    for id in recent_ids {
        learned_at.insert(id.to_owned(), T_US - days(5));
    }
    for id in mid_ids {
        learned_at.insert(id.to_owned(), T_US - days(30));
    }
    for id in old_ids {
        learned_at.insert(id.to_owned(), T_US - days(60));
    }
    let cfg = cfg();
    let plan = quiz_composer(
        &states,
        &graph,
        &cfg,
        T_US,
        &mut sampler(7),
        Some(&learned_at),
    );
    assert_eq!(
        i64::try_from(plan.questions.len()).unwrap(),
        cfg.quiz.questions
    );
    let mut by_stratum: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for question in &plan.questions {
        by_stratum
            .entry(question.stratum)
            .or_default()
            .insert(question.topic.clone());
        // The authored expected time is 30 s, so the budget is 45 s.
        assert_eq!(question.time_budget_secs, 45);
    }
    assert_eq!(by_stratum.get("recent").map(BTreeSet::len), Some(4));
    assert_eq!(by_stratum.get("mid").map(BTreeSet::len), Some(2));
    assert_eq!(by_stratum.get("old").map(BTreeSet::len), Some(2));
    assert!(
        by_stratum
            .get("recent")
            .unwrap()
            .is_subset(&id_set(&recent_ids))
    );
    assert!(by_stratum.get("mid").unwrap().is_subset(&id_set(&mid_ids)));
    assert!(by_stratum.get("old").unwrap().is_subset(&id_set(&old_ids)));
}

#[test]
fn quiz_sampling_is_seed_deterministic() {
    let (graph, states) = ten_learned();
    let cfg = cfg();
    let first = quiz_composer(&states, &graph, &cfg, T_US, &mut sampler(42), None);
    let second = quiz_composer(&states, &graph, &cfg, T_US, &mut sampler(42), None);
    // `tests/test_selector.py:322-327`, with 2.0's own generator (trap T11).
    assert_eq!(first.topics(), second.topics());
    assert_eq!(first.questions.len(), 8);
}

#[test]
fn failed_quiz_retake_respects_delay() {
    let (graph, states) = ten_learned();
    let cfg = cfg();
    let failed_day = NaiveDate::from_ymd_opt(2026, 7, 14).unwrap();
    let failed = QuizState {
        last_at: Some(failed_day),
        xp_since: 0,
        retake_pending: true,
    };
    // `tests/test_selector.py:656-669`.
    assert!(!quiz_is_due(
        Some(&failed),
        &states,
        &graph,
        &cfg,
        T_US,
        None
    ));
    assert!(!quiz_is_due(
        Some(&failed),
        &states,
        &graph,
        &cfg,
        T_US + 6 * 3_600_000_000,
        None
    ));
    assert!(quiz_is_due(
        Some(&failed),
        &states,
        &graph,
        &cfg,
        T_US + days(QUIZ_RETAKE_DELAY_DAYS) + 3_600_000_000,
        None
    ));
    let ok = QuizState {
        last_at: Some(failed_day),
        xp_since: 0,
        retake_pending: false,
    };
    assert!(!quiz_is_due(
        Some(&ok),
        &states,
        &graph,
        &cfg,
        T_US + days(QUIZ_RETAKE_DELAY_DAYS) + 3_600_000_000,
        None
    ));
}

#[test]
fn quiz_difficulty_adapts_up_within_a_bounded_band() {
    // `tests/test_selector.py:693-698`.
    let base = quiz_difficulty_target(0);
    let once = quiz_difficulty_target(1);
    let twice = quiz_difficulty_target(2);
    assert_eq!(base, DIFFICULTY_TARGET);
    assert_eq!(base, "80-85% expected accuracy");
    assert_eq!(once, "75-80% expected accuracy");
    assert_eq!(twice, "70-75% expected accuracy");
    assert_eq!(quiz_difficulty_target(99), twice);
    assert_eq!(quiz_difficulty_target(-5), base);
}

#[test]
fn quiz_cadence_uses_activity_days() {
    let (graph, states) = ten_learned();
    let cfg = cfg();
    let last = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
    let quiz = QuizState {
        last_at: Some(last),
        xp_since: 0,
        retake_pending: false,
    };
    let now = common::noon_us(2026, 7, 30);
    // `tests/test_selector.py:722-739`.
    assert!(quiz_is_due(Some(&quiz), &states, &graph, &cfg, now, None));
    let few = [
        NaiveDate::from_ymd_opt(2026, 7, 5).unwrap(),
        NaiveDate::from_ymd_opt(2026, 7, 12).unwrap(),
        NaiveDate::from_ymd_opt(2026, 7, 20).unwrap(),
    ];
    assert!(!quiz_is_due(
        Some(&quiz),
        &states,
        &graph,
        &cfg,
        now,
        Some(&few)
    ));
    let enough: Vec<NaiveDate> = (2..2 + u32::try_from(cfg.quiz.cadence_days).unwrap())
        .map(|day| NaiveDate::from_ymd_opt(2026, 7, day).unwrap())
        .collect();
    assert_eq!(i64::try_from(enough.len()).unwrap(), cfg.quiz.cadence_days);
    assert!(quiz_is_due(
        Some(&quiz),
        &states,
        &graph,
        &cfg,
        now,
        Some(&enough)
    ));
    let repeats = vec![NaiveDate::from_ymd_opt(2026, 7, 5).unwrap(); 10];
    assert!(!quiz_is_due(
        Some(&quiz),
        &states,
        &graph,
        &cfg,
        now,
        Some(&repeats)
    ));
    let mut out_of_window = vec![last];
    for day in 1..12 {
        out_of_window.push(NaiveDate::from_ymd_opt(2026, 8, day).unwrap());
    }
    assert!(!quiz_is_due(
        Some(&quiz),
        &states,
        &graph,
        &cfg,
        now,
        Some(&out_of_window)
    ));
}

#[test]
fn quiz_budget_is_the_expected_time_times_one_and_a_half() {
    let graph = graph_of(
        vec![
            topic("fast").expected(30).build(),
            topic("slow").expected(60).build(),
            // 45 * 1.5 == 67.5, which rounds half to EVEN: 68.
            topic("half").expected(45).build(),
        ],
        &[],
    );
    // `selector.py:676-678`, with the banker's rounding of trap T3.
    assert_eq!(quiz_budget(&graph, "fast"), 45);
    assert_eq!(quiz_budget(&graph, "slow"), 90);
    assert_eq!(quiz_budget(&graph, "half"), 68);
}

#[test]
fn quiz_budget_of_an_unknown_topic_is_zero_and_the_composer_needs_a_learned_topic() {
    let graph = graph_of(vec![topic("a").build()], &[]);
    assert_eq!(quiz_budget(&graph, "ghost"), 0);
    let none: BTreeMap<String, TopicState> = BTreeMap::new();
    let plan = quiz_composer(&none, &graph, &cfg(), T_US, &mut sampler(1), None);
    assert!(plan.questions.is_empty());
    assert_eq!(plan.total_time_budget_secs(), 0);
}

#[test]
fn quiz_is_due_reads_the_cadence_edges() {
    let (graph, states) = ten_learned();
    let cfg = cfg();
    // No quiz state at all: the first quiz is due at once.
    assert!(quiz_is_due(None, &states, &graph, &cfg, T_US, None));
    // A state with no last quiz: due as well.
    let never = QuizState {
        last_at: None,
        xp_since: 0,
        retake_pending: false,
    };
    assert!(quiz_is_due(Some(&never), &states, &graph, &cfg, T_US, None));
    assert_eq!(quiz_retake_available_at(Some(&never)), None);
    assert_eq!(quiz_retake_available_at(None), None);
    // A pending retake with no date is due at once.
    let pending = QuizState {
        retake_pending: true,
        ..never
    };
    assert!(quiz_is_due(
        Some(&pending),
        &states,
        &graph,
        &cfg,
        T_US,
        None
    ));
    // Enough XP since the last quiz is due, and an instant with no calendar
    // date is never due.
    let rich = QuizState {
        last_at: Some(NaiveDate::from_ymd_opt(2026, 7, 14).unwrap()),
        xp_since: cfg.quiz.cadence_xp,
        retake_pending: false,
    };
    assert!(quiz_is_due(Some(&rich), &states, &graph, &cfg, T_US, None));
    assert!(!quiz_is_due(
        Some(&rich),
        &states,
        &graph,
        &cfg,
        i64::MAX,
        None
    ));
    assert_eq!(utc_date(i64::MAX), None);
}

// --------------------------------------------------------------------------- //
// The quiz boundaries, read AT the threshold (M3 review round 1, finding #10)
// --------------------------------------------------------------------------- //

#[test]
fn the_quiz_recency_window_is_read_at_its_boundary() {
    // `selector.py:84` holds `QUIZ_RECENT_DAYS = 14` and `selector.py:711` is
    // `(t - learned_at[tid]) <= timedelta(days=QUIZ_RECENT_DAYS)`, so a topic
    // learned exactly 14 days ago is still recent. Three learned topics, aged 13,
    // 14, and 15 days: 1.0 puts q13 and q14 in `recent` and q15 in `mid`, for seed
    // 7 and for seed 42 (finding #10).
    assert_eq!(QUIZ_RECENT_DAYS, 14);
    let quiz_ids = ["q13", "q14", "q15"];
    let graph = graph_of(quiz_ids.iter().map(|id| topic(id).build()).collect(), &[]);
    let states: BTreeMap<String, TopicState> = quiz_ids
        .iter()
        .map(|id| ((*id).to_owned(), learned(0.9)))
        .collect();
    let mut learned_at: BTreeMap<String, i64> = BTreeMap::new();
    learned_at.insert("q13".to_owned(), T_US - days(13));
    learned_at.insert("q14".to_owned(), T_US - days(14));
    learned_at.insert("q15".to_owned(), T_US - days(15));

    let cfg = cfg();
    for seed in [7_u64, 42] {
        let plan = quiz_composer(
            &states,
            &graph,
            &cfg,
            T_US,
            &mut sampler(seed),
            Some(&learned_at),
        );
        let mut rows: Vec<(String, &str, i64)> = plan
            .questions
            .iter()
            .map(|question| {
                (
                    question.topic.clone(),
                    question.stratum,
                    question.time_budget_secs,
                )
            })
            .collect();
        rows.sort();
        assert_eq!(
            rows,
            vec![
                ("q13".to_owned(), "recent", 45),
                ("q14".to_owned(), "recent", 45),
                ("q15".to_owned(), "mid", 45),
            ],
            "seed {seed}"
        );
    }
}
