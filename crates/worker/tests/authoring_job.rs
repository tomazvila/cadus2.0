//! M6 R2 acceptance: the per-knowledge-point batch loop (A2, C6, T3, T6).
//!
//! The section 7 row of the spec names three checks, and each one is a test
//! here:
//!
//! 1. a template refused for a missing low edge is re-prompted with that exact
//!    message and is rescued on attempt 2;
//! 2. a knowledge point refused 5 times writes no `content_store` row and one
//!    decline record;
//! 3. the loop makes zero calls for an already-approved knowledge point.
//!
//! The rest of the file holds the checks the loop needs beside those three: the
//! T6 ledger of an authoring call, the stored row's own columns, a `rejected`
//! row that does not occupy a slot, an endpoint that refuses every attempt, and
//! a batch that keeps going past one decline. `authoring_job_key.rs`,
//! `authoring_job_served.rs` and `authoring_job_stamp.rs` hold the checks the
//! M6 review added.
//!
//! Every expected value is a LITERAL: a literal rejection sentence, a literal
//! digest, a literal row count, a literal status. Nothing is read back from the
//! code under test.
//!
//! The endpoint is a fake OpenAI-compatible server (`common::FakeModel`). No
//! test reaches a real provider.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;
use cadus_worker::authoring::job::{Outcome, bank_target, slots_taken};
use cadus_worker::authoring::prompt::{AuthoringSpec, Kind};
use serde_json::{Value, json};

use common::{
    FakeModel, LOW_EDGE, RETRY_HEADER, SQUARES_KEY as KP_KEY, STORED_DIGEST, assert_batch,
    assert_one_row, author, author_expect, author_within_expect, batch, content_rows,
    good_arguments, handle, ledger_shape, missing_low_edge, proof_spec, seed_content,
    squares_spec as spec, the_one_decline, tool_reply,
};

/// The body the loop stores for `good_arguments`, character for character.
///
/// The three server-side fields lead it, the gate's `space_size` closes it, and
/// the model's `constraints` and `distractors` are absent because both are empty
/// and the document skips an empty list.
const STORED_BODY: &str = r#"{"v":1,"topic_id":"perfect-squares","answer_kind":"numeric","statement":"Compute ${a}^{{2}}$.","params":{"a":{"kind":"int","low":1,"high":12}},"answer_expr":"a**2","solution_sketch":"${a} \\times {a}$ gives the answer.","hints":["What does squaring a number mean?"],"samples":[{"params":{"a":1},"expected":"1"},{"params":{"a":12},"expected":"144"}],"space_size":12}"#;

/// A second knowledge point, for the batch test.
fn other_spec() -> AuthoringSpec {
    AuthoringSpec {
        kp_id: "cubes".to_owned(),
        topic_id: "perfect-cubes".to_owned(),
        ..spec()
    }
}

/// Five refusals of the low edge, the shape of a doomed knowledge point.
fn five_refusals() -> Vec<(u16, String)> {
    (0..5).map(|_| tool_reply(&missing_low_edge())).collect()
}

/// One refusal of the low edge, then the template the gate accepts, authored:
/// the rescue on attempt 2, with its endpoint and its report.
async fn rescued_on_attempt_two(db: &TestDb) -> (FakeModel, cadus_worker::AuthoringReport) {
    let fake = FakeModel::start(vec![
        tool_reply(&missing_low_edge()),
        tool_reply(&good_arguments()),
    ])
    .await;
    let report = author_expect(db, &fake, Kind::Template, &spec(), Outcome::Stored, 2).await;
    (fake, report)
}

/// The `model_call_log` rows of `purpose = 'authoring'`, as
/// `(count, non-NULL user ids)`.
async fn ledger(db: &TestDb) -> (i64, i64) {
    ledger_shape(&db.admin, "authoring").await
}

/// Seed `count` template rows of this status under the knowledge point.
async fn seed_templates(db: &TestDb, status: &str, count: u32) {
    for index in 0..count {
        let digest = format!("sha256:{status}-{index}");
        seed_content(&db.admin, &digest, KP_KEY, "template", status).await;
    }
}

// --------------------------------------------------------------------------- //
// 1. The rescue on attempt 2
// --------------------------------------------------------------------------- //

/// The acceptance literal: a template refused for a missing low edge is
/// re-prompted with that EXACT message and is rescued on attempt 2.
///
/// The retry block is what 1.0 measures as the yield lever
/// (`problem_templates.py:1386-1393`), so the test reads the second request's
/// user message and asserts the whole two-line block, not a substring of it.
#[tokio::test]
async fn a_missing_low_edge_is_re_prompted_verbatim_and_rescued_on_attempt_two() {
    TestDb::with(|db| async move {
        let (fake, report) = rescued_on_attempt_two(&db).await;

        assert_eq!(report.kp_id, KP_KEY);
        assert_eq!(report.digest.as_deref(), Some(STORED_DIGEST));
        assert!(report.decline.is_none());

        // The first attempt carries no retry block at all.
        let first = fake.user_message(0);
        assert!(
            !first.contains(RETRY_HEADER),
            "attempt 1 must carry no retry block, it read:\n{first}"
        );

        // The second attempt carries the gate's own sentence, indented by four
        // spaces under the header.
        let second = fake.user_message(1);
        assert!(
            second.contains(&format!("{RETRY_HEADER}\n    {LOW_EDGE}\n")),
            "attempt 2 must quote the rejection verbatim, it read:\n{second}"
        );
        assert_eq!(fake.call_count(), 2);

        // One row, `pending`, with the attempt count on it (T3).
        assert_one_row(&db.admin, KP_KEY, "template", STORED_DIGEST, "pending", 2).await;
    })
    .await;
}

/// The stored body carries the server's fields and the gate's satisfying count.
///
/// `v`, `topic_id`, `answer_kind` and `space_size` are the server's, never the
/// model's (`problem_templates.py:309`). The model sent none of them.
#[tokio::test]
async fn the_stored_body_carries_the_server_fields_and_the_gate_space_size() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;

        author(&db, &fake, Kind::Template, &spec()).await;

        let row = assert_one_row(&db.admin, KP_KEY, "template", STORED_DIGEST, "pending", 1).await;
        let body = &row.body;
        assert_eq!(
            body,
            &serde_json::from_str::<Value>(STORED_BODY).expect("the literal body reads")
        );
        assert_eq!(body["v"], json!(1));
        assert_eq!(body["topic_id"], json!("perfect-squares"));
        assert_eq!(body["answer_kind"], json!("numeric"));
        assert_eq!(body["space_size"], json!(12));
        assert_eq!(body["statement"], json!("Compute ${a}^{{2}}$."));
        assert_eq!(body["answer_expr"], json!("a**2"));
    })
    .await;
}

/// T6: one ledger row per HTTP attempt, `purpose = 'authoring'`, no user id.
///
/// Two authoring attempts against a fake endpoint that answers on the first HTTP
/// attempt of each call give two rows. An offline authoring call belongs to no
/// tenant, so `user_id` is NULL on both.
#[tokio::test]
async fn every_authoring_call_writes_its_own_ledger_row() {
    TestDb::with(|db| async move {
        let (_fake, report) = rescued_on_attempt_two(&db).await;

        assert_eq!(report.http_attempts.len(), 2);
        assert_eq!(ledger(&db).await, (2, 0));
    })
    .await;
}

// --------------------------------------------------------------------------- //
// 2. The decline path
// --------------------------------------------------------------------------- //

/// The acceptance literal: a knowledge point refused 5 times writes no
/// `content_store` row and one decline record.
#[tokio::test]
async fn five_refusals_write_no_row_and_one_decline_record() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(five_refusals()).await;

        let batch = batch(&db, &fake, Kind::Template, &[spec()]).await;

        assert_batch(&batch, [0, 0, 0, 1, 5]);
        let decline = the_one_decline(&batch, KP_KEY, Kind::Template, 5);
        assert_eq!(decline.reasons.len(), 5);
        for reason in &decline.reasons {
            assert_eq!(reason, &format!("edge-coverage: {LOW_EDGE}"));
        }

        assert_eq!(fake.call_count(), 5);
        assert!(content_rows(&db.admin, KP_KEY).await.is_empty());
    })
    .await;
}

/// A knowledge point whose answer kind the gate can never accept declines with
/// ZERO model calls (T3).
///
/// `TEMPLATABLE_KINDS` is `numeric` and `expression`. Five calls for a `proof`
/// knowledge point buy five copies of one refusal.
#[tokio::test]
async fn an_undecidable_answer_kind_declines_with_no_call() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;

        let report = author_expect(
            &db,
            &fake,
            Kind::Template,
            &proof_spec(),
            Outcome::Declined,
            0,
        )
        .await;

        assert_eq!(
            report.decline.expect("a decline record").reasons,
            vec!["answer kind proof is not symbolically decidable".to_owned()]
        );
        assert_eq!(fake.call_count(), 0);
        assert!(content_rows(&db.admin, KP_KEY).await.is_empty());
    })
    .await;
}

/// The zero-call guard covers EVERY kind whose gate refuses the answer kind
/// (T3, finding F19).
///
/// `gate_diagnosis` holds the same rule as the template gate and refuses a
/// `proof` document with the same sentence, so a `proof` knowledge point must
/// cost zero calls for `diagnosis` too. The old guard read `template` alone, so
/// a diagnosis pass over a `proof` topic paid five calls for one refusal that no
/// retry can fix.
#[tokio::test]
async fn an_undecidable_answer_kind_declines_with_no_call_for_every_gated_kind() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;

        for kind in [Kind::Template, Kind::Diagnosis] {
            let report = author_expect(&db, &fake, kind, &proof_spec(), Outcome::Declined, 0).await;

            let decline = report.decline.expect("a decline record");
            assert_eq!(decline.kp_id, KP_KEY);
            assert_eq!(decline.kind, kind);
            assert_eq!(decline.attempts, 0);
            assert_eq!(
                decline.reasons,
                vec!["answer kind proof is not symbolically decidable".to_owned()]
            );
        }

        assert_eq!(fake.call_count(), 0);
        assert!(content_rows(&db.admin, KP_KEY).await.is_empty());
    })
    .await;
}

/// An endpoint that refuses every request declines with the endpoint's reason,
/// and the reason is not fed back as authoring feedback.
///
/// A 400 never retries inside the client (spec section 6.5), so five authoring
/// attempts are five HTTP attempts.
#[tokio::test]
async fn an_endpoint_that_refuses_every_attempt_declines() {
    TestDb::with(|db| async move {
        let refusals: Vec<(u16, String)> = (0..5)
            .map(|_| (400, json!({"error": "no such model"}).to_string()))
            .collect();
        let fake = FakeModel::start(refusals).await;

        let report = author_expect(&db, &fake, Kind::Template, &spec(), Outcome::Declined, 5).await;

        let decline = report.decline.expect("a decline record");
        assert_eq!(decline.reasons.len(), 5);
        assert_eq!(
            decline.reasons[0],
            "model endpoint answered 400: {\"error\":\"no such model\"}"
        );
        // No gate ever ran, so no attempt carried a retry block.
        for index in 0..5 {
            let message = fake.user_message(index);
            assert!(
                !message.contains(RETRY_HEADER),
                "a transport failure is not authoring feedback, call {index} read:\n{message}"
            );
        }
        assert!(content_rows(&db.admin, KP_KEY).await.is_empty());
        assert_eq!(ledger(&db).await, (5, 0));
    })
    .await;
}

/// One decline never stops the batch: the second knowledge point still stores.
#[tokio::test]
async fn a_decline_does_not_stop_the_batch() {
    TestDb::with(|db| async move {
        let mut replies = five_refusals();
        replies.push(tool_reply(&good_arguments()));
        let fake = FakeModel::start(replies).await;

        let batch = batch(&db, &fake, Kind::Template, &[spec(), other_spec()]).await;

        assert_batch(&batch, [1, 0, 0, 1, 6]);
        the_one_decline(&batch, KP_KEY, Kind::Template, 5);
        assert!(content_rows(&db.admin, KP_KEY).await.is_empty());
        assert_eq!(
            content_rows(&db.admin, "perfect-cubes/cubes").await.len(),
            1
        );
    })
    .await;
}

// --------------------------------------------------------------------------- //
// 3. Zero calls for a knowledge point that is already served
// --------------------------------------------------------------------------- //

/// The acceptance literal: the loop makes zero calls for an already-approved
/// knowledge point.
///
/// The bank target of `template` is 3 (1.0 `BANK_TARGET`), so the test seeds the
/// full bank and asserts that the endpoint saw nothing at all.
#[tokio::test]
async fn an_approved_knowledge_point_costs_zero_calls() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        seed_templates(&db, "approved", 3).await;

        let report = author_expect(&db, &fake, Kind::Template, &spec(), Outcome::Skipped, 0).await;

        assert!(report.digest.is_none());
        assert_eq!(fake.call_count(), 0);
        assert_eq!(ledger(&db).await, (0, 0));
        // The seeded rows are untouched: three rows, all `approved`.
        let rows = content_rows(&db.admin, KP_KEY).await;
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|row| row.status == "approved"));
    })
    .await;
}

/// A slot a human has not read yet is occupied too (1.0 `:1352-1374`).
///
/// One approved row and two pending rows fill the bank, so a nightly pass does
/// not put a fourth document in front of a reviewer.
#[tokio::test]
async fn a_pending_slot_is_occupied_and_costs_zero_calls() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        seed_templates(&db, "approved", 1).await;
        seed_templates(&db, "pending", 2).await;

        assert_eq!(
            slots_taken(&handle(&db), KP_KEY, Kind::Template)
                .await
                .unwrap(),
            3
        );
        assert_eq!(bank_target(Kind::Template), 3);

        author_expect(&db, &fake, Kind::Template, &spec(), Outcome::Skipped, 0).await;

        assert_eq!(fake.call_count(), 0);
    })
    .await;
}

/// A `rejected` row occupies nothing: a human refused that body, and the
/// knowledge point still needs a document.
#[tokio::test]
async fn a_rejected_row_does_not_occupy_a_slot() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        seed_templates(&db, "rejected", 3).await;

        assert_eq!(
            slots_taken(&handle(&db), KP_KEY, Kind::Template)
                .await
                .unwrap(),
            0
        );

        author_expect(&db, &fake, Kind::Template, &spec(), Outcome::Stored, 1).await;

        assert_eq!(fake.call_count(), 1);
        assert_eq!(content_rows(&db.admin, KP_KEY).await.len(), 4);
    })
    .await;
}

/// A body a human already rejected never comes back as `pending` (C6).
///
/// The digest is the identity of the document, so the second insert conflicts
/// and does nothing. The reviewer's verdict stands, and the pass says so:
/// `Outcome::Rejected` and not `Outcome::Duplicate` (M6 review finding F6).
#[tokio::test]
async fn a_rejected_body_is_not_resurrected_as_pending() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        seed_content(&db.admin, STORED_DIGEST, KP_KEY, "template", "rejected").await;

        let report = author_expect(
            &db,
            &fake,
            Kind::Template,
            &spec(),
            Outcome::Rejected { same_body: true },
            1,
        )
        .await;

        assert_eq!(report.digest.as_deref(), Some(STORED_DIGEST));
        let row = assert_one_row(&db.admin, KP_KEY, "template", STORED_DIGEST, "rejected", 1).await;
        assert_eq!(row.body, json!({}));
    })
    .await;
}

/// A full bank makes no call, for every kind (spec section 2.2, step 1).
///
/// No kind is gateless now: unit R6 gave `teach` and `hint_ladder` their gates
/// and unit R7 gave `diagnosis` its gate. The zero-call rule that the gateless
/// kinds carried is the bank rule alone.
#[tokio::test]
async fn a_full_bank_makes_no_call_for_every_kind() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;
        for (digest, kind) in [
            ("sha256:aa11", Kind::Teach),
            ("sha256:bb22", Kind::HintLadder),
            ("sha256:cc33", Kind::Diagnosis),
        ] {
            seed_content(&db.admin, digest, KP_KEY, kind.as_str(), "approved").await;
        }

        for kind in [Kind::Teach, Kind::HintLadder, Kind::Diagnosis] {
            author_expect(&db, &fake, kind, &spec(), Outcome::Skipped, 0).await;
        }

        assert_eq!(fake.call_count(), 0);
    })
    .await;
}

/// An attempt bound of 0 makes no call and declines, which is the dry run of R8.
#[tokio::test]
async fn an_attempt_bound_of_zero_makes_no_call() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&good_arguments())]).await;

        author_within_expect(&db, &fake, 0, Kind::Template, &spec(), Outcome::Declined, 0).await;

        assert_eq!(fake.call_count(), 0);
    })
    .await;
}
