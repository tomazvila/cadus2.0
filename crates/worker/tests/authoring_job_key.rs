//! M6 review, findings F1 and F6, and unit R6: the key of one document, the
//! refused body, and the two instruction kinds (C6, L4, L5).
//!
//! The M6 review adds three checks of the `content_store` key: one body under
//! two knowledge points stores two rows, a duplicate is counted as a duplicate
//! and never as a store, and a pass that reproduces a body a reviewer refused
//! declines with the reviewer's reason. Unit R6 adds the gates of the teach page
//! and the hint ladder, so both kinds reach the table gated.
//!
//! Every expected value is a LITERAL, and every endpoint is `common::FakeModel`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;
use cadus_worker::authoring::job::{Outcome, bank_target};
use cadus_worker::authoring::prompt::{AuthoringSpec, Kind};
use serde_json::{Value, json};

use common::{
    FakeModel, OTHER_TEACH_DIGEST, RETRY_HEADER, SQUARES_KEY as KP_KEY, STORED_DIGEST,
    STORED_LADDER_BODY, STORED_LADDER_DIGEST, STORED_TEACH_BODY, STORED_TEACH_DIGEST, Seed,
    assert_batch, assert_one_row, author, author_expect, batch, content_rows_of_kind,
    good_arguments, ladder_arguments, named_reply, proof_spec, seed_content, slots,
    squares_spec as spec, teach_arguments, the_one_decline, tool_reply,
};

/// A second knowledge point, for the two-row test.
fn other_spec() -> AuthoringSpec {
    AuthoringSpec {
        kp_id: "cubes".to_owned(),
        topic_id: "perfect-cubes".to_owned(),
        ..spec()
    }
}

/// A reply that carries the teach page the gate accepts.
fn teach_reply() -> (u16, String) {
    named_reply("emit_teach", &teach_arguments())
}

/// The same ladder with the last rung stating the answer of the exemplar.
fn ladder_that_names_the_answer() -> Value {
    json!({
        "hints": [
            "What does the small 2 above the number ask you to do?",
            "A square is the number multiplied by itself, so $7^2$ is 49."
        ]
    })
}

/// The tool arguments of a teach page with no `worked_example.steps`.
///
/// The concept and the worked problem are both there, so only the missing
/// solution earns the refusal.
fn teach_without_steps() -> Value {
    json!({
        "concept": "Squaring a number multiplies it by itself.",
        "worked_example": {"problem": "Compute $6^2$."}
    })
}

/// The gate's sentence for [`teach_without_steps`]
/// (`crates/core/src/instruction.rs`).
const NO_STEPS: &str = "a teach page needs 'worked_example.steps': the complete solution, one \
step per entry, ending with the final answer — a concept with no worked solution teaches nothing";

/// The gate's give-away sentence for the last rung of
/// [`ladder_that_names_the_answer`] (`crates/core/src/instruction.rs`).
const NAMES_THE_ANSWER: &str = "rung 1 reads 'A square is the number multiplied by itself, so \
$7^2$ is 49.', which names the answer '49' this knowledge point serves — a hint is a question, \
never the final step (Hard Rule 3)";

/// The retry block of attempt 2 carries the gate's own words, indented under
/// the header.
fn assert_retry_block(fake: &FakeModel, sentence: &str) {
    let second = fake.user_message(1);
    assert!(
        second.contains(&format!("{RETRY_HEADER}\n    {sentence}\n")),
        "the retry block did not carry the literal sentence: {second}"
    );
}

// --------------------------------------------------------------------------- //
// M6 review: the key of one document (F1) and the refused body (F6)
// --------------------------------------------------------------------------- //

/// F1: two knowledge points that earn ONE teach page store TWO rows.
///
/// A teach page carries no knowledge-point field: the serve reader of L4 refuses
/// an unknown field, so the stored body of `perfect-squares/squares` and the
/// stored body of `perfect-cubes/cubes` are the same bytes. The key of the table
/// covers the knowledge point, so the two documents are two rows, both `pending`
/// and both counted.
///
/// A key of the body alone gave both rows one digest: the second insert vanished
/// under `ON CONFLICT (digest) DO NOTHING`, the batch counted two stores, and
/// the second knowledge point held no document at all.
#[tokio::test]
async fn one_teach_body_under_two_knowledge_points_stores_two_rows() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![teach_reply(), teach_reply()]).await;

        let batch = batch(&db, &fake, Kind::Teach, &[spec(), other_spec()]).await;

        assert_batch(&batch, [2, 0, 0, 0, 2]);

        // One row per knowledge point, with its own digest.
        let squares = assert_one_row(
            &db.admin,
            KP_KEY,
            "teach",
            STORED_TEACH_DIGEST,
            "pending",
            1,
        )
        .await;
        let cubes = assert_one_row(
            &db.admin,
            "perfect-cubes/cubes",
            "teach",
            OTHER_TEACH_DIGEST,
            "pending",
            1,
        )
        .await;

        // The two rows hold the same body and two different keys.
        assert_eq!(squares.body, cubes.body);
        assert_ne!(squares.digest, cubes.digest);

        // The second knowledge point holds its slot now, so the next pass makes
        // no call for it.
        assert_eq!(slots(&db, "perfect-cubes/cubes", Kind::Teach).await, 1);
    })
    .await;
}

/// F1: a document the table already holds is a DUPLICATE, never a store.
///
/// The bank target of `template` is 3, so a knowledge point with one `pending`
/// row is authored again. The model answers the same arguments, the body is the
/// same, and the digest is the same: the insert writes nothing. The operator
/// reads one store and one duplicate, and the table holds one row.
#[tokio::test]
async fn a_repeated_body_is_counted_as_a_duplicate_and_never_as_a_store() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![
            tool_reply(&good_arguments()),
            tool_reply(&good_arguments()),
        ])
        .await;

        let batch = batch(&db, &fake, Kind::Template, &[spec(), spec()]).await;

        assert_batch(&batch, [1, 1, 0, 0, 2]);
        assert!(batch.declines.is_empty());

        // The row keeps the accounting of the pass that WROTE it.
        assert_one_row(&db.admin, KP_KEY, "template", STORED_DIGEST, "pending", 1).await;
    })
    .await;
}

/// F6: a pass that reproduces a refused body declines, with the reviewer's own
/// reason in the decline record (C6).
///
/// A `rejected` row occupies no slot, so the knowledge point is authored again.
/// The model answers the body the reviewer refused, the digest is the refused
/// digest, and the insert writes nothing. The pass therefore stores nothing, and
/// the batch counts a decline and not a store.
#[tokio::test]
async fn a_re_author_of_a_refused_body_declines_with_the_reviewer_reason() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        Seed::new(STORED_DIGEST, KP_KEY, "template", "rejected")
            .reason("the statement asks for two answers")
            .insert(&db.admin)
            .await;

        assert_eq!(
            slots(&db, KP_KEY, Kind::Template).await,
            0,
            "a rejected row occupies no slot, so the pass runs"
        );

        let batch = batch(&db, &fake, Kind::Template, &[spec()]).await;

        assert_batch(&batch, [0, 0, 0, 1, 1]);
        let decline = the_one_decline(&batch, KP_KEY, Kind::Template, 1);
        assert_eq!(
            decline.reasons,
            vec![
                "a reviewer refused this exact body already: the statement asks for two answers"
                    .to_owned()
            ]
        );

        // The refused row is untouched: one row, still `rejected`, still with
        // the reviewer's reason.
        let rows = sqlx::query_as::<_, (String, String, Option<String>)>(
            "SELECT digest, status, review_reason FROM content_store WHERE kp_id = $1",
        )
        .bind(KP_KEY)
        .fetch_all(&db.admin)
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, STORED_DIGEST);
        assert_eq!(rows[0].1, "rejected");
        assert_eq!(
            rows[0].2.as_deref(),
            Some("the statement asks for two answers")
        );
    })
    .await;
}

/// F6: the same pass through `author_one`, with the report of one knowledge
/// point.
///
/// The outcome names the reproduced body, the report carries the refused digest,
/// and the decline record is the one the batch collects.
#[tokio::test]
async fn a_refused_teach_page_reports_the_rejection_and_stores_nothing() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![teach_reply()]).await;
        Seed::new(STORED_TEACH_DIGEST, KP_KEY, "teach", "rejected")
            .reason("the worked example skips the last step")
            .insert(&db.admin)
            .await;

        let report = author_expect(
            &db,
            &fake,
            Kind::Teach,
            &spec(),
            Outcome::Rejected { same_body: true },
            1,
        )
        .await;

        assert_eq!(report.digest.as_deref(), Some(STORED_TEACH_DIGEST));
        assert_eq!(report.cost_usd, None);
        let decline = report
            .decline
            .expect("a rejection carries a decline record");
        assert_eq!(decline.kp_id, KP_KEY);
        assert_eq!(
            decline.reasons,
            vec![
                "a reviewer refused this exact body already: the worked example skips the last step"
                    .to_owned()
            ]
        );

        // Nothing new reached the table, and the verdict stands.
        let rows = content_rows_of_kind(&db.admin, KP_KEY, "teach").await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "rejected");
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Unit R6: the teach page (L4) and the hint ladder (L5) reach the table
// --------------------------------------------------------------------------- //

/// A gated teach page reaches `content_store` as one `pending` row (L4, C6).
#[tokio::test]
async fn a_teach_page_is_gated_and_stored_pending() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![teach_reply()]).await;

        let report = author(&db, &fake, Kind::Teach, &spec()).await;

        assert_eq!(report.outcome, Outcome::Stored);
        assert_eq!(report.attempts, 1);
        assert_eq!(report.digest.as_deref(), Some(STORED_TEACH_DIGEST));

        let row = assert_one_row(
            &db.admin,
            KP_KEY,
            "teach",
            STORED_TEACH_DIGEST,
            "pending",
            1,
        )
        .await;
        assert_eq!(
            row.body,
            serde_json::from_str::<Value>(STORED_TEACH_BODY).unwrap()
        );

        // The call is the teach call: the forced tool is the teach tool.
        assert_eq!(fake.call_count(), 1);
        assert_eq!(
            fake.calls()[0]["tool_choice"]["function"]["name"],
            json!("emit_teach")
        );
    })
    .await;
}

/// ACCEPTANCE, at the loop. A teach body with no `worked_example.steps` is
/// refused, the LITERAL sentence reaches attempt 2, and the complete page is
/// stored (L4, spec section 7 row R6).
///
/// The gate test in `crates/core/tests/instruction_gate.rs` proves the rule. This
/// test proves the LOOP runs that gate for `teach`: a loop that stored the tool
/// arguments unread would store this page on attempt 1, with one call and a body
/// no route can serve.
#[tokio::test]
async fn a_teach_page_with_no_steps_is_re_prompted_and_rescued() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![
            named_reply("emit_teach", &teach_without_steps()),
            teach_reply(),
        ])
        .await;

        let report = author_expect(&db, &fake, Kind::Teach, &spec(), Outcome::Stored, 2).await;

        assert_eq!(report.digest.as_deref(), Some(STORED_TEACH_DIGEST));
        assert_retry_block(&fake, NO_STEPS);

        // Attempt 1 stored nothing: the table holds the rescued page alone.
        let row = assert_one_row(
            &db.admin,
            KP_KEY,
            "teach",
            STORED_TEACH_DIGEST,
            "pending",
            2,
        )
        .await;
        assert_eq!(
            row.body,
            serde_json::from_str::<Value>(STORED_TEACH_BODY).unwrap()
        );
    })
    .await;
}

/// A ladder whose last rung names the answer is refused, the LITERAL sentence
/// reaches attempt 2, and the clean ladder is stored (L5, Hard Rule 3).
#[tokio::test]
async fn a_ladder_that_names_the_answer_is_re_prompted_and_rescued() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![
            named_reply("emit_hint_ladder", &ladder_that_names_the_answer()),
            named_reply("emit_hint_ladder", &ladder_arguments()),
        ])
        .await;

        let report = author_expect(&db, &fake, Kind::HintLadder, &spec(), Outcome::Stored, 2).await;

        assert_eq!(report.digest.as_deref(), Some(STORED_LADDER_DIGEST));
        assert_retry_block(&fake, NAMES_THE_ANSWER);

        let row = assert_one_row(
            &db.admin,
            KP_KEY,
            "hint_ladder",
            STORED_LADDER_DIGEST,
            "pending",
            2,
        )
        .await;
        assert_eq!(
            row.body,
            serde_json::from_str::<Value>(STORED_LADDER_BODY).unwrap()
        );
    })
    .await;
}

/// One approved teach page fills the bank of that kind, so the next pass makes
/// ZERO calls: `teach` and `hint_ladder` keep one document per knowledge point.
#[tokio::test]
async fn an_approved_teach_page_fills_the_bank_of_one() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![teach_reply()]).await;
        seed_content(&db.admin, "digest-teach", KP_KEY, "teach", "approved").await;

        assert_eq!(bank_target(Kind::Teach), 1);
        assert_eq!(slots(&db, KP_KEY, Kind::Teach).await, 1);

        let report = author(&db, &fake, Kind::Teach, &spec()).await;

        assert_eq!(report.outcome, Outcome::Skipped);
        assert_eq!(report.attempts, 0);
        assert_eq!(fake.call_count(), 0);
        assert_eq!(
            content_rows_of_kind(&db.admin, KP_KEY, "teach").await.len(),
            1
        );
    })
    .await;
}

/// A knowledge point whose answer kind no checker decides still gets a teach
/// page: only the TEMPLATE gate reads the answer kind.
#[tokio::test]
async fn an_undecidable_answer_kind_still_gets_a_teach_page() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![teach_reply()]).await;
        let proof = proof_spec();

        author_expect(&db, &fake, Kind::Template, &proof, Outcome::Declined, 0).await;
        author_expect(&db, &fake, Kind::Teach, &proof, Outcome::Stored, 1).await;

        assert_eq!(fake.call_count(), 1);
    })
    .await;
}
