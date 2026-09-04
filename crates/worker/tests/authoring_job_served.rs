//! M6 review, findings F2, F15 and F25, and M6 review 2, finding V1: the two
//! instruction gates read the material the knowledge point serves (L4, L5).
//!
//! The templates of a knowledge point, `approved` AND `pending`, render the
//! instances a learner reads, so a rung or a worked step that names one of
//! their answers gives the answer away. Every expected value is a LITERAL.
//! The endpoint is a fake OpenAI-compatible server (`common::FakeModel`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::instruction::ServedInstance;
use cadus_store::content::{Admin, approve};
use cadus_store::test_support::TestDb;
use cadus_worker::authoring::job::{Outcome, served_instances};
use cadus_worker::authoring::prompt::Kind;
use serde_json::{Value, json};

use common::{
    FakeModel, NAMES_AN_INSTANCE_ANSWER, RETRY_HEADER, SQUARES_KEY as KP_KEY, STORED_DIGEST,
    STORED_LADDER_DIGEST, assert_one_row, author, author_within, content_rows_of_kind,
    good_arguments, handle, ladder_arguments, ladder_that_names_an_instance_answer, named_reply,
    seed_approved_template, set_content_status, squares_spec as spec,
};

/// The `content_store` body of an approved template of [`KP_KEY`]: the
/// perfect-squares template, `a` over 1 to 12, answer `a**2`. The serve path
/// renders it, so its instance answers are answers a learner reads.
const STORED_BODY: &str = r#"{"v":1,"topic_id":"perfect-squares","answer_kind":"numeric","statement":"Compute ${a}^{{2}}$.","params":{"a":{"kind":"int","low":1,"high":12}},"answer_expr":"a**2","solution_sketch":"${a} \\times {a}$ gives the answer.","hints":["What does squaring a number mean?"],"samples":[{"params":{"a":1},"expected":"1"},{"params":{"a":12},"expected":"144"}],"space_size":12}"#;

/// The tool arguments of a teach page that works a problem OUTSIDE the parameter
/// space of the template: `a` runs 1 to 12, so no instance asks for $15^2$ and no
/// served answer is 225.
fn teach_outside_the_bank() -> Value {
    json!({
        "concept": "Squaring a number multiplies it by itself.",
        "worked_example": {
            "problem": "Compute $15^2$.",
            "steps": [
                "Write the base twice with a multiplication sign between them.",
                "The product is 225."
            ]
        }
    })
}

/// The instances `served_instances` reads off the perfect-squares template, in
/// order: the two worked samples first (`a = 1` and `a = 12`), then the eight
/// instances drawn from the fixed gate seed, each pair once.
///
/// Eight draws over `a` in 1 to 12 repeat, so seven pairs stand here. The list is
/// a function of the document and the seed alone, so it is the same list on every
/// run.
const SERVED_INSTANCES: [(&str, &str); 7] = [
    ("Compute $1^{2}$.", "1"),
    ("Compute $12^{2}$.", "144"),
    ("Compute $9^{2}$.", "81"),
    ("Compute $6^{2}$.", "36"),
    ("Compute $11^{2}$.", "121"),
    ("Compute $7^{2}$.", "49"),
    ("Compute $10^{2}$.", "100"),
];

/// [`SERVED_INSTANCES`] as the type the gate reads.
fn served_list() -> Vec<ServedInstance> {
    SERVED_INSTANCES
        .iter()
        .map(|(problem, answer)| ServedInstance {
            problem: (*problem).to_owned(),
            answer: (*answer).to_owned(),
        })
        .collect()
}

/// The served instances of this knowledge point, read through the loop's own
/// reader.
async fn served(db: &TestDb, kp_id: &str) -> Vec<ServedInstance> {
    served_instances(&handle(db), kp_id).await.unwrap()
}

/// FIX2-M6-A, finding V1. The templates the gates read are the `approved` rows
/// AND the `pending` ones.
///
/// `cadus-worker author` writes all four kinds in ONE process, template first,
/// and every kind enters `content_store` as `pending`. A read of the approved
/// rows alone therefore answered an EMPTY list for every knowledge point of a
/// fresh curriculum, and the hint gate of that pass judged the ladder against the
/// exemplars alone. A `rejected` template still contributes nothing: a human
/// refused that body, and it serves nobody.
#[tokio::test]
async fn the_served_instances_are_the_instances_of_the_stored_templates() {
    TestDb::with(|db| async move {
        // No template row at all: A6 serves the exemplars, and nothing else.
        assert_eq!(served(&db, KP_KEY).await, Vec::<ServedInstance>::new());

        // A PENDING template is the material the reviewer is about to approve,
        // so its instances gate the page and the ladder of the same pass.
        seed_approved_template(&db.admin, "sha256:pending-one", KP_KEY, STORED_BODY).await;
        set_content_status(&db.admin, "sha256:pending-one", "pending").await;
        assert_eq!(served(&db, KP_KEY).await, served_list());

        // A second row with the same body adds no pair: the list holds each
        // problem and answer once.
        seed_approved_template(&db.admin, STORED_DIGEST, KP_KEY, STORED_BODY).await;
        assert_eq!(served(&db, KP_KEY).await, served_list());

        // A REJECTED template serves nothing, so it contributes nothing (C6).
        set_content_status(&db.admin, "sha256:pending-one", "rejected").await;
        set_content_status(&db.admin, STORED_DIGEST, "rejected").await;
        assert_eq!(served(&db, KP_KEY).await, Vec::<ServedInstance>::new());

        // The template of ANOTHER knowledge point is not read.
        set_content_status(&db.admin, STORED_DIGEST, "approved").await;
        assert_eq!(
            served(&db, "perfect-cubes/cubes").await,
            Vec::<ServedInstance>::new()
        );
    })
    .await;
}

/// Findings F2 and F15. A rung that states the answer of a rendered instance of
/// an approved template is refused, and the LITERAL message reaches the next
/// attempt as its feedback. The exemplar answer is 49; the rung names 81, which
/// only the template serves.
#[tokio::test]
async fn a_rung_that_names_a_template_instance_answer_is_refused() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![
            named_reply("emit_hint_ladder", &ladder_that_names_an_instance_answer()),
            named_reply("emit_hint_ladder", &ladder_arguments()),
        ])
        .await;
        seed_approved_template(&db.admin, STORED_DIGEST, KP_KEY, STORED_BODY).await;

        let report = author(&db, &fake, Kind::HintLadder, &spec()).await;

        // Attempt 1 is refused, attempt 2 carries the gate's own sentence and is
        // stored.
        assert_eq!(report.outcome, Outcome::Stored);
        assert_eq!(report.attempts, 2);
        assert_eq!(report.digest.as_deref(), Some(STORED_LADDER_DIGEST));

        let second = fake.user_message(1);
        assert!(
            second.contains(&format!("{RETRY_HEADER}\n    {NAMES_AN_INSTANCE_ANSWER}\n")),
            "the retry block did not carry the literal sentence: {second}"
        );

        assert_one_row(
            &db.admin,
            KP_KEY,
            "hint_ladder",
            STORED_LADDER_DIGEST,
            "pending",
            2,
        )
        .await;
    })
    .await;
}

/// The same rung passes when the knowledge point serves no template: 81 is then
/// no answer of the served material, so the gate has nothing to refuse. The
/// approved template is what makes the difference, and this test is the control.
#[tokio::test]
async fn the_same_rung_passes_when_no_template_is_approved() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![named_reply(
            "emit_hint_ladder",
            &ladder_that_names_an_instance_answer(),
        )])
        .await;

        let report = author(&db, &fake, Kind::HintLadder, &spec()).await;

        assert_eq!(report.outcome, Outcome::Stored);
        assert_eq!(report.attempts, 1);
        assert_eq!(
            content_rows_of_kind(&db.admin, KP_KEY, "hint_ladder")
                .await
                .len(),
            1
        );
    })
    .await;
}

/// FIX2-M6-A, finding V1. ACCEPTANCE, end to end.
///
/// `cadus-worker author` authors the four kinds in ONE process, template first,
/// and every document enters `content_store` as `pending` (C6). Before this fix
/// the two instruction gates read the APPROVED templates alone, so the ladder of
/// a fresh curriculum was judged with an empty instance set and a rung that
/// stated a rendered answer was stored `pending`. One click then served it.
///
/// The pass here is that pass: a template, a teach page, and a give-away ladder,
/// in the order of `prompt::KINDS`. The template is `pending` the whole time, and
/// the ladder is refused against it. The reviewer then approves the template, and
/// the ladder is still not in the table: it was never stored, so nothing serves
/// it.
#[tokio::test]
async fn one_pass_gates_the_ladder_against_the_pending_template_of_the_same_pass() {
    TestDb::with(|db| async move {
        let fake = FakeModel::start(vec![
            named_reply("emit_template", &good_arguments()),
            named_reply("emit_teach", &teach_outside_the_bank()),
            named_reply("emit_hint_ladder", &ladder_that_names_an_instance_answer()),
        ])
        .await;

        // Kind 1 of the pass. The row is `pending`: no human has read it yet.
        let template = author(&db, &fake, Kind::Template, &spec()).await;
        assert_eq!(template.outcome, Outcome::Stored);
        assert_eq!(template.digest.as_deref(), Some(STORED_DIGEST));
        assert_one_row(&db.admin, KP_KEY, "template", STORED_DIGEST, "pending", 1).await;

        // Kind 2. The page works $15^2$, which the template never renders, so no
        // served answer stands in its last step.
        let teach = author(&db, &fake, Kind::Teach, &spec()).await;
        assert_eq!(teach.outcome, Outcome::Stored);
        assert_eq!(teach.attempts, 1);

        // Kind 3. The rung states 81, which no exemplar answers and the PENDING
        // template renders. One attempt, so the pass declines with the gate's own
        // sentence and stores nothing.
        let ladder = author_within(&db, &fake, 1, Kind::HintLadder, &spec()).await;
        assert_eq!(ladder.outcome, Outcome::Declined);
        assert_eq!(ladder.attempts, 1);
        assert_eq!(ladder.digest, None);
        assert_eq!(
            ladder.decline.expect("a decline record").reasons,
            vec![format!("hint-answer: {NAMES_AN_INSTANCE_ANSWER}")]
        );
        assert!(
            content_rows_of_kind(&db.admin, KP_KEY, "hint_ladder")
                .await
                .is_empty()
        );

        // The reviewer approves the template. The give-away ladder is still not in
        // the table, so the L5 route has nothing to serve for this knowledge point.
        let decision = approve(Admin::new(&handle(&db)), STORED_DIGEST, None)
            .await
            .unwrap();
        assert_eq!(decision.status, "approved");
        assert!(
            content_rows_of_kind(&db.admin, KP_KEY, "hint_ladder")
                .await
                .is_empty()
        );
        assert_one_row(&db.admin, KP_KEY, "template", STORED_DIGEST, "approved", 1).await;
    })
    .await;
}
