//! M3 U1: the CPython-parity numeric helpers, the two digests, and the config tree.
//!
//! Every expected value here is a LITERAL. The literals come from three places, and
//! none of them is the code under test:
//!
//! - the pinned pairs of the port specification, sections 7 to 9
//!   (`docs/reference/projector-1.0-spec.md`);
//! - a direct run of CPython 3.13 and of 1.0's `cadus.projector` on this box;
//! - the committed 1.0 oracle fixtures under `tests/fixtures/events/`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use chrono::NaiveDate;

use cadus_core::config::Config;
use cadus_core::learner::problem_text_hash;
use cadus_core::numeric::{
    TimeError, days_between, local_day, neumaier_sum, resolve_timezone, round_dp, round_half_even,
    round_half_even_i64,
};

// --------------------------------------------------------------------------- //
// Trap T1 and T2: compensated summation against naive accumulation
// --------------------------------------------------------------------------- //

/// A naive left-to-right accumulation, which is what 1.0's `+=` sites do (trap T2).
fn naive_sum(values: &[f64]) -> f64 {
    let mut total = 0.0_f64;
    for &x in values {
        total += x;
    }
    total
}

#[test]
fn neumaier_sum_reproduces_the_two_pinned_cpython_totals() {
    // Spec section 7, trap T1: `sum([0.1]*10) == 1.0` exactly in CPython 3.12+.
    assert_eq!(neumaier_sum(&[0.1; 10]), 1.0);
    // Spec section 7, trap T1: `sum([1.0, 1e100, 1.0, -1e100]) == 2.0`.
    assert_eq!(neumaier_sum(&[1.0, 1e100, 1.0, -1e100]), 2.0);
}

#[test]
fn the_naive_loop_gives_the_other_pinned_totals() {
    // Trap T1 records what a naive loop gives instead. The two algorithms must stay
    // apart: 1.0 uses `sum()` at some sites and `+=` at others (trap T2).
    assert_eq!(naive_sum(&[0.1; 10]), 0.999_999_999_999_999_9);
    assert_eq!(naive_sum(&[1.0, 1e100, 1.0, -1e100]), 0.0);
    assert_ne!(naive_sum(&[0.1; 10]), neumaier_sum(&[0.1; 10]));
}

#[test]
fn neumaier_sum_of_nothing_is_zero_and_of_one_value_is_that_value() {
    assert_eq!(neumaier_sum(&[]), 0.0);
    assert_eq!(neumaier_sum(&[-3.5]), -3.5);
    assert_eq!(neumaier_sum(&[0.0]), 0.0);
}

#[test]
fn neumaier_sum_keeps_order_sensitive_totals() {
    // CPython: sum([1e16, 1.0, -1e16]) == 1.0; the naive loop gives 0.0.
    assert_eq!(neumaier_sum(&[1e16, 1.0, -1e16]), 1.0);
    assert_eq!(naive_sum(&[1e16, 1.0, -1e16]), 0.0);
}

// --------------------------------------------------------------------------- //
// Trap T3: round-half-even
// --------------------------------------------------------------------------- //

#[test]
fn round_half_even_reproduces_the_pinned_python_results() {
    // Spec section 7, trap T3.
    assert_eq!(round_half_even(2.5), 2.0);
    assert_eq!(round_half_even(0.5), 0.0);
    assert_eq!(round_half_even(-3.5), -4.0);
    assert_eq!(round_half_even(4.5), 4.0);
}

#[test]
fn round_half_even_differs_from_the_rust_primitive_on_every_half_way_case() {
    // `f64::round` is half-away-from-zero and diverges on all four.
    assert_eq!(2.5_f64.round(), 3.0);
    assert_eq!(0.5_f64.round(), 1.0);
    assert_eq!((-3.5_f64).round(), -4.0);
    assert_eq!(4.5_f64.round(), 5.0);
    assert_ne!(round_half_even(2.5), 2.5_f64.round());
}

#[test]
fn round_half_even_covers_the_other_python_results() {
    // Verified against CPython 3.13 on this box.
    assert_eq!(round_half_even(1.5), 2.0);
    assert_eq!(round_half_even(-2.5), -2.0);
    assert_eq!(round_half_even(2.675), 3.0);
    assert_eq!(round_half_even(-0.5), 0.0);
    assert_eq!(round_half_even(0.0), 0.0);
    assert_eq!(round_half_even(-1.4), -1.0);
    assert_eq!(round_half_even(1.4), 1.0);
}

#[test]
fn round_half_even_i64_matches_the_python_int_result() {
    assert_eq!(round_half_even_i64(2.5), 2);
    assert_eq!(round_half_even_i64(0.5), 0);
    assert_eq!(round_half_even_i64(-3.5), -4);
    assert_eq!(round_half_even_i64(4.5), 4);
    assert_eq!(round_half_even_i64(-0.5), 0);
    // Trap T7: the XP artifact that reaches the log unrounded.
    assert_eq!(round_half_even_i64(8.924_999_999_999_999), 9);
    // Spec section 8: the regrade model literals, `xp.total == -4` then `== 7`.
    assert_eq!(round_half_even_i64(-3.5), -4);
    assert_eq!(round_half_even_i64(7.0), 7);
}

#[test]
fn round_half_even_i64_saturates_rather_than_panics() {
    assert_eq!(round_half_even_i64(f64::NAN), 0);
    assert_eq!(round_half_even_i64(f64::INFINITY), i64::MAX);
    assert_eq!(round_half_even_i64(f64::NEG_INFINITY), i64::MIN);
}

// --------------------------------------------------------------------------- //
// Trap T4: correctly-rounded decimal
// --------------------------------------------------------------------------- //

#[test]
fn round_dp_reproduces_the_pinned_python_results() {
    // Spec section 7, trap T4.
    assert_eq!(round_dp(2.675, 2), 2.67);
    assert_eq!(round_dp(1.000_000_5, 6), 1.000_001);
    assert_eq!(round_dp(0.500_000_5, 6), 0.5);
}

#[test]
fn round_dp_differs_from_scale_round_divide() {
    // The rejected implementation: `(x * 10f64.powi(n)).round() / 10f64.powi(n)`.
    let scaled = (2.675_f64 * 100.0).round() / 100.0;
    assert_eq!(scaled, 2.68);
    assert_ne!(round_dp(2.675, 2), scaled);
}

#[test]
fn round_dp_covers_the_other_python_results() {
    // Verified against CPython 3.13 on this box.
    assert_eq!(round_dp(8.924_999_999_999_999, 2), 8.92);
    assert_eq!(round_dp(1.005, 2), 1.0);
    assert_eq!(round_dp(0.125, 2), 0.12);
    assert_eq!(round_dp(0.135, 2), 0.14);
    assert_eq!(round_dp(-2.675, 2), -2.67);
    assert_eq!(round_dp(8.925, 2), 8.93);
    assert_eq!(round_dp(2.5, 0), 2.0);
    // Spec section 9: the two velocity values of the oracle model, already rounded.
    assert_eq!(round_dp(1.419_6, 4), 1.419_6);
    assert_eq!(round_dp(0.024_6, 4), 0.024_6);
}

#[test]
fn round_dp_keeps_a_non_finite_value_and_a_negative_zero() {
    assert!(round_dp(f64::NAN, 2).is_nan());
    assert_eq!(round_dp(f64::INFINITY, 2), f64::INFINITY);
    assert!(round_dp(-0.0, 2).is_sign_negative());
}

// --------------------------------------------------------------------------- //
// Trap T8: the day difference
// --------------------------------------------------------------------------- //

#[test]
fn days_between_reproduces_the_python_two_step_division() {
    // 1.0 computes `(t - t0).total_seconds() / 86400.0`, which divides TWICE.
    // 2026-03-02T09:00:00Z to 2026-03-10T22:53:40Z, verified in CPython 3.13.
    assert_eq!(
        days_between(1_772_442_000_000_000, 1_773_183_220_000_000),
        8.578_935_185_185_186
    );
    // Ten whole days is exact.
    assert_eq!(
        days_between(1_783_166_400_000_000, 1_784_030_400_000_000),
        10.0
    );
    assert_eq!(days_between(0, 0), 0.0);
    // A negative difference: the fold sees one when an event predates `t0`.
    assert_eq!(
        days_between(1_784_030_400_000_000, 1_783_166_400_000_000),
        -10.0
    );
}

#[test]
fn days_between_differs_from_the_single_division() {
    // A single division by 86_400_000_000.0 diverges for about one input in four.
    // CPython gives 39.03421031467593 for this difference; the single division
    // gives 39.034210314675924.
    let delta_us = 3_372_555_771_188_i64;
    assert_eq!(days_between(0, delta_us), 39.034_210_314_675_93);
    #[expect(clippy::cast_precision_loss, reason = "the rejected implementation")]
    let single = delta_us as f64 / 86_400_000_000.0;
    assert_eq!(single, 39.034_210_314_675_924);
    assert_ne!(days_between(0, delta_us), single);
}

#[test]
fn days_between_saturates_rather_than_overflowing() {
    let days = days_between(i64::MIN, i64::MAX);
    assert!(days.is_finite());
    assert!(days > 0.0);
}

// --------------------------------------------------------------------------- //
// Trap T9: the local day
// --------------------------------------------------------------------------- //

#[test]
fn local_day_reproduces_the_pinned_new_york_result() {
    // Spec section 8 (`tests/test_xp.py:118-122`): 2026-07-14T01:00Z is 2026-07-13
    // in America/New_York and 2026-07-14 in UTC.
    let us = 1_783_990_800_000_000;
    assert_eq!(
        local_day(us, Some("America/New_York")).unwrap(),
        NaiveDate::from_ymd_opt(2026, 7, 13).unwrap()
    );
    assert_eq!(
        local_day(us, None).unwrap(),
        NaiveDate::from_ymd_opt(2026, 7, 14).unwrap()
    );
    assert_eq!(
        local_day(us, Some("UTC")).unwrap(),
        NaiveDate::from_ymd_opt(2026, 7, 14).unwrap()
    );
}

#[test]
fn local_day_crosses_the_date_line_and_the_epoch() {
    // Verified against CPython's `zoneinfo` on this box.
    assert_eq!(
        local_day(1_767_225_600_000_000, Some("Pacific/Kiritimati")).unwrap(),
        NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
    );
    assert_eq!(
        local_day(1_767_225_600_000_000, Some("Pacific/Pago_Pago")).unwrap(),
        NaiveDate::from_ymd_opt(2025, 12, 31).unwrap()
    );
    assert_eq!(
        local_day(1_783_990_800_000_000, Some("Asia/Kolkata")).unwrap(),
        NaiveDate::from_ymd_opt(2026, 7, 14).unwrap()
    );
    assert_eq!(
        local_day(-3_600_000_000, None).unwrap(),
        NaiveDate::from_ymd_opt(1969, 12, 31).unwrap()
    );
}

#[test]
fn local_day_handles_both_daylight_saving_transitions() {
    // 2026-11-01T05:30Z is 01:30 EDT, and 2026-03-08T06:30Z is 01:30 EST.
    assert_eq!(
        local_day(1_793_511_000_000_000, Some("America/New_York")).unwrap(),
        NaiveDate::from_ymd_opt(2026, 11, 1).unwrap()
    );
    assert_eq!(
        local_day(1_772_951_400_000_000, Some("America/New_York")).unwrap(),
        NaiveDate::from_ymd_opt(2026, 3, 8).unwrap()
    );
}

#[test]
fn an_unknown_time_zone_is_an_error_value_not_a_panic() {
    assert_eq!(
        local_day(0, Some("Mars/Olympus_Mons")).unwrap_err(),
        TimeError::UnknownTimezone("Mars/Olympus_Mons".to_owned())
    );
    assert!(resolve_timezone(Some("")).is_err());
    assert!(resolve_timezone(None).is_ok());
}

// --------------------------------------------------------------------------- //
// Trap T17: the problem-text digest
// --------------------------------------------------------------------------- //

#[test]
fn problem_text_hash_matches_the_pinned_value() {
    // Spec section 9, verified against 1.0's `cadus.projector.problem_text_hash`.
    assert_eq!(problem_text_hash("x"), "11f6ad8ec52a");
    assert_eq!(problem_text_hash(""), "da39a3ee5e6b");
}

#[test]
fn problem_text_hash_does_not_normalize_whitespace() {
    // Spec trap T17: `" padded "` hashes differently from `"padded"`.
    assert_eq!(problem_text_hash(" padded "), "98195010d723");
    assert_eq!(problem_text_hash("padded"), "35b1ac6f9cc1");
    assert_ne!(problem_text_hash(" padded "), problem_text_hash("padded"));
}

#[test]
fn problem_text_hash_hashes_utf8_bytes_and_matches_the_stream_fixture() {
    assert_eq!(
        problem_text_hash("|-3| = ? — non-ASCII kept"),
        "45a795f19809"
    );
    // The first `text_hash` of `stream_1.jsonl` is this topic's first problem text.
    assert_eq!(
        problem_text_hash("[absolute-value] synthetic problem #0: evaluate the expression."),
        "154f4091f0c5"
    );
    assert_eq!(problem_text_hash("x").len(), 12);
}

// --------------------------------------------------------------------------- //
// Trap T16: the config preimage and its digest
// --------------------------------------------------------------------------- //

/// The exact bytes 1.0 hashes: `Config().model_dump_json()` (spec section 9).
const CONFIG_HASH_PREIMAGE: &str = r#"{"fire":{"due_threshold":0.5,"interval_table":[2.0,4.5,10.0,21.0,45.0,100.0,220.0,480.0],"early_floor":0.15,"decay_cap":3.0,"min_credit":0.05,"knockout_weight":0.8,"explicit_speed_threshold":1.0,"speed_clamp":[0.33,3.0]},"ability":{"ewma_alpha":0.3},"lesson":{"kp_pass":"2consec|3of4","fail_after":5,"retry_delay_days":1},"review":{"questions":4,"pass_weighted":0.65},"selector":{"lesson_ratio_min":0.25,"max_reviews_per_lesson":3},"quiz":{"cadence_days":7,"cadence_xp":200,"questions":8,"retake_below":0.8},"diag":{"max_questions":40,"coverage_radius":3,"sibling_credit":0.5,"conditional_max":1.0},"xp":{"daily_goal":40,"tiers":{"perfect":1.3,"nearly_perfect":1.0,"passable":0.85,"nearly_passable":0.3,"poor":0.0,"blowoff":-0.5}},"drill":{"questions":20,"target_secs":6},"error_tags":["sign-error","arithmetic-slip","algebra-slip","wrong-method","formula-recall","misread-problem","incomplete","notation","units","timing-unreliable","blowoff","blank_answer"],"timezone":null}"#;

#[test]
fn the_config_hash_preimage_is_byte_for_byte_the_pydantic_dump() {
    assert_eq!(
        Config::default().hash_preimage().unwrap(),
        CONFIG_HASH_PREIMAGE
    );
}

#[test]
fn the_default_config_hashes_to_the_pinned_value() {
    // Spec section 9, verified against 1.0's `cadus.projector.config_hash`.
    assert_eq!(Config::default().config_hash().unwrap(), "797575e985c12149");
    assert_eq!(Config::default().config_hash().unwrap().len(), 16);
}

#[test]
fn the_config_hash_moves_when_any_constant_moves() {
    let mut config = Config::default();
    config.xp.daily_goal = 41;
    assert_ne!(config.config_hash().unwrap(), "797575e985c12149");

    let config = Config {
        timezone: Some("America/New_York".to_owned()),
        ..Config::default()
    };
    assert_ne!(config.config_hash().unwrap(), "797575e985c12149");
}

#[test]
fn the_default_config_carries_the_pinned_headline_values() {
    // Spec section 8 (`tests/test_model.py:261-279`).
    let config = Config::default();
    assert_eq!(config.fire.due_threshold, 0.5);
    assert_eq!(
        config.fire.interval_table,
        vec![2.0, 4.5, 10.0, 21.0, 45.0, 100.0, 220.0, 480.0]
    );
    assert_eq!(config.fire.knockout_weight, 0.8);
    assert_eq!(config.fire.speed_clamp, (0.33, 3.0));
    assert_eq!(config.xp.daily_goal, 40);
    assert_eq!(config.xp.tiers.blowoff, -0.5);
    assert_eq!(config.quiz.questions, 8);
    // Spec section 8 (`tests/test_xp.py:57-62`): the multipliers, exactly.
    assert_eq!(config.xp.tiers.perfect, 1.3);
    assert_eq!(config.xp.tiers.nearly_perfect, 1.0);
    assert_eq!(config.xp.tiers.passable, 0.85);
    assert_eq!(config.xp.tiers.nearly_passable, 0.3);
    assert_eq!(config.xp.tiers.poor, 0.0);
    assert_eq!(config.error_tags.len(), 12);
    assert_eq!(
        config.error_tags.first().map(String::as_str),
        Some("sign-error")
    );
    assert_eq!(
        config.error_tags.last().map(String::as_str),
        Some("blank_answer")
    );
    assert_eq!(config.timezone, None);
}

#[test]
fn the_config_tree_round_trips_through_its_own_preimage() {
    let parsed: Config = serde_json::from_str(CONFIG_HASH_PREIMAGE).unwrap();
    assert_eq!(parsed, Config::default());
}

#[test]
fn the_config_tree_rejects_an_unknown_key() {
    let text = CONFIG_HASH_PREIMAGE.replace(r#""timezone":null"#, r#""timezone":null,"nope":1"#);
    assert!(serde_json::from_str::<Config>(&text).is_err());
}
