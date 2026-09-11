//! Live hint compatibility after review changes preserves answer submission.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use axum::http::StatusCode;
use cadus_store::test_support::TestDb;
use common::{
    KEY, LESSON, POOL_ANSWER, answer_task_ok, assert_refused, drill_app, hint_task,
    learner_with_pool_row, problem_id_of, put_state, seed_content, serve_ok, stored_state,
};
use serde_json::json;

async fn template_and_hint(db: &TestDb) {
    seed_content(db, KEY, "template", "source-t1", json!({})).await;
    sqlx::query(
        "UPDATE serving_pool AS sp
         SET content_digest = cs.digest,
             source_curriculum_digest = cs.approved_curriculum_digest,
             source_review_engine_digest = cs.approved_review_engine_digest
         FROM content_store AS cs
         WHERE sp.kp_id = $1 AND cs.digest = 'source-t1'",
    )
    .bind(KEY)
    .execute(&db.admin)
    .await
    .unwrap();
    seed_content(
        db,
        KEY,
        "hint_ladder",
        "hint",
        json!({"hints":["Add the ones first."]}),
    )
    .await;
    refresh_hint(db).await;
}
async fn refresh_hint(db: &TestDb) {
    sqlx::query(
        "UPDATE content_store
         SET approved_template_context_digest = public.cadus_template_context(
             kp_id, NULL, approved_curriculum_digest, approved_review_engine_digest)
         WHERE digest = 'hint'",
    )
    .execute(&db.admin)
    .await
    .unwrap();
}

#[tokio::test]
async fn template_bank_drift_refuses_hints_until_review_and_retired_sources_stay_refused() {
    TestDb::with(|db| async move {
        let user = learner_with_pool_row(&db, "hint-context@example.test").await;
        template_and_hint(&db).await;
        let app = drill_app(&db);
        let id = problem_id_of(&serve_ok(&app, user, LESSON).await);
        assert_eq!(hint_task(&app, user, LESSON, &id).await.0, StatusCode::OK);
        seed_content(&db, KEY, "template", "source-t2", json!({})).await;
        assert_refused(
            &hint_task(&app, user, LESSON, &id).await,
            StatusCode::CONFLICT,
            "no_hint_ladder",
        );
        refresh_hint(&db).await;
        // Ordinary practice retains both approved sources, so T1 remains covered.
        assert_eq!(hint_task(&app, user, LESSON, &id).await.0, StatusCode::OK);
        sqlx::query("UPDATE content_store SET status='rejected' WHERE digest='source-t1'")
            .execute(&db.admin)
            .await
            .unwrap();
        refresh_hint(&db).await;
        assert_refused(
            &hint_task(&app, user, LESSON, &id).await,
            StatusCode::CONFLICT,
            "no_hint_ladder",
        );
        let graded = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id":id,"answer":POOL_ANSWER}),
        )
        .await;
        assert_eq!(graded["correct"], true);
    })
    .await;
}

#[tokio::test]
async fn legacy_live_questions_without_provenance_refuse_hints_but_keep_grading() {
    TestDb::with(|db| async move {
        let user = learner_with_pool_row(&db, "legacy-hint-context@example.test").await;
        template_and_hint(&db).await;
        let app = drill_app(&db);
        let id = problem_id_of(&serve_ok(&app, user, LESSON).await);
        let mut state = stored_state(&db, user).await;
        state.served.get_mut(LESSON).unwrap().handoff = None;
        put_state(&db, user, &state).await;
        assert_refused(
            &hint_task(&app, user, LESSON, &id).await,
            StatusCode::CONFLICT,
            "hint_context_changed",
        );
        let graded = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id":id,"answer":POOL_ANSWER}),
        )
        .await;
        assert_eq!(graded["correct"], true);
    })
    .await;
}
