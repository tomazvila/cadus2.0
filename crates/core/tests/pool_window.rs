//! M4 U3 acceptance, part 2: the two anti-repeat windows and the counters
//! (D5, D-S6).
//!
//! Every window size and every document body below is a literal from
//! `docs/reference/serving-1.0-spec.md` section 5.5 and from the 1.0 files that
//! hold the two sizes (`cadus/projector.py:92`, `cadus_web/state.py:133`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use cadus_core::pool::{Avoid, PoolCounters, Ring, TaskMemory, pick};

// --------------------------------------------------------------------------
// Acceptance 4: the ring keeps 20 and drops the oldest
// --------------------------------------------------------------------------

#[test]
fn the_ring_keeps_twenty_and_drops_the_oldest() {
    let mut ring = Ring::new();
    for index in 0..25 {
        ring.push(&format!("hash{index:02}"));
    }
    assert_eq!(ring.len(), 20);
    assert!(!ring.is_empty());
    assert_eq!(ring.hashes()[0], "hash05", "the newest 20 start at hash05");
    assert_eq!(ring.hashes()[19], "hash24", "the newest entry is last");
    assert!(!ring.contains("hash04"), "hash04 fell out of the window");
    assert!(ring.contains("hash05"));
    assert!(ring.contains("hash24"));

    ring.clear();
    assert!(ring.is_empty());
    assert_eq!(ring.len(), 0);
}

#[test]
fn the_task_memory_keeps_twelve_and_drops_the_oldest() {
    let mut task = TaskMemory::new();
    for index in 0..15 {
        task.push(&format!("hash{index:02}"));
    }
    assert_eq!(task.len(), 12);
    assert_eq!(task.hashes()[0], "hash03");
    assert_eq!(task.hashes()[11], "hash14");
    assert!(!task.contains("hash02"));
    assert!(task.contains("hash03"));
}

#[test]
fn an_over_long_stored_window_loads_to_a_legal_window() {
    let over_long: Vec<String> = (0..30).map(|index| format!("hash{index:02}")).collect();
    let ring = Ring::from_hashes(over_long.clone());
    assert_eq!(ring.len(), 20);
    assert_eq!(ring.hashes()[0], "hash10");

    let task = TaskMemory::from_hashes(over_long);
    assert_eq!(task.len(), 12);
    assert_eq!(task.hashes()[0], "hash18");
}

#[test]
fn the_ring_keeps_a_repeated_digest_the_way_the_one_zero_fold_does() {
    // `cadus/projector.py:213-224` appends and truncates. It does not deduplicate.
    let mut ring = Ring::new();
    ring.push("e4047cd6798e");
    ring.push("e4047cd6798e");
    assert_eq!(ring.len(), 2);
    assert_eq!(ring.hashes(), ["e4047cd6798e", "e4047cd6798e"]);
}

// --------------------------------------------------------------------------
// The D-S6 documents
// --------------------------------------------------------------------------

#[test]
fn the_two_windows_are_the_d_s6_serde_documents() {
    let ring = Ring::from_hashes(["e4047cd6798e", "c3d8b10562c1"]);
    let body = serde_json::to_string(&ring).unwrap();
    assert_eq!(body, r#"{"hashes":["e4047cd6798e","c3d8b10562c1"]}"#);
    assert_eq!(serde_json::from_str::<Ring>(&body).unwrap(), ring);

    let task = TaskMemory::from_hashes(["44b34b7dc138"]);
    let body = serde_json::to_string(&task).unwrap();
    assert_eq!(body, r#"{"hashes":["44b34b7dc138"]}"#);
    assert_eq!(serde_json::from_str::<TaskMemory>(&body).unwrap(), task);

    // A missing array reads as an empty window, so a state row written before
    // this milestone still loads.
    assert_eq!(serde_json::from_str::<Ring>("{}").unwrap(), Ring::new());
    assert_eq!(
        serde_json::from_str::<TaskMemory>("{}").unwrap(),
        TaskMemory::new()
    );

    // An unknown key is a refusal, not a silently dropped instruction.
    assert!(serde_json::from_str::<Ring>(r#"{"texts":[]}"#).is_err());
}

#[test]
fn the_counters_round_trip_as_a_document() {
    let mut counters = PoolCounters::new();
    assert_eq!(counters, PoolCounters::default());
    counters.record(pick(&["aaaaaaaaaaaa"], &Avoid::none()).unwrap());
    assert_eq!(counters.served, 1);
    assert_eq!(counters.blocked, 0);
    assert_eq!(counters.pool_exhausted, 0);

    counters.record_undecodable(2);
    assert_eq!(counters.pool_row_undecodable, 2);

    let body = serde_json::to_string(&counters).unwrap();
    assert_eq!(
        body,
        r#"{"served":1,"blocked":0,"pool_exhausted":0,"pool_row_undecodable":2}"#
    );
    assert_eq!(
        serde_json::from_str::<PoolCounters>(&body).unwrap(),
        counters
    );
}
