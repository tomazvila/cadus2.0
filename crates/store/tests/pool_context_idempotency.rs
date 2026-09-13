//! Idempotency regression: the ON CONFLICT DO UPDATE with the IS DISTINCT FROM
//! predicate on source metadata.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::pool::Source;
use cadus_store::pool::{
    GenerationContext, NewInstance, insert_batch_for_user, unclaimed_depth,
};
use cadus_store::test_support::TestDb;
use common::{KP, seed_template};

const CONTENT_DIGEST: &str = "deadbeefdeadbeef";
const OTHER_DIGEST: &str = "feedbaafeedbaafeedbaafeedbaafeedbaaf";

const OLD_CURRICULUM: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const NEW_CURRICULUM: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const OLD_ENGINE: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccc";
const NEW_ENGINE: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddd";

fn old_context() -> GenerationContext {
    GenerationContext {
        curriculum_digest: OLD_CURRICULUM.to_string(),
        review_engine_digest: OLD_ENGINE.to_string(),
    }
}

fn new_context() -> GenerationContext {
    GenerationContext {
        curriculum_digest: NEW_CURRICULUM.to_string(),
        review_engine_digest: NEW_ENGINE.to_string(),
    }
}

fn make_instance(
    hash: &str,
    source: Source,
    content_digest: Option<&str>,
    context: Option<GenerationContext>,
) -> NewInstance {
    NewInstance {
        source,
        content_digest: content_digest.map(str::to_string),
        generation_context: context,
        problem: cadus_core::pool::PoolProblem {
            v: 1,
            text: format!("Compute ${hash}$."),
            bindings: Default::default(),
            seed: 0,
        },
        expected_answer: cadus_core::pool::PoolAnswer {
            answer_contract: None,
            v: 1,
            answer: "42".to_string(),
        },
        instance_hash: hash.to_string(),
    }
}

/// Read the four source-metadata fields of one pool row.
async fn read_metadata(
    admin: &sqlx::PgPool,
    user: uuid::Uuid,
    kp_id: &str,
    hash: &str,
) -> (String, Option<String>, Option<String>, Option<String>) {
    sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>)>(
        r#"
        SELECT source, content_digest, source_curriculum_digest,
               source_review_engine_digest
        FROM serving_pool
        WHERE user_id = $1 AND kp_id = $2 AND instance_hash = $3
        "#,
    )
    .bind(user)
    .bind(kp_id)
    .bind(hash)
    .fetch_one(admin)
    .await
    .unwrap()
}

/// Read the problem and answer of one pool row.
async fn read_body(
    admin: &sqlx::PgPool,
    user: uuid::Uuid,
    kp_id: &str,
    hash: &str,
) -> (serde_json::Value, serde_json::Value) {
    sqlx::query_as::<_, (serde_json::Value, serde_json::Value)>(
        r#"
        SELECT problem, expected_answer
        FROM serving_pool
        WHERE user_id = $1 AND kp_id = $2 AND instance_hash = $3
        "#,
    )
    .bind(user)
    .bind(kp_id)
    .bind(hash)
    .fetch_one(admin)
    .await
    .unwrap()
}

// ---------------------------------------------------------------------------
// Exact duplicate with nullable metadata reports 0
// ---------------------------------------------------------------------------

#[tokio::test]
async fn exact_nullable_duplicate_reports_zero() {
    TestDb::with(|db| async move {
        let user = db.seed_user("nullable-dup@example.test").await;
        seed_template(&db.admin, CONTENT_DIGEST, KP, "approved", "ok").await;

        let rows = vec![make_instance("h1", Source::Template, Some(CONTENT_DIGEST), None)];
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows).await.unwrap(), 1, "first insert counts 1");
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows).await.unwrap(), 0, "exact duplicate reports 0");
        assert_eq!(unclaimed_depth(&db.admin, user, KP).await.unwrap(), 1);
    })
    .await;
}

// ---------------------------------------------------------------------------
// Identical full-context duplicate reports 0
// ---------------------------------------------------------------------------

#[tokio::test]
async fn identical_full_context_duplicate_reports_zero() {
    TestDb::with(|db| async move {
        let user = db.seed_user("full-dup@example.test").await;
        seed_template(&db.admin, CONTENT_DIGEST, KP, "approved", "ok").await;

        let ctx = Some(old_context());
        let rows = vec![make_instance("h2", Source::Template, Some(CONTENT_DIGEST), ctx.clone())];
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows).await.unwrap(), 1);
        let rows2 = vec![make_instance("h2", Source::Template, Some(CONTENT_DIGEST), ctx)];
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows2).await.unwrap(), 0, "identical full-context duplicate reports 0");
        assert_eq!(unclaimed_depth(&db.admin, user, KP).await.unwrap(), 1);
    })
    .await;
}

// ---------------------------------------------------------------------------
// Curriculum digest refresh reports 1 and persists
// ---------------------------------------------------------------------------

#[tokio::test]
async fn curriculum_refresh_reports_one() {
    TestDb::with(|db| async move {
        let user = db.seed_user("refresh-cur@example.test").await;
        seed_template(&db.admin, CONTENT_DIGEST, KP, "approved", "ok").await;

        let rows = vec![make_instance("h3", Source::Template, Some(CONTENT_DIGEST), Some(old_context()))];
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows).await.unwrap(), 1);

        // Change only the curriculum digest; engine stays OLD_ENGINE.
        let changed = GenerationContext {
            curriculum_digest: NEW_CURRICULUM.to_string(),
            review_engine_digest: OLD_ENGINE.to_string(),
        };
        let rows = vec![make_instance("h3", Source::Template, Some(CONTENT_DIGEST), Some(changed))];
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows).await.unwrap(), 1, "curriculum refresh reports 1");
        assert_eq!(unclaimed_depth(&db.admin, user, KP).await.unwrap(), 1);

        let (_, _, cur, eng) = read_metadata(&db.admin, user, KP, "h3").await;
        assert_eq!(cur.as_deref(), Some(NEW_CURRICULUM), "curriculum persisted");
        assert_eq!(eng.as_deref(), Some(OLD_ENGINE), "engine unchanged");
    })
    .await;
}

// ---------------------------------------------------------------------------
// Engine digest refresh reports 1 and persists
// ---------------------------------------------------------------------------

#[tokio::test]
async fn engine_refresh_reports_one() {
    TestDb::with(|db| async move {
        let user = db.seed_user("refresh-eng@example.test").await;
        seed_template(&db.admin, CONTENT_DIGEST, KP, "approved", "ok").await;

        let rows = vec![make_instance("h4", Source::Template, Some(CONTENT_DIGEST), Some(old_context()))];
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows).await.unwrap(), 1);

        let changed = GenerationContext {
            curriculum_digest: OLD_CURRICULUM.to_string(),
            review_engine_digest: NEW_ENGINE.to_string(),
        };
        let rows = vec![make_instance("h4", Source::Template, Some(CONTENT_DIGEST), Some(changed))];
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows).await.unwrap(), 1, "engine refresh reports 1");
        assert_eq!(unclaimed_depth(&db.admin, user, KP).await.unwrap(), 1);

        let (_, _, cur, eng) = read_metadata(&db.admin, user, KP, "h4").await;
        assert_eq!(cur.as_deref(), Some(OLD_CURRICULUM), "curriculum unchanged");
        assert_eq!(eng.as_deref(), Some(NEW_ENGINE), "engine persisted");
    })
    .await;
}

// ---------------------------------------------------------------------------
// Source change refresh reports 1 and persists
// ---------------------------------------------------------------------------

#[tokio::test]
async fn source_change_refresh_reports_one() {
    TestDb::with(|db| async move {
        let user = db.seed_user("refresh-src@example.test").await;
        seed_template(&db.admin, CONTENT_DIGEST, KP, "approved", "ok").await;

        let rows = vec![make_instance("h5", Source::Template, Some(CONTENT_DIGEST), None)];
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows).await.unwrap(), 1);

        let rows = vec![make_instance("h5", Source::Exemplar, Some(CONTENT_DIGEST), None)];
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows).await.unwrap(), 1, "source change reports 1");
        assert_eq!(unclaimed_depth(&db.admin, user, KP).await.unwrap(), 1);

        let (src, _, _, _) = read_metadata(&db.admin, user, KP, "h5").await;
        assert_eq!(src, "exemplar", "source persisted");
    })
    .await;
}

// ---------------------------------------------------------------------------
// Content-digest refresh reports 1, persists, then identical replay 0
// ---------------------------------------------------------------------------

#[tokio::test]
async fn content_digest_refresh_reports_one_then_replay_zero() {
    TestDb::with(|db| async move {
        let user = db.seed_user("refresh-cd@example.test").await;
        seed_template(&db.admin, CONTENT_DIGEST, KP, "approved", "ok").await;
        seed_template(&db.admin, OTHER_DIGEST, KP, "approved", "ok").await;

        let rows = vec![make_instance("h9", Source::Template, Some(CONTENT_DIGEST), None)];
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows).await.unwrap(), 1);

        // Refresh with a different content_digest.
        let rows = vec![make_instance("h9", Source::Template, Some(OTHER_DIGEST), None)];
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows).await.unwrap(), 1, "content_digest refresh reports 1");
        assert_eq!(unclaimed_depth(&db.admin, user, KP).await.unwrap(), 1);

        let (_, cd, _, _) = read_metadata(&db.admin, user, KP, "h9").await;
        assert_eq!(cd.as_deref(), Some(OTHER_DIGEST), "new content_digest persisted");

        // Replay with the same new digest reports 0.
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows).await.unwrap(), 0, "identical content_digest replay reports 0");
        assert_eq!(unclaimed_depth(&db.admin, user, KP).await.unwrap(), 1);
    })
    .await;
}

// ---------------------------------------------------------------------------
// Claimed row is never updated; source metadata stays unchanged
// ---------------------------------------------------------------------------

#[tokio::test]
async fn claimed_row_is_not_overwritten() {
    TestDb::with(|db| async move {
        let user = db.seed_user("claimed-row@example.test").await;
        seed_template(&db.admin, CONTENT_DIGEST, KP, "approved", "ok").await;

        let rows = vec![make_instance("h6", Source::Template, Some(CONTENT_DIGEST), Some(old_context()))];
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows).await.unwrap(), 1);

        let claimed = common::claim_fresh(&db.app, user, KP).await;
        assert_eq!(claimed.row.instance_hash, "h6");

        // Snapshot the stored metadata before the attempted update.
        let (src_before, cd_before, cur_before, eng_before) = read_metadata(&db.admin, user, KP, "h6").await;
        let (prob_before, ans_before) = read_body(&db.admin, user, KP, "h6").await;

        let rows = vec![make_instance("h6", Source::Exemplar, Some(OTHER_DIGEST), Some(old_context()))];
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows).await.unwrap(), 0, "claimed row not updated");

        let (src_after, cd_after, cur_after, eng_after) = read_metadata(&db.admin, user, KP, "h6").await;
        let (prob_after, ans_after) = read_body(&db.admin, user, KP, "h6").await;
        assert_eq!((src_after, cd_after, cur_after, eng_after), (src_before, cd_before, cur_before, eng_before), "metadata unchanged after blocked update");
        assert_eq!((prob_after, ans_after), (prob_before, ans_before), "body unchanged after blocked update");
        assert_eq!(unclaimed_depth(&db.admin, user, KP).await.unwrap(), 0);
    })
    .await;
}

// ---------------------------------------------------------------------------
// Different problem blocks the update; stored problem stays unchanged
// ---------------------------------------------------------------------------

#[tokio::test]
async fn different_problem_is_not_overwritten() {
    TestDb::with(|db| async move {
        let user = db.seed_user("diff-prob@example.test").await;
        seed_template(&db.admin, CONTENT_DIGEST, KP, "approved", "ok").await;

        let rows = vec![make_instance("h7", Source::Template, Some(CONTENT_DIGEST), None)];
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows).await.unwrap(), 1);

        let (src_before, cd_before, cur_before, eng_before) = read_metadata(&db.admin, user, KP, "h7").await;
        let (prob_before, ans_before) = read_body(&db.admin, user, KP, "h7").await;

        let different = NewInstance {
            problem: cadus_core::pool::PoolProblem {
                v: 1,
                text: "Different.".to_string(),
                bindings: Default::default(),
                seed: 99,
            },
            ..make_instance("h7", Source::Template, Some(CONTENT_DIGEST), Some(new_context()))
        };
        let rows = vec![different];
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows).await.unwrap(), 0, "different problem blocks update");

        let (src_after, cd_after, cur_after, eng_after) = read_metadata(&db.admin, user, KP, "h7").await;
        let (prob_after, ans_after) = read_body(&db.admin, user, KP, "h7").await;
        assert_eq!((src_after, cd_after, cur_after, eng_after), (src_before, cd_before, cur_before, eng_before), "metadata unchanged after blocked update");
        assert_eq!((prob_after, ans_after), (prob_before, ans_before), "problem/answer unchanged after blocked update");
        assert_eq!(unclaimed_depth(&db.admin, user, KP).await.unwrap(), 1);
    })
    .await;
}

// ---------------------------------------------------------------------------
// Different answer blocks the update; stored answer stays unchanged
// ---------------------------------------------------------------------------

#[tokio::test]
async fn different_answer_is_not_overwritten() {
    TestDb::with(|db| async move {
        let user = db.seed_user("diff-ans@example.test").await;
        seed_template(&db.admin, CONTENT_DIGEST, KP, "approved", "ok").await;

        let rows = vec![make_instance("h8", Source::Template, Some(CONTENT_DIGEST), None)];
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows).await.unwrap(), 1);

        let (src_before, cd_before, cur_before, eng_before) = read_metadata(&db.admin, user, KP, "h8").await;
        let (prob_before, ans_before) = read_body(&db.admin, user, KP, "h8").await;

        let different = NewInstance {
            expected_answer: cadus_core::pool::PoolAnswer {
                answer_contract: None,
                v: 1,
                answer: "99".to_string(),
            },
            ..make_instance("h8", Source::Template, Some(CONTENT_DIGEST), Some(new_context()))
        };
        let rows = vec![different];
        assert_eq!(insert_batch_for_user(&db.admin, user, KP, &rows).await.unwrap(), 0, "different answer blocks update");

        let (src_after, cd_after, cur_after, eng_after) = read_metadata(&db.admin, user, KP, "h8").await;
        let (prob_after, ans_after) = read_body(&db.admin, user, KP, "h8").await;
        assert_eq!((src_after, cd_after, cur_after, eng_after), (src_before, cd_before, cur_before, eng_before), "metadata unchanged after blocked update");
        assert_eq!((prob_after, ans_after), (prob_before, ans_before), "problem/answer unchanged after blocked update");
        assert_eq!(unclaimed_depth(&db.admin, user, KP).await.unwrap(), 1);
    })
    .await;
}