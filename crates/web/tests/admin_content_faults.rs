//! Part of `tests/admin_content.rs`: the store faults and the document shapes
//! the review routes report. The header of that file gives the requirements
//! and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::{AppState, create_app};
use common::admin::*;
use common::{SESSION_TOKEN_ONE, expose_to_policies, fail_reads, fail_updates, hide_column, send};

/// The digest of the pending teach page of `KEY`.
const TEACH: &str = "r5-teach-digest";

/// The show path of `TEACH`.
const TEACH_SHOW_PATH: &str = "/api/admin/content/r5-teach-digest";

/// The approve path of `TEACH`.
const TEACH_APPROVE_PATH: &str = "/api/admin/content/r5-teach-digest/approve";

/// The text of the re-gate read of the pending pages, and of no other
/// `content_store` read.
const REGATE_NEEDLE: &str = "kind IN ($5, $6)";

/// The text of the re-gate read of the pending pages and the approved
/// template, and of no other `content_store` read.
const RE_REGATE_NEEDLE: &str = "ORDER BY kind, digest";

/// A pending teach page of `KEY` whose worked example is the authored
/// exemplar, which the gate refuses once the point serves (Hard Rule 1).
fn teach_seed() -> Seed<'static> {
    Seed {
        digest: TEACH,
        kp_id: KEY,
        kind: "teach",
        status: "pending",
        body: json!({
            "concept": "Take the smaller count away.",
            "worked_example": {"problem": "Compute $7 - 2$.", "steps": ["7 - 2 = 5."]}
        }),
        attempts: 1,
        cost: None,
        curriculum_digest: None,
        review_engine_digest: None,
    }
}

/// One pending template of `digest` for `KEY`, with `body`.
async fn seed_template_body(db: &TestDb, digest: &str, body: Value) {
    let mut seed = Seed::template(digest, KEY, "pending");
    seed.body = body;
    seed_row(db, &seed).await;
}

/// The show answer of `digest`: its instance list and its note.
async fn instances_of(app: &Router, digest: &str) -> (usize, String) {
    let answer = admin_get(app, &format!("/api/admin/content/{digest}")).await;
    assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
    let count = answer.body["instances"].as_array().unwrap().len();
    let note = answer.body["instances_note"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    (count, note)
}

/// A review write that fails past the row lookup is `500 internal_error`.
#[tokio::test]
async fn a_review_write_that_fails_is_500() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;
        fail_updates(&db, "content_store", "NEW.status = 'approved'").await;

        let body = fixture_approve_body(&db, PENDING).await;
        let answer = admin_post(&app, APPROVE_PATH, &body).await;
        assert_eq!(answer.status.as_u16(), 500, "{}", answer.body);
        assert_eq!(answer.code(), "internal_error");
    })
    .await;
}

/// A `content_store` read that fails is `500` on the list and on the show.
#[tokio::test]
async fn a_content_read_that_fails_is_500_on_the_list_and_the_show() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;
        hide_column(&db, "content_store", "body").await;

        for path in [LIST_PATH, SHOW_PATH] {
            let answer = admin_get(&app, path).await;
            assert_eq!(answer.status.as_u16(), 500, "{}", answer.body);
            assert_eq!(answer.code(), "internal_error");
        }
    })
    .await;
}

/// The show needs the curriculum to render instances: without one it is `503`.
#[tokio::test]
async fn the_show_without_a_curriculum_is_503() {
    TestDb::with(|db| async move {
        let app = create_app(
            AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
                .with_admin(Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS)),
        );
        seed_admin(&db).await;
        seed_pending(&db).await;

        let answer = admin_get(&app, SHOW_PATH).await;
        assert_eq!(answer.status.as_u16(), 503, "{}", answer.body);
        assert_eq!(answer.code(), "curriculum_unavailable");
    })
    .await;
}

/// A template row whose body is not a template document renders no instance
/// and names the cause; the queue line falls back to the body text.
#[tokio::test]
async fn a_body_that_is_not_a_template_document_is_noted() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_template_body(&db, "r5-odd", json!({"v": 1})).await;

        let (count, note) = instances_of(&app, "r5-odd").await;
        assert_eq!(count, 0);
        assert!(
            note.starts_with("the body does not read as a template document"),
            "{note}"
        );

        let list = admin_get(&app, LIST_PATH).await;
        assert_eq!(item_of(&list.body, "r5-odd")["summary"], "{\"v\":1}");
    })
    .await;
}

/// A document whose answer expression leaves the grammar does not compile,
/// and a document no draw satisfies renders nothing: each names the cause.
#[tokio::test]
async fn a_document_that_does_not_compile_or_draw_is_noted() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        let mut broken = template_body();
        broken["answer_expr"] = json!("a -");
        seed_template_body(&db, "r5-broken", broken).await;
        let mut dry = template_body();
        dry["constraints"] = json!([{"op": "gt", "left": "a", "right": {"lit": 100}}]);
        seed_template_body(&db, "r5-dry", dry).await;

        for digest in ["r5-broken", "r5-dry"] {
            let (count, note) = instances_of(&app, digest).await;
            assert_eq!(count, 0, "{digest}");
            assert!(!note.is_empty(), "{digest} carries no note");
        }
    })
    .await;
}

/// A serving key the curriculum does not name renders its instances with no
/// exemplar envelope.
#[tokio::test]
async fn a_key_outside_the_curriculum_renders_without_an_envelope() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_row(&db, &Seed::template("r5-other", OTHER_KEY, "pending")).await;

        let (count, note) = instances_of(&app, "r5-other").await;
        assert_eq!(count, 8);
        assert_eq!(note, "");
    })
    .await;
}

/// The show of an approved document carries its approval stamp.
#[tokio::test]
async fn the_show_of_an_approved_document_carries_its_stamp() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;
        let body = fixture_approve_body(&db, PENDING).await;
        let approved = admin_post(&app, APPROVE_PATH, &body).await;
        assert_eq!(approved.status.as_u16(), 200, "{}", approved.body);

        let answer = admin_get(&app, SHOW_PATH).await;
        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_eq!(
            answer.body["approved_at"].as_str(),
            approved.body["approved_at"].as_str()
        );
    })
    .await;
}

/// An approval of a template judges the pending pages of its knowledge point
/// again: the page that works the exemplar is rejected and named in the
/// answer. A second approved template with the same instances adds no
/// instance twice.
#[tokio::test]
async fn an_approval_rejects_the_page_that_the_served_material_gives_away() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;
        seed_approved(&db, KEY, "r5-same-", 1).await;
        seed_row(&db, &teach_seed()).await;

        let body = fixture_approve_body(&db, PENDING).await;
        let answer = admin_post(&app, APPROVE_PATH, &body).await;
        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        let rejected = answer.body["rejected_documents"].as_array().unwrap();
        assert_eq!(rejected.len(), 1, "{}", answer.body);
        assert_eq!(rejected[0]["digest"], TEACH);
        assert_eq!(rejected[0]["kind"], "teach");
        assert!(
            rejected[0]["reason"]
                .as_str()
                .unwrap()
                .contains("which is exemplar 0"),
            "{}",
            answer.body
        );
        let (status, reason, _, _) = row_state(&db, TEACH).await;
        assert_eq!(status, "rejected");
        assert!(reason.is_some());
    })
    .await;
}

/// An approval of a page changes no answer set, so nothing is judged again.
#[tokio::test]
async fn an_approval_of_a_page_judges_nothing_again() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_row(&db, &teach_seed()).await;

        let body = fixture_approve_body_instruction(&db).await;
        let answer = admin_post(&app, TEACH_APPROVE_PATH, &body).await;
        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_eq!(answer.body["rejected_documents"], json!([]));

        let shown = admin_get(&app, TEACH_SHOW_PATH).await;
        assert_eq!(shown.body["status"], "approved", "{}", shown.body);
    })
    .await;
}

/// A re-gate that does not run, at its read or at its rejection write, keeps
/// the approval and answers `null` for the list.
#[tokio::test]
async fn a_regate_that_does_not_run_answers_null_and_keeps_the_approval() {
    for fault in [0, 1, 2] {
        TestDb::with(move |db| async move {
            let app = app(&db);
            seed_admin(&db).await;
            seed_pending(&db).await;
            seed_row(&db, &teach_seed()).await;
            let body = fixture_approve_body(&db, PENDING).await;
            match fault {
                0 => {
                    expose_to_policies(&db, "content_store").await;
                    fail_reads(&db, "content_store", REGATE_NEEDLE).await;
                }
                1 => fail_updates(&db, "content_store", "NEW.status = 'rejected'").await,
                _ => {
                    expose_to_policies(&db, "content_store").await;
                    fail_reads(&db, "content_store", RE_REGATE_NEEDLE).await;
                }
            }

            let answer = admin_post(&app, APPROVE_PATH, &body).await;
            assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
            assert_eq!(answer.body["status"], "approved");
            assert_eq!(answer.body["rejected_documents"], Value::Null);
            let (status, _, _, _) = row_state(&db, TEACH).await;
            assert_eq!(status, "pending");
        })
        .await;
    }
}

/// A reject body that is not JSON at all is `422`.
#[tokio::test]
async fn a_reject_body_that_is_not_json_is_unprocessable() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;

        let request = Request::builder()
            .method("POST")
            .uri(REJECT_PATH)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", SESSION_TOKEN_ONE.0))
            .body(Body::from("not json"))
            .unwrap();
        let answer = send(&app, request).await;
        assert_invalid_request(&answer, "The request body must be a JSON object.");
    })
    .await;
}
