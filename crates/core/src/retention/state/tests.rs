use super::*;
use crate::event::{SchemaVersion, Secs, Slug, Timestamp};

/// One probe of `kp` at `delay_days` with the named provenance.
pub(crate) fn probe(
    kp: &str,
    delay_days: u32,
    outcome: AttemptOutcome,
    assisted: bool,
    exposure: Option<Exposure>,
) -> RetentionProbe {
    RetentionProbe {
        ts: Timestamp::from_micros(0),
        session: Some("s1".to_owned()),
        v: SchemaVersion::current(),
        kp: Slug::new(kp).expect("a slug"),
        topic: Slug::new("t1").expect("a slug"),
        delay_days,
        item_digest: Some(format!("{kp}-{delay_days}")),
        outcome,
        assisted,
        exposure,
        secs: Secs::new(12).expect("in range"),
        policy: Some("v1:test".to_owned()),
    }
}

/// Record one independent first exposure for the state fixtures.
pub(crate) fn record(
    state: &mut RetentionState,
    cfg: &RetentionConfig,
    delay: u32,
    outcome: AttemptOutcome,
) {
    state.apply(
        &probe("kp1", delay, outcome, false, Some(Exposure::First)),
        cfg,
    );
}

/// Record the correct seven-day and incorrect thirty-day pair used by total fixtures.
pub(crate) fn record_pair(state: &mut RetentionState, cfg: &RetentionConfig) {
    record(state, cfg, 7, AttemptOutcome::Correct);
    record(state, cfg, 30, AttemptOutcome::Incorrect);
}

#[test]
fn an_empty_state_reports_empty() {
    assert!(RetentionState::default().is_empty());
}

#[test]
fn an_independent_correct_probe_counts_once_everywhere() {
    let cfg = RetentionConfig::default();
    let mut state = RetentionState::default();
    record(&mut state, &cfg, 7, AttemptOutcome::Correct);
    let tally = &state.by_delay[&7];
    assert_eq!(tally.probes, 1);
    assert_eq!(tally.correct, 1);
    assert_eq!(tally.independent, 1);
    assert_eq!(tally.independent_correct, 1);
    assert_eq!(tally.retained_accuracy(), Some(1.0));
    assert_eq!(tally.assistance_dependence(), Some(0.0));
    assert_eq!(tally.mean_independent_secs(), Some(12.0));
    assert!(!state.is_empty());
    assert!(state.is_done("t1", "kp1", 7));
    assert!(
        !state.is_done("t2", "kp1", 7),
        "another topic keeps its own kp1"
    );
}

#[test]
fn an_assisted_correct_probe_is_no_independent_evidence() {
    let cfg = RetentionConfig::default();
    let mut state = RetentionState::default();
    state.apply(
        &probe(
            "kp1",
            7,
            AttemptOutcome::Correct,
            true,
            Some(Exposure::First),
        ),
        &cfg,
    );
    let tally = &state.by_delay[&7];
    assert_eq!(tally.correct, 1);
    assert_eq!(tally.assisted, 1);
    assert_eq!(tally.independent, 0);
    assert_eq!(tally.retained_accuracy(), None);
    assert_eq!(tally.assistance_dependence(), Some(1.0));
}

#[test]
fn a_repeated_item_is_no_independent_evidence() {
    let cfg = RetentionConfig::default();
    let mut state = RetentionState::default();
    state.apply(
        &probe(
            "kp1",
            7,
            AttemptOutcome::Correct,
            false,
            Some(Exposure::Repeat),
        ),
        &cfg,
    );
    let tally = &state.by_delay[&7];
    assert_eq!(tally.repeated, 1);
    assert_eq!(tally.independent, 0);
    assert_eq!(tally.retained_accuracy(), None);
}

#[test]
fn an_unknown_exposure_probe_is_no_independent_evidence() {
    let cfg = RetentionConfig::default();
    let mut state = RetentionState::default();
    state.apply(&probe("kp1", 7, AttemptOutcome::Correct, false, None), &cfg);
    let tally = &state.by_delay[&7];
    assert_eq!(tally.unknown_exposure, 1);
    assert_eq!(tally.independent, 0);
}

#[test]
fn an_ungraded_probe_leaves_the_accuracy_alone() {
    let cfg = RetentionConfig::default();
    let mut state = RetentionState::default();
    let ungraded = AttemptOutcome::Ungraded {
        reason: "model-unavailable".to_owned(),
    };
    state.apply(
        &probe("kp1", 7, ungraded, false, Some(Exposure::First)),
        &cfg,
    );
    state.apply(
        &probe(
            "kp2",
            7,
            AttemptOutcome::Correct,
            false,
            Some(Exposure::First),
        ),
        &cfg,
    );
    let tally = &state.by_delay[&7];
    assert_eq!(tally.probes, 2);
    assert_eq!(tally.ungraded, 1);
    assert_eq!(tally.independent, 1);
    assert_eq!(tally.retained_accuracy(), Some(1.0));
}

#[test]
fn an_off_bucket_delay_reports_under_the_configured_one() {
    let cfg = RetentionConfig::default();
    let mut state = RetentionState::default();
    state.apply(
        &probe(
            "kp1",
            45,
            AttemptOutcome::Incorrect,
            false,
            Some(Exposure::First),
        ),
        &cfg,
    );
    assert_eq!(state.by_delay[&30].probes, 1);
    assert!(state.is_done("t1", "kp1", 45));
}

#[test]
fn the_session_and_digest_windows_stay_bounded() {
    let cfg = RetentionConfig::default();
    let mut state = RetentionState::default();
    for index in 0..(DIGEST_WINDOW + 5) {
        let mut event = probe("kp1", 7, AttemptOutcome::Correct, false, None);
        event.session = Some(format!("s{index}"));
        event.item_digest = Some(format!("d{index}"));
        state.apply(&event, &cfg);
    }
    assert_eq!(state.sessions.len(), SESSION_WINDOW);
    assert_eq!(state.digests.len(), DIGEST_WINDOW);
    assert_eq!(state.digests.last().unwrap(), "d68");
}

#[test]
fn the_rate_rule_counts_the_probes_of_one_session() {
    let cfg = RetentionConfig::default();
    let mut state = RetentionState::default();
    state.apply(&probe("kp1", 7, AttemptOutcome::Correct, false, None), &cfg);
    assert_eq!(state.probes_in_session("s1"), 1);
    assert_eq!(state.probes_in_session("s2"), 0);
}

#[test]
fn the_total_sums_every_delay() {
    let cfg = RetentionConfig::default();
    let mut state = RetentionState::default();
    record_pair(&mut state, &cfg);
    let total = state.total();
    assert_eq!(total.probes, 2);
    assert_eq!(total.independent, 2);
    assert_eq!(total.independent_correct, 1);
    assert_eq!(total.retained_accuracy(), Some(0.5));
}
