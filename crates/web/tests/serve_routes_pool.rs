//! M5 U7 acceptance, the A6 pool miss: an empty pool falls back to the
//! authored exemplars, never to a model.
//!
//! The header of `serve_routes.rs` gives the requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::pool::operator_flags;
use cadus_store::test_support::TestDb;
use common::{
    EXEMPLAR_TEXT_2, EXPECTED_ANSWER, KEY, LESSON, POOL_TEXT, PROBLEM_TEXT, close_live,
    drill_app as app, learner_with_drill_pool_row, seed_learner, seed_open_session, serve_ok,
    stored_state,
};

/// Fail the test if `text` is not one of the two authored statements of
/// `addition/kp1`.
fn assert_authored(text: &str) {
    assert!(
        text == PROBLEM_TEXT || text == EXEMPLAR_TEXT_2,
        "the serve dealt {text:?}, which is not an authored exemplar"
    );
}

/// `serving-1.0-spec.md` section 7.2. An empty pool must NOT generate. The serve
/// instantiates the knowledge point's authored exemplars in process, writes the
/// first pool rows of the pair with `source = 'exemplar'`, claims one, and the
/// A6 operator view then reports the fallback.
#[tokio::test]
async fn a_pool_miss_instantiates_an_exemplar_and_raises_the_a6_flag() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "fallback@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;

        let served = serve_ok(&app, user, LESSON).await;
        // `addition/kp1` authors two exemplars. One batch takes one
        // `created_at`, so the pop orders the two by their random ids: the rule
        // is that ONE of the authored statements is served, never that a
        // position wins (`insert_batch`, "the order inside one batch").
        assert_authored(served["text"].as_str().unwrap());
        assert!(
            !serde_json::to_string(&served)
                .unwrap()
                .contains(EXPECTED_ANSWER),
            "the fallback leaked the exemplar answer"
        );

        let rows = sqlx::query!(
            r#"
            SELECT source AS "source!", content_digest,
                   (claimed_at IS NOT NULL) AS "claimed!"
            FROM serving_pool WHERE user_id = $1 AND kp_id = $2
            ORDER BY claimed_at NULLS LAST, id
            "#,
            user,
            KEY
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2, "the whole exemplar list did not go in");
        assert!(rows.iter().all(|row| row.source == "exemplar"));
        assert!(rows.iter().all(|row| row.content_digest.is_none()));
        assert_eq!(
            rows.iter().filter(|row| row.claimed).count(),
            1,
            "the serve claimed more than one row"
        );

        let flags = operator_flags(&db.admin).await.unwrap();
        let flag = flags
            .iter()
            .find(|flag| flag.kp_id == KEY)
            .unwrap_or_else(|| panic!("the A6 view lost {KEY}"));
        assert_eq!(flag.approved_templates, 0);
        assert!(flag.needs_template, "the A6 flag did not rise");
        assert!(
            flag.last_exemplar_at.is_some(),
            "the A6 view records no exemplar serve"
        );
        assert_eq!(flag.pool_depth, 1);

        // The D5 windows recorded the instance, so the next serve avoids it.
        let stored = stored_state(&db, user).await;
        assert_eq!(stored.ring("addition").len(), 1);
        assert_eq!(stored.memory(LESSON).len(), 1);
    })
    .await;
}

/// The D-S6 row of one serve names both topics and holds the authored solution.
///
/// `serve_topic` is the topic the STATEMENT came from and `topic` is the topic
/// the attempt records against. They are the same topic for a lesson, and the
/// hint ladder and the pre-authored diagnosis key on the first one (M5 review 1,
/// findings F10 and F16). `solution_sketch` is the exemplar's own worked
/// solution, which the grade reply of unit U8 reveals after the attempt commits
/// (findings F2 and F11).
///
/// Hard Rule 1 still holds: neither value leaves the D-S6 row on this route.
#[tokio::test]
async fn a_serve_stores_the_serve_topic_and_the_authored_solution() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "sketch@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;

        let served = serve_ok(&app, user, LESSON).await;
        let text = served["text"].as_str().unwrap().to_string();
        // One batch takes one `created_at`, so either authored exemplar may win
        // the pop. The solution is the one the WINNER authored.
        let solution = if text == PROBLEM_TEXT {
            "Add the parts to reach 13.5."
        } else {
            "Add the parts to reach 13.25."
        };

        let stored = stored_state(&db, user).await;
        let live = &stored.served[LESSON];
        assert_eq!(live.topic.as_deref(), Some("addition"));
        assert_eq!(live.serve_topic.as_deref(), Some("addition"));
        assert_eq!(live.kp.as_deref(), Some("kp1"));
        assert_eq!(
            live.solution_sketch.as_deref(),
            Some(solution),
            "the D-S6 row holds no authored solution"
        );

        // Trap W7: scan the RAW payload. The serve reveals neither.
        let raw = serde_json::to_string(&served).unwrap();
        assert!(
            !raw.contains("solution"),
            "the serve leaked a solution: {raw}"
        );
        assert!(
            !raw.contains("serve_topic"),
            "the serve leaked the serve topic: {raw}"
        );
    })
    .await;
}

/// The rotation never runs dry. A pair whose whole authored list is claimed
/// serves the least recently served exemplar again, and `last_exemplar_at` moves
/// with it, so the A6 view keeps showing a knowledge point that still needs a
/// template.
#[tokio::test]
async fn the_exemplar_rotation_serves_the_list_again_when_it_is_exhausted() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "rotate@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;

        let mut texts = Vec::new();
        for _ in 0..4 {
            let served = serve_ok(&app, user, LESSON).await;
            texts.push(served["text"].as_str().unwrap().to_string());
            // Close the live problem so the next call serves a new one.
            close_live(&db, user, LESSON).await;
        }

        // The first two serves cover both authored statements: the ring blocks
        // the one already served.
        for text in &texts {
            assert_authored(text);
        }
        assert_ne!(texts[0], texts[1], "the second serve repeated the first");
        // Both statements are used now, so the rotation repeats the LEAST
        // recently served one and keeps alternating.
        assert_eq!(texts[2], texts[0]);
        assert_eq!(texts[3], texts[1]);

        // The pool still holds exactly the two authored rows: the rotation
        // writes no duplicate (the A5 unique index).
        let rows = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM serving_pool WHERE user_id = $1 AND kp_id = $2"#,
            user,
            KEY
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(rows, 2);
    })
    .await;
}

/// The pop wins over the fallback: a pair with an unclaimed pool row serves that
/// row and writes no exemplar row at all.
#[tokio::test]
async fn the_serve_pops_the_pool_before_it_falls_back() {
    TestDb::with(|db| async move {
        let user = learner_with_drill_pool_row(&db, "pop@example.com").await;
        let app = app(&db);

        let served = serve_ok(&app, user, LESSON).await;
        assert_eq!(served["text"], POOL_TEXT);

        let sources = sqlx::query_scalar!(
            r#"SELECT source AS "source!" FROM serving_pool WHERE user_id = $1"#,
            user
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();
        assert_eq!(sources, vec!["template".to_string()]);
    })
    .await;
}
