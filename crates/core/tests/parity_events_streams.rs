//! U5 acceptance, part 2: the per-stream properties of the 20 committed
//! streams, one test module per stream (R5, D4, C2).
//!
//! Every expectation is a LITERAL that the 1.0 oracle produced and this repository
//! committed. A stream whose Rust digest differs from its 1.0 digest is a RUST
//! BUG. The failure report names the stream, prints the first differing window
//! of the two blobs, and -- with `CADUS_ORACLE_PYTHON` set -- bisects the stream
//! to the event index after which the two models diverge.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;

use cadus_core::event::{Event, WorkQuality};
use cadus_core::fire::{
    AttemptResult, PropagationKind, apply_attempt, memory_at, quality_q, raw_delta,
};
use cadus_core::learner::TopicState;
use cadus_core::projector::{
    Projector, apply_regrades, blob_digest, canonical_blob, project, project_incremental,
};
use common::events::{cfg, input, labeled_difference, stream, tree};
use common::parity::{GOAL, StreamDigests, assert_folds_to, fold_in, incremental_row, row};

/// The non-UTC zone the digests file carries beside UTC.
const NEW_YORK: &str = "America/New_York";

/// The tolerance of the implicit-credit comparison: the two sides compute the same
/// product from the same inputs, so only the last bit may differ.
const CREDIT_EPSILON: f64 = 1e-12;

/// The row of one stream, its corrected events, and a fresh projector over the
/// tree at the oracle goal.
fn corrected_stream(number: usize) -> (&'static StreamDigests, Vec<Event>, Projector<'static>) {
    let entry = row(number);
    let corrected = apply_regrades(&stream(&entry.stream));
    let projector = Projector::new(tree(), cfg()).with_goal(GOAL);
    (entry, corrected, projector)
}

// --------------------------------------------------------------------------- //
// The per-stream properties
// --------------------------------------------------------------------------- //

/// The fold of `name` folds to its committed digest in every recorded zone.
fn check_digests(number: usize) {
    let entry = row(number);
    assert_folds_to(&entry.stream, None, &entry.digests["UTC"]);
    assert_folds_to(&entry.stream, Some(NEW_YORK), &entry.digests[NEW_YORK]);
}

/// Removing every `regraded` event re-folds to the pre-correction digest.
///
/// `apply_regrades` works on copies, so deleting the corrections must give the
/// uncorrected model back. That pins that the corrected events stay intact.
fn check_pre_correction(number: usize) {
    let entry = row(number);
    let bare: Vec<Event> = stream(&entry.stream)
        .into_iter()
        .filter(|event| event.type_name() != "regraded")
        .collect();
    assert_eq!(bare.len(), entry.events_without_regrades);
    let model = fold_in(&bare, None);
    let actual = blob_digest(&model).expect("the digest builds");
    assert_eq!(
        actual, entry.digests["UTC_no_regrades"],
        "{}: the fold without the corrections is not the pre-correction model",
        entry.stream
    );
}

/// `project_incremental` reproduces the 1.0 result at EVERY split of the stream.
///
/// At most splits that means it equals the full replay. At the splits
/// `incremental_1_0.json` records it does NOT, and that is 1.0 behavior rather
/// than a defect: `project_incremental` seeds FIRe from the cached model, so a
/// `regraded` event in the new half that supersedes a grade the prior half
/// already folded never reaches FIRe (`projector.py:794-830`; 1.0 routes such a
/// stream down the full-replay path instead). The port must diverge at the same
/// splits and to the same model, so both sides are pinned against 1.0 literals.
fn check_incremental(number: usize) {
    let entry = row(number);
    let carry = incremental_row(number);
    let events = stream(&entry.stream);
    let input = input().with_goal(GOAL);
    let full_model = project(&events, &input).expect("the fold succeeds");
    let full = canonical_blob(&full_model).expect("the blob builds");
    assert_eq!(
        blob_digest(&full_model).expect("the digest builds"),
        carry.full_digest,
        "{}: the full replay is not the 1.0 full replay",
        entry.stream
    );

    for split in 0..=events.len() {
        let (prior, fresh) = events.split_at(split);
        let cached = project(prior, &input).expect("the prefix folds");
        let incremental =
            project_incremental(&cached, prior, fresh, &input).expect("the resume folds");
        let actual = canonical_blob(&incremental).expect("the blob builds");

        if let Some(expected) = carry.mismatching_digests.get(&split.to_string()) {
            // A correction carried over from the new half. 1.0 lands on its own
            // model here, and so must the port.
            let digest = blob_digest(&incremental).expect("the digest builds");
            assert_eq!(
                &digest, expected,
                "{} split {split}: the carried-over correction gives a different \
                 model than 1.0 gives",
                entry.stream
            );
            assert!(
                actual != full,
                "{} split {split}: 1.0 leaves the full replay here, but the port does not",
                entry.stream
            );
        } else {
            assert!(
                actual == full,
                "{} split {split}: {}",
                entry.stream,
                labeled_difference("incremental", &actual, "full replay", &full, 200)
            );
        }
    }
}

/// `apply_regrades` is idempotent: applying it twice equals applying it once.
fn check_regrade_idempotence(number: usize) {
    let entry = row(number);
    let events = stream(&entry.stream);
    let once = apply_regrades(&events);
    let twice = apply_regrades(&once);
    assert_eq!(
        once, twice,
        "{}: apply_regrades is not idempotent",
        entry.stream
    );
    // The second pass has no correction left to consume, so the fold is unmoved too.
    assert_eq!(
        blob_digest(&fold_in(&once, None)).expect("the digest builds"),
        entry.digests["UTC"],
        "{}: the pre-corrected stream folds differently",
        entry.stream
    );
}

/// `repNum` and `memoryBase` stay at or above zero after EVERY event.
fn check_invariants_after_every_event(number: usize) {
    let (entry, corrected, mut projector) = corrected_stream(number);
    for (index, event) in corrected.iter().enumerate() {
        projector.apply(event, true);
        for (tid, state) in projector.topics() {
            assert!(
                state.rep_num >= 0.0,
                "{} event {index} ({}): {tid} has repNum {}",
                entry.stream,
                event.type_name(),
                state.rep_num
            );
            assert!(
                state.memory_base >= 0.0,
                "{} event {index} ({}): {tid} has memoryBase {}",
                entry.stream,
                event.type_name(),
                state.memory_base
            );
        }
    }
}

/// `memory_at` is non-increasing in `t` for every `t >= t0`.
fn check_memory_is_non_increasing(number: usize) {
    let entry = row(number);
    let model = fold_in(&stream(&entry.stream), None);
    // A ladder of offsets from `t0`, in days, out past the longest interval table entry.
    let offsets = [
        0.0_f64, 0.5, 1.0, 2.0, 7.0, 30.0, 90.0, 365.0, 1000.0, 5000.0,
    ];
    for (tid, state) in &model.topics {
        let Some(t0) = state.t0 else { continue };
        let mut previous = f64::INFINITY;
        for offset in offsets {
            #[allow(clippy::cast_possible_truncation)]
            let t_us = t0.micros() + (offset * 86_400_000_000.0) as i64;
            let memory = memory_at(state, t_us);
            assert!(
                memory >= 0.0,
                "{}: {tid} has memory {memory} at t0 + {offset} days",
                entry.stream
            );
            assert!(
                memory <= previous,
                "{}: {tid} memory rose from {previous} to {memory} at t0 + {offset} days",
                entry.stream
            );
            previous = memory;
        }
    }
}

/// The implicit credit a neighbor absorbs never exceeds the direct credit the same
/// grade would give that neighbor.
///
/// The fold runs FIRe on a passing `lesson_result` and on every `review_result`
/// (`projector.py:226-255`), so the check walks those events, and drives
/// `apply_attempt` on the states the fold holds at that instant.
fn check_implicit_credit(number: usize) {
    let (entry, corrected, mut projector) = corrected_stream(number);
    let cfg = cfg();
    let mut checked = 0_usize;

    for event in &corrected {
        let graded: Option<(&str, bool, WorkQuality, bool)> = match event {
            Event::LessonResult(body) if body.passed => {
                Some((body.topic.as_str(), true, body.quality_tier, body.assisted))
            }
            Event::ReviewResult(body) => Some((
                body.topic.as_str(),
                body.passed,
                body.quality_tier,
                body.assisted,
            )),
            _ => None,
        };

        if let Some((topic, passed, quality, assisted)) = graded {
            let states: BTreeMap<String, TopicState> = projector.topics().clone();
            let t_us = event.ts().micros();
            let attempt = AttemptResult::new(topic, passed, quality).with_assisted(assisted);
            let (_new_states, props) = apply_attempt(&states, &attempt, tree(), cfg, t_us);
            let grade = quality_q(quality);
            let default = TopicState::default();

            for prop in &props {
                let recipient = states.get(&prop.topic).unwrap_or(&default);
                // The direct credit the SAME grade gives this recipient at this instant.
                let direct = match prop.kind {
                    PropagationKind::Credit => {
                        raw_delta(grade, memory_at(recipient, t_us), true, cfg, assisted)
                    }
                    // A failure carries no early factor, so the recipient's direct
                    // delta is the explicit topic's own delta.
                    PropagationKind::Penalty => raw_delta(grade, 0.0, false, cfg, assisted),
                };
                assert!(
                    prop.weight > 0.0 && prop.weight <= 1.0,
                    "{}: {} carries weight {}",
                    entry.stream,
                    prop.topic,
                    prop.weight
                );
                assert!(
                    prop.raw_delta.abs() <= direct.abs() + CREDIT_EPSILON,
                    "{}: implicit {} on {} is {} but the direct credit is {}",
                    entry.stream,
                    prop.kind.as_str(),
                    prop.topic,
                    prop.raw_delta,
                    direct
                );
                checked += 1;
            }
        }
        projector.apply(event, true);
    }

    assert!(
        checked > 0,
        "{}: no propagation was checked, so the property is vacuous",
        entry.stream
    );
}

/// Declare the per-stream property tests, one test function per stream, so the
/// runner spreads the folds across threads.
macro_rules! stream_tests {
    ($($name:ident => $number:literal),* $(,)?) => {
        $(
            mod $name {
                use super::*;

                #[test]
                fn folds_to_the_committed_digests() {
                    check_digests($number);
                }

                #[test]
                fn without_the_corrections_folds_to_the_pre_correction_digest() {
                    check_pre_correction($number);
                }

                #[test]
                fn incremental_reproduces_the_1_0_fold_at_every_split() {
                    check_incremental($number);
                }

                #[test]
                fn apply_regrades_is_idempotent() {
                    check_regrade_idempotence($number);
                }

                #[test]
                fn holds_the_floor_invariants_after_every_event() {
                    check_invariants_after_every_event($number);
                }

                #[test]
                fn memory_is_non_increasing_after_t0() {
                    check_memory_is_non_increasing($number);
                }

                #[test]
                fn implicit_credit_never_exceeds_the_direct_credit() {
                    check_implicit_credit($number);
                }
            }
        )*
    };
}

stream_tests! {
    stream_1 => 1,
    stream_2 => 2,
    stream_3 => 3,
    stream_4 => 4,
    stream_5 => 5,
    stream_6 => 6,
    stream_7 => 7,
    stream_8 => 8,
    stream_9 => 9,
    stream_10 => 10,
    stream_11 => 11,
    stream_12 => 12,
    stream_13 => 13,
    stream_14 => 14,
    stream_15 => 15,
    stream_16 => 16,
    stream_17 => 17,
    stream_18 => 18,
    stream_19 => 19,
    stream_20 => 20,
}
