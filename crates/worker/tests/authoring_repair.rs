//! FIX-M6-A2 acceptance: the LaTeX escape repair and the prompt digest.
//!
//! The M6 review names two defects of the authoring boundary, and each one is a
//! test here:
//!
//! - **F3** — no repair of the model's under-escaped LaTeX runs anywhere in the
//!   pipeline. `verify_kind` hands the decoded tool arguments straight to the
//!   gates, no gate reads a control character, and `"$\times$"` with one
//!   backslash reaches `content_store` as `$<TAB>imes$` on `template`, `teach`
//!   and `hint_ladder` alike (spec section 5, trap T1).
//! - **F4** — spec section 2.2 asks for the prompt digest on the row, so a
//!   prompt edit marks the affected rows for re-authoring. `content_store` had
//!   no such column, and nothing recorded which prompt authored which approved
//!   row.
//!
//! Every expected value is a LITERAL: a literal repaired statement, a literal
//! rejection sentence, a literal status, a literal row count. Nothing is read
//! back from the code under test.
//!
//! Every `\t`, `\n` and `\r` inside a string literal of this file is the control
//! character a JSON decoder hands back for an under-escaped `\times`, `\neq` and
//! `\rightarrow`. That is the input the repair exists for.
//!
//! The endpoint is a fake OpenAI-compatible server (`common::FakeModel`). No
//! test reaches a real provider.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;
use cadus_worker::authoring::job::{Outcome, render_stale, stale_rows, stale_slots};
use cadus_worker::authoring::prompt::{Kind, prompt_digest};
use cadus_worker::authoring::repair::CONTROL_CHARACTER;
use serde_json::{Value, json};

use common::{
    ContentRow, FakeModel, SQUARES_KEY as KP_KEY, Seed, author_within, author_within_expect,
    content_rows, handle, squares_spec as spec,
};

/// The statement the model sends, as the JSON decoder hands it back.
///
/// The model wrote `"Compute ${a} \times {a}$."` with ONE backslash. `\t` is a
/// valid JSON escape, so the decoded value carries a TAB and the command is
/// gone.
const MANGLED_STATEMENT: &str = "Compute ${a} \times {a}$.";

/// The statement the repair restores, character for character.
const REPAIRED_STATEMENT: &str = "Compute ${a} \\times {a}$.";

/// The concept line the model sends for a teach page, mangled the same way.
const MANGLED_CONCEPT: &str = "A square is $a \times a$.";

/// The concept line the repair restores.
const REPAIRED_CONCEPT: &str = "A square is $a \\times a$.";

/// A prompt digest that is NOT the digest of any current kind. It stands for
/// the prompt an operator has since edited (spec section 2.2, "Prompt digest").
const OLD_PROMPT: &str = "sha256:0000000000000000";

/// A reply that carries a complete tool call with these arguments.
fn tool_reply(arguments: &Value) -> (u16, String) {
    common::named_reply("emit", arguments)
}

/// The tool arguments of a template the gate accepts, with a MANGLED statement.
fn mangled_template() -> Value {
    let mut arguments = common::good_arguments();
    arguments["statement"] = json!(MANGLED_STATEMENT);
    arguments
}

/// The same template with a statement the repair cannot name.
///
/// `\u{1}` is not one of the six JSON escape letters, so no repair restores it
/// and the boundary refuses the body.
fn unrepairable_template() -> Value {
    let mut arguments = mangled_template();
    arguments["statement"] = json!("Compute ${a}\u{1}^{{2}}$.");
    arguments
}

/// The tool arguments of a teach page the gate accepts, MANGLED.
fn mangled_teach() -> Value {
    json!({
        "concept": MANGLED_CONCEPT,
        "worked_example": {
            "problem": "Compute $6^2$.",
            "steps": ["$6 \times 6 = 36$."]
        }
    })
}

/// Seed one APPROVED `content_store` row of this kind with this prompt digest.
async fn seed_approved(db: &TestDb, digest: &str, kp_id: &str, kind: &str, prompt: Option<&str>) {
    let mut seed = Seed::new(digest, kp_id, kind, "approved");
    if let Some(prompt) = prompt {
        seed = seed.prompt(prompt);
    }
    seed.insert(&db.admin).await;
}

/// The mangled teach page as the model sends it, beside an approved teach row
/// of an OLDER prompt: the setup of a re-author after a prompt edit.
async fn old_teach_row(db: &TestDb) -> FakeModel {
    seed_approved(
        db,
        "sha256:old-teach-row",
        KP_KEY,
        "teach",
        Some(OLD_PROMPT),
    )
    .await;
    FakeModel::start(vec![tool_reply(&mangled_teach())]).await
}

/// One pass of this kind with an attempt bound of one, and its outcome.
async fn one_pass(
    db: &TestDb,
    fake: &FakeModel,
    kind: Kind,
    outcome: Outcome,
) -> cadus_worker::AuthoringReport {
    let spent = u32::from(outcome != Outcome::Skipped);
    author_within_expect(db, fake, 1, kind, &spec(), outcome, spent).await
}

/// The one `content_store` row of the knowledge point, and the proof that no
/// stored field keeps the TAB.
async fn the_repaired_row(db: &TestDb) -> ContentRow {
    let rows = content_rows(&db.admin, KP_KEY).await;
    assert_eq!(rows.len(), 1);
    assert!(
        !rows[0].body.to_string().contains('\t'),
        "no stored field keeps the TAB"
    );
    rows[0].clone()
}

/// One pass that stores the mangled template, and the one row it stored.
async fn store_mangled_template(db: &TestDb) -> ContentRow {
    let fake = FakeModel::start(vec![tool_reply(&mangled_template())]).await;
    one_pass(db, &fake, Kind::Template, Outcome::Stored).await;
    the_repaired_row(db).await
}

// --------------------------------------------------------------------------- //
// F3: the LaTeX escape repair of the boundary
// --------------------------------------------------------------------------- //

/// F3: an under-escaped `\times` is repaired before the gate, on a TEMPLATE.
///
/// A mangled template is worse than one mangled problem: the structure mangles
/// every instance it ever renders, and the C6 approval then binds to those
/// bytes. The test therefore reads the STORED body and asserts the repaired
/// statement, character for character, and that the row carries no TAB at all.
#[tokio::test]
async fn an_under_escaped_command_is_repaired_before_the_template_gate() {
    TestDb::with(|db| async move {
        // The decoded arguments carry the TAB. That is the bug under test.
        assert!(
            MANGLED_STATEMENT.contains('\t'),
            "the TAB is the bug being repaired"
        );
        assert!(!MANGLED_STATEMENT.contains("\\times"));

        let row = store_mangled_template(&db).await;

        assert_eq!(row.body["statement"], json!(REPAIRED_STATEMENT));
    })
    .await;
}

/// F3: the repair runs on a TEACH page too, and it reaches the worked steps.
///
/// The finding names `template`, `teach` and `hint_ladder` together, because
/// `verify_kind` is the one door all three go through.
#[tokio::test]
async fn an_under_escaped_command_is_repaired_before_the_teach_gate() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&mangled_teach())]).await;

        one_pass(&db, &fake, Kind::Teach, Outcome::Stored).await;

        let row = the_repaired_row(&db).await;
        assert_eq!(row.kind, "teach");
        assert_eq!(row.body["concept"], json!(REPAIRED_CONCEPT));
        assert_eq!(
            row.body["worked_example"]["steps"][0],
            json!("$6 \\times 6 = 36$.")
        );
    })
    .await;
}

/// F3: a control character the repair cannot name refuses the body, with the
/// literal message the next attempt reads.
///
/// 1.0 DROPS such a character (`prompts.py:1046`). 2.0 refuses instead: the
/// pipeline is offline and it retries, so the refusal costs one attempt and
/// buys a document nobody has to read twice.
#[tokio::test]
async fn a_control_character_the_repair_cannot_name_refuses_the_body() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![
            tool_reply(&unrepairable_template()),
            tool_reply(&mangled_template()),
        ])
        .await;

        // Attempt 1 is refused, and the LITERAL message is the feedback of
        // attempt 2.
        author_within_expect(&db, &fake, 2, Kind::Template, &spec(), Outcome::Stored, 2).await;
        assert!(
            fake.user_message(1).contains(CONTROL_CHARACTER),
            "the retry block carries the literal control-character message"
        );

        // Attempt 2 stored the repaired document, and nothing else is in the
        // table.
        let row = the_repaired_row(&db).await;
        assert_eq!(row.body["statement"], json!(REPAIRED_STATEMENT));
    })
    .await;
}

// --------------------------------------------------------------------------- //
// F4: the prompt digest on the row
// --------------------------------------------------------------------------- //

/// F4: the stored row names the prompt that authored it.
///
/// Spec section 2.2, "Prompt digest": the digest is a column on
/// `content_store`, never part of the content digest, because the C6 approval
/// binds to the content.
#[tokio::test]
async fn the_stored_row_carries_the_prompt_digest_of_its_kind() {
    TestDb::with(|db| async move {
        let row = store_mangled_template(&db).await;

        assert_eq!(row.prompt_digest, Some(prompt_digest(Kind::Template)));
        // The prompt digest is 16 hex characters, as every 2.0 digest is.
        assert_eq!(row.prompt_digest.as_deref().unwrap_or_default().len(), 16);
        // It is NOT the content digest: the two answer different questions.
        assert_ne!(row.prompt_digest.as_deref().unwrap_or_default(), row.digest);
    })
    .await;
}

/// F4, the acceptance check: a prompt edit re-authors the row and never
/// unapproves it.
///
/// The seeded row stands for a document an EARLIER prompt authored: its
/// `prompt_digest` is not the digest of any current kind. The pass then
///
/// - re-authors the knowledge point, although the bank of `teach` is 1 and the
///   row occupies it, and
/// - leaves the old row `approved`, because C6 binds the approval to the
///   content and a prompt edit changes no content (spec section 2.2).
#[tokio::test]
async fn a_prompt_edit_re_authors_the_row_and_keeps_the_old_approval() {
    TestDb::with(|db| async move {
        let fake = old_teach_row(&db).await;

        assert_eq!(
            stale_slots(&handle(&db), KP_KEY, Kind::Teach)
                .await
                .unwrap(),
            1,
            "the seeded row names a prompt that is not the current one"
        );

        let report = author_within(&db, &fake, 1, Kind::Teach, &spec()).await;

        assert_eq!(report.outcome, Outcome::Stored);
        assert_eq!(fake.call_count(), 1);

        let rows = content_rows(&db.admin, KP_KEY).await;
        assert_eq!(rows.len(), 2);
        // The old row is untouched and still approved.
        assert_eq!(rows[0].digest, "sha256:old-teach-row");
        assert_eq!(rows[0].status, "approved");
        assert_eq!(rows[0].prompt_digest.as_deref(), Some(OLD_PROMPT));
        // The re-authored row is pending, and it names the current prompt.
        assert_eq!(rows[1].status, "pending");
        assert_eq!(rows[1].prompt_digest, Some(prompt_digest(Kind::Teach)));
        assert_eq!(rows[1].body["concept"], json!(REPAIRED_CONCEPT));
    })
    .await;
}

/// F4: a SECOND pass over the same knowledge point makes no call.
///
/// The re-authored row holds the slot from that point on, so a nightly pass
/// after a prompt edit re-authors once and never grows the review queue.
#[tokio::test]
async fn the_pass_after_a_re_author_makes_no_model_call() {
    TestDb::with(|db| async move {
        let fake = old_teach_row(&db).await;

        author_within(&db, &fake, 1, Kind::Teach, &spec()).await;
        let second = author_within(&db, &fake, 1, Kind::Teach, &spec()).await;

        assert_eq!(second.outcome, Outcome::Skipped);
        assert_eq!(second.attempts, 0);
        assert_eq!(fake.call_count(), 1, "the second pass called nothing");
        assert_eq!(content_rows(&db.admin, KP_KEY).await.len(), 2);
    })
    .await;
}

/// F4: a row with NO prompt digest is not stale.
///
/// NULL means "the prompt is not recorded", which every row written before
/// migration `0012` carries. A pass that read NULL as stale would re-author the
/// whole bank on the first run after the deployment.
#[tokio::test]
async fn a_row_with_no_prompt_digest_is_not_stale() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![tool_reply(&mangled_teach())]).await;
        seed_approved(&db, "sha256:legacy-row", KP_KEY, "teach", None).await;

        assert_eq!(
            stale_slots(&handle(&db), KP_KEY, Kind::Teach)
                .await
                .unwrap(),
            0
        );

        one_pass(&db, &fake, Kind::Teach, Outcome::Skipped).await;

        assert_eq!(fake.call_count(), 0);
        assert_eq!(content_rows(&db.admin, KP_KEY).await.len(), 1);
    })
    .await;
}

/// F4: `--stale` lists the approved rows an older prompt wrote, and nothing
/// else.
///
/// The listed text is the operator's output, so the test asserts it byte for
/// byte.
#[tokio::test]
async fn the_stale_listing_names_the_approved_rows_of_an_older_prompt() {
    TestDb::with(|db| async move {
        // One approved row of an older prompt: it is listed.
        seed_approved(&db, "sha256:old-teach", KP_KEY, "teach", Some(OLD_PROMPT)).await;
        // One approved row of the CURRENT prompt: it is not.
        seed_approved(
            &db,
            "sha256:current-teach",
            "perfect-cubes/cubes",
            "teach",
            Some(&prompt_digest(Kind::Teach)),
        )
        .await;
        // One row with no prompt digest: it is not.
        seed_approved(&db, "sha256:legacy-teach", "bare/kp1", "teach", None).await;

        let rows = stale_rows(&handle(&db), &[Kind::Teach]).await.unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kp_id, KP_KEY);
        assert_eq!(rows[0].digest, "sha256:old-teach");
        assert_eq!(rows[0].prompt_digest, OLD_PROMPT);
        assert_eq!(
            render_stale(&rows),
            "stale documents\n\
             kp_id kind digest prompt_digest\n\
             perfect-squares/squares teach sha256:old-teach sha256:0000000000000000\n\
             stale: rows 1\n"
        );
    })
    .await;
}

/// F4: a PENDING row of an older prompt is not listed.
///
/// It is already in front of a reviewer, and a reviewer reads the body and not
/// the prompt.
#[tokio::test]
async fn the_stale_listing_skips_a_pending_row() {
    TestDb::with(|db| async move {
        Seed::new("sha256:pending-teach", KP_KEY, "teach", "pending")
            .prompt(OLD_PROMPT)
            .insert(&db.admin)
            .await;

        let rows = stale_rows(&handle(&db), &[Kind::Teach]).await.unwrap();

        assert!(rows.is_empty());
        assert_eq!(
            render_stale(&rows),
            "stale documents\nkp_id kind digest prompt_digest\nstale: rows 0\n"
        );
    })
    .await;
}
