//! Benchmark B at LIFETIME log length: the serve read and the grade transaction
//! over a log of [`SEEDED_EVENTS`] events (M5 review 1, findings F15 and F18).
//!
//! Requirements: L1 (serve a problem, p95 < 150 ms), L2 (grade a verifiable
//! answer, p95 < 300 ms), L4, L5, C2, C3, D4 (`through_seq` is the fold cursor),
//! D-O1, D-O2, D-S2 (the log grows forever), D-S6.
//!
//! `crates/store/tests/bench_serve_roundtrip.rs` and
//! `crates/store/tests/bench_grade_transaction.rs` seed 0 and 201 events. D-S2
//! says the log grows forever, so those two numbers describe a new account and
//! not a learner. This file seeds the log of a learner two hundred sessions in
//! and holds the SAME two segments of `docs/reference/l1-budget.md`: 100 ms for
//! the serve transaction and 150 ms for the grade transaction.
//!
//! # The two findings
//!
//! - **F18** (`benchmark_long_log_serve_holds_the_l1_segment`): the serve, teach
//!   and hint routes open with `cadus_web::serve::open`, which read the whole
//!   log and then folded it. Both terms grow with the lifetime event count, so
//!   the three routes passed their 150 ms budget on the log alone.
//! - **F15** (`benchmark_long_log_grade_holds_the_l2_segment`): the grade
//!   transaction read the whole log twice, once in `open` and once inside
//!   `project_current`, and then folded it twice.
//!
//! # The fixture is a lifetime, not a session
//!
//! [`SESSIONS`] sessions of [`EVENTS_PER_SESSION`] events each. Every session
//! but the last one is closed. The last one is OPEN and carries
//! [`OPEN_SESSION_EVENTS`] events, so the log of the CURRENT session is short
//! while the log of the account is long. That is the shape D-S2 produces, and it
//! is the shape a per-request whole-log read fails on.
//!
//! # What runs when
//!
//! The file carries two gates, and they run under different rules.
//!
//! - **The counting gate runs ALWAYS** (it needs the test DSN and nothing else).
//!   It asserts the LITERAL row counts and the LITERAL fold cursor of every
//!   sample: the open read decodes the open session's window and never
//!   [`SEEDED_EVENTS`], and each fold advances by exactly one line. Those
//!   numbers are deterministic, so a contended runner cannot move them, and a
//!   whole-log read that comes back fails them at once.
//! - **The timing gate runs under `CADUS_BENCH`**, the switch every other
//!   benchmark of `docs/reference/l1-budget.md` takes. Spec section 10.5: a
//!   benchmark beside a test suite measures the scheduler, so a timing
//!   assertion never runs inside `cargo test --workspace`. It fails at p95 above
//!   the segment, never on p50 and never on one slow sample. A debug build holds
//!   a budget ten times wider, the rule `crates/core/tests/answer_check.rs`
//!   already carries for L2.
//!
//! Without `CADUS_BENCH` the run still takes [`samples`] samples and still
//! prints the percentiles; it takes three of them instead of a hundred, so the
//! counting gate stays cheap.
//!
//! # What the serve half measures (M5 review 2, findings V1 and V8)
//!
//! The serve appends `task_served` the FIRST time a session serves a task, and
//! it folds and saves in the same transaction, so the fold cursor never falls
//! behind the head of the log. The append is idempotent per task id: the first
//! serve of a task pays one fold of the whole log, and the 19 hand-offs after it
//! append nothing and fold nothing. [`serve_once`] drives both shapes. The run
//! times the ONE first serve on its own and prints it, and the p50 and p95 of
//! the table then describe the repeated hand-off, which is the shape of the
//! load. Section 8 of `docs/reference/l1-budget.md` records both numbers.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::Instant;

use cadus_store::test_support::TestDb;
use cadus_testkit::bench::{Percentiles, benchmarks_are_on, budget, write_artifact};
use common::bench::{artifact_json, dsn_set, release, report, rounds, timed_rounds, timed_step};
use common::long_log::{
    BENCH_SAMPLES, BENCH_WARMUPS, COUNTING_SAMPLES, OPEN_SESSION_EVENTS, Read, SEEDED_EVENTS,
    SERVE_P95_BUDGET_NS, log_len, seed, serve_once,
};
use serde_json::json;

/// F18: the serve transaction holds its 100 ms segment at lifetime log length.
#[tokio::test]
async fn benchmark_long_log_serve_holds_the_l1_segment() {
    if !dsn_set("long-log serve") {
        return;
    }
    TestDb::with(|db| async move {
        let run = seed(&db).await;
        let (input, avoid) = run.views();

        // The FIRST serve of the task appends the cadence line and folds the
        // whole log once (V1, V8). It is a different transaction from the 19
        // hand-offs that follow it, so the run times it on its own, prints it,
        // and pins its two literals. No segment of
        // `docs/reference/l1-budget.md` covers it yet: section 8 records the
        // number and names the open question.
        let start = Instant::now();
        let first = serve_once(&run.app, run.user, &input, &avoid)
            .await
            .unwrap_or_else(|err| panic!("the first serve of the task failed: {err}"));
        let first_serve_ns = start.elapsed().as_nanos();
        assert_eq!(
            first.seq,
            SEEDED_EVENTS as i64 + 1,
            "the first serve appended the cadence line at another seq"
        );
        assert_eq!(
            first.folded_through,
            SEEDED_EVENTS as i64 + 1,
            "the first serve left the fold cursor behind the head of the log"
        );
        release(&db.admin, first.claimed).await;

        for _ in 0..rounds(BENCH_WARMUPS, 1) {
            let read = serve_once(&run.app, run.user, &input, &avoid)
                .await
                .unwrap_or_else(|err| panic!("a warm-up serve failed: {err}"));
            release(&db.admin, read.claimed).await;
        }

        let count = rounds(BENCH_SAMPLES, COUNTING_SAMPLES);
        let (timings, reads): (Vec<u128>, Vec<Read>) = timed_rounds(count, |index| {
            let (db, run, input, avoid) = (&db, &run, &input, &avoid);
            async move {
                let (nanos, read) = timed_step(
                    format!("serve sample {index} failed"),
                    serve_once(&run.app, run.user, input, avoid),
                )
                .await;
                release(&db.admin, read.claimed).await;
                (nanos, read)
            }
        })
        .await;

        let times = Percentiles::of(&timings);
        let limit = budget(SERVE_P95_BUDGET_NS);
        report(
            "long-log serve",
            &times,
            timings.len(),
            &format!(
                ", log {SEEDED_EVENTS} events, {} rows decoded per request, budget {limit} ns, \
                 first serve of the task {first_serve_ns} ns",
                reads[0].rows
            ),
        );
        if benchmarks_are_on() {
            write_artifact(
                "benchmark-b-long-log-serve.json",
                &artifact_json(
                    "B-long-log-serve",
                    "serve_ns",
                    &times,
                    limit,
                    &[
                        ("log_events", json!(SEEDED_EVENTS)),
                        ("rows_decoded", json!(reads[0].rows)),
                        ("samples", json!(count)),
                        ("first_serve_ns", json!(first_serve_ns)),
                    ],
                ),
            );
        }

        for (index, read) in reads.iter().enumerate() {
            assert!(
                !read.replayed,
                "serve sample {index} took the full-replay branch; a read route never does"
            );
            // The open read decodes the OPEN SESSION and nothing older: its 100
            // seeded events plus the ONE cadence line the first serve of the run
            // appended. A number that reaches SEEDED_EVENTS means the whole-log
            // read came back.
            assert_eq!(
                read.rows,
                OPEN_SESSION_EVENTS + 1,
                "serve sample {index} decoded another window"
            );
            // The first serve of the run appended the cadence line before the
            // warm-ups. Every sample after it finds that line in the window and
            // appends nothing, so `seq` stays 0.
            assert_eq!(
                read.seq, 0,
                "serve sample {index} appended a second cadence line for one task"
            );
            // The serve folds and saves whenever it appends, so the cursor
            // stands at the head of the log after every serve (V1, V8).
            assert_eq!(
                read.folded_through,
                SEEDED_EVENTS as i64 + 1,
                "serve sample {index} folded to another cursor"
            );
        }
        // One task, one cadence line, however many hand-offs the run drove.
        assert_eq!(
            log_len(&db.admin, run.user).await,
            SEEDED_EVENTS as i64 + 1,
            "the serve run grew the log by more than the one cadence line"
        );
        assert!(
            !benchmarks_are_on() || times.p95 < limit,
            "the p95 serve transaction over {SEEDED_EVENTS} events took {} ns, and the budget \
             is {limit} ns",
            times.p95
        );
    })
    .await;
}
