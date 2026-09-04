//! M6 review 2, finding V3: a re-author that reproduces the body of a STALE row
//! stamps the current prompt on that row (C6, spec section 2.2, "Prompt digest").
//!
//! The row then leaves the stale set and the pass after it makes no model call.
//! Every expected value is a LITERAL. The endpoint is a fake OpenAI-compatible
//! server (`common::FakeModel`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;
use cadus_worker::authoring::job::{Outcome, render_stale, slots_taken, stale_rows, stale_slots};
use cadus_worker::authoring::prompt::{KINDS, Kind, prompt_digest};
use serde_json::Value;

use common::{
    FakeModel, OLD_PROMPT, SQUARES_KEY as KP_KEY, STORED_DIGEST, STORED_TEACH_BODY,
    STORED_TEACH_DIGEST, Seed, assert_batch, assert_one_row, author_within_expect, batch,
    content_rows_of_kind, good_arguments, handle, named_reply, squares_spec as spec,
    teach_arguments, tool_reply,
};

/// The `--stale` listing of an empty stale set, character for character
/// (`render_stale`).
const NO_STALE_ROWS: &str = "stale documents\nkp_id kind digest prompt_digest\nstale: rows 0\n";

/// A reply that carries the teach page the gate accepts.
fn teach_reply() -> (u16, String) {
    named_reply("emit_teach", &teach_arguments())
}

/// Seed the teach row of [`STORED_TEACH_BODY`] with this status, with the prompt
/// stamp of an EDITED prompt.
///
/// The digest is the digest the pass computes for that body, so a re-author that
/// reproduces the body collides with this row.
async fn seed_stale_teach(db: &TestDb, status: &str) {
    Seed::new(STORED_TEACH_DIGEST, KP_KEY, "teach", status)
        .body(STORED_TEACH_BODY)
        .prompt(OLD_PROMPT)
        .insert(&db.admin)
        .await;
}

/// One pass of the teach kind with an attempt bound of one, and its outcome.
async fn one_teach_pass(
    db: &TestDb,
    fake: &FakeModel,
    outcome: Outcome,
    spent: u32,
) -> cadus_worker::AuthoringReport {
    author_within_expect(db, fake, 1, Kind::Teach, &spec(), outcome, spent).await
}

/// The stale slots of the teach kind, as the loop counts them.
async fn stale_teach_slots(db: &TestDb) -> i64 {
    stale_slots(&handle(db), KP_KEY, Kind::Teach).await.unwrap()
}

/// The `--stale` listing of every kind, as the operator reads it.
async fn stale_listing(db: &TestDb) -> String {
    render_stale(&stale_rows(&handle(db), &KINDS).await.unwrap())
}

/// V3, the acceptance check: two passes after a prompt edit make ONE model call,
/// and the stale set is empty after the first pass.
///
/// The seeded row is the row an EDITED prompt marked: it is `approved`, it holds
/// the body the model reproduces, and it names an older prompt. The pass
/// therefore re-authors the knowledge point, and the model answers the same
/// document.
///
/// The insert writes nothing, because the digest is the digest of that row. The
/// pass stamps the CURRENT prompt on the row instead, so
///
/// - the outcome is [`Outcome::Refreshed`],
/// - the row leaves the stale set and `--stale` lists nothing,
/// - the SECOND pass makes no call, and
/// - the approval, the body and the digest stand (C6).
///
/// Before the fix the row kept the old stamp: every pass counted it as stale,
/// re-authored it, and paid for one model call, forever.
#[tokio::test]
async fn a_re_author_of_the_same_body_stamps_the_current_prompt_and_the_loop_stops() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![teach_reply()]).await;
        seed_stale_teach(&db, "approved").await;

        // The bank of `teach` is 1, the row holds the slot, and the slot is
        // stale: the pass re-authors it.
        assert_eq!(
            slots_taken(&handle(&db), KP_KEY, Kind::Teach)
                .await
                .unwrap(),
            1
        );
        assert_eq!(stale_teach_slots(&db).await, 1);
        assert_eq!(
            stale_listing(&db).await,
            "stale documents\n\
             kp_id kind digest prompt_digest\n\
             perfect-squares/squares teach sha256:fc031ed7deaa7d60 0000000000000000\n\
             stale: rows 1\n"
        );

        let first = one_teach_pass(&db, &fake, Outcome::Refreshed, 1).await;

        assert_eq!(first.digest.as_deref(), Some(STORED_TEACH_DIGEST));
        assert!(first.decline.is_none());
        assert_eq!(fake.call_count(), 1);

        // The row left the stale set, and the operator's listing is empty.
        assert_eq!(stale_teach_slots(&db).await, 0);
        assert_eq!(stale_listing(&db).await, NO_STALE_ROWS);

        // C6: the approval, the digest and the body stand. The stamp is the
        // only column the pass wrote.
        let row = assert_one_row(
            &db.admin,
            KP_KEY,
            "teach",
            STORED_TEACH_DIGEST,
            "approved",
            1,
        )
        .await;
        assert_eq!(
            row.body,
            serde_json::from_str::<Value>(STORED_TEACH_BODY).expect("the literal body reads")
        );
        assert_ne!(row.prompt_digest.as_deref(), Some(OLD_PROMPT));
        assert_eq!(row.prompt_digest.as_deref().unwrap_or_default().len(), 16);
        assert_eq!(row.prompt_digest, Some(prompt_digest(Kind::Teach)));

        // The second pass makes no call: the bank is full and nothing is stale.
        one_teach_pass(&db, &fake, Outcome::Skipped, 0).await;

        assert_eq!(
            fake.call_count(),
            1,
            "the two passes together pay for ONE model call"
        );
        assert_eq!(
            content_rows_of_kind(&db.admin, KP_KEY, "teach").await.len(),
            1
        );
    })
    .await;
}

/// V3: the batch counts a refresh where it counts a duplicate, and the summary
/// prints the count.
///
/// The pass paid for its call and wrote no document, so the count stands beside
/// `stored` and never inside it (M6 review finding F1).
#[tokio::test]
async fn a_refreshed_row_is_counted_as_a_duplicate_and_never_as_a_store() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![teach_reply()]).await;
        seed_stale_teach(&db, "approved").await;

        let batch = batch(&db, &fake, Kind::Teach, &[spec()]).await;

        assert_batch(&batch, [0, 1, 0, 0, 1]);
        assert!(batch.declines.is_empty());

        assert_eq!(stale_teach_slots(&db).await, 0);
        assert_eq!(
            content_rows_of_kind(&db.admin, KP_KEY, "teach").await.len(),
            1
        );
    })
    .await;
}

/// V3: a collision with a row that ALREADY names the current prompt is a
/// duplicate, not a refresh.
///
/// The bank target of `template` is 3, so a knowledge point with one `pending`
/// row is authored again. The first pass wrote that row with the current stamp,
/// so the second pass has no stamp to write: the outcome names the duplicate,
/// and a pass that reported a refresh here would count a write it did not make.
#[tokio::test]
async fn a_collision_with_the_current_prompt_stamp_stays_a_duplicate() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![
            tool_reply(&good_arguments()),
            tool_reply(&good_arguments()),
        ])
        .await;

        author_within_expect(&db, &fake, 1, Kind::Template, &spec(), Outcome::Stored, 1).await;
        author_within_expect(
            &db,
            &fake,
            1,
            Kind::Template,
            &spec(),
            Outcome::Duplicate,
            1,
        )
        .await;

        assert_eq!(fake.call_count(), 2);

        let row = assert_one_row(&db.admin, KP_KEY, "template", STORED_DIGEST, "pending", 1).await;
        assert_eq!(row.prompt_digest, Some(prompt_digest(Kind::Template)));
        assert_eq!(
            stale_slots(&handle(&db), KP_KEY, Kind::Template)
                .await
                .unwrap(),
            0
        );
    })
    .await;
}

/// V3 and F6: a row a reviewer REFUSED keeps its old stamp.
///
/// A `rejected` row occupies no slot and is never stale, so it drives no
/// re-author and there is nothing to clear. The pass declines with the
/// reviewer's reason, and it writes no column of that row.
#[tokio::test]
async fn a_refused_row_keeps_the_prompt_stamp_it_has() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![teach_reply()]).await;
        Seed::new(STORED_TEACH_DIGEST, KP_KEY, "teach", "rejected")
            .body(STORED_TEACH_BODY)
            .reason("the worked example skips the last step")
            .prompt(OLD_PROMPT)
            .insert(&db.admin)
            .await;

        one_teach_pass(&db, &fake, Outcome::Rejected { same_body: true }, 1).await;

        let row = assert_one_row(
            &db.admin,
            KP_KEY,
            "teach",
            STORED_TEACH_DIGEST,
            "rejected",
            1,
        )
        .await;
        assert_eq!(row.prompt_digest.as_deref(), Some(OLD_PROMPT));
    })
    .await;
}

/// V3: a stale row that WAITS for a reviewer is stamped as well.
///
/// [`stale_slots`] counts `approved` AND `pending` rows, so a `pending` row an
/// older prompt wrote drives a re-author exactly as an approved row does. The
/// stamp therefore covers both, and the row stays `pending`: the reviewer reads
/// the body, and the prompt stamp is not a verdict (C6).
///
/// A stamp that named `approved` alone left this row stale, and the pass paid
/// for one model call per run until a reviewer read the queue.
#[tokio::test]
async fn a_stale_row_that_waits_for_a_reviewer_is_stamped_and_stays_pending() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![teach_reply()]).await;
        seed_stale_teach(&db, "pending").await;

        assert_eq!(stale_teach_slots(&db).await, 1);

        one_teach_pass(&db, &fake, Outcome::Refreshed, 1).await;

        assert_eq!(fake.call_count(), 1);
        assert_eq!(stale_teach_slots(&db).await, 0);

        let row = assert_one_row(
            &db.admin,
            KP_KEY,
            "teach",
            STORED_TEACH_DIGEST,
            "pending",
            1,
        )
        .await;
        assert_eq!(row.prompt_digest, Some(prompt_digest(Kind::Teach)));
    })
    .await;
}
