//! Part of `tests/admin_content.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::admin::*;

use common::Answer;

// --------------------------------------------------------------------------- //
// The queue
// --------------------------------------------------------------------------- //

/// ACCEPTANCE. The list flags a knowledge point with fewer than three approved
/// templates.
///
/// `band/kp1` holds two approved templates, so its queue line carries
/// `approved_templates: 2` and `bank_warning: true`. `band/kp2` holds three, so
/// its line carries `bank_warning: false`. The reason is 1.0's
/// `cmd_list`: a knowledge point serves from its approved slots alone.
#[tokio::test]
async fn the_list_flags_a_knowledge_point_below_the_bank_target() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;
        seed_approved(&db, KEY, "r5-kp1-ok", 2).await;
        seed_row(&db, &Seed::template("r5-kp2-pending", OTHER_KEY, "pending")).await;
        seed_approved(&db, OTHER_KEY, "r5-kp2-ok", 3).await;

        let answer = admin_get(&app, "/api/admin/content?status=pending").await;

        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_eq!(answer.body.get("bank_target"), Some(&json!(3)));

        let thin = item_of(&answer.body, PENDING);
        assert_eq!(thin.get("kp_id"), Some(&json!("band/kp1")));
        assert_eq!(thin.get("approved_templates"), Some(&json!(2)));
        assert_eq!(thin.get("bank_warning"), Some(&json!(true)));

        let full = item_of(&answer.body, "r5-kp2-pending");
        assert_eq!(full.get("kp_id"), Some(&json!("band/kp2")));
        assert_eq!(full.get("approved_templates"), Some(&json!(3)));
        assert_eq!(full.get("bank_warning"), Some(&json!(false)));
    })
    .await;
}

/// A knowledge point with no approved template is flagged too.
///
/// 1.0 prints the note only when the count is above zero. 2.0 flags a bank of
/// zero as well: spec section 3.2 says "fewer than 3 approved templates", and no
/// approved template is the worst case of the same fault.
#[tokio::test]
async fn an_empty_bank_is_flagged() {
    TestDb::with(|db| async move {
        let answer = list_after_pending(&db).await;

        let line = item_of(&answer.body, PENDING);
        assert_eq!(line.get("approved_templates"), Some(&json!(0)));
        assert_eq!(line.get("bank_warning"), Some(&json!(true)));
    })
    .await;
}

/// One queue line carries the T3 numbers and the first 64 characters of the
/// statement.
#[tokio::test]
async fn a_queue_line_carries_the_summary_and_the_authoring_bill() {
    TestDb::with(|db| async move {
        let answer = list_after_pending(&db).await;

        let line = item_of(&answer.body, PENDING);
        assert_eq!(line.get("kind"), Some(&json!("template")));
        assert_eq!(line.get("status"), Some(&json!("pending")));
        assert_eq!(line.get("authoring_attempts"), Some(&json!(4)));
        assert_eq!(line.get("authoring_cost_usd"), Some(&json!("0.012500")));
        assert_eq!(line.get("summary"), Some(&json!("Compute ${a} - {b}$.")));
        assert_eq!(answer.body.get("limit"), Some(&json!(200)));
    })
    .await;
}

/// The three query parameters select the queue.
#[tokio::test]
async fn the_queue_filters_by_status_kind_and_serving_key() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_pending(&db).await;
        seed_approved(&db, OTHER_KEY, "r5-other", 1).await;

        let pending = admin_get(&app, "/api/admin/content?status=pending").await;
        assert_eq!(
            pending
                .body
                .get("items")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            item_of(&pending.body, PENDING).get("status"),
            Some(&json!("pending"))
        );

        let by_kp = admin_get(&app, "/api/admin/content?kp=band/kp2").await;
        assert_eq!(
            by_kp
                .body
                .get("items")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            item_of(&by_kp.body, "r5-other0").get("kp_id"),
            Some(&json!("band/kp2"))
        );

        let by_kind = admin_get(&app, "/api/admin/content?kind=teach").await;
        assert_eq!(by_kind.body.get("items"), Some(&json!([])));
    })
    .await;
}

/// `page` reads the queue past the rows one read answers (T3).
///
/// The bill of the operator screen prices EVERY stored document, and one read
/// answers at most 200 of them. 201 rows are therefore two pages: a full one,
/// then a page of one row. The pages are cut from one order, so between them
/// they name each of the 201 digests once.
#[tokio::test]
async fn the_page_parameter_reads_the_queue_past_one_page() {
    TestDb::with(|db| async move {
        let app = app(&db);
        seed_admin(&db).await;
        seed_approved(&db, KEY, "r5-page", 201).await;

        let digests = |answer: &Answer| -> Vec<String> {
            answer
                .body
                .get("items")
                .and_then(Value::as_array)
                .expect("the answer carries no items array")
                .iter()
                .map(|item| {
                    item.get("digest")
                        .and_then(Value::as_str)
                        .expect("a queue line carries no digest")
                        .to_string()
                })
                .collect()
        };

        let first = admin_get(&app, LIST_PATH).await;
        let second = admin_get(&app, "/api/admin/content?page=1").await;
        assert_eq!(first.status.as_u16(), 200, "{}", first.body);
        assert_eq!(second.status.as_u16(), 200, "{}", second.body);

        let mut all = digests(&first);
        assert_eq!(all.len(), 200);
        assert_eq!(digests(&second).len(), 1);
        all.extend(digests(&second));
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 201, "the two pages name one digest twice");

        // An absent page and page 0 are the same page.
        let zero = admin_get(&app, "/api/admin/content?page=0").await;
        assert_eq!(zero.body.get("items"), first.body.get("items"));

        // A page past the last one answers no row, and it is not a failure.
        let past = admin_get(&app, "/api/admin/content?page=2").await;
        assert_eq!(past.status.as_u16(), 200, "{}", past.body);
        assert_eq!(past.body.get("items"), Some(&json!([])));

        // A page that is not a page is refused. It does NOT read as page 0: a
        // bill added up from the first page of a request for a later page is
        // wrong with nothing on screen to say so (A6).
        let refused = admin_get(&app, "/api/admin/content?page=two").await;
        assert_eq!(refused.status.as_u16(), 422, "{}", refused.body);
        assert_eq!(refused.code(), "invalid_request");
    })
    .await;
}
