//! M6 R3 acceptance: the T3 accounting of the authoring pipeline (T3, T6).
//!
//! The section 7 row of the spec names three checks, and each one is a test
//! here:
//!
//! 1. a 4-attempt knowledge point raises the alert and the row's
//!    `authoring_attempts` is `4`;
//! 2. a reply with no `usage` block writes a zeros row with a NULL cost;
//! 3. the cost on the row equals the sum of that knowledge point's call rows.
//!
//! The rest of the file holds the checks the accounting needs beside those
//! three: the per-term rounding that makes check 3 an equality, a total the
//! money column cannot hold, a pass whose calls report no price, the alert of a
//! declined knowledge point, and the operator list.
//!
//! Every expected value is a LITERAL: a literal money text, a literal row count,
//! a literal attempt count. Nothing is read back from the code under test.
//!
//! The endpoint is a fake OpenAI-compatible server (`common::FakeModel`). No
//! test reaches a real provider.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;
use cadus_worker::authoring::cost::{ATTEMPT_ALERT, alerting};
use cadus_worker::authoring::job::{Outcome, store_pending};
use cadus_worker::authoring::prompt::Kind;
use serde_json::{Value, json};
use sqlx::PgPool;

use common::{
    FakeModel, SQUARES_KEY as KP_KEY, STORED_DIGEST, author_expect, closed_handle, good_arguments,
    handle, ledger_shape, missing_low_edge, reply, squares_spec as spec,
};

/// The three prices of the three-attempt pass, in the order the calls run.
const PRICES: [f64; 3] = [0.001234, 0.0002, 0.5];

/// The sum of [`PRICES`], as `numeric(12,6)` writes it.
///
/// Worked by hand: 0.001234 + 0.000200 + 0.500000 = 0.501434.
const PRICE_SUM: &str = "0.501434";

/// A reply that carries an `emit_template` call and this `usage` block.
fn reply_with_usage(arguments: &Value, usage: Option<Value>) -> (u16, String) {
    reply("emit_template", &arguments.to_string(), usage)
}

/// A `usage` block that reports these tokens and this price.
fn usage(cost: f64) -> Value {
    json!({"prompt_tokens": 900, "completion_tokens": 300, "cost": cost})
}

/// A refusal of the low edge, priced at one thousandth.
fn priced_refusal() -> (u16, String) {
    reply_with_usage(&missing_low_edge(), Some(usage(0.001)))
}

/// `refusals` priced refusals, then the accepted template priced the same,
/// authored: the endpoint and the report.
async fn refusals_then_store(
    db: &TestDb,
    refusals: usize,
) -> (FakeModel, cadus_worker::AuthoringReport) {
    let mut replies: Vec<(u16, String)> = (0..refusals).map(|_| priced_refusal()).collect();
    replies.push(reply_with_usage(&good_arguments(), Some(usage(0.001))));
    let fake = FakeModel::start(replies).await;
    let spent = u32::try_from(refusals).unwrap() + 1;
    let report = author_expect(db, &fake, Kind::Template, &spec(), Outcome::Stored, spent).await;
    (fake, report)
}

/// The `(authoring_attempts, authoring_cost_usd)` of the one row of a knowledge
/// point.
async fn accounting(pool: &PgPool, kp_id: &str) -> (i32, Option<String>) {
    sqlx::query_as::<_, (i32, Option<String>)>(
        "SELECT authoring_attempts, authoring_cost_usd::text FROM content_store
          WHERE kp_id = $1",
    )
    .bind(kp_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The count of `content_store` rows of a knowledge point.
async fn row_count(pool: &PgPool, kp_id: &str) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM content_store WHERE kp_id = $1")
        .bind(kp_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The sum of the `cost_usd` of every authoring call row, as its exact text.
async fn ledger_sum(pool: &PgPool) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT sum(cost_usd)::text FROM model_call_log WHERE purpose = 'authoring'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The count of authoring call rows, and how many of them name a tenant.
async fn authoring_ledger(pool: &PgPool) -> (i64, i64) {
    ledger_shape(pool, "authoring").await
}

/// Seed one `content_store` row with this attempt count and price.
async fn seed_row(pool: &PgPool, digest: &str, kp_id: &str, attempts: i32, cost: Option<&str>) {
    sqlx::query(
        "INSERT INTO content_store
             (digest, kp_id, kind, body, status, authoring_attempts, authoring_cost_usd)
         VALUES ($1, $2, 'template', '{}'::jsonb, 'pending', $3, $4::text::numeric)",
    )
    .bind(digest)
    .bind(kp_id)
    .bind(attempts)
    .bind(cost)
    .execute(pool)
    .await
    .unwrap();
}

/// Store the empty body under the knowledge point with this attempt count and
/// this spend, through the loop's own write.
async fn store(db: &TestDb, attempts: u32, spend: &[&str]) -> cadus_worker::AuthoringStored {
    let spend: Vec<String> = spend.iter().map(|text| (*text).to_owned()).collect();
    store_pending(&handle(db), KP_KEY, Kind::Template, "{}", attempts, &spend)
        .await
        .unwrap()
}

// --------------------------------------------------------------------------- //
// 1. The four-attempt alert
// --------------------------------------------------------------------------- //

/// The acceptance literal: a 4-attempt knowledge point raises the alert and the
/// row's `authoring_attempts` is 4.
///
/// Three refusals and one accepted document make four model calls. T3 alerts
/// above three (`REQUIREMENTS.md`, T3), so this pass alerts, the row says `4`,
/// and the operator list names it.
#[tokio::test]
async fn a_four_attempt_knowledge_point_alerts_and_the_row_says_four() {
    TestDb::with(|db| async move {
        let (fake, report) = refusals_then_store(&db, 3).await;

        assert!(report.alert, "four attempts must raise the T3 alert");
        assert_eq!(fake.call_count(), 4);

        // The stored row carries the count, and the bill of four calls.
        let (attempts, cost) = accounting(&db.admin, KP_KEY).await;
        assert_eq!(attempts, 4);
        assert_eq!(cost.as_deref(), Some("0.004000"));

        // The operator list names the knowledge point, once.
        let alerts = alerting(&handle(&db)).await.unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kp_id, KP_KEY);
        assert_eq!(alerts[0].kind, "template");
        assert_eq!(alerts[0].digest, STORED_DIGEST);
        assert_eq!(alerts[0].attempts, 4);
        assert_eq!(alerts[0].cost_usd.as_deref(), Some("0.004000"));

        // T6: one ledger row per HTTP attempt, and no tenant on an offline call.
        assert_eq!(authoring_ledger(&db.admin).await, (4, 0));
    })
    .await;
}

/// Three attempts are inside the bound: the row says `3` and nothing alerts.
///
/// The bound is "above 3", so this is the pass that proves the alert reads `>`
/// and not `>=`.
#[tokio::test]
async fn a_three_attempt_knowledge_point_stays_quiet() {
    TestDb::with(|db| async move {
        let (_fake, report) = refusals_then_store(&db, 2).await;

        assert_eq!(ATTEMPT_ALERT, 3);
        assert!(!report.alert, "three attempts are inside the T3 bound");
        assert_eq!(accounting(&db.admin, KP_KEY).await.0, 3);
        assert!(alerting(&handle(&db)).await.unwrap().is_empty());
    })
    .await;
}

/// The operator list is a store read, so a closed pool fails it.
#[tokio::test]
async fn the_alert_list_fails_on_a_closed_pool() {
    TestDb::with(|db| async move {
        let err = alerting(&closed_handle(&db).await)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("store error: "), "{err}");
    })
    .await;
}

/// A declined knowledge point alerts too, and stores no row.
///
/// Five refusals spend five calls. The row that would carry the bill never
/// exists, so the alert and the ledger are the only places that spend is named.
#[tokio::test]
async fn a_declined_knowledge_point_alerts_and_stores_no_row() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start((0..5).map(|_| priced_refusal()).collect()).await;

        let report = author_expect(&db, &fake, Kind::Template, &spec(), Outcome::Declined, 5).await;

        assert!(report.alert);
        assert_eq!(report.cost_usd, None);
        assert_eq!(row_count(&db.admin, KP_KEY).await, 0);
        assert_eq!(authoring_ledger(&db.admin).await, (5, 0));
        assert_eq!(ledger_sum(&db.admin).await.as_deref(), Some("0.005000"));
        assert!(alerting(&handle(&db)).await.unwrap().is_empty());
    })
    .await;
}

// --------------------------------------------------------------------------- //
// 2. The reply with no usage block
// --------------------------------------------------------------------------- //

/// The acceptance literal: a reply with no `usage` block writes a zeros row with
/// a NULL cost.
///
/// The call happened and the operator paid for it, so the row exists. Every
/// token count the reply did not report is 0, and the price it did not report is
/// NULL: an unknown price is never a guess (`crate::model_log`). The stored
/// document then carries a NULL cost and an exact attempt count.
#[tokio::test]
async fn a_reply_with_no_usage_block_writes_a_zeros_row_with_a_null_cost() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![reply_with_usage(&good_arguments(), None)]).await;

        let report = author_expect(&db, &fake, Kind::Template, &spec(), Outcome::Stored, 1).await;
        assert_eq!(report.cost_usd, None);

        let row =
            sqlx::query_as::<_, (String, i32, i32, i32, i32, Option<String>, Option<String>)>(
                "SELECT purpose, input_tokens_cached, input_tokens_uncached, output_tokens,
                    reasoning_tokens, cost_usd::text, model_id
               FROM model_call_log",
            )
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(row.0, "authoring");
        assert_eq!(row.1, 0);
        assert_eq!(row.2, 0);
        assert_eq!(row.3, 0);
        assert_eq!(row.4, 0);
        assert_eq!(row.5, None);
        assert_eq!(row.6.as_deref(), Some("qwen3.6"));
        assert_eq!(authoring_ledger(&db.admin).await, (1, 0));

        // The document is stored, with no money on it and an exact count.
        assert_eq!(accounting(&db.admin, KP_KEY).await, (1, None));
    })
    .await;
}

// --------------------------------------------------------------------------- //
// 3. The cost on the row is the sum of the call rows
// --------------------------------------------------------------------------- //

/// The acceptance literal: the cost on the row equals the sum of that knowledge
/// point's call rows.
///
/// Three calls at 0.001234, 0.0002 and 0.5 cost 0.501434 together. The test
/// asserts that literal on the stored row AND against the sum the ledger holds,
/// so a change to either side breaks it.
#[tokio::test]
async fn the_cost_on_the_row_is_the_sum_of_that_knowledge_points_call_rows() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![
            reply_with_usage(&missing_low_edge(), Some(usage(PRICES[0]))),
            reply_with_usage(&missing_low_edge(), Some(usage(PRICES[1]))),
            reply_with_usage(&good_arguments(), Some(usage(PRICES[2]))),
        ])
        .await;

        let report = author_expect(&db, &fake, Kind::Template, &spec(), Outcome::Stored, 3).await;

        assert_eq!(report.cost_usd.as_deref(), Some(PRICE_SUM));

        let (attempts, cost) = accounting(&db.admin, KP_KEY).await;
        assert_eq!(attempts, 3);
        assert_eq!(cost.as_deref(), Some("0.501434"));

        // The same number, read from the ledger rows of those three calls.
        assert_eq!(authoring_ledger(&db.admin).await, (3, 0));
        assert_eq!(ledger_sum(&db.admin).await.as_deref(), Some("0.501434"));
        assert_eq!(cost, ledger_sum(&db.admin).await);
    })
    .await;
}

/// The sum rounds every term to six places first, exactly as each ledger row
/// does, so the two sides agree on a price under the scale of the column.
///
/// Two calls at 0.0000005 each store 0.000001 apiece, and the stored total is
/// 0.000002. A sum of the RAW texts would be 0.000001, which no ledger row
/// holds.
#[tokio::test]
async fn the_sum_rounds_every_term_the_way_a_ledger_row_does() {
    TestDb::with(|db| async move {
        let stored = store(&db, 2, &["0.0000005", "0.0000005"]).await;

        assert!(stored.inserted);
        assert_eq!(stored.cost_usd.as_deref(), Some("0.000002"));
        assert_eq!(
            accounting(&db.admin, KP_KEY).await,
            (2, Some("0.000002".to_owned()))
        );
    })
    .await;
}

/// A pass whose calls report no price stores a NULL cost and an exact count.
///
/// Zero is a measurement and NULL is the absence of one, so an unpriced pass is
/// never stored as free.
#[tokio::test]
async fn a_pass_with_no_price_stores_a_null_cost_and_an_exact_count() {
    TestDb::with(|db| async move {
        let stored = store(&db, 2, &[]).await;

        assert!(stored.inserted);
        assert_eq!(stored.cost_usd, None);
        assert_eq!(accounting(&db.admin, KP_KEY).await, (2, None));
    })
    .await;
}

/// A total the money column cannot hold is NULL, and the document still stores.
///
/// `numeric(12,6)` keeps six digits before the point. An overflow raises inside
/// the statement and would lose the document, so the sum guards itself and the
/// row keeps the attempt count.
#[tokio::test]
async fn a_total_the_money_column_cannot_hold_is_null() {
    TestDb::with(|db| async move {
        let stored = store(&db, 1, &["999999", "999999"]).await;

        assert!(stored.inserted);
        assert_eq!(stored.cost_usd, None);
        assert_eq!(accounting(&db.admin, KP_KEY).await, (1, None));
    })
    .await;
}

/// The operator list holds the documents above the bound only, dearest first.
#[tokio::test]
async fn alerting_lists_the_documents_above_the_bound_dearest_first() {
    TestDb::with(|db| async move {
        seed_row(&db.admin, "sha256:aaaa", "t/one", 1, Some("0.1")).await;
        seed_row(&db.admin, "sha256:bbbb", "t/two", 3, Some("0.2")).await;
        seed_row(&db.admin, "sha256:cccc", "t/three", 4, Some("0.3")).await;
        seed_row(&db.admin, "sha256:dddd", "t/four", 5, None).await;

        let alerts = alerting(&handle(&db)).await.unwrap();

        assert_eq!(alerts.len(), 2);
        assert_eq!(alerts[0].kp_id, "t/four");
        assert_eq!(alerts[0].attempts, 5);
        assert_eq!(alerts[0].cost_usd, None);
        assert_eq!(alerts[1].kp_id, "t/three");
        assert_eq!(alerts[1].attempts, 4);
        assert_eq!(alerts[1].cost_usd.as_deref(), Some("0.300000"));
    })
    .await;
}

/// A second pass that authors the same body leaves the first bill alone.
///
/// The digest is the primary key and the approval binds to it (C6), so the
/// second pass inserts nothing. Its own calls are in the ledger; the row keeps
/// the accounting of the pass that wrote it.
#[tokio::test]
async fn a_duplicate_body_leaves_the_first_bill_alone() {
    TestDb::with(|db| async move {
        let first = store(&db, 1, &["0.25"]).await;
        assert!(first.inserted);

        let second = store(&db, 4, &["0.75"]).await;

        assert!(!second.inserted);
        assert_eq!(second.cost_usd, None);
        assert_eq!(
            accounting(&db.admin, KP_KEY).await,
            (1, Some("0.250000".to_owned()))
        );
    })
    .await;
}

/// A body that does not read as JSON is a configuration error, and a spend text
/// the money cast cannot read is the error of the statement that sums it.
///
/// Neither one reaches the table: the pass hands the loop a verified body and a
/// spend of decimal texts, so both errors name a defect of the caller.
#[tokio::test]
async fn an_unreadable_body_and_an_unreadable_spend_are_errors() {
    TestDb::with(|db| async move {
        let unreadable = store_pending(&handle(&db), KP_KEY, Kind::Template, "{", 1, &[])
            .await
            .unwrap_err()
            .to_string();
        assert!(
            unreadable.starts_with("configuration error: the verified body does not read: "),
            "{unreadable}"
        );

        let unpriced = store_pending(
            &handle(&db),
            KP_KEY,
            Kind::Template,
            "{}",
            1,
            &["free".to_owned()],
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(unpriced.starts_with("store error: "), "{unpriced}");
        assert_eq!(row_count(&db.admin, KP_KEY).await, 0);
    })
    .await;
}
