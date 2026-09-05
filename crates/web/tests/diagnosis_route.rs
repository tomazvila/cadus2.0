//! M5 U9 acceptance: the A4 client surface.
//!
//! Requirements: A4, C3, D7, L3, R4, T1. Spec
//! `docs/reference/web-service-1.0-spec.md` sections 2.1, 4.3 step 9, 6.2, trap
//! W14, and row U9 of section 11. Ruling D-M5-1 is binding.
//!
//! The four acceptance checks of row U9 land here:
//!
//! 1. a matching distractor returns `status:"ready"` and writes no job row —
//!    [`a_matching_distractor_is_ready_and_writes_no_job_row`];
//! 2. a rolled-back grade leaves no job row —
//!    [`a_rolled_back_grade_leaves_no_job_row`];
//! 3. a NOTIFY for tenant A never reaches tenant B's stream —
//!    [`a_notify_for_one_tenant_never_reaches_another_tenants_stream`];
//! 4. a poll for another tenant's id is `404 unknown_diagnosis` —
//!    [`a_poll_for_another_tenants_id_is_404_unknown_diagnosis`].
//!
//! The second acceptance check of M6 row R7 joins them, because it is the same
//! route: a learner answer that matches a distractor of an AUTHORED document
//! returns `status:"ready"` and writes no job row —
//! [`an_authored_distractor_document_is_ready_and_writes_no_job_row`].
//!
//! Every expected value is a LITERAL: a literal status code, a literal error
//! code, a literal wire status, a literal tag, a literal count. Nothing here
//! re-reads a constant from the code under test.
//!
//! Every call presents a real session cookie, and `auth::layer::tenant_layer`
//! binds the tenant from it (FIX-M5-C). No test here writes a request extension
//! by hand.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{EXPECTED_ANSWER, LESSON, PROBLEM_TEXT, SESSION, exemplar, put_state, stored_state};

use common::diagnosis::*;

use cadus_core::curriculum::AnswerKind;
use cadus_core::template::{GateSpec, gate_diagnosis_body};

// --------------------------------------------------------------------------- //
// Acceptance 1: a matching distractor is `ready` and writes no job row
// --------------------------------------------------------------------------- //

/// Spec section 6.2: the request path checks `content_store` for a kind-
/// `diagnosis` row that names the learner's wrong answer. A hit answers inline
/// and **writes no job row** — that is the difference between a bill that scales
/// with attempts and one that scales with distinct misconceptions.
#[tokio::test]
async fn a_matching_distractor_is_ready_and_writes_no_job_row() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "u9-ready@example.test").await;
        seed_slip_distractor(&db, "u9-digest-ready").await;

        let (status, body) = answer(&app, user, DISTRACTOR_ANSWER).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["correct"], json!(false));
        assert_eq!(
            body["diagnosis"],
            ready_slip(),
            "a matching distractor answers inline: {body}"
        );
        assert_eq!(
            jobs_of(&db, user).await.len(),
            0,
            "a pre-authored hit must write no diagnosis_jobs row"
        );
    })
    .await;
}

/// M6 row R7, second acceptance check: a learner answer that matches a
/// distractor returns `status:"ready"` and writes no `diagnosis_jobs` row.
///
/// The body under test is the one the R7 AUTHORING gate writes, not a
/// hand-written one: the test runs `gate_diagnosis_body` over a raw authored
/// list, stores what it returns, and answers with it. That is the wiring the row
/// names — the gate that drops a tag and the grade path that reads the document
/// share one definition.
///
/// The learner writes `13.0` and the document names `13`. The checker decides
/// the form, so one authored answer names every spelling of one mistake.
#[tokio::test]
async fn an_authored_distractor_document_is_ready_and_writes_no_job_row() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "r7-authored@example.test").await;
        let raw = json!({
            "distractors": [
                { "answer": DISTRACTOR_ANSWER, "error_tag": "arithmetic-slip",
                  "note": DISTRACTOR_NOTE },
                { "answer": "2.5", "error_tag": "carelessness",
                  "note": "You subtracted where the problem adds." },
            ],
            "v": 1,
            "topic_id": "addition",
            "answer_kind": "numeric",
        })
        .to_string();
        let exemplars = vec![exemplar(PROBLEM_TEXT, EXPECTED_ANSWER)];
        let gate_spec = GateSpec {
            answer_kind: AnswerKind::Numeric,
            exemplars: &exemplars,
        };
        let vocabulary: Vec<String> = VOCABULARY.iter().map(|tag| (*tag).to_string()).collect();
        let (doc, dropped) = gate_diagnosis_body(&raw, &gate_spec, &vocabulary)
            .expect("the gate accepts the authored list");
        assert_eq!(
            dropped,
            vec!["carelessness".to_owned()],
            "the gate drops the tag outside the vocabulary"
        );
        seed_distractors(
            &db,
            "r7-digest-authored",
            &serde_json::to_value(&doc).unwrap(),
        )
        .await;

        let (status, body) = answer(&app, user, "13.0").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["correct"], json!(false));
        assert_eq!(
            body["diagnosis"],
            ready_slip(),
            "the authored document answers inline: {body}"
        );
        assert_eq!(
            jobs_of(&db, user).await.len(),
            0,
            "a pre-authored hit must write no diagnosis_jobs row"
        );
    })
    .await;
}

/// The other half of section 6.2: a miss the bank does not name enqueues one
/// job, and the reply carries its id.
#[tokio::test]
async fn a_miss_with_no_distractor_enqueues_one_job_inside_the_grade() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "u9-pending@example.test").await;
        seed_distractors(
            &db,
            "u9-digest-pending",
            &json!({
                "v": 1,
                "distractors": [
                    { "answer": DISTRACTOR_ANSWER, "error_tag": "arithmetic-slip",
                      "note": DISTRACTOR_NOTE }
                ]
            }),
        )
        .await;

        let (status, body) = answer(&app, user, UNKNOWN_MISS).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["diagnosis"]["status"], json!("pending"));
        let id = body["diagnosis"]["id"].as_str().unwrap().to_string();

        let rows = jobs_of(&db, user).await;
        assert_eq!(rows.len(), 1, "one miss enqueues exactly one job");
        assert_eq!(rows[0].0.to_string(), id, "the reply names the row's id");
        assert_eq!(rows[0].1, ATTEMPT_ID);
        // The payload carries what the section 6.3 user message names, and the
        // verdict the server already reached is NOT in it.
        assert_eq!(rows[0].2["v"], json!(1));
        assert_eq!(rows[0].2["problem"], json!(PROBLEM_TEXT));
        assert_eq!(rows[0].2["expected"], json!(EXPECTED_ANSWER));
        assert_eq!(rows[0].2["given_answer"], json!(UNKNOWN_MISS));
        assert_eq!(rows[0].2["answer_kind"], json!("numeric"));
        assert_eq!(rows[0].2["session"], json!(SESSION));
        assert_eq!(rows[0].2["topic"], json!("addition"));
        assert_eq!(rows[0].2["kp"], json!("kp1"));
    })
    .await;
}

/// A correct answer and a blank one owe no diagnosis at all (spec section 2.1).
#[tokio::test]
async fn a_correct_and_a_blank_answer_are_not_offered_and_write_no_row() {
    TestDb::with(|db| async move {
        let app = app(&db);

        let right = learner(&db, "u9-right@example.test").await;
        let (status, body) = answer(&app, right, EXPECTED_ANSWER).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["correct"], json!(true));
        assert_eq!(body["diagnosis"], json!({ "status": "not_offered" }));
        assert_eq!(jobs_of(&db, right).await.len(), 0);

        let blank = learner(&db, "u9-blank@example.test").await;
        let (status, body) = answer(&app, blank, "   ").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["correct"], json!(false));
        assert_eq!(body["work_quality"], json!("poor"));
        assert_eq!(body["diagnosis"], json!({ "status": "not_offered" }));
        assert_eq!(jobs_of(&db, blank).await.len(), 0);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Acceptance 2: a rolled-back grade leaves no job row
// --------------------------------------------------------------------------- //

/// Spec section 4.3 step 9: the enqueue is INSIDE the grade transaction.
///
/// A request whose `attempt_id` already stands appends nothing and rolls the
/// whole transaction back (step 6). The queue must therefore hold no NEW row,
/// and the reply must name the job that stands against that attempt — not a
/// second one, and not nothing.
///
/// `n` is the position of the attempt in the LOG (`cadus_web::grade`), so the
/// number of a new attempt is free on a dense log. The log seeded below has a
/// gap: attempt `-3` of the task stands with a pending job of its own, and `-2`
/// is open. The first answer therefore takes `-2`, and the second one computes
/// `-3` and meets the standing row.
#[tokio::test]
async fn a_rolled_back_grade_leaves_no_job_row() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "u9-replay@example.test").await;
        seed_attempt(&db, user, 2, ATTEMPT_ID_3).await;
        let standing = seed_pending_job(&db, user, ATTEMPT_ID_3).await.to_string();

        let (status, first) = answer(&app, user, UNKNOWN_MISS).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(first["task_status"], json!("continue"));
        assert_eq!(first["attempt_id"], json!(ATTEMPT_ID_2));
        let id = first["diagnosis"]["id"].as_str().unwrap().to_string();
        assert_eq!(jobs_of(&db, user).await.len(), 2);

        // The stored document is the one the first COMMIT wrote, so its
        // `pending_diagnoses` map stands. The live problem goes back, and the
        // map names the job of the attempt that already stands.
        let mut scratch = stored_state(&db, user).await;
        assert_eq!(
            scratch
                .pending_diagnoses
                .get(ATTEMPT_ID_2)
                .map(String::as_str),
            Some(id.as_str()),
            "the commit records the job id against the attempt"
        );
        scratch
            .pending_diagnoses
            .insert(ATTEMPT_ID_3.to_string(), standing.clone());
        scratch.served.insert(LESSON.to_string(), live_problem());
        put_state(&db, user, &scratch).await;

        let (status, second) = answer(&app, user, UNKNOWN_MISS).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(second["task_status"], json!("already_recorded"));
        assert_eq!(second["attempt_id"], json!(ATTEMPT_ID_3));
        assert_eq!(
            second["diagnosis"],
            json!({ "id": standing, "status": "pending" }),
            "the replay names the standing job: {second}"
        );
        assert_eq!(
            jobs_of(&db, user).await.len(),
            2,
            "a grade that appends nothing and rolls back must add no job row"
        );
    })
    .await;
}

/// The same rule at the layer that owns it (spec section 4.3 step 9).
///
/// The enqueue runs inside the CALLER's transaction, so a transaction that rolls
/// back takes the job row with it. The route test above proves the grade path
/// hands its own transaction over; this one proves the hand-over is what decides
/// the row's fate.
#[tokio::test]
async fn an_enqueue_inside_a_rolled_back_transaction_leaves_no_row() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "u9-rollback@example.test").await;
        let payload = json!({ "v": 1, "given_answer": "99" });

        let mut tx = cadus_store::begin_tenant(&db.app, user).await.unwrap();
        let id = cadus_store::diagnosis::enqueue(&mut tx, user, ATTEMPT_ID, &payload)
            .await
            .unwrap();
        // The row is visible INSIDE the transaction that wrote it.
        assert_eq!(
            cadus_store::diagnosis::job(&mut *tx, id)
                .await
                .unwrap()
                .map(|row| row.attempt_id),
            Some(ATTEMPT_ID.to_string())
        );
        tx.rollback().await.unwrap();

        assert_eq!(
            jobs_of(&db, user).await.len(),
            0,
            "a rolled-back transaction must leave no diagnosis_jobs row"
        );

        // A committed one stands, and a second enqueue of the same attempt is
        // the SAME row: the enqueue is idempotent (unique (user_id, attempt_id)).
        let mut tx = cadus_store::begin_tenant(&db.app, user).await.unwrap();
        let first = cadus_store::diagnosis::enqueue(&mut tx, user, ATTEMPT_ID, &payload)
            .await
            .unwrap();
        let again = cadus_store::diagnosis::enqueue(&mut tx, user, ATTEMPT_ID, &payload)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(first, again);
        assert_eq!(jobs_of(&db, user).await.len(), 1);
    })
    .await;
}
