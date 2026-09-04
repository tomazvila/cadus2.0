//! Part of `tests/admin_content.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::admin::*;

use cadus_store::test_support::TestDb;
use serde_json::{Value, json};

// --------------------------------------------------------------------------- //
// The two writes
// --------------------------------------------------------------------------- //

/// ACCEPTANCE. A reject with no reason is `422`, and it writes nothing.
///
/// The three refused bodies are the three ways a reason goes missing: an empty
/// object, a null field, and a field of spaces alone. 1.0 makes `--reason` a
/// required argument of `cmd_reject`.
#[tokio::test]
async fn a_reject_without_a_reason_is_unprocessable() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;

        for body in [json!({}), json!({"reason": null}), json!({"reason": "   "})] {
            let answer = admin_post(&app, REJECT_PATH, &body).await;

            assert_invalid_request(
                &answer,
                "A rejection needs a reason: send a JSON object with a non-empty \"reason\" \
                 string.",
            );
        }

        assert_eq!(
            row_state(&db, PENDING).await,
            ("pending".to_string(), None, None, false)
        );
    })
    .await;
}

/// A reject with a reason writes the verdict and the reason.
#[tokio::test]
async fn a_reject_with_a_reason_writes_the_verdict() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;

        let answer = admin_post(
            &app,
            REJECT_PATH,
            &json!({"reason": "  the low edge is missing  "}),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_eq!(
            answer.body,
            json!({"digest": "r5-pending-digest", "status": "rejected"})
        );
        assert_eq!(
            row_state(&db, PENDING).await,
            (
                "rejected".to_string(),
                Some("the low edge is missing".to_string()),
                None,
                false
            )
        );
    })
    .await;
}

/// An approve stamps the row with the reviewer, and a second approve is the
/// same answer.
#[tokio::test]
async fn an_approve_stamps_the_reviewer_and_is_idempotent() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let reviewer = seed_admin(&db).await;
        seed_pending(&db).await;

        let first = admin_post(&app, APPROVE_PATH, &json!({})).await;
        assert_eq!(first.status.as_u16(), 200, "{}", first.body);
        assert_eq!(first.body.get("digest"), Some(&json!("r5-pending-digest")));
        assert_eq!(first.body.get("status"), Some(&json!("approved")));
        let stamp = first
            .body
            .get("approved_at")
            .and_then(Value::as_str)
            .expect("the answer carries no approved_at")
            .to_string();

        let second = admin_post(&app, APPROVE_PATH, &json!({})).await;
        assert_eq!(second.status.as_u16(), 200, "{}", second.body);
        assert_eq!(
            second.body.get("approved_at").and_then(Value::as_str),
            Some(stamp.as_str())
        );

        let (status, reason, approved_by, stamped) = row_state(&db, PENDING).await;
        assert_eq!(status, "approved");
        assert_eq!(reason, None);
        assert_eq!(approved_by, Some(reviewer));
        assert!(stamped, "the approved row carries no approved_at");
    })
    .await;
}

/// A write on a digest the table does not hold is `404 not_found`.
#[tokio::test]
async fn a_write_on_an_unknown_digest_is_not_found() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;

        for (path, body) in [
            (format!("/api/admin/content/{ABSENT}/approve"), json!({})),
            (
                format!("/api/admin/content/{ABSENT}/reject"),
                json!({"reason": "no good"}),
            ),
        ] {
            let answer = admin_post(&app, &path, &body).await;

            assert_eq!(answer.status.as_u16(), 404, "{path} gave {}", answer.body);
            assert_eq!(answer.code(), "not_found");
        }
    })
    .await;
}

/// A deployment with no admin connection refuses both writes with `503`.
///
/// `cadus_app` holds SELECT on `content_store` and nothing else, so the write
/// would fail at the server with SQLSTATE 42501 and the reviewer would read
/// `500`. The named path answers before the statement runs.
#[tokio::test]
async fn a_write_with_no_admin_connection_is_service_unavailable() {
    TestDb::with(|db| async move {
        let app = app_without_admin(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;

        for (path, body) in [
            (APPROVE_PATH, json!({})),
            (REJECT_PATH, json!({"reason": "no good"})),
        ] {
            let answer = admin_post(&app, path, &body).await;

            assert_eq!(answer.status.as_u16(), 503, "{path} gave {}", answer.body);
            assert_eq!(answer.code(), "admin_path_unavailable");
        }

        assert_eq!(
            row_state(&db, PENDING).await,
            ("pending".to_string(), None, None, false)
        );
    })
    .await;
}

/// A reject body that is not a JSON object is `422`, and the handler never
/// panics on it.
#[tokio::test]
async fn a_reject_body_that_is_not_an_object_is_unprocessable() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;

        for body in [json!("a string"), json!([1, 2, 3]), json!(7)] {
            let answer = admin_post(&app, REJECT_PATH, &body).await;

            assert_invalid_request(&answer, "The request body must be a JSON object.");
        }
    })
    .await;
}

/// A rejection reason past the bound is `422`.
#[tokio::test]
async fn a_reason_past_the_bound_is_unprocessable() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;

        let long = "x".repeat(1_001);
        let answer = admin_post(&app, REJECT_PATH, &json!({ "reason": long })).await;

        assert_invalid_request(&answer, "The rejection reason is too long.");
        assert_eq!(
            row_state(&db, PENDING).await,
            ("pending".to_string(), None, None, false)
        );
    })
    .await;
}
