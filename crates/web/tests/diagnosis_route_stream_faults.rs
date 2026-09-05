//! The A4 client routes under a fault, and the notices the stream keeps
//! quiet about: a job that is absent, one that is still pending, a re-read
//! the tenant bind refuses, and a stream that fell behind the hub.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::diagnosis::*;

use std::time::Duration;

use cadus_store::diagnosis::Notice;
use cadus_web::diagnosis::DiagnosisHub;
use common::{assert_internal, fail_reads, fail_tenant_bind};

/// Fail the test when `body` carries a frame within two seconds.
async fn assert_no_frame(body: &mut Body) {
    assert_eq!(next_frame(body, Duration::from_secs(2)).await, None);
}

/// A tenant bind that fails and a job read that fails each stop the poll.
#[tokio::test]
async fn a_bind_or_a_read_that_fails_is_500_on_the_poll() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "poll-fault@example.com").await;
        let job = seed_done_job(&db, user, ATTEMPT_ID, &ready_slip()).await;
        let path = format!("/api/diagnosis/{job}");
        fail_reads(&db, "diagnosis_jobs", "FROM diagnosis_jobs").await;
        assert_internal(&app, Method::GET, &path, user, None).await;
        fail_tenant_bind(&db).await;
        assert_internal(&app, Method::GET, &path, user, None).await;
    })
    .await;
}

/// A notice for a row that is absent, a row that is still pending, and a row
/// the tenant bind cannot re-read each makes no frame.
#[tokio::test]
async fn a_notice_the_re_read_cannot_answer_makes_no_frame() {
    TestDb::with(|db| async move {
        let hub = Arc::new(DiagnosisHub::new());
        let app = app_with(&db, &hub);
        let user = learner(&db, "quiet-stream@example.com").await;
        let mut stream = open_stream(&app, user).await;

        hub.publish(Notice {
            job_id: Uuid::new_v4(),
            user_id: user,
        });
        assert_no_frame(&mut stream).await;

        let pending = seed_pending_job(&db, user, ATTEMPT_ID).await;
        hub.publish(Notice {
            job_id: pending,
            user_id: user,
        });
        assert_no_frame(&mut stream).await;

        let done = seed_done_job(&db, user, ATTEMPT_ID_2, &ready_slip()).await;
        fail_tenant_bind(&db).await;
        hub.publish(Notice {
            job_id: done,
            user_id: user,
        });
        assert_no_frame(&mut stream).await;
    })
    .await;
}

/// A stream that fell behind the hub logs the miss and carries on with the
/// next notice of its tenant.
#[tokio::test]
async fn a_stream_that_fell_behind_carries_on() {
    TestDb::with(|db| async move {
        let hub = Arc::new(DiagnosisHub::new());
        let app = app_with(&db, &hub);
        let user = learner(&db, "lagging-stream@example.com").await;
        let mut stream = open_stream(&app, user).await;

        let other = Uuid::new_v4();
        for _ in 0..400 {
            hub.publish(Notice {
                job_id: Uuid::new_v4(),
                user_id: other,
            });
        }
        let done = seed_done_job(&db, user, ATTEMPT_ID, &ready_slip()).await;
        hub.publish(Notice {
            job_id: done,
            user_id: user,
        });

        let frame = next_frame(&mut stream, Duration::from_secs(10))
            .await
            .expect("the stream must carry the notice after the miss");
        assert!(frame.contains(&done.to_string()), "{frame}");
    })
    .await;
}
