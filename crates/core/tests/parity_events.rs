//! U5 acceptance, part 1: the committed 1.0 indexes over the 20 streams, and
//! the live oracle that re-derives them (R5, D4, C2).
//!
//! Every expectation is a LITERAL that the 1.0 oracle produced and this repository
//! committed. `tests/fixtures/events/digests_1_0.json` holds one digest per stream
//! per zone (`scripts/oracle/digest_streams_1_0.py`), and
//! `tests/fixtures/events/selector_1_0.json` holds the 1.0 `compose_session` plan of
//! the 10 seeded states (`scripts/oracle/dump_selector_1_0.py`). No expectation
//! calls the code under test.
//!
//! `tests/fixtures/events/coverage.md` records which branch of spec section 9 each
//! stream reaches, measured against the live 1.0 code by
//! `scripts/oracle/coverage_streams_1_0.py`.
//!
//! A stream whose Rust digest differs from its 1.0 digest is a RUST BUG. The
//! failure report names the stream, prints the first differing 200-byte window of
//! the two blobs, and -- with `CADUS_ORACLE_PYTHON` set -- bisects the stream to the
//! event index after which the two models diverge.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::BTreeSet;

use cadus_core::event::Event;
use cadus_core::projector::PROJECTOR_VERSION;
use common::events::{ORACLE_PROJECTOR_VERSION, fixture, stream};
use common::parity::{
    DigestIndex, GOAL, IncrementalIndex, digest_index, incremental_index, incremental_row,
    oracle_index, row,
};

/// The number of committed `stream_N.jsonl` streams.
const STREAMS: usize = 20;

/// The coverage stream `stream_u3_coverage.jsonl`, which `incremental_1_0.json`
/// records beside the numbered streams.
///
/// It is the only committed stream that carries the `profile_reset` incremental
/// divergence class (M3 review round 1, finding #4).
const COVERAGE_STREAM: &str = "stream_u3_coverage.jsonl";

/// The `config_hash` of the default config (spec section 9).
const CONFIG_HASH: &str = "797575e985c12149";

/// The 1.0 fold digest of `stream_1.jsonl` (`docs/plans/M3.md`).
const STREAM_1_UTC: &str = "ba128459985e0815db7446cb2af16452ec07d304b7efaa0952fc6567404245f5";

/// The non-UTC zone the digests file carries beside UTC.
const NEW_YORK: &str = "America/New_York";

// --------------------------------------------------------------------------- //
// The committed index itself
// --------------------------------------------------------------------------- //

#[test]
fn the_digest_index_holds_the_pinned_metadata() {
    let index = digest_index();
    assert_eq!(index.oracle, "scripts/oracle/digest_streams_1_0.py");
    assert_eq!(index.generator, "scripts/oracle/gen_stream_1_0.py");
    assert_eq!(index.now, "2000-01-01T00:00:00+00:00");
    assert_eq!(index.goal, GOAL);
    assert_eq!(index.projector_version, ORACLE_PROJECTOR_VERSION);
    assert_eq!(index.projector_version, 3);
    assert_eq!(PROJECTOR_VERSION, 6);
    assert_eq!(index.config_hash, CONFIG_HASH);
    assert_eq!(index.zones, ["UTC", NEW_YORK, "UTC_no_regrades"]);
    assert_eq!(index.streams.len(), STREAMS);
    // The digest `docs/plans/M3.md` pins is the first row's UTC digest.
    assert_eq!(index.streams[0].digests["UTC"], STREAM_1_UTC);
    for number in 1..=STREAMS {
        let entry = row(number);
        assert_eq!(entry.digests.len(), 3, "{}", entry.stream);
        for zone in ["UTC", NEW_YORK, "UTC_no_regrades"] {
            let digest = &entry.digests[zone];
            assert_eq!(digest.len(), 64, "{} {zone}", entry.stream);
            assert!(
                digest.bytes().all(|b| b.is_ascii_hexdigit()),
                "{} {zone}",
                entry.stream
            );
        }
    }
}

#[test]
fn the_incremental_index_holds_the_pinned_metadata() {
    let index = incremental_index();
    assert_eq!(index.oracle, "scripts/oracle/incremental_splits_1_0.py");
    assert_eq!(index.config_hash, CONFIG_HASH);
    assert_eq!(index.projector_version, ORACLE_PROJECTOR_VERSION);
    // The 20 numbered streams and the coverage stream.
    assert_eq!(index.streams.len(), STREAMS + 1);
    assert_eq!(index.streams[STREAMS].stream, COVERAGE_STREAM);
    for number in 1..=STREAMS {
        let entry = incremental_row(number);
        let digests = row(number);
        assert_eq!(entry.events, digests.events, "{}", entry.stream);
        assert_eq!(
            entry.full_digest, digests.digests["UTC"],
            "{}",
            entry.stream
        );
        assert_eq!(
            entry.mismatching_splits.len(),
            entry.mismatching_digests.len(),
            "{}",
            entry.stream
        );
        for split in &entry.mismatching_splits {
            assert!(
                entry.mismatching_digests.contains_key(&split.to_string()),
                "{} split {split} has no 1.0 digest",
                entry.stream
            );
        }
    }
    // `stream_1.jsonl` carries no correction into a later half, so 1.0 agrees with
    // the full replay at every one of its splits (spec section 9).
    assert!(incremental_row(1).mismatching_splits.is_empty());

    // The coverage row: the `profile_reset` class. Its digests are pinned the same
    // way, and `tests/projector.rs` asserts the port lands on them.
    let coverage = incremental_index()
        .streams
        .iter()
        .find(|entry| entry.stream == COVERAGE_STREAM)
        .expect("the coverage stream has a row");
    assert_eq!(coverage.events, 33);
    assert_eq!(
        coverage.mismatching_splits,
        [21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31]
    );
    assert_eq!(coverage.mismatching_digests.len(), 11);
}

#[test]
fn every_committed_stream_holds_its_recorded_event_count() {
    for number in 1..=STREAMS {
        let entry = row(number);
        let events = stream(&entry.stream);
        assert_eq!(events.len(), entry.events, "{}", entry.stream);
        let bare = events
            .iter()
            .filter(|event| event.type_name() != "regraded")
            .count();
        assert_eq!(bare, entry.events_without_regrades, "{}", entry.stream);
    }
}

#[test]
fn the_seeded_family_reaches_every_event_type() {
    // Spec section 9 item 1: every one of the 16 types, the six no-ops included.
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for number in 2..=STREAMS {
        let per_stream: BTreeSet<&str> = stream(&format!("stream_{number}.jsonl"))
            .iter()
            .map(Event::type_name)
            .collect();
        assert_eq!(
            per_stream.len(),
            16,
            "stream_{number}.jsonl misses an event type: {per_stream:?}"
        );
        seen.extend(per_stream);
    }
    // The 1.0 generator wrote the 16 types of 1.0. `retention_probe` is new in 2.0
    // (D-F11) and no committed 1.0 stream carries one.
    for name in Event::TYPE_NAMES {
        if name == "retention_probe" {
            assert!(!seen.contains(name));
            continue;
        }
        assert!(seen.contains(name), "no stream carries a `{name}` event");
    }
}

#[test]
fn the_coverage_table_is_committed_beside_the_fixtures() {
    // `coverage.md` is the measured answer to spec section 9, and the report the
    // review reads. A stream family with no coverage record is not a parity suite.
    let text = std::fs::read_to_string(fixture("coverage.md")).expect("the table reads");
    assert!(text.contains("scripts/oracle/coverage_streams_1_0.py"));
    for item in 1..=10 {
        assert!(
            text.contains(&format!("\n| {item} | `")),
            "coverage.md records no spec section 9 item {item}"
        );
    }

    // Two guards the seeded family does NOT reach. The table said `all` for a probe
    // beside each of them, and a reviewer read the boundary as measured (M3 review
    // round 1, findings #7 and #15). Each now has a row of its own that reads
    // `none`, and a boundary stream that pins it.
    for (probe, pinned_by) in [
        (
            "diag.placed_balance_zero",
            "boundary/placed_balance_zero.jsonl",
        ),
        (
            "streak.reference_day_at_goal",
            "boundary/streak_reference_day_at_goal.jsonl",
        ),
    ] {
        assert!(
            text.contains(&format!("| `{probe}` |")),
            "coverage.md has no `{probe}` row"
        );
        assert!(
            text.contains(&format!("`{probe}` -- ")),
            "coverage.md does not say where `{probe}` is pinned"
        );
        assert!(
            fixture(pinned_by).exists(),
            "{pinned_by} is missing, so `{probe}` is unpinned"
        );
    }
}

// --------------------------------------------------------------------------- //
// The live 1.0 oracle
// --------------------------------------------------------------------------- //

#[test]
fn the_live_oracle_reproduces_every_committed_digest() {
    let Ok(python) = std::env::var("CADUS_ORACLE_PYTHON") else {
        eprintln!("skipped: CADUS_ORACLE_PYTHON is not set");
        return;
    };
    let text = oracle_index(
        &python,
        "scripts/oracle/digest_streams_1_0.py",
        "digests_live.json",
    );
    let live: DigestIndex = serde_json::from_str(&text).expect("the live index parses");
    let committed = digest_index();
    assert_eq!(live.streams.len(), committed.streams.len());
    assert_eq!(live.config_hash, committed.config_hash);
    assert_eq!(live.projector_version, committed.projector_version);
    for (fresh, old) in live.streams.iter().zip(committed.streams.iter()) {
        assert_eq!(fresh.stream, old.stream);
        assert_eq!(fresh.events, old.events, "{}", old.stream);
        assert_eq!(
            fresh.digests, old.digests,
            "{}: the live 1.0 fold moved away from the committed digests",
            old.stream
        );
    }
}

#[test]
fn the_live_oracle_reproduces_the_committed_incremental_reference() {
    let Ok(python) = std::env::var("CADUS_ORACLE_PYTHON") else {
        eprintln!("skipped: CADUS_ORACLE_PYTHON is not set");
        return;
    };
    let text = oracle_index(
        &python,
        "scripts/oracle/incremental_splits_1_0.py",
        "incremental_live.json",
    );
    let live: IncrementalIndex = serde_json::from_str(&text).expect("the live index parses");
    let committed = incremental_index();
    assert_eq!(live.streams.len(), committed.streams.len());
    for (fresh, old) in live.streams.iter().zip(committed.streams.iter()) {
        assert_eq!(fresh.stream, old.stream);
        assert_eq!(fresh.full_digest, old.full_digest, "{}", old.stream);
        assert_eq!(
            fresh.mismatching_splits, old.mismatching_splits,
            "{}: the live 1.0 incremental fold diverges at other splits now",
            old.stream
        );
        assert_eq!(
            fresh.mismatching_digests, old.mismatching_digests,
            "{}: the live 1.0 incremental fold gives another model now",
            old.stream
        );
    }
}
