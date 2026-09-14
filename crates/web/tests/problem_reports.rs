//! Authenticated report reconstruction and source-bound grading overlays.
//! Proof documents are explicit test doubles of the verified worker boundary.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::Db;
use cadus_store::reports;
use common::*;

const ORIGINAL: &str = "Compute 8 - 5.";
const REPAIRED_SOLUTION: &str = "Subtract five from eight to get three.";

fn report_body(attempt: &str) -> Value {
    json!({
        "request_id": Uuid::new_v4().to_string(),
        "problem_id": PROBLEM_ID,
        "attempt_id": attempt,
        "note": "Please check the expected answer."
    })
}

async fn create(app: &Router, user: Uuid, task: &str, body: Value) -> (StatusCode, Value) {
    let (status, text) = call(
        app,
        Method::POST,
        &format!("/api/task/{task}/report"),
        Some(user),
        Some(body),
    )
    .await;
    (status, parse(&text))
}

async fn poll(app: &Router, user: Option<Uuid>, id: &str) -> (StatusCode, Value) {
    let (status, text) = call(app, Method::GET, &format!("/api/reports/{id}"), user, None).await;
    (status, parse(&text))
}

fn original_problem() -> cadus_web::state::ServedProblem {
    let mut live = lesson_problem(5.0, "kp1", Vec::new());
    live.text = ORIGINAL.to_owned();
    live.expected.answer = "4".to_owned();
    live
}

async fn handoff(db: &TestDb, user: Uuid) {
    seed_event(
        db,
        user,
        2,
        BASE_US,
        SESSION,
        json!({
            "type":"ordinary_problem_served","v":2,"ts":"2026-01-01T00:00:00Z",
            "session":SESSION,"task_id":LESSON,"problem_id":PROBLEM_ID,
            "kp_id":"addition/kp1","item_digest":"a".repeat(64),
            "item_source":"exemplar","exposure":"first"
        }),
    )
    .await;
}

async fn reported_learner(db: &TestDb, email: &str) -> (Uuid, String) {
    let user = lesson_learner(db, email, original_problem()).await;
    handoff(db, user).await;
    let id = format!("{LESSON}-submitted");
    let mut verdict = a_miss();
    verdict.given_answer = "3";
    let event = attempt_payload(LESSON, &id, "kp1", (ORIGINAL, "4"), &verdict);
    seed_attempt_row(db, user, 3, &id, &event).await;
    (user, id)
}

fn assert_public(body: &Value) {
    let object = body.as_object().unwrap();
    for field in object.keys() {
        assert!(
            [
                "report_id",
                "status",
                "stage",
                "attempt",
                "max_attempts",
                "retryable",
                "result"
            ]
            .contains(&field.as_str()),
            "private field exposed: {field}"
        );
    }
    for field in [
        "report_id",
        "status",
        "stage",
        "attempt",
        "max_attempts",
        "retryable",
    ] {
        assert!(object.contains_key(field), "missing public field: {field}");
    }
    assert_eq!(body["max_attempts"], 3);
}

async fn worker_db(db: &TestDb) -> Db {
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

#[tokio::test]
async fn reports_require_authentication_and_bind_every_attempt_identifier() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let (owner, attempt) = reported_learner(&db, "report-owner@example.com").await;
        let other = seed_learner(&db, "report-other@example.com").await;
        let path = format!("/api/task/{LESSON}/report");
        let mut request = Request::builder()
            .method(Method::POST)
            .uri(&path)
            .header("content-type", "application/json")
            .header("sec-fetch-site", "same-origin")
            .body(Body::from(report_body(&attempt).to_string()))
            .unwrap();
        request.headers_mut().remove("cookie");
        assert_eq!(send(&app, request).await.status, StatusCode::UNAUTHORIZED);
        assert_eq!(
            poll(&app, None, &Uuid::new_v4().to_string()).await.0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            create(&app, other, LESSON, report_body(&attempt)).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            create(&app, owner, "another-task", report_body(&attempt))
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        let mut wrong_problem = report_body(&attempt);
        wrong_problem["problem_id"] = json!("another-problem");
        assert_eq!(
            create(&app, owner, LESSON, wrong_problem).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            create(&app, owner, LESSON, report_body("another-attempt"))
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        let (status, body) = create(&app, owner, LESSON, report_body(&attempt)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_public(&body);
        let id = body["report_id"].as_str().unwrap();
        assert_eq!(poll(&app, Some(other), id).await.0, StatusCode::NOT_FOUND);
        let (status, own) = poll(&app, Some(owner), id).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(own, body);
    })
    .await;
}

#[tokio::test]
async fn a_real_submitted_attempt_supplies_evidence_and_body_spoofs_are_ignored() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let user = lesson_learner(&db, "report-submit@example.com", original_problem()).await;
        handoff(&db, user).await;
        let (status, grade) = answer_task(
            &app,
            user,
            LESSON,
            json!({"problem_id":PROBLEM_ID,"answer":"3"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{grade}");
        assert_eq!(grade["outcome"], "incorrect");
        let events = events_of_type(&db, user, "attempt").await;
        assert_eq!(events.len(), 1);
        let attempt = events[0]["attempt_id"].as_str().unwrap();
        let mut request = report_body(attempt);
        request["problem"] = json!({"text":"FORGED QUESTION","expected":"999"});
        request["answer"] = json!("FORGED ANSWER");
        request["source"] = json!({"engine_digest":"FORGED"});
        request["user_id"] = json!(Uuid::new_v4());
        let (status, response) = create(&app, user, LESSON, request).await;
        assert_eq!(status, StatusCode::OK, "{response}");
        let id: Uuid = response["report_id"].as_str().unwrap().parse().unwrap();
        let input: Value = sqlx::query_scalar("SELECT input FROM problem_reports WHERE id=$1")
            .bind(id)
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(input["attempt"]["given_answer"], "3");
        assert_eq!(input["source"]["problem"]["text"], ORIGINAL);
        assert_eq!(input["source"]["problem"]["expected"], "4");
        assert_eq!(
            input["source"]["engine_digest"],
            cadus_core::review_engine::DIGEST
        );
        assert!(!input.to_string().contains("FORGED"));
        assert!(input["event_seq"].as_i64().unwrap() > 2);
        assert_eq!(model_calls(&db).await, 0);
    })
    .await;
}

#[tokio::test]
async fn report_json_ids_and_note_bounds_are_enforced_before_enqueue() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let (user, attempt) = reported_learner(&db, "report-input@example.com").await;
        let path = format!("/api/task/{LESSON}/report");
        for body in [None, Some(Value::Null), Some(json!([])), Some(json!({}))] {
            let (status, text) = call(&app, Method::POST, &path, Some(user), body).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{text}");
        }
        let mut malformed = Request::builder()
            .method(Method::POST)
            .uri(&path)
            .header("content-type", "application/json")
            .body(Body::from("{"))
            .unwrap();
        present_session(malformed.headers_mut(), user);
        assert_eq!(
            tower::ServiceExt::oneshot(app.clone(), malformed)
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        for (field, value) in [
            ("request_id", json!("not-a-uuid")),
            ("problem_id", json!("")),
            ("attempt_id", json!("")),
            ("note", json!({"spoof":true})),
            ("note", json!("x".repeat(2001))),
        ] {
            let mut body = report_body(&attempt);
            body[field] = value;
            let (status, result) = create(&app, user, LESSON, body).await;
            assert_eq!(
                status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "{field}: {result}"
            );
        }
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM problem_reports")
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(count, 0);
        let mut boundary = report_body(&attempt);
        boundary["note"] = json!("\u{00e9}".repeat(2000));
        let (status, body) = create(&app, user, LESSON, boundary).await;
        assert_eq!(status, StatusCode::OK, "{body}");
    })
    .await;
}

#[tokio::test]
async fn duplicate_requests_and_attempts_return_one_job_and_reject_rebinding() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let (user, attempt) = reported_learner(&db, "report-dedup@example.com").await;
        let request = report_body(&attempt);
        let (status, first) = create(&app, user, LESSON, request.clone()).await;
        assert_eq!(status, StatusCode::OK, "{first}");
        assert_eq!(create(&app, user, LESSON, request.clone()).await.1, first);
        assert_eq!(
            create(&app, user, LESSON, report_body(&attempt)).await.1,
            first
        );
        for field in ["attempt_id", "problem_id"] {
            let mut rebound = request.clone();
            rebound[field] = json!("another-id");
            assert_eq!(
                create(&app, user, LESSON, rebound).await.0,
                StatusCode::UNPROCESSABLE_ENTITY
            );
        }
        assert_eq!(
            create(&app, user, "another-task", request).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM problem_reports WHERE user_id=$1")
                .bind(user)
                .fetch_one(&db.admin)
                .await
                .unwrap();
        assert_eq!(count, 1);
    })
    .await;
}

#[tokio::test]
async fn verified_terminal_publication_updates_http_grades_without_model_calls() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let (reporter, attempt) = reported_learner(&db, "report-publish@example.com").await;
        let (status, queued) = create(&app, reporter, LESSON, report_body(&attempt)).await;
        assert_eq!(status, StatusCode::OK, "{queued}");
        let worker = worker_db(&db).await;
        let job = reports::claim(&worker).await.unwrap().unwrap();
        assert!(
            reports::record_step(
                &worker,
                &job,
                "critic",
                &json!({
                    "private_trace":"INTERNAL MODEL REASONING","source_hash":job.source_hash
                })
            )
            .await
            .unwrap()
        );
        let running = poll(&app, Some(reporter), queued["report_id"].as_str().unwrap()).await;
        assert_eq!(running.0, StatusCode::OK);
        assert_eq!(running.1["status"], "running");
        assert_public(&running.1);
        assert!(!running.1.to_string().contains("INTERNAL MODEL REASONING"));
        // The store/HTTP boundary consumes verified worker output. This fixture
        // deliberately supplies proof doubles and makes no live-verifier claim.
        let verification = json!({
            "source_hash":job.source_hash,"status":"completed","supported":true,
            "learner":{"status":"proved","message":"fixture proof"},
            "candidate":{"status":"proved","message":"fixture proof"},
            "engine":{"name":"fixture"},"evidence":{"scope":"formalized mathematics"}
        });
        let correction = json!({
            "candidate_answer":"3","solution":REPAIRED_SOLUTION,
            "accepted_answers":["3","three"],
            "formal_problem":{"kind":"numeric_expression","expression":"8-5"},
            "verification":verification,
            "regressions":[{"answer":"3","correct":true},{"answer":"4","correct":false}]
        });
        let result = json!({
            "resolution":"confirmed_issue","message":"Corrected the expected expression.",
            "qwen_verdict":"correct","verification":"proved",
            "corrected_answer":"3","solution":REPAIRED_SOLUTION,
            "grade_corrected":false,"content_published":false
        });
        assert!(
            reports::finish(&worker, &job, &result, Some(&correction))
                .await
                .unwrap()
        );
        let (status, terminal) =
            poll(&app, Some(reporter), queued["report_id"].as_str().unwrap()).await;
        assert_eq!(status, StatusCode::OK, "{terminal}");
        assert_eq!(terminal["status"], "completed");
        assert_eq!(terminal["result"]["grade_corrected"], true);
        assert_eq!(terminal["result"]["content_published"], true);
        assert_public(&terminal);
        assert!(!terminal.to_string().contains("INTERNAL MODEL REASONING"));
        assert!(terminal.get("input").is_none());
        assert!(terminal.get("source_hash").is_none());
        for (index, answer, expected_outcome) in [
            (0, "  three  ", "correct"),
            (1, "6/2", "correct"),
            (2, "4", "incorrect"),
        ] {
            let learner = lesson_learner(
                &db,
                &format!("report-grade-{index}@example.com"),
                original_problem(),
            )
            .await;
            let (status, body) = answer_task(
                &app,
                learner,
                LESSON,
                json!({"problem_id":PROBLEM_ID,"answer":answer}),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(body["outcome"], expected_outcome, "{body}");
            assert!(body.to_string().contains(REPAIRED_SOLUTION), "{body}");
            let events = events_of_type(&db, learner, "attempt").await;
            assert_eq!(events.len(), 1);
            assert_eq!(events[0]["problem"]["expected"], "3");
            assert_eq!(events[0]["given_answer"], answer);
        }
        // A different immutable statement has a separate correction identity.
        let mut distinct = original_problem();
        distinct.text = "Compute 9 - 4.".to_owned();
        distinct.expected.answer = "5".to_owned();
        let learner = lesson_learner(&db, "report-distinct@example.com", distinct).await;
        let (status, body) = answer_task(
            &app,
            learner,
            LESSON,
            json!({"problem_id":PROBLEM_ID,"answer":"3"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["outcome"], "incorrect");
        assert_eq!(
            events_of_type(&db, learner, "attempt").await[0]["problem"]["expected"],
            "5"
        );
        assert_eq!(model_calls(&db).await, 0);
        worker.pool().close().await;
    })
    .await;
}

#[tokio::test]
async fn repeated_corrections_keep_one_identity_after_the_expected_answer_changes() {
    TestDb::with(|db| async move {
        let app=lesson_app(&db);
        let (first,attempt)=reported_learner(&db,"report-version-one@example.com").await;
        assert_eq!(create(&app,first,LESSON,report_body(&attempt)).await.0,StatusCode::OK);
        let worker=worker_db(&db).await;
        let first_job=reports::claim(&worker).await.unwrap().unwrap();
        let publish=|hash:&str,answers:Value|json!({
            "candidate_answer":"3","solution":REPAIRED_SOLUTION,"accepted_answers":answers,
            "formal_problem":{"kind":"numeric_expression","expression":"8-5"},
            "verification":{"source_hash":hash,"status":"completed","supported":true,
                "learner":{"status":"proved"},"candidate":{"status":"proved"}},
            "regressions":[{"answer":"3","correct":true},{"answer":"4","correct":false}]
        });
        let result=json!({"resolution":"confirmed_issue","qwen_verdict":"correct","verification":"proved"});
        assert!(reports::finish(&worker,&first_job,&result,Some(&publish(&first_job.source_hash,json!(["3"])))).await.unwrap());
        let mut current=original_problem();
        current.expected.answer="3".to_owned();
        let second=lesson_learner(&db,"report-version-two@example.com",current).await;
        handoff(&db,second).await;
        let second_attempt=format!("{LESSON}-version-two");
        let mut verdict=a_miss();
        verdict.given_answer="three";
        let event=attempt_payload(LESSON,&second_attempt,"kp1",(ORIGINAL,"3"),&verdict);
        seed_attempt_row(&db,second,3,&second_attempt,&event).await;
        assert_eq!(create(&app,second,LESSON,report_body(&second_attempt)).await.0,StatusCode::OK);
        let second_job=reports::claim(&worker).await.unwrap().unwrap();
        assert_eq!(second_job.source_hash,first_job.source_hash);
        assert_eq!(second_job.input["correction_version"],1);
        assert_eq!(second_job.input["source"]["problem"]["expected"],"3");
        assert!(reports::finish(&worker,&second_job,&result,Some(&publish(&second_job.source_hash,json!(["3","three"])))).await.unwrap());
        let mut tx=cadus_store::begin_tenant(&db.app,second).await.unwrap();
        let latest=reports::published(&mut tx,&first_job.source_hash).await.unwrap().unwrap();
        assert_eq!(latest["version"],2);
        tx.rollback().await.unwrap();
        let third=lesson_learner(&db,"report-version-three@example.com",original_problem()).await;
        let (status,grade)=answer_task(&app,third,LESSON,json!({"problem_id":PROBLEM_ID,"answer":"  three  "})).await;
        assert_eq!(status,StatusCode::OK,"{grade}");
        assert_eq!(grade["outcome"],"correct");
        assert_eq!(events_of_type(&db,third,"attempt").await[0]["problem"]["expected"],"3");
        worker.pool().close().await;
    }).await;
}

#[tokio::test]
async fn reporting_a_served_question_never_creates_a_learner_attempt_or_reveals_its_answer() {
    TestDb::with(|db|async move{
        let app=lesson_app(&db);
        let user=lesson_learner(&db,"report-before-answer@example.com",original_problem()).await;
        handoff(&db,user).await;
        let body=json!({"request_id":Uuid::new_v4(),"problem_id":PROBLEM_ID,"report_kind":"served","note":"Check the question."});
        let (status,queued)=create(&app,user,LESSON,body).await;
        assert_eq!(status,StatusCode::OK,"{queued}");
        assert!(events_of_type(&db,user,"attempt").await.is_empty());
        let worker=worker_db(&db).await;
        let job=reports::claim(&worker).await.unwrap().unwrap();
        assert_eq!(job.input["content_only"],true);
        assert!(job.input["attempt"].is_null());
        assert_eq!(job.input["event_seq"],0);
        let correction=json!({"candidate_answer":"3","solution":REPAIRED_SOLUTION,"accepted_answers":["3"],
            "formal_problem":{"kind":"numeric_expression","expression":"8-5"},
            "verification":{"source_hash":job.source_hash,"status":"completed","supported":true,
                "learner":{"status":"proved"},"candidate":{"status":"proved"}}});
        let result=json!({"resolution":"confirmed_issue","qwen_verdict":"correct","verification":"proved",
            "corrected_answer":"3","solution":REPAIRED_SOLUTION});
        assert!(reports::finish(&worker,&job,&result,Some(&correction)).await.unwrap());
        let hidden=poll(&app,Some(user),queued["report_id"].as_str().unwrap()).await;
        assert_eq!(hidden.0,StatusCode::OK);
        assert!(hidden.1["result"].get("corrected_answer").is_none());
        assert!(!hidden.1.to_string().contains(REPAIRED_SOLUTION));
        assert!(events_of_type(&db,user,"attempt").await.is_empty());
        assert!(events_of_type(&db,user,"regraded").await.is_empty());
        let (status,grade)=answer_task(&app,user,LESSON,json!({"problem_id":PROBLEM_ID,"answer":"3"})).await;
        assert_eq!(status,StatusCode::OK,"{grade}");
        assert_eq!(grade["outcome"],"correct");
        let shown=poll(&app,Some(user),queued["report_id"].as_str().unwrap()).await;
        assert_eq!(shown.1["result"]["content_published"],true);
        assert_eq!(shown.1["result"]["grade_corrected"],false);
        worker.pool().close().await;
    }).await;
}

#[test]
fn source_identity_preserves_statement_contract_and_context_but_versions_answer_repairs() {
    let original = json!({"problem":{"text":"Compute 2+2.","expected":"5","answer_contract":{"type":"exact"}},
        "curriculum_digest":"curriculum-one","engine_digest":"engine-one"});
    let identity = reports::source_identity(&original);
    let mut repaired = original.clone();
    repaired["problem"]["expected"] = json!("4");
    assert_eq!(identity, reports::source_identity(&repaired));
    for changed in [
        json!({"problem":{"text":"Compute 2+3.","expected":"5","answer_contract":{"type":"exact"}},"curriculum_digest":"curriculum-one","engine_digest":"engine-one"}),
        json!({"problem":{"text":"Compute 2+2.","expected":"5","answer_contract":{"type":"numeric"}},"curriculum_digest":"curriculum-one","engine_digest":"engine-one"}),
        json!({"problem":{"text":"Compute 2+2.","expected":"5","answer_contract":{"type":"exact"}},"curriculum_digest":"curriculum-two","engine_digest":"engine-one"}),
        json!({"problem":{"text":"Compute 2+2.","expected":"5","answer_contract":{"type":"exact"}},"curriculum_digest":"curriculum-one","engine_digest":"engine-two"}),
    ] {
        assert_ne!(identity, reports::source_identity(&changed));
    }
    assert_eq!(original["problem"]["expected"], "5");
}
