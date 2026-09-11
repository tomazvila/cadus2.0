//! Endpoint failure and partial paid passes must be visible at the CLI boundary.
#![allow(clippy::unwrap_used)]
mod common;
use cadus_store::test_support::TestDb;
use cadus_worker::authoring::{
    budget::Budget,
    job::{AuthoringJob, author_one},
    prompt::Kind,
};
use common::{FakeModel, handle, run_binary, squares_spec, superuser_dsn};

#[tokio::test]
async fn permanent_endpoint_statuses_stop_the_shared_job_after_one_request() {
    TestDb::with(|db| async move {
        for status in [401, 403, 404] {
            let fake = FakeModel::start(vec![(status, "{}".to_owned()); 10]).await;
            let job = AuthoringJob::new(fake.client(4000, 2000))
                .with_budget(Budget::new(5_000_000, 500_000, 16000).unwrap());
            let first = author_one(&handle(&db), &job, Kind::Template, &squares_spec())
                .await
                .unwrap();
            assert_eq!(first.attempts, 1);
            assert_eq!(job.endpoint_failure(), Some(status));
            let second = author_one(&handle(&db), &job, Kind::Template, &squares_spec())
                .await
                .unwrap();
            assert_eq!(second.attempts, 0);
            assert_eq!(fake.call_count(), 1);
        }
    })
    .await;
}

#[tokio::test]
async fn the_cli_reports_endpoint_failure_budget_denial_and_all_declines_as_exit_two() {
    TestDb::with(|db| async move {
        let cases = [
            (
                vec![(404, "{}".to_owned())],
                "5",
                "permanent author endpoint failure HTTP 404",
                1,
            ),
            (
                vec![(429, "{}".to_owned())],
                "0.5",
                "author reservation budget exhausted",
                1,
            ),
            (
                vec![common::tool_reply(&common::missing_low_edge()); 5],
                "5",
                "all requested author documents declined",
                5,
            ),
        ];
        for (replies, budget, reason, calls) in cases {
            let fake = FakeModel::start(replies).await;
            let run = run_binary(
                &superuser_dsn(&db.name),
                &fake.base_url,
                &[
                    "author",
                    "--kp",
                    "perfect-squares/kp1",
                    "--kind",
                    "template",
                    "--budget-usd",
                    budget,
                    "--request-reserve-usd",
                    "0.5",
                ],
            )
            .await;
            assert_eq!(run.code, Some(2), "{}", run.stderr);
            assert!(run.stderr.contains(reason), "{}", run.stderr);
            assert!(run.stderr.contains("partial result: stored 0, declined 1"));
            assert_eq!(fake.call_count(), calls);
        }
    })
    .await;
}
