//! Part of `tests/diagnosis_route.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::diagnosis::*;

use std::time::Duration;

use axum::http::header;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::diagnosis::DiagnosisHub;
use http_body_util::BodyExt;
use tower::ServiceExt;

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
    common::present_session(request.headers_mut(), user);
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
        let alice = common::seed_learner(&db, "u9-alice@example.test").await;
        let bob = common::seed_learner(&db, "u9-bob@example.test").await;
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
        let alice = common::seed_learner(&db, "u9-poll-alice@example.test").await;
        let bob = common::seed_learner(&db, "u9-poll-bob@example.test").await;
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
        let user = common::seed_learner(&db, "u9-refusals@example.test").await;

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
