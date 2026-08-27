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
//! Every expected value is a LITERAL: a literal status code, a literal error
//! code, a literal wire status, a literal tag, a literal count. Nothing here
//! re-reads a constant from the code under test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use cadus_core::curriculum::{
    AnswerKind, Catalog, Course, Curriculum, Exemplar, KnowledgePoint, RawCurriculum, RawUnit,
    Slug, Topic, Unit,
};
use cadus_core::event::{Event, SchemaVersion, SessionStart, Timestamp};
use cadus_core::pool::PoolAnswer;
use cadus_store::diagnosis::JobRow;
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::diagnosis::{DiagnosisHub, job_view, match_distractor};
use cadus_web::state::{Content, ServedProblem, TaskProgress, Tenant, WebState};
use cadus_web::{AppState, create_app};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use tower::ServiceExt;

/// The Unix microsecond instant of 2026-01-01T00:00:00Z.
const BASE_US: i64 = 1_767_225_600_000_000;

/// The session id every seeded log opens.
const SESSION: &str = "s_2026-01-01a";

/// The task id of the `addition` lesson (`assign_ids`: `{session}-{type}-{topic}`).
const LESSON: &str = "s_2026-01-01a-lesson-addition";

/// The serving key of the first knowledge point of `addition`.
const KEY: &str = "addition/kp1";

/// The statement of the served problem.
const PROBLEM_TEXT: &str = "Compute 8 + 5.5.";

/// The authored answer of the served problem.
const EXPECTED_ANSWER: &str = "13.5";

/// The `problem_id` every seeded state hands the client.
const PROBLEM_ID: &str = "p0000000000000000000000000000001";

/// The `attempt_id` of the first answer of the lesson (`{task_id}-{n}`, n = 1).
const ATTEMPT_ID: &str = "s_2026-01-01a-lesson-addition-1";

/// The wrong answer the authored distractor names.
const DISTRACTOR_ANSWER: &str = "13";

/// The prose the authored distractor carries.
const DISTRACTOR_NOTE: &str = "You added the whole parts and dropped the half.";

/// A wrong answer NO authored distractor names.
const UNKNOWN_MISS: &str = "99";

// --------------------------------------------------------------------------- //
// The fixture curriculum
// --------------------------------------------------------------------------- //

/// One knowledge point with its authored exemplars.
fn kp(id: &str, exemplars: Vec<Exemplar>) -> KnowledgePoint {
    KnowledgePoint {
        id: Slug::new(id).unwrap(),
        name: format!("The {id} point"),
        key_prerequisites: Vec::new(),
        exemplars,
        constraints: None,
    }
}

/// One exemplar.
fn exemplar(problem: &str, answer: &str) -> Exemplar {
    Exemplar {
        problem: problem.to_string(),
        answer: answer.to_string(),
        solution_sketch: Some(format!("Add the parts to reach {answer}.")),
    }
}

/// One topic of the fixture curriculum.
fn topic(id: &str, points: Vec<KnowledgePoint>) -> Topic {
    Topic {
        id: Slug::new(id).unwrap(),
        name: format!("The {id} topic"),
        core: true,
        difficulty: 0.3,
        drill: false,
        answer_kind: AnswerKind::Numeric,
        expected_time_secs: 30,
        prerequisites: Vec::new(),
        encompassings_extra: Vec::new(),
        knowledge_points: points,
        diagnostic_exemplar: None,
        anki_seeds: Vec::new(),
    }
}

/// The fixture curriculum: one course, one module, two topics.
fn graph() -> Curriculum {
    let catalog = Catalog {
        courses: vec![Course {
            id: Slug::new("c1").unwrap(),
            name: "Foundations".to_string(),
            order: 0,
            mastery_floor: Vec::new(),
            mastery_floor_course: None,
        }],
    };
    Curriculum::build(RawCurriculum {
        catalog,
        units: vec![RawUnit {
            course_id: "c1".to_string(),
            file_name: "00-M1.yaml".to_string(),
            unit: Unit {
                unit: "M1".to_string(),
                course: Slug::new("c1").unwrap(),
                module: "M1".to_string(),
                topics: vec![
                    topic(
                        "addition",
                        vec![
                            kp("kp1", vec![exemplar(PROBLEM_TEXT, EXPECTED_ANSWER)]),
                            kp("kp2", vec![exemplar("Compute 40 + 2.5.", "42.5")]),
                        ],
                    ),
                    topic("subtraction", vec![kp("kp1", vec![])]),
                ],
            },
            first_load_index: 0,
        }],
    })
    .unwrap()
}

// --------------------------------------------------------------------------- //
// The harness
// --------------------------------------------------------------------------- //

/// The router of a test, sharing `hub` with the caller.
fn app_with(db: &TestDb, hub: &Arc<DiagnosisHub>) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_content(Arc::new(Content::new(graph())))
            .with_diagnosis(Arc::clone(hub)),
    )
}

/// The router of a test that needs no push.
fn app(db: &TestDb) -> Router {
    app_with(db, &Arc::new(DiagnosisHub::new()))
}

/// One request against the router. `tenant` is the bound learner.
async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    tenant: Option<Uuid>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let payload = match body {
        Some(value) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let mut request = builder.body(payload).unwrap();
    if let Some(user) = tenant {
        request.extensions_mut().insert(Tenant(user));
    }
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let body = serde_json::from_str(&text).unwrap_or(Value::Null);
    (status, body)
}

/// Answer the lesson's live problem.
async fn answer(app: &Router, user: Uuid, given: &str) -> (StatusCode, Value) {
    call(
        app,
        Method::POST,
        &format!("/api/task/{LESSON}/answer"),
        Some(user),
        Some(json!({ "problem_id": PROBLEM_ID, "answer": given })),
    )
    .await
}

/// Open `SESSION` in the log of `user`.
async fn seed_open_session(db: &TestDb, user: Uuid) {
    let event = Event::SessionStart(SessionStart {
        ts: Timestamp::from_micros(BASE_US),
        session: Some(SESSION.to_string()),
        v: SchemaVersion,
    });
    let ts = DateTime::<Utc>::from_timestamp_micros(BASE_US).unwrap();
    sqlx::query!(
        r#"
        INSERT INTO events (user_id, seq, ts, type, session_id, v, payload)
        VALUES ($1, 1, $2, $3, $4, 1, $5)
        "#,
        user,
        ts,
        event.type_name(),
        event.session(),
        serde_json::to_value(&event).unwrap()
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// The problem that is live on the learner's screen.
fn live_problem() -> ServedProblem {
    let now = Utc::now().timestamp_micros() as f64 / 1_000_000.0;
    ServedProblem {
        problem_id: PROBLEM_ID.to_string(),
        task_id: LESSON.to_string(),
        topic: Some("addition".to_string()),
        kp: Some("kp1".to_string()),
        answer_kind: Some("numeric".to_string()),
        text: PROBLEM_TEXT.to_string(),
        expected: PoolAnswer {
            v: 1,
            answer: EXPECTED_ANSWER.to_string(),
        },
        solution_sketch: Some("Add the parts to reach 13.5.".to_string()),
        started_at: now - 12.0,
        hints_given: Vec::new(),
        index: 0,
        rework: None,
    }
}

/// A D-S6 document holding one live lesson problem.
fn state_with_live_problem() -> WebState {
    let mut scratch = WebState::for_session(SESSION);
    scratch.tasks.insert(
        LESSON.to_string(),
        TaskProgress {
            task_id: LESSON.to_string(),
            task_type: "lesson".to_string(),
            total: 0,
            served: 1,
            answered: 0,
            done: false,
            current_kp: Some("kp1".to_string()),
        },
    );
    scratch.served.insert(LESSON.to_string(), live_problem());
    scratch
}

/// Write a D-S6 document for `user`.
async fn put_state(db: &TestDb, user: Uuid, scratch: &WebState) {
    sqlx::query!(
        r#"
        INSERT INTO web_states (user_id, doc) VALUES ($1, $2)
        ON CONFLICT (user_id) DO UPDATE SET doc = excluded.doc
        "#,
        user,
        scratch.to_doc().unwrap()
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// The stored D-S6 document of `user`.
async fn stored_state(db: &TestDb, user: Uuid) -> WebState {
    let doc = sqlx::query_scalar!(
        r#"SELECT doc AS "doc!" FROM web_states WHERE user_id = $1"#,
        user
    )
    .fetch_one(&db.admin)
    .await
    .unwrap();
    WebState::from_doc(&doc).unwrap()
}

/// Seed a learner with an open session and one live lesson problem.
async fn learner(db: &TestDb, email: &str) -> Uuid {
    let user = db.seed_user(email).await;
    seed_open_session(db, user).await;
    put_state(db, user, &state_with_live_problem()).await;
    user
}

/// Put one approved kind-`diagnosis` document on `KEY` (spec section 6.2).
async fn seed_distractors(db: &TestDb, digest: &str, body: &Value) {
    sqlx::query!(
        r#"
        INSERT INTO content_store (digest, kp_id, kind, body, status, approved_at)
        VALUES ($1, $2, 'diagnosis', $3, 'approved', now())
        "#,
        digest,
        KEY,
        body,
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Every job row of `user`, oldest first.
async fn jobs_of(db: &TestDb, user: Uuid) -> Vec<(Uuid, String, Value)> {
    sqlx::query!(
        r#"
        SELECT id AS "id!", attempt_id AS "attempt_id!", payload AS "payload!"
        FROM diagnosis_jobs WHERE user_id = $1 ORDER BY created_at, id
        "#,
        user
    )
    .fetch_all(&db.admin)
    .await
    .unwrap()
    .into_iter()
    .map(|row| (row.id, row.attempt_id, row.payload))
    .collect()
}

/// Seed one finished job row and return its id.
async fn seed_done_job(db: &TestDb, user: Uuid, attempt_id: &str, result: &Value) -> Uuid {
    sqlx::query_scalar!(
        r#"
        INSERT INTO diagnosis_jobs (user_id, attempt_id, status, payload, result, finished_at)
        VALUES ($1, $2, 'done', '{}'::jsonb, $3, now())
        RETURNING id AS "id!"
        "#,
        user,
        attempt_id,
        result,
    )
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

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
        seed_distractors(
            &db,
            "u9-digest-ready",
            &json!({
                "v": 1,
                "distractors": [
                    { "answer": DISTRACTOR_ANSWER,
                      "error_tag": "arithmetic-slip",
                      "note": DISTRACTOR_NOTE }
                ]
            }),
        )
        .await;

        let (status, body) = answer(&app, user, DISTRACTOR_ANSWER).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["correct"], json!(false));
        assert_eq!(
            body["diagnosis"],
            json!({
                "status": "ready",
                "error_tags": ["arithmetic-slip"],
                "prose": "You added the whole parts and dropped the half.",
            }),
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
/// A replayed request meets the standing `attempt_id`, appends nothing, and
/// rolls the whole transaction back (step 6). The queue must therefore still
/// hold exactly ONE row, and the replay must name the same job — not a second
/// one, and not nothing.
#[tokio::test]
async fn a_rolled_back_grade_leaves_no_job_row() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "u9-replay@example.test").await;

        let (status, first) = answer(&app, user, UNKNOWN_MISS).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(first["task_status"], json!("continue"));
        let id = first["diagnosis"]["id"].as_str().unwrap().to_string();
        assert_eq!(jobs_of(&db, user).await.len(), 1);

        // The request comes again: a stale tab, or a client that lost the first
        // reply. The stored document is the one the first COMMIT wrote, so its
        // `pending_diagnoses` map stands; only the live problem is put back.
        let mut scratch = stored_state(&db, user).await;
        assert_eq!(
            scratch
                .pending_diagnoses
                .get(ATTEMPT_ID)
                .map(String::as_str),
            Some(id.as_str()),
            "the commit records the job id against the attempt"
        );
        scratch.served.insert(LESSON.to_string(), live_problem());
        put_state(&db, user, &scratch).await;

        let (status, second) = answer(&app, user, UNKNOWN_MISS).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(second["task_status"], json!("already_recorded"));
        assert_eq!(
            second["diagnosis"],
            json!({ "id": id, "status": "pending" }),
            "the replay names the standing job: {second}"
        );
        assert_eq!(
            jobs_of(&db, user).await.len(),
            1,
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
        let user = db.seed_user("u9-rollback@example.test").await;
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

// --------------------------------------------------------------------------- //
// Acceptance 3: a NOTIFY for tenant A never reaches tenant B's stream
// --------------------------------------------------------------------------- //

/// Read the next non-empty data frame of a streaming body, or `None`.
async fn next_frame(body: &mut Body, within: Duration) -> Option<String> {
    let deadline = tokio::time::Instant::now() + within;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            return None;
        }
        let frame = match tokio::time::timeout(left, body.frame()).await {
            Ok(Some(Ok(frame))) => frame,
            _ => return None,
        };
        let Some(bytes) = frame.data_ref() else {
            continue;
        };
        let text = String::from_utf8_lossy(bytes).into_owned();
        if !text.trim().is_empty() {
            return Some(text);
        }
    }
}

/// Open one `/api/diagnosis/stream` as `user`.
async fn open_stream(app: &Router, user: Uuid) -> Body {
    let mut request = Request::builder()
        .method(Method::GET)
        .uri("/api/diagnosis/stream")
        .body(Body::empty())
        .unwrap();
    request.extensions_mut().insert(Tenant(user));
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    response.into_body()
}

/// Trap W14: `LISTEN/NOTIFY` carries no row-level security and no tenant
/// binding, so the payload holds ids only and the handler re-reads the row
/// through `begin_tenant` before it writes a byte.
///
/// The whole path is real here: one process `LISTEN` connection, a `NOTIFY` sent
/// from another connection, two open streams. A's stream reads the finished
/// diagnosis; B's stream reads nothing at all.
#[tokio::test]
async fn a_notify_for_one_tenant_never_reaches_another_tenants_stream() {
    TestDb::with(|db| async move {
        let hub = Arc::new(DiagnosisHub::new());
        let app = app_with(&db, &hub);
        let alice = db.seed_user("u9-alice@example.test").await;
        let bob = db.seed_user("u9-bob@example.test").await;
        let job = seed_done_job(
            &db,
            alice,
            "s_2026-01-01a-lesson-addition-4",
            &json!({
                "error_tags": ["sign-error"],
                "prose": "You dropped the minus sign.",
                "model_id": "deepseek/deepseek-v4-pro",
            }),
        )
        .await;

        let listening = tokio::spawn({
            let hub = Arc::clone(&hub);
            let pool = Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
            async move {
                let _ = hub.listen(&pool).await;
            }
        });

        let mut alice_stream = open_stream(&app, alice).await;
        let mut bob_stream = open_stream(&app, bob).await;

        // The LISTEN connection is established asynchronously, so the notice is
        // repeated until it lands. Every repeat names Alice, so a repeat is one
        // more chance for Bob's stream to leak, never fewer.
        let notifying = tokio::spawn({
            let admin = db.admin.clone();
            let payload = format!("{job}:{alice}");
            async move {
                for _ in 0..100 {
                    let _ = sqlx::query("SELECT pg_notify('diagnosis_done', $1)")
                        .bind(&payload)
                        .execute(&admin)
                        .await;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        });

        let frame = next_frame(&mut alice_stream, Duration::from_secs(10))
            .await
            .expect("Alice's stream must carry her finished diagnosis");
        assert!(
            frame.contains("event: diagnosis"),
            "the frame names the section 2.1 event: {frame}"
        );
        assert!(
            frame.contains(&job.to_string()) && frame.contains("\"status\":\"ready\""),
            "the frame carries the re-read row: {frame}"
        );
        assert!(
            frame.contains("You dropped the minus sign."),
            "the frame carries the prose: {frame}"
        );

        assert_eq!(
            next_frame(&mut bob_stream, Duration::from_secs(2)).await,
            None,
            "a NOTIFY for Alice must never reach Bob's stream"
        );

        notifying.abort();
        listening.abort();
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Acceptance 4: a poll for another tenant's id is 404 unknown_diagnosis
// --------------------------------------------------------------------------- //

/// C3: the poll reads inside `begin_tenant` and the statement names no
/// `user_id`, so the `tenant_isolation` policy is the ONE guard. A caller
/// therefore learns nothing about another tenant's queue, not even that the id
/// exists.
#[tokio::test]
async fn a_poll_for_another_tenants_id_is_404_unknown_diagnosis() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let alice = db.seed_user("u9-poll-alice@example.test").await;
        let bob = db.seed_user("u9-poll-bob@example.test").await;
        let job = seed_done_job(
            &db,
            alice,
            "s_2026-01-01a-lesson-addition-2",
            &json!({ "error_tags": ["sign-error"], "prose": "Mind the sign." }),
        )
        .await;

        let (status, body) = call(
            &app,
            Method::GET,
            &format!("/api/diagnosis/{job}"),
            Some(bob),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], json!("unknown_diagnosis"));

        // The same id, read by its owner, is the whole document.
        let (status, body) = call(
            &app,
            Method::GET,
            &format!("/api/diagnosis/{job}"),
            Some(alice),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            json!({
                "id": job.to_string(),
                "status": "ready",
                "error_tags": ["sign-error"],
                "prose": "Mind the sign.",
            })
        );
    })
    .await;
}

/// An id that is not a uuid, and an id no row carries, give the same `404`.
/// Both routes need a session first.
#[tokio::test]
async fn an_unknown_id_and_a_missing_session_are_the_pinned_refusals() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = db.seed_user("u9-refusals@example.test").await;

        for id in ["not-a-uuid", "11111111-1111-4111-8111-111111111111"] {
            let (status, body) = call(
                &app,
                Method::GET,
                &format!("/api/diagnosis/{id}"),
                Some(user),
                None,
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{id}");
            assert_eq!(body["error"]["code"], json!("unknown_diagnosis"), "{id}");
        }

        for uri in [
            "/api/diagnosis/11111111-1111-4111-8111-111111111111",
            "/api/diagnosis/stream",
        ] {
            let (status, body) = call(&app, Method::GET, uri, None, None).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}");
            assert_eq!(body["error"]["code"], json!("unauthorized"), "{uri}");
        }
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The poll fallback rules (spec section 2.1, last paragraph)
// --------------------------------------------------------------------------- //

/// One row, built by hand, so the reader is measured and not the writer.
fn row(status: &str, age_secs: i64, result: Option<Value>) -> JobRow {
    JobRow {
        id: Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
        attempt_id: "t-1".to_string(),
        status: status.to_string(),
        result,
        created_at: DateTime::<Utc>::from_timestamp_micros(BASE_US - age_secs * 1_000_000).unwrap(),
    }
}

/// "A job still `pending` after 30 s is reported `failed`, never left open."
///
/// The five stored statuses map onto the four wire statuses, `running` reads as
/// `pending`, and the deadline turns a stuck job into a reported failure. Every
/// value below is a literal.
#[test]
fn the_wire_status_follows_the_poll_rules() {
    let now = DateTime::<Utc>::from_timestamp_micros(BASE_US).unwrap();

    assert_eq!(job_view(&row("pending", 0, None), now).status, "pending");
    assert_eq!(job_view(&row("running", 29, None), now).status, "pending");
    assert_eq!(job_view(&row("pending", 30, None), now).status, "pending");
    assert_eq!(job_view(&row("pending", 31, None), now).status, "failed");
    assert_eq!(job_view(&row("running", 600, None), now).status, "failed");
    assert_eq!(job_view(&row("failed", 1, None), now).status, "failed");
    assert_eq!(job_view(&row("capped", 1, None), now).status, "capped");
    assert_eq!(job_view(&row("done", 9_000, None), now).status, "ready");
    // A status no build writes fails closed, so no client is left open on it.
    assert_eq!(job_view(&row("sending", 1, None), now).status, "failed");

    // An unfinished row carries no tags and no prose, whatever `result` holds.
    let stuck = job_view(
        &row(
            "pending",
            99,
            Some(json!({ "error_tags": ["sign-error"], "prose": "half-written" })),
        ),
        now,
    );
    assert_eq!(
        stuck.body,
        json!({
            "id": "33333333-3333-4333-8333-333333333333",
            "status": "failed",
            "error_tags": [],
        })
    );

    // A finished row carries the three fields of section 2.1.
    let done = job_view(
        &row(
            "done",
            5,
            Some(json!({
                "error_tags": ["wrong-method"],
                "prose": "You used the area formula.",
                "model_id": "deepseek/deepseek-v4-pro",
            })),
        ),
        now,
    );
    assert_eq!(
        done.body,
        json!({
            "id": "33333333-3333-4333-8333-333333333333",
            "status": "ready",
            "error_tags": ["wrong-method"],
            "prose": "You used the area formula.",
            "model_id": "deepseek/deepseek-v4-pro",
        })
    );
}

// --------------------------------------------------------------------------- //
// The distractor match (spec sections 5.3 and 6.2)
// --------------------------------------------------------------------------- //

/// The 11 tags of spec section 5.3, spelled out.
const VOCABULARY: [&str; 11] = [
    "sign-error",
    "arithmetic-slip",
    "algebra-slip",
    "wrong-method",
    "formula-recall",
    "misread-problem",
    "incomplete",
    "notation",
    "units",
    "timing-unreliable",
    "blowoff",
];

/// The match runs through the checker, and the tag runs through the vocabulary.
///
/// `13` and `13.0` are the same wrong answer, exactly as they would be the same
/// right one. A tag the vocabulary lacks is dropped silently — the 1.0 lesson of
/// `prompts.py:529-536` — and a hit that keeps neither a tag nor prose is no hit
/// at all, so the caller enqueues instead of answering with an empty document.
#[test]
fn the_distractor_match_reads_the_checker_and_the_vocabulary() {
    let vocabulary: Vec<String> = VOCABULARY.iter().map(|tag| (*tag).to_string()).collect();
    let body = json!({
        "v": 1,
        "distractors": [
            { "answer": "13", "error_tag": "arithmetic-slip", "note": "You dropped the half." },
            { "answer": "8", "error_tag": "not-a-real-tag", "note": "You stopped early." },
            { "answer": "5.5", "error_tag": "made-up" },
        ]
    });

    let hit = match_distractor(&body, "13.0", AnswerKind::Numeric, &vocabulary).unwrap();
    assert_eq!(hit.error_tags, vec!["arithmetic-slip".to_string()]);
    assert_eq!(hit.prose.as_deref(), Some("You dropped the half."));

    // The tag is outside the vocabulary; the prose still stands.
    let dropped = match_distractor(&body, "8", AnswerKind::Numeric, &vocabulary).unwrap();
    assert_eq!(dropped.error_tags, Vec::<String>::new());
    assert_eq!(dropped.prose.as_deref(), Some("You stopped early."));

    // Neither a tag nor prose survives, so this is not a usable diagnosis.
    assert_eq!(
        match_distractor(&body, "5.5", AnswerKind::Numeric, &vocabulary),
        None
    );
    // No distractor names this answer.
    assert_eq!(
        match_distractor(&body, "41", AnswerKind::Numeric, &vocabulary),
        None
    );
    // A body with no distractor list matches nothing and raises nothing.
    assert_eq!(
        match_distractor(&json!({ "v": 1 }), "13", AnswerKind::Numeric, &vocabulary),
        None
    );
    assert_eq!(
        match_distractor(&json!("nonsense"), "13", AnswerKind::Numeric, &vocabulary),
        None
    );
}

// --------------------------------------------------------------------------- //
// M5 U11: the one diagnosis result that writes no row (T6, spec section 7)
// --------------------------------------------------------------------------- //

/// Read `/metrics` from the same router and return the exposition text.
async fn scrape(app: &Router) -> String {
    let request = Request::builder()
        .method(Method::GET)
        .uri("/metrics")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// A pre-authored hit counts `ready_preauthored`, and it is the only label of
/// `cadus_diagnosis_jobs_total` that no row can carry.
///
/// The hit writes NO job row (spec section 6.2), so the queue holds nothing to
/// count and the counter of this process is the whole record of it. `enqueued`
/// stays at zero in the same scrape, which is the saving the label exists to
/// show.
#[tokio::test]
async fn a_preauthored_hit_counts_ready_preauthored_and_enqueues_nothing() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "u11-counted@example.test").await;
        seed_distractors(
            &db,
            "u11-digest-ready",
            &json!({
                "v": 1,
                "distractors": [
                    { "answer": DISTRACTOR_ANSWER,
                      "error_tag": "arithmetic-slip",
                      "note": DISTRACTOR_NOTE }
                ]
            }),
        )
        .await;

        let (status, body) = answer(&app, user, DISTRACTOR_ANSWER).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["diagnosis"]["status"], json!("ready"));

        let text = scrape(&app).await;
        assert!(
            text.contains("cadus_diagnosis_jobs_total{result=\"ready_preauthored\"} 1\n"),
            "the scrape carries no pre-authored count:\n{text}"
        );
        assert!(
            text.contains("cadus_diagnosis_jobs_total{result=\"enqueued\"} 0\n"),
            "a pre-authored hit must enqueue nothing:\n{text}"
        );
    })
    .await;
}
