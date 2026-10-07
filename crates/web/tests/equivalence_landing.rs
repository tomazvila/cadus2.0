//! The background check owns the verdict: a done, accepted job rewrites the
//! stored attempt once, the poll reply derives from the stored job, and a
//! repeat of the normalized answer meets the cache.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::event::{
    AttemptOutcome, Event, Regraded, RegradedAttempt, SchemaVersion, Slug, Timestamp, WorkQuality,
};
use cadus_store::equivalence::{self, JOB_DONE, Verdict};
use cadus_store::test_support::TestDb;
use cadus_web::equivalence::cache_item_digest;
use common::{
    LESSON, PROBLEM_ID, answer_task_ok, get_ok, lesson_app, lesson_learner, lesson_problem,
};
use serde_json::json;
use sqlx::types::Uuid;

/// The wrong-looking answer the background check later accepts.
const WRONG_FIRST_PASS: &str = "13";

/// What the worker does when it accepts: the lines, the `regraded` event, the
/// cache row and the settle.
async fn worker_accepts(db: &TestDb, user: Uuid, job: Uuid, answer: &str) {
    let detail = equivalence::detail(&db.admin, job).await.unwrap().unwrap();
    let payload: equivalence::JobPayload = serde_json::from_value(detail.payload).unwrap();
    let verdict = Verdict {
        equivalent: true,
        reason: "the same value in other words".to_owned(),
        model: "test-model".to_owned(),
    };
    equivalence::add_step(&db.admin, job, "Reading your answer: `13`")
        .await
        .unwrap();
    equivalence::add_step(
        &db.admin,
        job,
        "Result: correct. the same value in other words",
    )
    .await
    .unwrap();
    let mut tx = db.admin.begin().await.unwrap();
    equivalence::cache_put(
        &mut *tx,
        &payload.item_digest,
        &equivalence::cache_key(answer),
        &verdict,
    )
    .await
    .unwrap();
    let event = Event::Regraded(Regraded {
        ts: Timestamp::from_micros(chrono_now()),
        session: None,
        v: SchemaVersion::current(),
        task_id: payload.task_id.clone(),
        topic: Slug::new(&payload.topic).unwrap(),
        attempts: vec![RegradedAttempt {
            attempt_id: format!("{LESSON}-1"),
            outcome: Some(AttemptOutcome::Correct),
            work_quality: WorkQuality::NearlyPerfect,
            error_tags: Vec::new(),
            grader_note: Some("equivalence (background model)".to_owned()),
        }],
        quality_tier: None,
        xp: None,
        reason: "test".to_owned(),
    });
    cadus_store::state::append_event(&mut tx, user, &event, None)
        .await
        .unwrap();
    equivalence::settle(
        &mut *tx,
        job,
        JOB_DONE,
        Some(&serde_json::to_value(&verdict).unwrap()),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

fn chrono_now() -> i64 {
    sqlx::types::chrono::Utc::now().timestamp_micros()
}

async fn job_count(db: &TestDb, user: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM equivalence_jobs WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

#[tokio::test]
async fn an_accepted_check_rewrites_the_attempt_once_and_the_repeat_hits_the_cache() {
    TestDb::with(|db| async move {
        let served = lesson_problem(5.0, "kp1", Vec::new());
        let user = lesson_learner(&db, "landing@example.test", served.clone()).await;
        let app = lesson_app(&db);
        let reply = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": WRONG_FIRST_PASS}),
        )
        .await;
        assert_eq!(reply["correct"], json!(false), "{reply}");
        assert_eq!(reply["equivalence"]["status"], json!("pending"));
        assert_eq!(reply["equivalence"]["steps"], json!([]));
        let job = Uuid::parse_str(reply["equivalence"]["id"].as_str().unwrap()).unwrap();
        let uri = format!("/api/equivalence/{job}");

        // Unfinished: a poll (and a reload) reads pending and no figures.
        let pending = get_ok(&app, user, &uri).await;
        assert_eq!(pending["verdict"]["status"], json!("pending"));
        assert!(pending["verdict"].get("xp").is_none());

        worker_accepts(&db, user, job, WRONG_FIRST_PASS).await;
        let first = get_ok(&app, user, &uri).await;
        assert_eq!(first["verdict"]["status"], json!("accepted"), "{first}");
        let steps = &first["verdict"]["steps"];
        assert_eq!(steps[0]["text"], json!("Reading your answer: `13`"));
        assert_eq!(
            steps[1]["text"],
            json!("Result: correct. the same value in other words")
        );
        // The rewrite ran exactly once, and a reload reads the same reply.
        let detail = equivalence::detail(&db.admin, job).await.unwrap().unwrap();
        assert!(detail.landed);
        let again = get_ok(&app, user, &uri).await;
        assert_eq!(first, again);

        // The repeat of the normalized answer, from another learner on the
        // same item, meets the cache: no new job.
        let other = lesson_learner(&db, "landing-two@example.test", served).await;
        let repeat = answer_task_ok(
            &app,
            other,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": " 13 "}),
        )
        .await;
        assert_eq!(
            repeat["equivalence"]["status"],
            json!("accepted"),
            "{repeat}"
        );
        assert_eq!(repeat["correct"], json!(true));
        assert_eq!(job_count(&db, other).await, 0);
    })
    .await;
}

#[tokio::test]
async fn a_changed_item_does_not_reuse_the_old_verdict() {
    TestDb::with(|db| async move {
        let served = lesson_problem(5.0, "kp1", Vec::new());
        let mut changed = served.clone();
        changed.text = "Compute 8 + 5.5 again.".to_owned();
        changed.handoff = None;
        let mut original = served;
        original.handoff = None;
        assert_ne!(cache_item_digest(&original), cache_item_digest(&changed));
        let verdict = Verdict {
            equivalent: true,
            reason: "same".to_owned(),
            model: "test-model".to_owned(),
        };
        equivalence::cache_put(
            &db.admin,
            &cache_item_digest(&original),
            &equivalence::cache_key("13"),
            &verdict,
        )
        .await
        .unwrap();
        let hit = equivalence::cache_hit(&db.admin, &cache_item_digest(&changed), "13")
            .await
            .unwrap();
        assert!(hit.is_none());
    })
    .await;
}

#[tokio::test]
async fn the_overturn_report_lists_items_by_rate() {
    TestDb::with(|db| async move {
        let served = lesson_problem(5.0, "kp1", Vec::new());
        let user = lesson_learner(&db, "overturn@example.test", served).await;
        let app = lesson_app(&db);
        let reply = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": WRONG_FIRST_PASS}),
        )
        .await;
        let job = Uuid::parse_str(reply["equivalence"]["id"].as_str().unwrap()).unwrap();
        worker_accepts(&db, user, job, WRONG_FIRST_PASS).await;
        let rows = equivalence::overturn_rates(&db.admin, 1, 10).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].checks, rows[0].overturned), (1, 1));
    })
    .await;
}
