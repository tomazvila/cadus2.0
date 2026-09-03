//! F15: the grade transaction at lifetime log length. The fixture, the two
//! transactions, and the argument are in `common/long_log.rs` and in the
//! header of `bench_long_log.rs`, which times the serve half.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use std::time::Instant;

use cadus_core::config::Config;
use cadus_core::event::Timestamp;
use cadus_core::pool::Avoid;
use cadus_core::projector::ProjectionInput;
use cadus_store::test_support::TestDb;
use common::bench::{
    Percentiles, artifact_json, budget, curriculum, dsn_set, report, restore, rounds, snapshot,
    timed, timed_rounds, write_artifact,
};
use common::events::BASE_US;
use common::long_log::{
    BENCH_SAMPLES, BENCH_WARMUPS, COUNTING_SAMPLES, GRADE_P95_BUDGET_NS, OPEN_SESSION_EVENTS, Read,
    SEEDED_EVENTS, grade_once, seed,
};
use serde_json::json;

/// F15: the grade transaction holds its 150 ms segment at lifetime log length.
#[tokio::test]
async fn benchmark_long_log_grade_holds_the_l2_segment() {
    if !dsn_set("long-log grade") {
        return;
    }
    TestDb::with(|db| async move {
        let graph = curriculum();
        let cfg = Config::default();
        let (user, ring, task) = seed(&db, &graph, &cfg).await;
        let input = ProjectionInput::new(&graph, &cfg, Timestamp::from_micros(BASE_US));
        let avoid = Avoid::new(&ring, &task);
        let app = db.pool_as("cadus_app", 1).await;

        // The snapshot is taken BEFORE the first grade, so every warm-up and
        // every sample starts from the same cursor and folds the same log.
        let snap = snapshot(&db.admin, user).await;
        for index in 0..rounds(BENCH_WARMUPS, 1) {
            let id = format!("warmup-{index}");
            let read = grade_once(&app, user, &input, &id, &avoid)
                .await
                .unwrap_or_else(|err| panic!("warm-up {index} did not grade: {err}"));
            restore(&db.admin, user, &id, read.claimed, &snap).await;
        }

        let count = rounds(BENCH_SAMPLES, COUNTING_SAMPLES);
        let (timings, reads): (Vec<u128>, Vec<Read>) = timed_rounds(count, |index| {
            let id = format!("sample-{index}");
            let (db, app, input, avoid, snap) = (&db, &app, &input, &avoid, &snap);
            async move {
                let start = Instant::now();
                let read = grade_once(app, user, input, &id, avoid)
                    .await
                    .unwrap_or_else(|err| panic!("grade sample {index} did not grade: {err}"));
                let nanos = start.elapsed().as_nanos();
                restore(&db.admin, user, &id, read.claimed, snap).await;
                (nanos, read)
            }
        })
        .await;

        let times = Percentiles::of(&timings);
        let limit = budget(GRADE_P95_BUDGET_NS);
        report(
            "long-log grade",
            &times,
            timings.len(),
            &format!(
                ", log {SEEDED_EVENTS} events, {} rows decoded per request, budget {limit} ns",
                reads[0].rows
            ),
        );
        if timed() {
            write_artifact(
                "benchmark-b-long-log-grade.json",
                &artifact_json(
                    "B-long-log-grade",
                    "grade_ns",
                    &times,
                    limit,
                    &[
                        ("log_events", json!(SEEDED_EVENTS)),
                        ("rows_decoded", json!(reads[0].rows)),
                        ("samples", json!(count)),
                    ],
                ),
            );
        }

        for (index, read) in reads.iter().enumerate() {
            assert_eq!(
                read.seq,
                SEEDED_EVENTS as i64 + 1,
                "grade sample {index} appended at another seq, so the fixture is growing"
            );
            assert!(
                !read.replayed,
                "grade sample {index} took the full-replay branch (spec section 4.3)"
            );
            assert_eq!(
                read.rows, OPEN_SESSION_EVENTS,
                "grade sample {index} decoded another window"
            );
            // The fold advanced onto the appended line. A cursor that stayed put
            // means the sample folded nothing and measured nothing.
            assert_eq!(
                read.folded_through,
                SEEDED_EVENTS as i64 + 1,
                "grade sample {index} folded to another cursor"
            );
        }
        assert!(
            !timed() || times.p95 < limit,
            "the p95 grade transaction over {SEEDED_EVENTS} events took {} ns, and the budget \
             is {limit} ns",
            times.p95
        );
    })
    .await;
}
