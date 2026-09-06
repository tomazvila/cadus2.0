//! U3 acceptance, part 4: the boundary streams, the compensated ability seed,
//! and the lesson knowledge-point gates (M3 review round 1, findings #6, #7,
//! #8, #14, #15).
//!
//! Every expected value below is a LITERAL from the 1.0 oracle on the committed
//! stream, or from `projector.py:860-872`. No expectation calls the code under test.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::BTreeMap;

use cadus_core::config::Config;
use cadus_core::event::TopicStatus;
use cadus_core::fire::initial_ability;
use cadus_core::learner::TopicState;
use cadus_core::projector::{blob_digest, canonical_blob, kp_failed, kp_passed};
use common::events::{assert_same_blob, event, fold, live_oracle_blob, stream};

// --------------------------------------------------------------------------- //
// The boundary streams — one guard per stream, read AT equality
// --------------------------------------------------------------------------- //
//
// Each stream under `tests/fixtures/events/boundary/` is the smallest stream that
// puts ONE guard of the fold exactly on its threshold, where a `<` and a `<=` port
// part company. The expected digest and every expected field below come from the
// 1.0 oracle on that committed stream:
//
//   cd /home/deploy/dev/cadus && .venv/bin/python \
//     scripts/oracle/dump_projector_1_0.py \
//     crates/core/tests/fixtures/events/boundary/<name>.jsonl \
//     --curriculum curriculum
//
// `the_live_oracle_agrees_on_every_boundary_stream` re-derives all five digests
// from the live 1.0 code, so a literal here can never drift away from 1.0.
// M3 review round 1, findings #6, #7, #8, #14, #15.

/// The 1.0 fold of `boundary/quiz_score_at_retake_threshold.jsonl` (finding #6).
const BOUNDARY_QUIZ_AT_THRESHOLD: &str =
    "38bc0908db878e777b373a58fe9203068d4fdf69bcfa7e73211c6e73331c3e96";

/// The 1.0 fold of `boundary/placed_balance_zero.jsonl` (finding #7).
const BOUNDARY_PLACED_BALANCE_ZERO: &str =
    "627f5cbcdf457b104682534c1a7985e9010e3e0a191c22aee22de707974061d7";

/// The 1.0 fold of `boundary/xp_window_cancelling_sum.jsonl` (finding #8).
const BOUNDARY_XP_CANCELLING_SUM: &str =
    "5d0ef91630f930c9e56576952c2d158309255739e5d5fc55be041e2a0d76de80";

/// The 1.0 fold of `boundary/velocity_window_start_day.jsonl` (finding #14).
const BOUNDARY_VELOCITY_WINDOW_START: &str =
    "22af94ca29e7d339b3112fb0a89d7da9e08ab050137e8cf1ad791df3c8d6e1ff";

/// The 1.0 fold of `boundary/streak_reference_day_at_goal.jsonl` (finding #15).
const BOUNDARY_STREAK_AT_GOAL: &str =
    "c5e3438c51dc169d775b88879ba672223292bf9c37b0ac8167316b924a8f4956";

/// Every boundary stream, with the 1.0 digest of its fold.
const BOUNDARY_STREAMS: [(&str, &str); 5] = [
    (
        "boundary/quiz_score_at_retake_threshold.jsonl",
        BOUNDARY_QUIZ_AT_THRESHOLD,
    ),
    (
        "boundary/placed_balance_zero.jsonl",
        BOUNDARY_PLACED_BALANCE_ZERO,
    ),
    (
        "boundary/xp_window_cancelling_sum.jsonl",
        BOUNDARY_XP_CANCELLING_SUM,
    ),
    (
        "boundary/velocity_window_start_day.jsonl",
        BOUNDARY_VELOCITY_WINDOW_START,
    ),
    (
        "boundary/streak_reference_day_at_goal.jsonl",
        BOUNDARY_STREAK_AT_GOAL,
    ),
];

#[test]
fn a_quiz_score_equal_to_the_retake_threshold_leaves_no_retake_pending() {
    // `projector.py:262` is `score < cfg.quiz.retake_below`, and `retake_below` is
    // 0.8. The stream scores exactly 0.8, so a `<=` port sets `retake_pending` and
    // serves a retake 1.0 never serves (finding #6).
    let events = stream("boundary/quiz_score_at_retake_threshold.jsonl");
    assert_eq!(events.len(), 2);
    let cfg = Config::default();
    assert!((cfg.quiz.retake_below - 0.8).abs() < f64::EPSILON);

    let model = fold(&events);
    assert!(!model.quiz.retake_pending);
    assert_eq!(
        model.quiz.last_at.map(|day| day.to_string()).as_deref(),
        Some("2026-05-04")
    );
    assert_eq!(blob_digest(&model).unwrap(), BOUNDARY_QUIZ_AT_THRESHOLD);
}

#[test]
fn an_initial_placement_balance_of_exactly_zero_places_nothing() {
    // `projector.py:340-344` is `tid in graph.topics and balance > 0.0` on the
    // NON-refresh path. A balance of exactly 0.0 leaves the topic untouched, so a
    // `>=` port seeds a state that makes the topic review-eligible (finding #7).
    let events = stream("boundary/placed_balance_zero.jsonl");
    assert_eq!(events.len(), 2);
    let model = fold(&events);

    // The 2.5 balance is placed; the 0.0 balance is not, and `finalize` keeps no
    // key for it.
    assert_eq!(model.topics.len(), 1);
    let placed = &model.topics["adding-integers"];
    assert_eq!(placed.status, TopicStatus::Placed);
    assert!((placed.rep_num - 2.0).abs() < f64::EPSILON);
    assert!(!model.topics.contains_key("absolute-value"));
    assert_eq!(blob_digest(&model).unwrap(), BOUNDARY_PLACED_BALANCE_ZERO);
}

#[test]
fn the_velocity_window_total_is_compensated_at_the_xp_site() {
    // Trap T1 at `xp.py:207`: the window total is a CPython `sum()`, which is
    // compensated since 3.12. The three awards are `1e16`, `1.0`, `-1e16` on one
    // day, so the compensated total is 1.0 and the naive total is 0.0. The
    // velocity then reads 1.0 / 28 = 0.0357 against a naive 0.0 (finding #8).
    let events = stream("boundary/xp_window_cancelling_sum.jsonl");
    assert_eq!(events.len(), 5);
    let model = fold(&events);

    assert!((model.velocity.xp_per_day_28d - 0.0357).abs() < f64::EPSILON);
    // The whole-log total is compensated too, and the per-day tally is the naive
    // `+=` of 1.0 — the two disagree on this stream, which is 1.0 behavior.
    assert_eq!(model.xp.total, 1);
    assert_eq!(model.xp.today, 0);
    assert_eq!(blob_digest(&model).unwrap(), BOUNDARY_XP_CANCELLING_SUM);
}

#[test]
fn an_xp_entry_on_the_first_day_of_the_window_is_inside_it() {
    // `xp.py:192-194` puts `window_days` days INCLUDING the reference day in the
    // window, and the membership test is `day >= start`. The reference day is
    // 2026-05-04, so the window starts on 2026-04-07, which is the day of the first
    // award. A `> start` port drops that award: 84 / 28 = 3.0 becomes 56 / 28 = 2.0,
    // and the ETA moves by years (finding #14).
    let events = stream("boundary/velocity_window_start_day.jsonl");
    assert_eq!(events.len(), 3);
    let model = fold(&events);

    assert_eq!(model.xp.total, 84);
    assert!((model.velocity.xp_per_day_28d - 3.0).abs() < f64::EPSILON);
    assert_eq!(
        model.velocity.eta.map(|day| day.to_string()).as_deref(),
        Some("2033-07-18")
    );
    assert_eq!(blob_digest(&model).unwrap(), BOUNDARY_VELOCITY_WINDOW_START);
}

#[test]
fn a_reference_day_exactly_at_the_goal_counts_toward_the_streak() {
    // `xp.py:178` is `if daily.get(today, 0.0) < goal`, so a day EQUAL to the goal
    // is not "in progress": the count starts at the reference day itself. The goal
    // is 40 and both days total exactly 40, so the streak is 2. A `<=` port starts
    // at yesterday and reports 1 (finding #15).
    let events = stream("boundary/streak_reference_day_at_goal.jsonl");
    assert_eq!(events.len(), 2);
    let model = fold(&events);

    assert_eq!(model.xp.goal, 40);
    assert_eq!(model.xp.today, 40);
    assert_eq!(model.xp.streak_days, 2);
    assert_eq!(model.xp.total, 80);
    assert_eq!(blob_digest(&model).unwrap(), BOUNDARY_STREAK_AT_GOAL);
}

#[test]
fn the_live_oracle_agrees_on_every_boundary_stream() {
    if std::env::var("CADUS_ORACLE_PYTHON").is_err() {
        eprintln!("skipped: CADUS_ORACLE_PYTHON is not set");
        return;
    }
    for (name, digest) in BOUNDARY_STREAMS {
        let expected = live_oracle_blob(name, None).expect("the oracle runs");
        let actual = canonical_blob(&fold(&stream(name))).unwrap();
        assert_same_blob(&actual, &expected, name);
        // The committed literal is the digest of those same 1.0 bytes.
        assert_eq!(
            blob_digest(&fold(&stream(name))).unwrap(),
            digest,
            "{name}: the committed digest is not the live 1.0 digest"
        );
    }
}

/// The four neighbor abilities, in the id order the port sums them in.
///
/// `sum()` of this list is 4.0 in CPython 3.12 and later, in EVERY order, because
/// the built-in is compensated. A naive left-to-right add of this order loses the
/// two small values against `1e100` and gives 0.0.
const CANCELLING_ABILITIES: [f64; 4] = [1.0, 1e100, 3.0, -1e100];

#[test]
fn the_ability_seed_of_an_untouched_topic_is_a_compensated_mean() {
    // Trap T1 at `fire.py:361`: the neighborhood mean is a CPython `sum()`. The four
    // touched neighbors below carry a cancelling pair, so the compensated total is
    // 4.0 and the mean is 1.0, where a naive total is 0.0 and the mean is 0.0.
    // `.venv/bin/python -c "print(sum([1.0, 1e100, 3.0, -1e100]))"` prints 4.0 on
    // 3.13.5, and prints it for every permutation of the list, so the 1.0 set order
    // (trap T5) does not move the answer (finding #8).
    let seeded = common::topic("seeded-topic", &[], 0.3, &[]);
    let neighbors = ["n1-neighbor", "n2-neighbor", "n3-neighbor", "n4-neighbor"];
    let mut topics = vec![seeded];
    for id in neighbors {
        topics.push(common::topic(id, &[], 0.3, &[]));
    }
    // One module, so every neighbor is in the seeded topic's neighborhood.
    let graph = common::graph(topics);

    let mut states: BTreeMap<String, TopicState> = BTreeMap::new();
    for (id, ability) in neighbors.iter().zip(CANCELLING_ABILITIES) {
        states.insert(
            (*id).to_owned(),
            TopicState {
                status: TopicStatus::Learning,
                ability,
                ..TopicState::default()
            },
        );
    }
    let cfg = Config::default();
    let seed = initial_ability("seeded-topic", &graph, &states, &cfg);
    assert!(
        (seed - 1.0).abs() < f64::EPSILON,
        "the ability seed is {seed}, so the neighborhood total is not compensated"
    );

    // The neutral prior still answers when no neighbor is touched, so the value
    // above is the mean and not the fallback.
    let untouched: BTreeMap<String, TopicState> = BTreeMap::new();
    let neutral = initial_ability("seeded-topic", &graph, &untouched, &cfg);
    assert!((neutral - 0.5).abs() < f64::EPSILON, "{neutral}");
}

// ---------------------------------------------------------------------------
// The lesson knowledge-point gates (`projector.py:860-872`)
// ---------------------------------------------------------------------------

/// `lesson.kp_pass` is `"2consec|3of4"`. Two correct answers at the TAIL pass
/// the knowledge point, and so do three correct out of the FIRST four. Every
/// expected value below is a literal.
#[test]
fn kp_passed_is_two_at_the_tail_or_three_of_the_first_four() {
    let cfg = Config::default();
    let rule = cfg.lesson.pass_rule();
    assert!(!kp_passed(&[], rule));
    assert!(!kp_passed(&[true], rule));
    assert!(kp_passed(&[true, true], rule));
    assert!(!kp_passed(&[true, false], rule));
    assert!(!kp_passed(&[false, true], rule));
    // A pass at the tail, not anywhere: the first two do not count once a later
    // answer stands.
    assert!(!kp_passed(&[true, true, false], rule));
    assert!(kp_passed(&[false, true, true], rule));
    // Three of the first FOUR, with no pair at the tail.
    assert!(kp_passed(&[true, false, true, true], rule));
    assert!(kp_passed(&[true, true, false, true], rule));
    assert!(!kp_passed(&[false, true, false, true, false], rule));
}

/// `lesson.fail_after` is 5: five answers that have not passed fail the
/// knowledge point, and a passed sequence never fails.
#[test]
fn kp_failed_is_five_answers_without_a_pass() {
    let cfg = Config::default();
    assert!(!kp_failed(&[false, false, false, false], &cfg));
    assert!(kp_failed(&[false, false, false, false, false], &cfg));
    assert!(kp_failed(&[false, true, false, true, false], &cfg));
    // Three of the first four is a pass, so five answers do not fail it.
    assert!(!kp_failed(&[true, true, false, true, false], &cfg));
}

// --------------------------------------------------------------------------- //
// The quiz rows that count as practice
// --------------------------------------------------------------------------- //

#[test]
fn a_missed_quiz_row_is_not_practice_of_its_topic() {
    // Only a CORRECT row on a curriculum topic records a practice instant
    // (`projector.py:262-265`). A miss at the trigger instant leaves the
    // remediation target open.
    let events = vec![
        event(
            r#"{"type":"remediation_triggered","ts":"2026-05-04T09:00:00Z","kind":"quiz-miss",
                "source_topic":"absolute-value","targets":["adding-integers"]}"#,
        ),
        event(
            r#"{"type":"quiz_result","ts":"2026-05-04T09:00:00Z","quiz_id":"q1","score":0.0,
                "per_topic":[{"topic":"adding-integers","correct":false,"secs":10}],"xp":0.0}"#,
        ),
    ];
    let model = fold(&events);
    assert_eq!(model.pending_remediation.len(), 1);
    assert_eq!(
        model.pending_remediation[0].targets[0].as_str(),
        "adding-integers"
    );
}
