//! Instruction approval and re-gating regressions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use sqlx::PgPool;

use super::{Regated, regate_knowledge_point};

#[test]
fn approval_rejects_policy_changes_between_review_and_write() {
    use super::check_approval_context;
    let reviewed = br#"{"policy_digest":"v1","template_context_digest":null,"curriculum_digest":"c1","review_engine_digest":"e1"}"#;
    assert!(check_approval_context(reviewed, Some("v1"), None, "c1", "e1").is_ok());
    for (body, current) in [
        (b"{}".as_slice(), Some("v1")),
        (reviewed.as_slice(), Some("v2")),
        (reviewed.as_slice(), None),
    ] {
        assert_eq!(
            check_approval_context(body, current, None, "c1", "e1")
                .unwrap_err()
                .code,
            "review_context_changed"
        );
    }
    assert!(check_approval_context(br#"{"policy_digest":7}"#, None, None, "c1", "e1").is_err());
    assert!(check_approval_context(reviewed, Some("v1"), None, "c2", "e1").is_err());
    assert!(check_approval_context(reviewed, Some("v1"), None, "c1", "e2").is_err());
}

#[test]
fn approval_rejects_template_bank_changes_between_review_and_write() {
    let reviewed = br#"{"policy_digest":null,"template_context_digest":"bank-v1","curriculum_digest":"c1","review_engine_digest":"e1"}"#;
    assert!(super::check_approval_context(reviewed, None, Some("bank-v1"), "c1", "e1").is_ok());
    for (body, current) in [
        (b"{}".as_slice(), Some("bank-v1")),
        (reviewed.as_slice(), Some("bank-v2")),
        (reviewed.as_slice(), None),
    ] {
        assert_eq!(
            super::check_approval_context(body, None, current, "c1", "e1")
                .unwrap_err()
                .code,
            "review_context_changed"
        );
    }
}

// ----------------------------------------------------------------------- //

/// The serving key of the knowledge point under test.
const KP: &str = "perfect-squares/squares";

/// A template document of `KP`. It renders `Compute $9^{2}$.` with the
/// answer 81, among the twelve squares of 1 to 12.
const TEMPLATE: &str = r#"{"v":1,"topic_id":"perfect-squares","answer_kind":"numeric","statement":"Compute ${a}^{{2}}$.","params":{"a":{"kind":"int","low":1,"high":12}},"answer_expr":"a**2","samples":[{"params":{"a":1},"expected":"1"},{"params":{"a":12},"expected":"144"}],"space_size":12}"#;

/// A ladder whose only rung states 81, the answer of a rendered instance.
const GIVE_AWAY: &str = r#"{"hints": ["For a base of 9 the product is 81."]}"#;

/// The gate sentence that ladder earns.
const GIVE_AWAY_REASON: &str = "rung 0 reads 'For a base of 9 the product is 81.', which \
names the answer '81' this knowledge point serves — a hint is a question, never the final step \
(Hard Rule 3)";

/// A teach page that works a problem the template never renders.
const CLEAN_PAGE: &str = r#"{"concept": "Squaring multiplies a number by itself.",
    "worked_example": {"problem": "Compute $15^2$.",
    "steps": ["Write the base twice.", "The product is 225."]}}"#;

/// Seed one `content_store` row.
async fn seed(pool: &PgPool, digest: &str, kind: &str, status: &str, body: &str) {
    sqlx::query(
        "INSERT INTO content_store (digest, kp_id, kind, body, status)
         VALUES ($1, $2, $3, $4::jsonb, $5)",
    )
    .bind(digest)
    .bind(KP)
    .bind(kind)
    .bind(body)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
}

/// The status and the review reason of one row.
async fn state_of(pool: &PgPool, digest: &str) -> (String, Option<String>) {
    sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status, review_reason FROM content_store WHERE digest = $1",
    )
    .bind(digest)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// FIX2-M6-A, part 3. Approving a template judges the PENDING pages and
/// ladders of that knowledge point again.
///
/// A ladder authored before any template existed was gated with an empty
/// instance set, so a rung that states a rendered answer stands `pending` and
/// one click serves it. The re-gate moves it to `rejected`, with the gate's
/// own sentence as the reason. The approved ladder beside it is not touched:
/// its existing review decision is preserved by this pending-document pass.
#[tokio::test]
async fn the_re_gate_refuses_a_pending_ladder_the_new_material_gives_away() {
    TestDb::with(|db| async move {
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        seed(
            &db.admin,
            "sha256:the-template",
            "template",
            "approved",
            TEMPLATE,
        )
        .await;
        seed(
            &db.admin,
            "sha256:the-ladder",
            "hint_ladder",
            "pending",
            GIVE_AWAY,
        )
        .await;
        seed(
            &db.admin,
            "sha256:passed",
            "hint_ladder",
            "approved",
            GIVE_AWAY,
        )
        .await;
        seed(&db.admin, "sha256:the-page", "teach", "pending", CLEAN_PAGE).await;

        let regated = regate_knowledge_point(&handle, &handle, None, KP)
            .await
            .expect("the re-gate runs");

        assert_eq!(
            regated,
            vec![Regated {
                digest: "sha256:the-ladder".to_owned(),
                kind: "hint_ladder".to_owned(),
                reason: GIVE_AWAY_REASON.to_owned(),
            }]
        );
        assert_eq!(
            state_of(&db.admin, "sha256:the-ladder").await,
            ("rejected".to_owned(), Some(GIVE_AWAY_REASON.to_owned()))
        );
        // This pending-document pass preserves an existing approval.
        assert_eq!(
            state_of(&db.admin, "sha256:passed").await,
            ("approved".to_owned(), None)
        );
        // The page names no served answer, so it waits for its reviewer.
        assert_eq!(
            state_of(&db.admin, "sha256:the-page").await,
            ("pending".to_owned(), None)
        );
        // A template is not an instruction document, so it is not judged.
        assert_eq!(
            state_of(&db.admin, "sha256:the-template").await,
            ("approved".to_owned(), None)
        );
    })
    .await;
}

/// The same ladder with no template on the knowledge point is left alone:
/// nothing serves 81, so the ladder gives nothing away. The stored TEMPLATE is
/// what makes the difference, and this test is the control.
#[tokio::test]
async fn the_re_gate_refuses_nothing_when_no_template_serves() {
    TestDb::with(|db| async move {
        let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        seed(
            &db.admin,
            "sha256:the-ladder",
            "hint_ladder",
            "pending",
            GIVE_AWAY,
        )
        .await;

        let regated = regate_knowledge_point(&handle, &handle, None, KP)
            .await
            .expect("the re-gate runs");

        assert_eq!(regated, Vec::new());
        assert_eq!(
            state_of(&db.admin, "sha256:the-ladder").await,
            ("pending".to_owned(), None)
        );
    })
    .await;
}
