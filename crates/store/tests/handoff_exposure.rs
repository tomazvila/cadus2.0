//! Lifetime ordinary hand-off identity is indexed by semantic finite case or digest.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::Instant;

use cadus_core::curriculum::FiniteCaseRole;
use cadus_core::event::{
    Event, Exposure, ItemSource, OrdinaryProblemServed, SchemaVersion, SessionStart, Timestamp,
};
use cadus_store::begin_tenant;
use cadus_store::state::{
    HandoffIdentity, advance_handoff_cursor, append_event, handoff_seen, load_learner_model,
    project_current,
};
use cadus_store::test_support::TestDb;

fn handoff(problem_id: &str, kp_id: &str, digest: &str, case_id: Option<&str>) -> Event {
    Event::OrdinaryProblemServed(OrdinaryProblemServed {
        ts: Timestamp::from_micros(1_767_225_600_000_000),
        session: Some("s1".to_owned()),
        v: SchemaVersion::current(),
        task_id: "s1-review-topic".to_owned(),
        problem_id: problem_id.to_owned(),
        kp_id: kp_id.to_owned(),
        item_digest: digest.to_owned(),
        item_source: ItemSource::Template,
        finite_case_id: case_id.map(str::to_owned),
        finite_case_role: case_id.map(|_| FiniteCaseRole::PracticeFresh),
        exposure: Exposure::First,
    })
}

#[tokio::test]
async fn a_finite_case_survives_rendering_changes_and_digest_fallback_stays_exact() {
    TestDb::with(|db| async move {
        let user = db.seed_user("handoff-exposure@example.test").await;
        let mut tx = begin_tenant(&db.app, user).await.unwrap();

        assert!(
            !handoff_seen(
                &mut tx,
                user,
                HandoffIdentity::Finite {
                    kp_id: "topic/kp1",
                    case_id: "case-a"
                },
            )
            .await
            .unwrap()
        );
        append_event(
            &mut tx,
            user,
            &handoff("p1", "topic/kp1", "old-render-digest", Some("case-a")),
            Some("ordinary-problem-served:p1"),
        )
        .await
        .unwrap();

        assert!(
            handoff_seen(
                &mut tx,
                user,
                HandoffIdentity::Finite {
                    kp_id: "topic/kp1",
                    case_id: "case-a"
                },
            )
            .await
            .unwrap(),
            "the same semantic case repeats after its rendering changes"
        );
        assert!(
            !handoff_seen(
                &mut tx,
                user,
                HandoffIdentity::Finite {
                    kp_id: "topic/kp1",
                    case_id: "case-b"
                },
            )
            .await
            .unwrap()
        );
        assert!(
            !handoff_seen(
                &mut tx,
                user,
                HandoffIdentity::Finite {
                    kp_id: "other/kp1",
                    case_id: "case-a"
                },
            )
            .await
            .unwrap(),
            "case ids are scoped by knowledge point"
        );
        assert!(
            handoff_seen(&mut tx, user, HandoffIdentity::Digest("old-render-digest"),)
                .await
                .unwrap()
        );
        assert!(
            !handoff_seen(&mut tx, user, HandoffIdentity::Digest("new-render-digest"),)
                .await
                .unwrap()
        );
    })
    .await;
}

#[tokio::test]
async fn a_handoff_at_twenty_thousand_events_keeps_the_cursor_current_within_l1() {
    TestDb::with(|db| async move {
        let run = common::long_log::seed(&db).await;
        let (input, _) = run.views();
        let start = Instant::now();
        let mut tx = begin_tenant(&run.app, run.user).await.unwrap();
        assert!(
            !handoff_seen(
                &mut tx,
                run.user,
                HandoffIdentity::Finite {
                    kp_id: common::long_log::KP_ID,
                    case_id: "long-log-case",
                },
            )
            .await
            .unwrap()
        );
        let seq = append_event(
            &mut tx,
            run.user,
            &handoff(
                "long-log-p1",
                common::long_log::KP_ID,
                "long-log-handoff",
                Some("long-log-case"),
            ),
            Some("ordinary-problem-served:long-log-p1"),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            handoff_seen(
                &mut tx,
                run.user,
                HandoffIdentity::Finite {
                    kp_id: common::long_log::KP_ID,
                    case_id: "long-log-case",
                },
            )
            .await
            .unwrap()
        );
        let plan = sqlx::query_scalar::<_, String>(
            r#"EXPLAIN (COSTS OFF)
                SELECT 1 FROM events
                 WHERE user_id = $1
                   AND ordinary_finite_case_id IS NOT NULL
                   AND ordinary_kp_id = $2
                   AND ordinary_finite_case_id = $3"#,
        )
        .bind(run.user)
        .bind(common::long_log::KP_ID)
        .bind("long-log-case")
        .fetch_all(&mut *tx)
        .await
        .unwrap()
        .join("\n");
        println!("finite hand-off plan:\n{plan}");
        assert!(
            plan.contains("events_ordinary_handoff_finite_identity"),
            "the finite lifetime lookup must use its partial expression index:\n{plan}",
        );
        assert!(
            advance_handoff_cursor(&mut tx, run.user, seq)
                .await
                .unwrap()
        );
        tx.commit().await.unwrap();
        let elapsed = start.elapsed();

        assert_eq!(seq, common::long_log::SEEDED_EVENTS as i64 + 1);
        let mut verify = begin_tenant(&run.app, run.user).await.unwrap();
        let projection = project_current(&mut verify, run.user, &input)
            .await
            .unwrap();
        assert_eq!(
            projection.through_seq, seq,
            "the hand-off and cursor commit together"
        );
        assert!(
            !projection.replayed,
            "the cached projection resumes at the exact head"
        );
        let non_neutral = Event::SessionStart(SessionStart {
            ts: Timestamp::from_micros(1_767_225_600_000_001),
            session: Some("new-session".to_owned()),
            v: SchemaVersion::current(),
        });
        let next_seq = append_event(&mut verify, run.user, &non_neutral, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(next_seq, seq + 1);
        assert!(
            !advance_handoff_cursor(&mut verify, run.user, next_seq)
                .await
                .unwrap(),
            "an adjacent learning event is never treated as a neutral hand-off",
        );
        assert_eq!(
            load_learner_model(&mut verify, run.user)
                .await
                .unwrap()
                .unwrap()
                .through_seq,
            seq,
            "the rejected event type leaves the cached cursor unmoved",
        );
        verify.rollback().await.unwrap();
        assert!(
            elapsed.as_nanos() < cadus_testkit::bench::budget(100_000_000),
            "the hand-off transaction took {elapsed:?} over a 20,000-event history",
        );
        println!("ordinary hand-off over 20,000 events: {elapsed:?}");
    })
    .await;
}

#[tokio::test]
async fn a_twenty_thousand_handoff_history_uses_the_full_finite_identity_index() {
    TestDb::with(|db| async move {
        let user = db.seed_user("handoff-index@example.test").await;
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        sqlx::query(
            r#"INSERT INTO events
                   (user_id, seq, ts, type, session_id, v, attempt_id, payload)
               SELECT $1, n, now(), 'ordinary_problem_served', NULL, 1, NULL,
                      jsonb_build_object(
                          'kp_id', CASE WHEN n = 20000 THEN 'target/kp1'
                                        ELSE 'other/kp' || (n % 17)::text END,
                          'finite_case_id', CASE WHEN n = 20000 THEN 'late-case'
                                                ELSE 'case-' || (n % 101)::text END,
                          'item_digest', 'digest-' || n::text)
                 FROM generate_series(1, 20000) AS n"#,
        )
        .bind(user)
        .execute(&mut *tx)
        .await
        .unwrap();

        let miss_started = Instant::now();
        assert!(
            !handoff_seen(
                &mut tx,
                user,
                HandoffIdentity::Finite {
                    kp_id: "target/kp1",
                    case_id: "missing-case",
                },
            )
            .await
            .unwrap()
        );
        let miss_elapsed = miss_started.elapsed();

        let hit_started = Instant::now();
        assert!(
            handoff_seen(
                &mut tx,
                user,
                HandoffIdentity::Finite {
                    kp_id: "target/kp1",
                    case_id: "late-case",
                },
            )
            .await
            .unwrap()
        );
        let hit_elapsed = hit_started.elapsed();

        let plan = sqlx::query_scalar::<_, String>(
            r#"EXPLAIN (COSTS OFF)
                SELECT 1 FROM events
                 WHERE user_id = $1
                   AND ordinary_finite_case_id IS NOT NULL
                   AND ordinary_kp_id = $2
                   AND ordinary_finite_case_id = $3"#,
        )
        .bind(user)
        .bind("target/kp1")
        .bind("late-case")
        .fetch_all(&mut *tx)
        .await
        .unwrap()
        .join("\n");
        println!(
            "20,000 ordinary hand-offs: miss {miss_elapsed:?}; late hit {hit_elapsed:?}\n{plan}"
        );
        assert!(
            plan.contains("events_ordinary_handoff_finite_identity")
                && plan.contains("ordinary_kp_id")
                && plan.contains("ordinary_finite_case_id"),
            "the complete semantic identity must be an index condition:\n{plan}",
        );
        let budget = cadus_testkit::bench::budget(100_000_000);
        assert!(
            miss_elapsed.as_nanos() < budget,
            "miss took {miss_elapsed:?}"
        );
        assert!(
            hit_elapsed.as_nanos() < budget,
            "late hit took {hit_elapsed:?}"
        );
        tx.rollback().await.unwrap();
    })
    .await;
}
