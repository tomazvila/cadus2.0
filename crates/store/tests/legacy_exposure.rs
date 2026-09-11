//! Derived legacy exposure preserves payloads and independently gates fresh credit.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use cadus_core::{curriculum::FiniteObjectiveDomain, learner::problem_text_hash};
use cadus_store::{Db, begin_tenant, content::Admin, state::*, test_support::TestDb};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

fn domain(text: &str) -> FiniteObjectiveDomain {
    serde_json::from_value(json!({"schema_version":1,"review_ref":"ai:finite-test",
        "cases":[{"id":"shift-2","role":"practice_fresh","variants":[
            {"problem":text,"answer":"y=2"}]},
            {"id":"other-case","role":"practice_fresh","variants":[
                {"problem":"Find the other line.","answer":"y=2"}]}]}))
    .unwrap()
}
async fn raw(db: &TestDb, user: Uuid, seq: i64, body: Value) {
    sqlx::query("INSERT INTO events (user_id, seq, ts, type, payload) VALUES ($1,$2,now(),$3,$4)")
        .bind(user)
        .bind(seq)
        .bind(body["type"].as_str().unwrap())
        .bind(body)
        .execute(&db.admin)
        .await
        .unwrap();
}
fn old_handoff(text: &str) -> Value {
    json!({"type":"ordinary_problem_served","kp_id":"midline/kp1",
           "item_digest":problem_text_hash(text)})
}
fn old_attempt(text: &str) -> Value {
    json!({"type":"attempt","topic":"midline","kp":"kp1",
           "problem":{"text":text,"expected":"2"},"correct":true,"exposure":"first"})
}
async fn seen(db: &TestDb, user: Uuid, case: &str) -> bool {
    let mut tx = begin_tenant(&db.app, user).await.unwrap();
    handoff_seen(
        &mut tx,
        user,
        HandoffIdentity::Finite {
            kp_id: "midline/kp1",
            case_id: case,
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn retained_aliases_bridge_prepolicy_abandonment_without_cross_tenant_or_equal_answer_leaks()
{
    TestDb::with(|db| async move {
        let user = db.seed_user("old-handoff@example.test").await;
        let other = db.seed_user("other-handoff@example.test").await;
        let admin_db = Db::new(db.admin.clone(), 0);
        let admin = Admin::new(&admin_db);
        let old = "State the numeric midline height: 2.";
        raw(&db, user, 1, old_handoff(old)).await;
        let d = domain("State the midline in the form y=value.");
        let retained = [ExposureAlias {
            case_id: "shift-2".into(),
            problem_text: old.into(),
            source_kind: AliasKind::ExposureOnly,
            source_policy_digest: Some("retired-policy".into()),
        }];
        let context =
            publish_finite_exposure(admin, "midline/kp1", &d, &retained, "ai:old-mapping", None)
                .await
                .unwrap();
        assert!(seen(&db, user, "shift-2").await);
        assert!(
            !seen(&db, user, "other-case").await,
            "equal numeric answers do not identify a case"
        );
        assert!(!seen(&db, other, "shift-2").await);
        assert!(
            d.case_for(old, "2", None).is_none(),
            "exposure-only text never becomes serving membership"
        );
        let changed = domain("Write the horizontal line equation y=2.");
        let next = publish_finite_exposure(
            admin,
            "midline/kp1",
            &changed,
            &[],
            "ai:render-revision",
            Some(&context),
        )
        .await
        .unwrap();
        assert_ne!(context, next);
        assert!(
            seen(&db, user, "shift-2").await,
            "retiring an active rendering preserves old aliases"
        );
        assert!(
            publish_finite_exposure(
                admin,
                "midline/kp1",
                &changed,
                &[],
                "ai:stale",
                Some(&context)
            )
            .await
            .is_err()
        );
    })
    .await;
}

#[tokio::test]
async fn v1_attempt_backfill_uses_exact_text_preserves_payload_and_records_discrepancies() {
    TestDb::with(|db| async move {
        let user = db.seed_user("legacy-attempt@example.test").await;
        let admin_db = Db::new(db.admin.clone(), 0);
        let admin = Admin::new(&admin_db);
        let text = "  Compute $√9$.  ";
        let mut body = old_attempt(text);
        body["item_digest"] = json!("wrong-original");
        raw(&db, user, 1, body.clone()).await;
        let page = backfill_attempt_digests(admin, user, false).await.unwrap();
        assert_eq!(page.filled, 1);
        assert_eq!(page.supplied_digest_discrepancies, vec![1]);
        assert!(page.finished);
        assert_eq!(page.total_unresolved, 0);
        let after: Value =
            sqlx::query_scalar("SELECT payload FROM events WHERE user_id=$1 AND seq=1")
                .bind(user)
                .fetch_one(&db.admin)
                .await
                .unwrap();
        assert_eq!(body, after);
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        assert!(
            handoff_seen(
                &mut tx,
                user,
                HandoffIdentity::Digest(&problem_text_hash(text))
            )
            .await
            .unwrap()
        );
        assert!(
            !handoff_seen(
                &mut tx,
                user,
                HandoffIdentity::Digest(&problem_text_hash(text.trim()))
            )
            .await
            .unwrap()
        );
        assert!(handoff_history_ready(&mut tx, user, None).await.unwrap());
        tx.commit().await.unwrap();
        let again = backfill_attempt_digests(admin, user, false).await.unwrap();
        assert_eq!(again.inspected, 0);
        assert_eq!(again.filled, 0);
        raw(
            &db,
            user,
            2,
            json!({"type":"task_served","problem":{"text_hash":"inert"}}),
        )
        .await;
        let replay = backfill_attempt_digests(admin, user, true).await.unwrap();
        assert_eq!(replay.already_correct, 1);
        assert_eq!(replay.filled, 0);
    })
    .await;
}

#[tokio::test]
async fn page_resume_and_new_writer_keep_a_fixed_snapshot_without_payload_rewrites() {
    TestDb::with(|db| async move {
        let user = db.seed_user("backfill-page@example.test").await;
        let admin_db = Db::new(db.admin.clone(), 0);
        let admin = Admin::new(&admin_db);
        sqlx::query(
            "INSERT INTO events (user_id,seq,ts,type,payload)
            SELECT $1,s,now(),'attempt',jsonb_build_object('type','attempt','problem',
            jsonb_build_object('text','old item ' || s)) FROM generate_series(1,503) s",
        )
        .bind(user)
        .execute(&db.admin)
        .await
        .unwrap();
        let first = backfill_attempt_digests(admin, user, false).await.unwrap();
        assert_eq!(first.inspected, 500);
        assert_eq!(first.through_seq, 500);
        assert!(!first.finished);
        let event = common::events::attempt("new-writer");
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        lock_web_state(&mut tx, user).await.unwrap();
        assert_eq!(
            append_event(&mut tx, user, &event, Some("new-writer"))
                .await
                .unwrap(),
            Some(504)
        );
        tx.commit().await.unwrap();
        let last = backfill_attempt_digests(admin, user, false).await.unwrap();
        assert_eq!(last.target_seq, 503);
        assert_eq!(last.inspected, 3);
        assert!(last.finished);
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM events WHERE user_id=$1 AND attempt_problem_digest IS NOT NULL",
        )
        .bind(user)
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(count, 504);
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        assert!(handoff_history_ready(&mut tx, user, None).await.unwrap());
    })
    .await;
}

#[tokio::test]
async fn malformed_or_conflicting_legacy_metadata_remains_unresolved() {
    TestDb::with(|db| async move {
        let user = db.seed_user("malformed-history@example.test").await;
        let admin_db = Db::new(db.admin.clone(), 0);
        let admin = Admin::new(&admin_db);
        raw(
            &db,
            user,
            1,
            json!({"type":"attempt","problem":{"text":23}}),
        )
        .await;
        raw(&db, user, 2, old_attempt("valid text")).await;
        sqlx::query(
            "UPDATE events SET attempt_problem_digest='aaaaaaaaaaaa' WHERE user_id=$1 AND seq=2",
        )
        .bind(user)
        .execute(&db.admin)
        .await
        .unwrap();
        let page = backfill_attempt_digests(admin, user, false).await.unwrap();
        assert_eq!(page.unresolved, vec![1, 2]);
        assert_eq!(page.total_unresolved, 2);
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        assert!(!handoff_history_ready(&mut tx, user, None).await.unwrap());
    })
    .await;
}

#[tokio::test]
async fn finite_reconciliation_requires_exact_ai_evidence_and_current_context() {
    TestDb::with(|db| async move {
        let user = db.seed_user("reconcile-history@example.test").await;
        let admin_db = Db::new(db.admin.clone(), 0);
        let admin = Admin::new(&admin_db);
        raw(
            &db,
            user,
            1,
            old_attempt("A historical out-of-scope question."),
        )
        .await;
        backfill_attempt_digests(admin, user, false).await.unwrap();
        let d = domain("State y=2.");
        let policy = d.fingerprint("midline/kp1").unwrap();
        let context = publish_finite_exposure(admin, "midline/kp1", &d, &[], "ai:partition", None)
            .await
            .unwrap();
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        assert!(
            !handoff_history_ready(&mut tx, user, Some(("midline/kp1", &policy)))
                .await
                .unwrap()
        );
        tx.commit().await.unwrap();
        let empty = BTreeMap::new();
        let request = |commit, decisions| ReconciliationRequest {
            user_id: user,
            kp_id: "midline/kp1",
            context_digest: &context,
            reviewed_irrelevant: decisions,
            review_ref: "ai:semantic-review",
            commit,
            restart: false,
        };
        let preview = reconcile_exposure_page(admin, request(false, &empty))
            .await
            .unwrap();
        assert_eq!(preview.items.len(), 1);
        assert_eq!(preview.unresolved_count, 1);
        assert!(!preview.committed);
        let mut stale = BTreeMap::new();
        stale.insert(1, "stale-fingerprint".into());
        assert!(
            reconcile_exposure_page(admin, request(true, &stale))
                .await
                .is_err()
        );
        let mut accepted = BTreeMap::new();
        accepted.insert(1, preview.items[0].fingerprint.clone());
        assert!(
            reconcile_exposure_page(admin, request(true, &accepted))
                .await
                .unwrap()
                .complete
        );
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        assert!(
            handoff_history_ready(&mut tx, user, Some(("midline/kp1", &policy)))
                .await
                .unwrap()
        );
        tx.commit().await.unwrap();
        publish_finite_exposure(
            admin,
            "midline/kp1",
            &d,
            &[],
            "ai:changed-context",
            Some(&context),
        )
        .await
        .unwrap();
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        assert!(
            !handoff_history_ready(&mut tx, user, Some(("midline/kp1", &policy)))
                .await
                .unwrap()
        );
    })
    .await;
}

#[tokio::test]
async fn empty_users_initialize_and_raw_legacy_imports_invalidate_fresh_credit() {
    TestDb::with(|db| async move {
        let user = db.seed_user("empty-history@example.test").await;
        let other = db.seed_user("isolated-history@example.test").await;
        let admin_db = Db::new(db.admin.clone(), 0);
        let admin = Admin::new(&admin_db);
        let d = domain("State y=2.");
        let policy = d.fingerprint("midline/kp1").unwrap();
        publish_finite_exposure(admin, "midline/kp1", &d, &[], "ai:partition", None)
            .await
            .unwrap();
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        lock_web_state(&mut tx, user).await.unwrap();
        assert!(
            handoff_history_ready(&mut tx, user, Some(("midline/kp1", &policy)))
                .await
                .unwrap()
        );
        let cross: i64 =
            sqlx::query_scalar("SELECT count(*) FROM exposure_history_progress WHERE user_id=$1")
                .bind(other)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(cross, 0);
        tx.commit().await.unwrap();
        raw(&db, user, 1, old_handoff("unmapped imported old text")).await;
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        assert!(
            !handoff_history_ready(&mut tx, user, Some(("midline/kp1", &policy)))
                .await
                .unwrap()
        );
        tx.commit().await.unwrap();
        raw(&db, user, 2, old_attempt("late unindexed attempt")).await;
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        assert!(!handoff_history_ready(&mut tx, user, None).await.unwrap());
    })
    .await;
}
