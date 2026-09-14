//! Report queue isolation, fencing, and atomic source-bound publication.
//!
//! Every case uses TestDb's migrated disposable database. Verifier documents
//! are explicit boundary-test doubles; these tests do not claim a live proof.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::event::Event;
use cadus_store::reports::{self, ReportJob};
use cadus_store::state::{append_event, project_and_save, project_current};
use cadus_store::test_support::TestDb;
use cadus_store::{Db, begin_tenant};
use common::events::{SESSION, start};
use common::state::{Scene, open_locked, read_log};
use serde_json::{Value, json};
use sqlx::types::Uuid;

const TASK: &str = "s_2026-01-01a-review-addition";

struct Fixture {
    user: Uuid,
    attempt: String,
    problem: String,
    source_hash: String,
    input: Value,
}

async fn fixture(db: &TestDb, email: &str, outcome: &str) -> Fixture {
    let user = db.seed_user(email).await;
    let attempt = Uuid::new_v4().to_string();
    let problem = Uuid::new_v4().to_string();
    let event = Event::from_json(&json!({
        "type":"attempt","v":2,"ts":"2026-01-01T00:00:00Z","session":SESSION,
        "attempt_id":attempt,"task_id":TASK,"topic":"addition","kp":"kp1",
        "task_type":"review","problem":{"text":"Compute 8 - 5.","expected":"4"},
        "given_answer":"3","answer_kind":"numeric","correct":outcome=="correct",
        "outcome":if outcome=="ungraded" { json!({"ungraded":{"reason":"reported format"}}) } else { json!(outcome) },
        "secs":12,"work_quality":"nearly_passable","assisted":false
    }).to_string()).unwrap();
    let handoff = Event::from_json(
        &json!({
            "type":"ordinary_problem_served","v":2,"ts":"2026-01-01T00:00:00Z",
            "session":SESSION,"task_id":TASK,"problem_id":problem,"kp_id":"addition/kp1",
            "item_digest":"a".repeat(64),"item_source":"exemplar","exposure":"first"
        })
        .to_string(),
    )
    .unwrap();
    let app = common::app_db(db);
    let mut tx = open_locked(&app, user).await;
    assert_eq!(
        append_event(&mut tx, user, &start(SESSION), None)
            .await
            .unwrap(),
        Some(1)
    );
    assert_eq!(
        append_event(&mut tx, user, &handoff, None).await.unwrap(),
        Some(2)
    );
    assert_eq!(
        append_event(&mut tx, user, &event, Some(&attempt))
            .await
            .unwrap(),
        Some(3)
    );
    tx.commit().await.unwrap();
    let event = json!(event);
    let source = json!({
        "problem":event["problem"],"curriculum_digest":"b".repeat(64),
        "engine_digest":cadus_core::review_engine::DIGEST
    });
    let source_hash: String =
        sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
            .bind(reports::source_identity(&source).to_string())
            .fetch_one(&db.admin)
            .await
            .unwrap();
    let input = json!({
        "attempt":event,"event_seq":3,"source":source,"note":"Please review the answer.",
        "problem_id":problem,"correction_version":0,"previous_correction":null,
        "task_policy":{"config":cadus_core::config::Config::default(),
            "knowledge_points":["kp1"],"expected_time_secs":60}
    });
    Fixture {
        user,
        attempt,
        problem,
        source_hash,
        input,
    }
}

async fn worker(db: &TestDb) -> Db {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SET ROLE cadus_admin")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(db.admin.connect_options().as_ref().clone())
        .await
        .unwrap();
    Db::new(pool, 3000)
}

fn row_id(row: &Value) -> Uuid {
    row["id"].as_str().unwrap().parse().unwrap()
}

/// The web-side dedup/capacity protocol runs under the tenant advisory lock.
async fn submit(db: &TestDb, fixture: &Fixture, request: Uuid) -> Value {
    let app = common::app_db(db);
    let mut tx = open_locked(&app, fixture.user).await;
    if let Some(existing) = reports::existing(&mut tx, fixture.user, request, &fixture.attempt)
        .await
        .unwrap()
    {
        tx.commit().await.unwrap();
        return existing;
    }
    assert!(
        reports::capacity(&mut tx, fixture.user, &fixture.attempt)
            .await
            .unwrap()
    );
    let owned = reports::attempt(
        &mut tx,
        fixture.user,
        TASK,
        &fixture.problem,
        &fixture.attempt,
    )
    .await
    .unwrap();
    assert!(owned.is_some());
    let row = reports::enqueue(
        &mut tx,
        fixture.user,
        request,
        TASK,
        &fixture.problem,
        &fixture.attempt,
        &fixture.source_hash,
        &fixture.input,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    row
}

async fn report(db: &TestDb, fixture: &Fixture, id: Uuid) -> Value {
    let mut tx = begin_tenant(&db.app, fixture.user).await.unwrap();
    let row = reports::get(&mut tx, fixture.user, id)
        .await
        .unwrap()
        .unwrap();
    tx.rollback().await.unwrap();
    row
}

async fn counts(db: &TestDb) -> (i64, i64) {
    let corrections = sqlx::query_scalar("SELECT count(*) FROM problem_corrections")
        .fetch_one(&db.admin)
        .await
        .unwrap();
    let regrades = sqlx::query_scalar("SELECT count(*) FROM events WHERE type='regraded'")
        .fetch_one(&db.admin)
        .await
        .unwrap();
    (corrections, regrades)
}

fn success() -> Value {
    json!({
        "resolution":"confirmed_issue","message":"Boundary-test verified correction",
        "qwen_verdict":"correct","verification":"proved",
        "grade_corrected":false,"content_published":false
    })
}

fn correction(fixture: &Fixture) -> Value {
    json!({
        "candidate_answer":"3","solution":"Subtract 5 from 8 to obtain 3.",
        "accepted_answers":["3","6/2"],"formal_problem":{"kind":"numeric_expression","expression":"8-5"},
        "verification":{
            "source_hash":fixture.source_hash,"supported":true,"status":"completed",
            "learner":{"status":"proved","message":"test double"},
            "candidate":{"status":"proved","message":"test double"},
            "engine":{"test_double":true},"evidence":{"scope":"formalized mathematics"}
        },
        "regressions":[{"answer":"3","correct":true},{"answer":"4","correct":false}]
    })
}

#[tokio::test]
async fn report_reads_and_attempt_reconstruction_are_tenant_bound() {
    TestDb::with(|db| async move {
        let alice = fixture(&db, "reports-alice@example.com", "incorrect").await;
        let bob = fixture(&db, "reports-bob@example.com", "incorrect").await;
        let request = Uuid::new_v4();
        let row = submit(&db, &alice, request).await;
        let mut tx = begin_tenant(&db.app, bob.user).await.unwrap();
        assert!(
            reports::get(&mut tx, bob.user, row_id(&row))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            reports::get(&mut tx, alice.user, row_id(&row))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            reports::existing(&mut tx, alice.user, request, &alice.attempt)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            reports::attempt(&mut tx, alice.user, TASK, &alice.problem, &alice.attempt)
                .await
                .unwrap()
                .is_none()
        );
        let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM problem_reports")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(visible, 0);
        tx.rollback().await.unwrap();

        let mut tx = begin_tenant(&db.app, alice.user).await.unwrap();
        assert!(
            reports::attempt(
                &mut tx,
                alice.user,
                TASK,
                "different-problem",
                &alice.attempt
            )
            .await
            .unwrap()
            .is_none()
        );
        assert!(
            reports::attempt(
                &mut tx,
                alice.user,
                "different-task",
                &alice.problem,
                &alice.attempt
            )
            .await
            .unwrap()
            .is_none()
        );
        tx.rollback().await.unwrap();

        let mut tx = begin_tenant(&db.app, bob.user).await.unwrap();
        let forbidden = reports::enqueue(
            &mut tx,
            alice.user,
            Uuid::new_v4(),
            TASK,
            &alice.problem,
            "forged-attempt",
            &alice.source_hash,
            &alice.input,
        )
        .await;
        assert!(
            forbidden.is_err(),
            "another tenant inserted an owned report"
        );
        tx.rollback().await.unwrap();
        assert_eq!(report(&db, &alice, row_id(&row)).await["status"], "queued");
    })
    .await;
}

#[tokio::test]
async fn request_and_attempt_dedup_precede_active_capacity() {
    TestDb::with(|db| async move {
        let alice = fixture(&db, "reports-dedup@example.com", "incorrect").await;
        let request = Uuid::new_v4();
        let first = submit(&db, &alice, request).await;
        assert_eq!(row_id(&submit(&db, &alice, request).await), row_id(&first));
        assert_eq!(
            row_id(&submit(&db, &alice, Uuid::new_v4()).await),
            row_id(&first)
        );
        let mut tx = begin_tenant(&db.app, alice.user).await.unwrap();
        for number in 0..2 {
            reports::enqueue(
                &mut tx,
                alice.user,
                Uuid::new_v4(),
                TASK,
                &alice.problem,
                &format!("other-{number}"),
                &alice.source_hash,
                &alice.input,
            )
            .await
            .unwrap();
        }
        assert!(
            !reports::capacity(&mut tx, alice.user, "fourth-attempt")
                .await
                .unwrap()
        );
        tx.commit().await.unwrap();
        assert_eq!(row_id(&submit(&db, &alice, request).await), row_id(&first));
        let bob = db.seed_user("reports-dedup-bob@example.com").await;
        let mut tx = begin_tenant(&db.app, bob).await.unwrap();
        assert!(
            reports::capacity(&mut tx, bob, "new-attempt")
                .await
                .unwrap()
        );
        tx.rollback().await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM problem_reports")
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(count, 3);
    })
    .await;
}

#[tokio::test]
async fn capacity_bounds_daily_reports_and_total_attempt_retries() {
    TestDb::with(|db| async move {
        let daily = fixture(&db, "reports-daily@example.com", "incorrect").await;
        for number in 0..20 {
            let mut tx = begin_tenant(&db.app, daily.user).await.unwrap();
            assert!(
                reports::capacity(&mut tx, daily.user, &format!("daily-{number}"))
                    .await
                    .unwrap()
            );
            reports::enqueue(
                &mut tx,
                daily.user,
                Uuid::new_v4(),
                TASK,
                &daily.problem,
                &format!("daily-{number}"),
                &daily.source_hash,
                &daily.input,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
            sqlx::query("UPDATE problem_reports SET status='unresolved' WHERE user_id=$1")
                .bind(daily.user)
                .execute(&db.admin)
                .await
                .unwrap();
        }
        let mut tx = begin_tenant(&db.app, daily.user).await.unwrap();
        assert!(
            !reports::capacity(&mut tx, daily.user, "day-21")
                .await
                .unwrap()
        );
        tx.rollback().await.unwrap();
        sqlx::query(
            "UPDATE problem_reports SET created_at=now()-interval '2 days' WHERE user_id=$1",
        )
        .bind(daily.user)
        .execute(&db.admin)
        .await
        .unwrap();
        let mut tx = begin_tenant(&db.app, daily.user).await.unwrap();
        assert!(
            reports::capacity(&mut tx, daily.user, "new-day")
                .await
                .unwrap()
        );
        tx.rollback().await.unwrap();

        let retry = fixture(&db, "reports-retries@example.com", "incorrect").await;
        for _ in 0..3 {
            submit(&db, &retry, Uuid::new_v4()).await;
            sqlx::query("UPDATE problem_reports SET status='unresolved' WHERE user_id=$1")
                .bind(retry.user)
                .execute(&db.admin)
                .await
                .unwrap();
        }
        let mut tx = begin_tenant(&db.app, retry.user).await.unwrap();
        assert!(
            !reports::capacity(&mut tx, retry.user, &retry.attempt)
                .await
                .unwrap()
        );
        assert!(
            reports::capacity(&mut tx, retry.user, "different-attempt")
                .await
                .unwrap()
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn expired_and_forged_leases_cannot_write_or_finish() {
    TestDb::with(|db| async move {
        let item = fixture(&db, "reports-leases@example.com", "incorrect").await;
        let row = submit(&db, &item, Uuid::new_v4()).await;
        let handle = worker(&db).await;
        let first = reports::claim(&handle).await.unwrap().unwrap();
        assert_eq!(first.id, row_id(&row));
        assert_eq!(first.attempt, 1);
        assert!(reports::claim(&handle).await.unwrap().is_none());
        let forged = ReportJob {
            id: first.id,
            user_id: first.user_id,
            lease: Uuid::new_v4(),
            input: first.input.clone(),
            source_hash: first.source_hash.clone(),
            attempt: first.attempt,
        };
        assert!(
            !reports::heartbeat(&handle, &forged, "forged")
                .await
                .unwrap()
        );
        assert!(
            !reports::record_step(&handle, &forged, "forged", &json!({}))
                .await
                .unwrap()
        );
        assert!(
            !reports::finish(&handle, &forged, &success(), Some(&correction(&item)))
                .await
                .unwrap()
        );
        assert!(
            reports::record_step(&handle, &first, "owned", &json!({"ok":true}))
                .await
                .unwrap()
        );
        sqlx::query("UPDATE problem_reports SET lease_until=now()-interval '1 second' WHERE id=$1")
            .bind(first.id)
            .execute(&db.admin)
            .await
            .unwrap();
        assert!(
            !reports::heartbeat(&handle, &first, "expired")
                .await
                .unwrap()
        );
        assert!(
            !reports::finish(&handle, &first, &success(), Some(&correction(&item)))
                .await
                .unwrap()
        );
        let second = reports::claim(&handle).await.unwrap().unwrap();
        assert_eq!(second.attempt, 2);
        assert_ne!(second.lease, first.lease);
        assert!(
            !reports::record_step(&handle, &first, "stale", &json!({}))
                .await
                .unwrap()
        );
        sqlx::query("UPDATE problem_reports SET lease_until=now()-interval '1 second' WHERE id=$1")
            .bind(second.id)
            .execute(&db.admin)
            .await
            .unwrap();
        let third = reports::claim(&handle).await.unwrap().unwrap();
        assert_eq!(third.attempt, 3);
        sqlx::query("UPDATE problem_reports SET lease_until=now()-interval '1 second' WHERE id=$1")
            .bind(third.id)
            .execute(&db.admin)
            .await
            .unwrap();
        assert!(reports::claim(&handle).await.unwrap().is_none());
        assert_eq!(report(&db, &item, first.id).await["status"], "failed");
        assert_eq!(counts(&db).await, (0, 0));
        let steps: i64 = sqlx::query_scalar("SELECT count(*) FROM problem_report_steps")
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(steps, 1);
        handle.pool().close().await;
    })
    .await;
}

#[tokio::test]
async fn unsupported_unresolved_and_wrong_source_proofs_cannot_publish() {
    TestDb::with(|db| async move {
        let handle = worker(&db).await;
        for case in 0..5 {
            let item = fixture(
                &db,
                &format!("reports-reject-{case}@example.com"),
                "incorrect",
            )
            .await;
            let row = submit(&db, &item, Uuid::new_v4()).await;
            let job = reports::claim(&handle).await.unwrap().unwrap();
            let mut proposed = correction(&item);
            let mut result = success();
            match case {
                0 => proposed["verification"]["supported"] = json!(false),
                1 => proposed["verification"]["status"] = json!("unresolved"),
                2 => proposed["verification"]["learner"]["status"] = json!("unresolved"),
                3 => proposed["verification"]["source_hash"] = json!("c".repeat(64)),
                4 => result["verification"] = json!("unresolved"),
                _ => unreachable!(),
            }
            result["grade_corrected"] = json!(true);
            result["content_published"] = json!(true);
            assert!(
                reports::finish(&handle, &job, &result, Some(&proposed))
                    .await
                    .unwrap()
            );
            let stored = report(&db, &item, row_id(&row)).await;
            assert_eq!(stored["result"]["resolution"], "needs_review");
            assert_eq!(stored["result"]["grade_corrected"], false);
            assert_eq!(stored["result"]["content_published"], false);
            assert_eq!(read_log(&common::app_db(&db), item.user).await.len(), 3);
        }
        assert_eq!(counts(&db).await, (0, 0));
        handle.pool().close().await;
    })
    .await;
}

#[tokio::test]
async fn verified_publication_appends_regrade_and_native_replay_preserves_original() {
    TestDb::with(|db| async move {
        let item = fixture(&db, "reports-publish@example.com", "ungraded").await;
        let scene = Scene::new(&db);
        let mut tx = open_locked(&scene.handle, item.user).await;
        let before = project_and_save(&mut tx, item.user, &scene.input(), None)
            .await
            .unwrap();
        assert_eq!(before.model.ungraded.len(), 1);
        assert_eq!(before.through_seq, 3);
        tx.commit().await.unwrap();
        let old_log = read_log(&scene.handle, item.user).await;
        let row = submit(&db, &item, Uuid::new_v4()).await;
        let handle = worker(&db).await;
        let job = reports::claim(&handle).await.unwrap().unwrap();
        assert!(
            reports::finish(&handle, &job, &success(), Some(&correction(&item)))
                .await
                .unwrap()
        );
        assert_eq!(counts(&db).await, (1, 1));
        let stored = report(&db, &item, row_id(&row)).await;
        assert_eq!(stored["status"], "completed");
        assert_eq!(stored["result"]["grade_corrected"], true);
        assert_eq!(stored["result"]["content_published"], true);
        let new_log = read_log(&scene.handle, item.user).await;
        assert_eq!(new_log.len(), 4);
        let raw: Vec<sqlx::types::Json<Event>> = sqlx::query_scalar(
            "SELECT payload FROM events WHERE user_id=$1 AND seq<=$2 ORDER BY seq",
        ).bind(item.user).bind(before.through_seq).fetch_all(&db.admin).await.unwrap();
        for (before, after) in old_log.iter().zip(raw) {
            let mut event = after.0;
            event.normalize();
            assert_eq!(before.event, event, "a historical event changed");
        }
        assert!(matches!(&new_log[2].event, Event::Attempt(attempt) if attempt.correct));
        let Event::Regraded(regraded) = &new_log.last().unwrap().event else {
            panic!("publication appended no native regrade");
        };
        assert_eq!(regraded.attempts[0].attempt_id, item.attempt);
        assert_eq!(
            regraded.attempts[0].outcome,
            Some(cadus_core::event::AttemptOutcome::Correct)
        );
        let mut tx = open_locked(&scene.handle, item.user).await;
        let replayed = project_current(&mut tx, item.user, &scene.input())
            .await
            .unwrap();
        assert!(replayed.replayed);
        assert_eq!(replayed.through_seq, 4);
        assert!(replayed.model.ungraded.is_empty());
        tx.rollback().await.unwrap();
        assert!(
            !reports::finish(&handle, &job, &success(), Some(&correction(&item)))
                .await
                .unwrap()
        );
        assert_eq!(counts(&db).await, (1, 1));
        handle.pool().close().await;
    })
    .await;
}

#[tokio::test]
async fn correction_version_cas_rejects_stale_jobs_and_formal_drift() {
    TestDb::with(|db| async move {
        let first = fixture(&db, "reports-version-a@example.com", "incorrect").await;
        let mut second = fixture(&db, "reports-version-b@example.com", "incorrect").await;
        assert_eq!(first.source_hash, second.source_hash);
        submit(&db, &first, Uuid::new_v4()).await;
        let second_row = submit(&db, &second, Uuid::new_v4()).await;
        let handle = worker(&db).await;
        let job_a = reports::claim(&handle).await.unwrap().unwrap();
        assert_eq!(job_a.user_id, first.user);
        assert!(
            reports::finish(&handle, &job_a, &success(), Some(&correction(&first)))
                .await
                .unwrap()
        );
        let stale = reports::claim(&handle).await.unwrap().unwrap();
        assert_eq!(stale.user_id, second.user);
        assert!(
            reports::finish(&handle, &stale, &success(), Some(&correction(&second)))
                .await
                .unwrap()
        );
        assert_eq!(
            report(&db, &second, row_id(&second_row)).await["result"]["resolution"],
            "needs_review"
        );
        assert_eq!(counts(&db).await, (1, 1));
        assert_eq!(read_log(&common::app_db(&db), second.user).await.len(), 3);
        let mut tx = begin_tenant(&db.app, second.user).await.unwrap();
        let published = reports::published(&mut tx, &second.source_hash)
            .await
            .unwrap()
            .unwrap();
        tx.rollback().await.unwrap();
        second.input["correction_version"] = json!(1);
        second.input["previous_correction"] = published;
        let drift_row = submit(&db, &second, Uuid::new_v4()).await;
        let drift_job = reports::claim(&handle).await.unwrap().unwrap();
        let mut drift = correction(&second);
        drift["formal_problem"]["expression"] = json!("8-4");
        assert!(
            reports::finish(&handle, &drift_job, &success(), Some(&drift))
                .await
                .unwrap()
        );
        assert_eq!(
            report(&db, &second, row_id(&drift_row)).await["result"]["resolution"],
            "needs_review"
        );
        assert_eq!(counts(&db).await, (1, 1));
        let final_row = submit(&db, &second, Uuid::new_v4()).await;
        let current = reports::claim(&handle).await.unwrap().unwrap();
        let mut revised = correction(&second);
        revised["accepted_answers"] = json!(["3", "1+2"]);
        assert!(
            reports::finish(&handle, &current, &success(), Some(&revised))
                .await
                .unwrap()
        );
        assert_eq!(counts(&db).await, (2, 2));
        let mut tx = begin_tenant(&db.app, second.user).await.unwrap();
        let newest = reports::published(&mut tx, &second.source_hash)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(newest["version"], 2);
        assert!(
            newest["body"]["accepted_answers"]
                .as_array()
                .unwrap()
                .contains(&json!("6/2"))
        );
        tx.rollback().await.unwrap();
        assert_eq!(
            report(&db, &second, row_id(&final_row)).await["result"]["content_published"],
            true
        );
        handle.pool().close().await;
    })
    .await;
}

#[tokio::test]
async fn failed_native_event_append_rolls_back_publication_and_final_status() {
    TestDb::with(|db| async move {
        let item = fixture(&db, "reports-atomic@example.com", "incorrect").await;
        let row = submit(&db, &item, Uuid::new_v4()).await;
        let handle = worker(&db).await;
        let mut job = reports::claim(&handle).await.unwrap().unwrap();
        job.input["attempt"] = json!({"type":"attempt"});
        assert!(
            reports::finish(&handle, &job, &success(), Some(&correction(&item)))
                .await
                .is_err()
        );
        assert_eq!(counts(&db).await, (0, 0));
        assert_eq!(read_log(&common::app_db(&db), item.user).await.len(), 3);
        let stored = report(&db, &item, row_id(&row)).await;
        assert_eq!(stored["status"], "running");
        assert!(stored["result"].is_null());
        handle.pool().close().await;
    })
    .await;
}

#[tokio::test]
async fn an_already_correct_attempt_does_not_get_a_redundant_regrade() {
    TestDb::with(|db| async move {
        let item = fixture(&db, "reports-already-correct@example.com", "correct").await;
        let row = submit(&db, &item, Uuid::new_v4()).await;
        let handle = worker(&db).await;
        let job = reports::claim(&handle).await.unwrap().unwrap();
        assert!(
            reports::finish(&handle, &job, &success(), Some(&correction(&item)))
                .await
                .unwrap()
        );
        assert_eq!(counts(&db).await, (1, 0));
        let stored = report(&db, &item, row_id(&row)).await;
        assert_eq!(stored["result"]["grade_corrected"], false);
        assert_eq!(stored["result"]["content_published"], true);
        handle.pool().close().await;
    })
    .await;
}

#[tokio::test]
async fn corrected_closed_review_replays_cached_result_and_session_xp() {
    TestDb::with(|db| async move {
        let item = fixture(&db, "report-closed-review@example.test", "incorrect").await;
        let scene = Scene::new(&db);
        let mut tx = open_locked(&scene.handle, item.user).await;
        let result = Event::from_json(&json!({
            "type":"review_result","ts":"2026-01-01T00:00:01Z","session":SESSION,
            "task_id":TASK,"topic":"addition","passed":false,"weighted_score":0.0,
            "inconclusive":true,"quality_tier":"poor","xp":0.0
        }).to_string()).unwrap();
        append_event(&mut tx,item.user,&result,None).await.unwrap();
        let before = project_and_save(&mut tx,item.user,&scene.input(),None).await.unwrap();
        tx.commit().await.unwrap();
        submit(&db,&item,Uuid::new_v4()).await;
        let handle = worker(&db).await;
        let job = reports::claim(&handle).await.unwrap().unwrap();
        reports::finish(&handle,&job,&success(),Some(&correction(&item))).await.unwrap();
        let mut tx = open_locked(&scene.handle,item.user).await;
        let replayed = project_current(&mut tx,item.user,&scene.input()).await.unwrap();
        assert!(replayed.replayed);
        assert!(replayed.model.xp.total > before.model.xp.total);
        assert!(replayed.view.session_xp[SESSION] > before.view.session_xp.get(SESSION).copied().unwrap_or_default());
        let effective = cadus_store::state::load_events(&mut tx,item.user).await.unwrap();
        let Event::ReviewResult(result) = &effective[3].event else { panic!() };
        assert!(result.passed);
        assert!(!result.inconclusive);
        assert_eq!(result.weighted_score,1.0);
        assert!(result.xp > 0.0);
        let original: Value = sqlx::query_scalar("SELECT payload FROM events WHERE user_id=$1 AND seq=4")
            .bind(item.user).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(original["passed"],false);
        assert_eq!(original["xp"],0.0);
        tx.rollback().await.unwrap();
        handle.pool().close().await;
    }).await;
}
