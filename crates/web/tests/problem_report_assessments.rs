//! HTTP report evidence and assessment-boundary disclosure regressions.
//! Terminal result fixtures exercise disclosure, without claiming live proof.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use cadus_core::answer::AnswerContract;
use cadus_core::integrated::{IntegratedSet, parse_item};
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::state::TaskProgress;
use cadus_web::{AppState, create_app};
use common::*;

const SECRET: &str = "SERVER_ONLY_VERIFIED_SOLUTION";
const INTEGRATED_TASK: &str = "s_2026-01-01a-multi-step";
const INTEGRATED_ITEM: &str = r#"
id: report-field-fixture
title: Staff a window
course: c1
topic: addition
component_topics: [addition, subtraction]
domain: workforce_capacity
scenario: Four visits take thirty minutes each. One clerk works sixty minutes.
given:
  - label: Visits
    value: "4"
steps:
  - id: minutes
    ask:
      prompt: How many person-minutes are needed?
      answer: "120"
      unit: person-minutes
      hints: []
    skills: [addition/kp1]
  - id: capacity
    ask:
      prompt: How many minutes can one clerk work?
      answer: "60"
      unit: min
      hints: []
    skills: [subtraction/kp1]
final:
  ask:
    prompt: How many clerks are needed?
    answer: "2"
    unit: clerks
    hints: []
  interpretation: Two clerks cover the work.
  skills: [subtraction/kp1]
"#;

async fn create(app: &Router, user: Uuid, task: &str, mut body: Value) -> (StatusCode, Value) {
    body["request_id"] = json!(Uuid::new_v4());
    let (status, raw) = call(
        app,
        Method::POST,
        &format!("/api/task/{task}/report"),
        Some(user),
        Some(body),
    )
    .await;
    (status, parse(&raw))
}
async fn poll(app: &Router, user: Uuid, id: &Value) -> Value {
    let (status, raw) = call(
        app,
        Method::GET,
        &format!("/api/reports/{}", id.as_str().unwrap()),
        Some(user),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    parse(&raw)
}
async fn terminal(db: &TestDb, id: &Value) {
    sqlx::query(
        "UPDATE problem_reports SET status='completed',stage='completed',result=$2 WHERE id=$1",
    )
    .bind(Uuid::parse_str(id.as_str().unwrap()).unwrap())
    .bind(
        json!({"resolution":"confirmed_issue","solution":SECRET,"corrected_answer":"2",
            "content_published":true,"grade_corrected":false}),
    )
    .execute(&db.admin)
    .await
    .unwrap();
}
async fn input(db: &TestDb, id: &Value) -> Value {
    sqlx::query_scalar("SELECT input FROM problem_reports WHERE id=$1")
        .bind(Uuid::parse_str(id.as_str().unwrap()).unwrap())
        .fetch_one(&db.admin)
        .await
        .unwrap()
}
async fn quiz_attempt(db: &TestDb, user: Uuid, task: &str) {
    seed_event(db,user,4,BASE_US,SESSION,json!({"type":"ordinary_problem_served","v":2,
        "ts":"2026-01-01T00:00:00Z","session":SESSION,"task_id":task,"problem_id":PROBLEM_ID,
        "kp_id":"addition/kp1","item_digest":"a".repeat(64),"item_source":"exemplar","exposure":"first"})).await;
    let mut event = attempt_payload(
        task,
        "quiz-report-attempt",
        "kp1",
        (PROBLEM_TEXT, EXPECTED_ANSWER),
        &a_miss(),
    );
    event["task_type"] = json!("quiz");
    seed_attempt_row(db, user, 5, "quiz-report-attempt", &event).await;
}
async fn quiz_closed(db: &TestDb, user: Uuid, task: &str, seq: i64) {
    let event = cadus_core::event::Event::from_json(
        &json!({"type":"quiz_result","ts":"2026-01-01T00:01:00Z",
        "session":SESSION,"quiz_id":task,"score":0.5,"xp":7.5,"per_topic":[]})
        .to_string(),
    )
    .unwrap();
    seed_event(db, user, seq, BASE_US + 60_000_000, SESSION, json!(event)).await;
}

#[tokio::test]
async fn quiz_report_details_require_the_matching_native_quiz_close() {
    TestDb::with(|db| async move {
        let app = quiz_app(&db);
        let user = quiz_learner(&db, "report-quiz-disclosure@example.test", first_question()).await;
        quiz_attempt(&db, user, QUIZ).await;
        let (status, queued) = create(
            &app,
            user,
            QUIZ,
            json!({"problem_id":PROBLEM_ID,"attempt_id":"quiz-report-attempt"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{queued}");
        terminal(&db, &queued["report_id"]).await;
        assert!(
            !poll(&app, user, &queued["report_id"])
                .await
                .to_string()
                .contains(SECRET)
        );
        quiz_closed(&db, user, "other-assessment", 6).await;
        assert!(
            !poll(&app, user, &queued["report_id"])
                .await
                .to_string()
                .contains(SECRET)
        );
        quiz_closed(&db, user, QUIZ, 7).await;
        assert_eq!(
            poll(&app, user, &queued["report_id"]).await["result"]["solution"],
            SECRET
        );
    })
    .await;
}

#[tokio::test]
async fn served_quiz_with_arbitrary_task_id_stays_private_after_one_answer() {
    TestDb::with(|db| async move {
        let app = quiz_app(&db);
        let user = quiz_learner(&db, "report-served-quiz@example.test", first_question()).await;
        let task = "assessment-42";
        let mut scratch = stored_state(&db, user).await;
        let mut live = scratch.served.remove(QUIZ).unwrap();
        live.task_id = task.to_owned();
        scratch.served.insert(task.to_owned(), live);
        scratch.tasks.insert(
            task.to_owned(),
            TaskProgress {
                task_id: task.to_owned(),
                task_type: "quiz".into(),
                total: 2,
                served: 1,
                answered: 0,
                done: false,
                current_kp: None,
            },
        );
        put_state(&db, user, &scratch).await;
        let (status, queued) = create(
            &app,
            user,
            task,
            json!({"problem_id":PROBLEM_ID,"report_kind":"served"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{queued}");
        let frozen = input(&db, &queued["report_id"]).await;
        assert_eq!(frozen["report_identity"]["task_type"], "quiz");
        assert_eq!(frozen["content_only"], true);
        terminal(&db, &queued["report_id"]).await;
        quiz_attempt(&db, user, task).await;
        assert!(
            !poll(&app, user, &queued["report_id"])
                .await
                .to_string()
                .contains(SECRET)
        );
        quiz_closed(&db, user, task, 6).await;
        assert_eq!(
            poll(&app, user, &queued["report_id"]).await["result"]["solution"],
            SECRET
        );
    })
    .await;
}

#[tokio::test]
async fn integrated_reports_bind_owned_digest_and_selected_step_or_final() {
    TestDb::with(|db|async move{
        let item=parse_item(INTEGRATED_ITEM).unwrap();
        let digest=item.digest();
        let set=IntegratedSet::from_items(vec![item]);
        assert!(set.get("report-field-fixture").is_some(),"{:?}",set.findings());
        let content=quiz_content().with_integrated(set);
        let app=create_app(AppState::new(Db::new(db.app.clone(),DEFAULT_CLIENT_TIMEOUT_MS)).with_content(Arc::new(content)));
        let user=seed_learner(&db,"report-integrated-http@example.test").await;
        let other=seed_learner(&db,"report-integrated-http-other@example.test").await;
        seed_open_session(&db,user).await;
        seed_event(&db,user,2,BASE_US,SESSION,json!({"type":"integrated_attempt","ts":"2026-01-01T00:00:00Z",
            "session":SESSION,"task_id":INTEGRATED_TASK,"attempt_id":"integrated-owned","item_id":"report-field-fixture",
            "item_digest":digest,"topic":"addition","steps":[{"id":"minutes","answer":"119",
                "contract":AnswerContract::Exact,"outcome":"incorrect","assisted":false,"skills":["addition/kp1"]},{"id":"capacity","answer":"60",
                "contract":AnswerContract::Exact,"outcome":"correct","assisted":false,"skills":["subtraction/kp1"]}],
            "final_field":{"id":"final","answer":"3","contract":AnswerContract::Exact,"outcome":"incorrect",
                "assisted":false,"skills":["subtraction/kp1"]},"skills_credited":[],"solved":false,"assisted":false,
            "reasoning_ungraded":"My original reasoning"})).await;
        let request=json!({"problem_id":"report-field-fixture","report_kind":"integrated","item_digest":digest,"field_id":"minutes"});
        assert_eq!(create(&app,other,INTEGRATED_TASK,request.clone()).await.0,StatusCode::NOT_FOUND);
        assert_eq!(create(&app,user,"other-task",request.clone()).await.0,StatusCode::NOT_FOUND);
        let mut wrong=request.clone();wrong["item_digest"]=json!("0".repeat(64));
        assert_eq!(create(&app,user,INTEGRATED_TASK,wrong).await.0,StatusCode::NOT_FOUND);
        let mut wrong=request.clone();wrong["field_id"]=json!("missing");
        assert_eq!(create(&app,user,INTEGRATED_TASK,wrong).await.0,StatusCode::NOT_FOUND);
        let mut forged=request.clone();forged["answer"]=json!("FORGED");forged["source"]=json!({"problem":"FORGED"});
        let (status,step)=create(&app,user,INTEGRATED_TASK,forged).await;
        assert_eq!(status,StatusCode::OK,"{step}");
        let step_input=input(&db,&step["report_id"]).await;
        assert_eq!(step_input["attempt"]["type"],"integrated_attempt");
        assert_eq!(step_input["submission"]["given_answer"],"119");
        assert_eq!(step_input["source"]["problem"]["expected"],"120");
        assert_eq!(step_input["report_identity"]["field"],"minutes");
        assert_eq!(step_input["report_identity"]["attempt_id"],"integrated-owned:minutes");
        assert!(!step_input.to_string().contains("FORGED"));
        let mut final_request=request;final_request["field_id"]=json!("final");
        let (status,final_report)=create(&app,user,INTEGRATED_TASK,final_request).await;
        assert_eq!(status,StatusCode::OK,"{final_report}");
        let final_input=input(&db,&final_report["report_id"]).await;
        assert_eq!(final_input["submission"]["given_answer"],"3");
        assert_eq!(final_input["source"]["problem"]["expected"],"2");
        assert_eq!(final_input["report_identity"]["attempt_id"],"integrated-owned:final");
        assert_ne!(step["report_id"],final_report["report_id"]);
        assert_eq!(model_calls(&db).await,0);
    }).await;
}

#[tokio::test]
async fn diagnostic_report_freezes_owned_answer_and_tracked_run_delta() {
    TestDb::with(|db| async move {
        let app = common::placement::app(&db);
        let user = seed_learner(&db, "report-diagnostic-http@example.test").await;
        let other = seed_learner(&db, "report-diagnostic-other@example.test").await;
        let (status, raw) = call(
            &app,
            Method::POST,
            "/api/diag/start",
            Some(user),
            Some(json!({"course":"c1"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let problem = parse(&raw)["probe"]["problem_id"].clone();
        let (status, raw) = call(
            &app,
            Method::POST,
            "/api/diag/answer",
            Some(user),
            Some(json!({"problem_id":problem,"answer":"999"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let request = json!({"problem_id":problem,"report_kind":"diagnostic","answer":"FORGED"});
        assert_eq!(
            create(&app, other, "diag", request.clone()).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            create(&app, user, "wrong-task", request.clone()).await.0,
            StatusCode::NOT_FOUND
        );
        let (status, queued) = create(&app, user, "diag", request).await;
        assert_eq!(status, StatusCode::OK, "{queued}");
        let frozen = input(&db, &queued["report_id"]).await;
        assert_eq!(frozen["attempt"]["type"], "diagnostic_answer");
        assert_eq!(frozen["submission"]["given_answer"], "999");
        assert_eq!(frozen["source"]["problem"]["expected"], "7");
        assert!(!frozen.to_string().contains("FORGED"));
        let run: Value = sqlx::query_scalar(
            "SELECT to_jsonb(r) FROM problem_report_diagnostics r WHERE user_id=$1",
        )
        .bind(user)
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(frozen["task_policy"]["diagnostic_run_id"], run["run_id"]);
        assert_eq!(
            frozen["task_policy"]["diagnostic_curriculum_digest"],
            run["curriculum_digest"]
        );
        let delta = frozen["task_policy"]["diagnostic_delta"]
            .as_object()
            .unwrap();
        assert!(delta.values().any(|value| value.as_f64().unwrap() > 0.0));
        assert_eq!(
            delta.len(),
            run["state"]["balances"].as_object().unwrap().len()
        );
        let weight = frozen["task_policy"]["diagnostic_weight"].as_f64().unwrap();
        assert!(weight > 0.0 && weight <= 1.0);
        assert_eq!(run["state"]["answered"].as_array().unwrap().len(), 1);
        assert_eq!(model_calls(&db).await, 0);
    })
    .await;
}
