//! `/api/admin/content*` — the C6 review surface (M6 R5).
//!
//! Requirements: C6 (a human approves, and the approval binds to the digest), A6
//! (a skipped check is explicit), T3 (the attempts and the money reach the
//! reviewer), R4 (the routes do local CPU work and database I/O only).
//!
//! Spec: `docs/reference/authoring-and-spa-1.0-spec.md` section 3.2 and row R5
//! of section 7. The four acceptance checks of row R5 are the four tests marked
//! ACCEPTANCE below.
//!
//! Every expected value here is a LITERAL: the status codes, the error codes,
//! the message text, the eight rendered instances with their computed answers,
//! and the row values the writes leave in `content_store`. Nothing is re-read
//! from the code under test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::admin::*;

use common::{SESSION_TOKEN_TWO, get, get_bearer, post_bearer, send};

// --------------------------------------------------------------------------- //
// The two refusals
// --------------------------------------------------------------------------- //

/// A request with no credential is `401 unauthorized` on all four routes.
#[tokio::test]
async fn a_request_with_no_session_is_unauthorized_on_all_four() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_pending(&db).await;

        for answer in [
            send(&app, get(LIST_PATH)).await,
            send(&app, get(SHOW_PATH)).await,
            send(&app, common::post(APPROVE_PATH, &json!({}))).await,
            send(
                &app,
                common::post(REJECT_PATH, &json!({"reason": "no good"})),
            )
            .await,
        ] {
            assert_eq!(answer.status.as_u16(), 401, "{}", answer.body);
            assert_eq!(answer.code(), "unauthorized");
        }
    })
    .await;
}

/// ACCEPTANCE. A live session on an account that is not an admin is
/// `403 forbidden` on all four routes.
///
/// The account exists and the session is live, so the refusal is the admin gate
/// and not the session guard. No answer carries a document field.
#[tokio::test]
async fn a_session_that_is_not_an_admin_is_forbidden_on_all_four() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_pending(&db).await;
        seed_account(&db, "r5-learner@example.test", SESSION_TOKEN_TWO.1, false).await;

        let learner_get = |uri: &'static str| {
            let app = app.clone();
            async move { send(&app, get_bearer(uri, SESSION_TOKEN_TWO.0)).await }
        };
        let learner_post = |uri: &'static str, body: Value| {
            let app = app.clone();
            async move { send(&app, post_bearer(uri, SESSION_TOKEN_TWO.0, &body)).await }
        };

        for answer in [
            learner_get(LIST_PATH).await,
            learner_get(SHOW_PATH).await,
            learner_post(APPROVE_PATH, json!({})).await,
            learner_post(REJECT_PATH, json!({"reason": "no good"})).await,
        ] {
            assert_forbidden(&answer);
            assert!(
                answer.body.get("items").is_none() && answer.body.get("body").is_none(),
                "the refusal carried a document: {}",
                answer.body
            );
        }

        // The refusal wrote nothing.
        assert_eq!(
            row_state(&db, PENDING).await,
            ("pending".to_string(), None, None, false)
        );
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The show route
// --------------------------------------------------------------------------- //

/// ACCEPTANCE. The show route renders eight instances with computed answers.
///
/// 1.0 `scripts/review_templates.py:52` prints `SAMPLE_INSTANCES = 8`, and its
/// docstring gives the reason: a bad corner is obvious in the instances and
/// invisible in the expression. The eight pairs are pinned in [`INSTANCES`].
#[tokio::test]
async fn the_show_route_returns_eight_instances_with_computed_answers() {
    TestDb::with(|db| async move {
        let answer = show_pending(&db).await;
        assert_eq!(answer.body.get("sample_instances"), Some(&json!(8)));
        let instances = answer
            .body
            .get("instances")
            .and_then(Value::as_array)
            .expect("the answer carries no instances array");
        assert_eq!(instances.len(), 8, "{}", answer.body);
        let expected: Vec<Value> = INSTANCES
            .iter()
            .map(|(text, expected)| json!({"text": text, "answer": expected}))
            .collect();
        assert_eq!(instances, &expected);
        assert_eq!(answer.body.get("instances_note"), Some(&Value::Null));

        // Each pinned answer is the answer of its pinned problem, by the test's
        // own arithmetic. The eight statements are distinct, as one pool row set
        // is distinct.
        for (text, expected) in INSTANCES {
            assert_eq!(difference_of(text), expected, "{text}");
        }
        let mut texts: Vec<&str> = INSTANCES.iter().map(|(text, _)| *text).collect();
        texts.sort_unstable();
        texts.dedup();
        assert_eq!(texts.len(), 8, "two pinned instances share one statement");
    })
    .await;
}

/// The show route carries the gate notes, the T3 numbers, and the body.
///
/// The gate note is the sentence `crates/core/tests/template_gate.rs` pins for
/// this document: neither corner of the crossed-corner rule satisfies the band,
/// so the gate skips the rule and says so (A6).
#[tokio::test]
async fn the_show_route_carries_the_gate_notes_and_the_authoring_bill() {
    TestDb::with(|db| async move {
        let answer = show_pending(&db).await;
        assert_eq!(answer.body.get("digest"), Some(&json!("r5-pending-digest")));
        assert_eq!(answer.body.get("kp_id"), Some(&json!("band/kp1")));
        assert_eq!(answer.body.get("kind"), Some(&json!("template")));
        assert_eq!(answer.body.get("status"), Some(&json!("pending")));
        assert_eq!(answer.body.get("authoring_attempts"), Some(&json!(4)));
        assert_eq!(
            answer.body.get("authoring_cost_usd"),
            Some(&json!("0.012500"))
        );
        assert_eq!(answer.body.get("approved_at"), Some(&Value::Null));
        assert_eq!(answer.body.get("review_reason"), Some(&Value::Null));
        assert_eq!(answer.body.get("body"), Some(&template_body()));
        assert_eq!(
            answer.body.get("gate"),
            Some(&json!({
                "kp_id": "band/kp1",
                "digest": "r5-pending-digest",
                "gated": true,
                "exhaustive": true,
                "finite_cases": [],
                "finite_policy_fingerprint": null,
                "instances_checked": 17,
                "notes": [
                    "the crossed-corner rule is skipped for a and b: the constraints admit no \
                     tuple at a=2 with b=9, nor at a=10 with b=1"
                ],
            }))
        );
    })
    .await;
}

/// A digest the table does not hold is `404 not_found` on the show route.
#[tokio::test]
async fn the_show_route_of_an_unknown_digest_is_not_found() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;

        let answer = admin_get(&app, &format!("/api/admin/content/{ABSENT}")).await;

        assert_eq!(answer.status.as_u16(), 404, "{}", answer.body);
        assert_eq!(answer.code(), "not_found");
        assert_eq!(
            answer
                .body
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str),
            Some("No stored document carries that digest.")
        );
    })
    .await;
}

/// A body that is not a template renders no instance and says why.
///
/// A teach page has no statement and no answer expression, so the show route
/// carries an empty instance list and a null gate block. A6 refuses a silent
/// empty list, so the kind is visible in the same answer.
///
/// The body is the document unit R6 defined
/// (`cadus_core::instruction::TeachPage`), so the reviewer line reads its
/// `concept`. That document denies an unknown field, so a stored teach row can
/// carry no headline field of its own.
#[tokio::test]
async fn a_teach_document_renders_no_instance_and_no_gate() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        let mut seed = Seed::template("r5-teach-digest", KEY, "pending");
        seed.kind = "teach";
        seed.body = json!({
            "concept": "Take from the next column.",
            "worked_example": {
                "problem": "Compute $52 - 27$.",
                "steps": ["Borrow ten from the tens column.", "$12 - 7 = 5$ and $4 - 2 = 2$."]
            }
        });
        seed_row(&db, &seed).await;

        let answer = admin_get(&app, "/api/admin/content/r5-teach-digest").await;

        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_instruction_document(&answer, "teach", "Take from the next column.");
    })
    .await;
}

/// The queue line of a hint ladder reads its widest rung (unit R6).
///
/// A ladder carries `hints` and nothing else, so the line has no `statement` and
/// no `concept` to read. Rung 0 is the widest nudge, so it is the sentence that
/// tells a reviewer which ladder the line is, and the raw JSON text never
/// reaches the queue.
#[tokio::test]
async fn a_hint_ladder_line_reads_its_widest_rung() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        let mut seed = Seed::template("r6-ladder-digest", KEY, "pending");
        seed.kind = "hint_ladder";
        seed.body = json!({
            "hints": [
                "Which column do you take from first?",
                "The ones column needs ten more before it can subtract."
            ]
        });
        seed_row(&db, &seed).await;

        let answer = admin_get(&app, "/api/admin/content/r6-ladder-digest").await;

        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_instruction_document(
            &answer,
            "hint_ladder",
            "Which column do you take from first?",
        );
    })
    .await;
}
