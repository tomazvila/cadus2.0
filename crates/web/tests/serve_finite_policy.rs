//! Authenticated serving and approval under a reviewed finite case policy.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use cadus_core::curriculum::{
    Curriculum, FiniteCaseRole, FiniteCaseVariant, FiniteObjectiveCase, FiniteObjectiveDomain, Slug,
};
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::create_app;
use common::*;

fn policy(role: FiniteCaseRole) -> FiniteObjectiveDomain {
    FiniteObjectiveDomain {
        schema_version: 1,
        review_ref: "sha256:test-reviewed-five-index-cases".to_owned(),
        cases: (1..=5)
            .map(|n| FiniteObjectiveCase {
                id: Slug::new(format!("case-{n}")).unwrap(),
                role,
                variants: vec![FiniteCaseVariant {
                    problem: format!("Return the index ${n}$."),
                    answer: n.to_string(),
                    answer_contract: None,
                }],
            })
            .collect(),
    }
}

fn graph(policy: &FiniteObjectiveDomain) -> Curriculum {
    let mut point = kp("kp1", vec![]);
    point.finite_objective_domain = Some(policy.clone());
    one_unit_curriculum(vec![topic("addition", vec![point])])
}

fn body() -> Value {
    json!({"v":1,"topic_id":"addition","answer_kind":"numeric",
        "statement":"Return the index ${a}$.","params":{"a":{"kind":"int","low":1,"high":5}},
        "answer_expr":"a","solution_sketch":"Read the displayed index.",
        "hints":["Read the index."],"samples":[{"params":{"a":1},"expected":"1"},{"params":{"a":5},"expected":"5"}]})
}

async fn seed_template(db: &TestDb, policy: &FiniteObjectiveDomain, status: &str) {
    sqlx::query("INSERT INTO content_store (digest,kp_id,kind,body,status,approved_policy_digest,approved_at,approved_curriculum_digest,approved_review_engine_digest) VALUES ('finite-content','addition/kp1','template',$1,$2,$3,now(),'curriculum-v1','engine-v1')")
        .bind(body()).bind(status).bind(policy.fingerprint(KEY).unwrap())
        .execute(&db.admin).await.unwrap();
}

async fn abandon_live(db: &TestDb, user: Uuid) {
    // Simulate starting another draw after abandoning the live question; keep
    // the durable events, task progress and avoidance memory intact.
    let mut scratch = stored_state(db, user).await;
    scratch.served.remove(LESSON);
    put_state(db, user, &scratch).await;
}

#[ignore]
#[tokio::test]
async fn five_approved_cases_rotate_for_twenty_authenticated_handoffs() {
    TestDb::with(|db| async move {
        let policy = policy(FiniteCaseRole::PracticeFresh);
        seed_template(&db, &policy, "approved").await;
        let user = seed_learner(&db, "finite-http@example.test").await;
        seed_open_session(&db, user).await;
        let app = app_with_content(&db, graph(&policy));
        let mut problems = Vec::new();
        for turn in 0..20 {
            let (status, reply) = serve_task(&app, user, LESSON).await;
            assert_eq!(status, StatusCode::OK, "turn {turn}: {reply}");
            let scratch = stored_state(&db, user).await;
            let handoff = scratch.served[LESSON].handoff.as_ref().unwrap();
            assert_eq!(handoff.item_source, cadus_core::event::ItemSource::Template);
            assert_eq!(
                handoff.exposure,
                if turn < 5 {
                    cadus_core::event::Exposure::First
                } else {
                    cadus_core::event::Exposure::Repeat
                }
            );
            problems.push(reply["text"].as_str().unwrap().to_owned());
            if turn == 0 {
                let (status, reload) = serve_task(&app, user, LESSON).await;
                assert_eq!(status, StatusCode::OK);
                assert_eq!(reload["problem_id"], reply["problem_id"]);
            }
            abandon_live(&db, user).await;
        }
        assert!(problems.windows(2).all(|pair| pair[0] != pair[1]));
        let first: std::collections::BTreeSet<_> = problems[..5].iter().collect();
        assert_eq!(first.len(), 5);
        assert_eq!(&problems[..5], &problems[5..10]);
        assert_eq!(
            events_of_type(&db, user, "ordinary_problem_served")
                .await
                .len(),
            20
        );
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM serving_pool WHERE user_id = $1")
            .bind(user)
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(rows, 5);
    })
    .await;
}

#[ignore]
#[tokio::test]
async fn taught_rehearsal_is_repeat_and_a_policy_change_requires_ai_reapproval() {
    TestDb::with(|db| async move {
        let old = policy(FiniteCaseRole::PracticeFresh);
        seed_template(&db, &old, "approved").await;
        let user = seed_learner(&db, "finite-policy-review@example.test").await;
        sqlx::query("UPDATE users SET is_admin = true WHERE id = $1")
            .bind(user)
            .execute(&db.admin)
            .await
            .unwrap();
        seed_open_session(&db, user).await;
        let current = policy(FiniteCaseRole::TaughtRehearsal);
        let app = create_app(
            state_with_content(&db, graph(&current))
                .with_admin(Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS)),
        );
        let (status, _) = serve_task(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::CONFLICT);
        let approve = "/api/admin/content/finite-content/approve";
        let (status, raw) = call(
            &app,
            Method::POST,
            approve,
            Some(user),
            Some(json!({"policy_digest":old.fingerprint(KEY).unwrap()})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{raw}");
        assert_eq!(parse(&raw)["error"]["code"], "review_context_changed");
        let fingerprint = current.fingerprint(KEY).unwrap();
        let (status, raw) = call(
            &app,
            Method::POST,
            approve,
            Some(user),
            Some(json!({"policy_digest":fingerprint})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_eq!(parse(&raw)["approved_policy_digest"], fingerprint);
        let (status, reply) = serve_task(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::OK, "{reply}");
        let scratch = stored_state(&db, user).await;
        assert_eq!(
            scratch.served[LESSON].handoff.as_ref().unwrap().exposure,
            cadus_core::event::Exposure::Repeat
        );
        abandon_live(&db, user).await;
        sqlx::query("UPDATE content_store SET status = 'rejected' WHERE digest = 'finite-content'")
            .execute(&db.admin)
            .await
            .unwrap();
        let (status, _) = serve_task(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::CONFLICT);
    })
    .await;
}
