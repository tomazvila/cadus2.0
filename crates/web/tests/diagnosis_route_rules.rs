//! Part of `tests/diagnosis_route.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{LESSON, put_state, scrape, stored_state};

use common::BASE_US;

use common::diagnosis::*;

use cadus_core::curriculum::AnswerKind;
use cadus_store::diagnosis::JobRow;
use cadus_web::diagnosis::{job_view, match_distractor};
use sqlx::types::chrono::{DateTime, Utc};

// --------------------------------------------------------------------------- //
// The poll fallback rules (spec section 2.1, last paragraph)
// --------------------------------------------------------------------------- //

/// One row, built by hand, so the reader is measured and not the writer.
fn row(status: &str, age_secs: i64, result: Option<Value>) -> JobRow {
    JobRow {
        id: Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
        attempt_id: "t-1".to_string(),
        status: status.to_string(),
        result,
        created_at: DateTime::<Utc>::from_timestamp_micros(BASE_US - age_secs * 1_000_000).unwrap(),
    }
}

/// "A job still `pending` after 30 s is reported `failed`, never left open."
///
/// The five stored statuses map onto the four wire statuses, `running` reads as
/// `pending`, and the deadline turns a stuck job into a reported failure. Every
/// value below is a literal.
#[test]
fn the_wire_status_follows_the_poll_rules() {
    let now = DateTime::<Utc>::from_timestamp_micros(BASE_US).unwrap();

    assert_eq!(job_view(&row("pending", 0, None), now).status, "pending");
    assert_eq!(job_view(&row("running", 29, None), now).status, "pending");
    assert_eq!(job_view(&row("pending", 30, None), now).status, "pending");
    assert_eq!(job_view(&row("pending", 31, None), now).status, "failed");
    assert_eq!(job_view(&row("running", 600, None), now).status, "failed");
    assert_eq!(job_view(&row("failed", 1, None), now).status, "failed");
    assert_eq!(job_view(&row("capped", 1, None), now).status, "capped");
    assert_eq!(job_view(&row("done", 9_000, None), now).status, "ready");
    // A status no build writes fails closed, so no client is left open on it.
    assert_eq!(job_view(&row("sending", 1, None), now).status, "failed");

    // An unfinished row carries no tags and no prose, whatever `result` holds.
    let stuck = job_view(
        &row(
            "pending",
            99,
            Some(json!({ "error_tags": ["sign-error"], "prose": "half-written" })),
        ),
        now,
    );
    assert_eq!(
        stuck.body,
        json!({
            "id": "33333333-3333-4333-8333-333333333333",
            "status": "failed",
            "error_tags": [],
        })
    );

    // A finished row carries the three fields of section 2.1.
    let done = job_view(
        &row(
            "done",
            5,
            Some(json!({
                "error_tags": ["wrong-method"],
                "prose": "You used the area formula.",
                "model_id": "deepseek/deepseek-v4-pro",
            })),
        ),
        now,
    );
    assert_eq!(
        done.body,
        json!({
            "id": "33333333-3333-4333-8333-333333333333",
            "status": "ready",
            "error_tags": ["wrong-method"],
            "prose": "You used the area formula.",
            "model_id": "deepseek/deepseek-v4-pro",
        })
    );
}

// --------------------------------------------------------------------------- //
// The distractor match (spec sections 5.3 and 6.2)
// --------------------------------------------------------------------------- //

/// The match runs through the checker, and the tag runs through the vocabulary.
///
/// `13` and `13.0` are the same wrong answer, exactly as they would be the same
/// right one. A tag the vocabulary lacks is dropped silently — the 1.0 lesson of
/// `prompts.py:529-536` — and a hit that keeps neither a tag nor prose is no hit
/// at all, so the caller enqueues instead of answering with an empty document.
#[test]
fn the_distractor_match_reads_the_checker_and_the_vocabulary() {
    let vocabulary: Vec<String> = VOCABULARY.iter().map(|tag| (*tag).to_string()).collect();
    let body = json!({
        "v": 1,
        "distractors": [
            { "answer": "13", "error_tag": "arithmetic-slip", "note": "You dropped the half." },
            { "answer": "8", "error_tag": "not-a-real-tag", "note": "You stopped early." },
            { "answer": "5.5", "error_tag": "made-up" },
        ]
    });

    let hit = match_distractor(&body, "13.0", AnswerKind::Numeric, &vocabulary).unwrap();
    assert_eq!(hit.error_tags, vec!["arithmetic-slip".to_string()]);
    assert_eq!(hit.prose.as_deref(), Some("You dropped the half."));

    // The tag is outside the vocabulary; the prose still stands.
    let dropped = match_distractor(&body, "8", AnswerKind::Numeric, &vocabulary).unwrap();
    assert_eq!(dropped.error_tags, Vec::<String>::new());
    assert_eq!(dropped.prose.as_deref(), Some("You stopped early."));

    // Neither a tag nor prose survives, so this is not a usable diagnosis.
    assert_eq!(
        match_distractor(&body, "5.5", AnswerKind::Numeric, &vocabulary),
        None
    );
    // No distractor names this answer.
    assert_eq!(
        match_distractor(&body, "41", AnswerKind::Numeric, &vocabulary),
        None
    );
    // A body with no distractor list matches nothing and raises nothing.
    assert_eq!(
        match_distractor(&json!({ "v": 1 }), "13", AnswerKind::Numeric, &vocabulary),
        None
    );
    assert_eq!(
        match_distractor(&json!("nonsense"), "13", AnswerKind::Numeric, &vocabulary),
        None
    );
}

// --------------------------------------------------------------------------- //
// M5 U11: the one diagnosis result that writes no row (T6, spec section 7)
// --------------------------------------------------------------------------- //

/// A pre-authored hit counts `ready_preauthored`, and it is the only label of
/// `cadus_diagnosis_jobs_total` that no row can carry.
///
/// The hit writes NO job row (spec section 6.2), so the queue holds nothing to
/// count and the counter of this process is the whole record of it. `enqueued`
/// stays at zero in the same scrape, which is the saving the label exists to
/// show.
#[tokio::test]
async fn a_preauthored_hit_counts_ready_preauthored_and_enqueues_nothing() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "u11-counted@example.test").await;
        seed_slip_distractor(&db, "u11-digest-ready").await;

        let (status, body) = answer(&app, user, DISTRACTOR_ANSWER).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["diagnosis"]["status"], json!("ready"));

        let text = scrape(&app).await;
        assert!(
            text.contains("cadus_diagnosis_jobs_total{result=\"ready_preauthored\"} 1\n"),
            "the scrape carries no pre-authored count:\n{text}"
        );
        assert!(
            text.contains("cadus_diagnosis_jobs_total{result=\"enqueued\"} 0\n"),
            "a pre-authored hit must enqueue nothing:\n{text}"
        );
    })
    .await;
}

/// A checker hit is `ready` at once, with the tag and the sentence, and it
/// writes no job row (D-G1).
#[tokio::test]
async fn a_checker_hit_is_ready_and_enqueues_nothing() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "g1-checker@example.test").await;

        let (status, body) = answer(&app, user, "-13.5").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["diagnosis"]["status"], json!("ready"));
        assert_eq!(body["diagnosis"]["error_tags"], json!(["sign-error"]));
        assert!(
            body["diagnosis"]["prose"]
                .as_str()
                .is_some_and(|prose| !prose.is_empty()),
            "{body}"
        );
        assert!(jobs_of(&db, user).await.is_empty());

        let text = scrape(&app).await;
        assert!(
            text.contains("cadus_diagnosis_jobs_total{result=\"ready_checker\"} 1\n"),
            "the scrape carries no checker count:\n{text}"
        );
        assert!(
            text.contains("cadus_diagnosis_jobs_total{result=\"enqueued\"} 0\n"),
            "a checker hit must enqueue nothing:\n{text}"
        );
    })
    .await;
}

/// A contract that does not compare with the one stored key skips the checker
/// rule: a sign flip of a `Property` item gets `pending`, not `ready`, and `ready_checker` stays 0.
#[tokio::test]
async fn a_contract_with_no_single_key_skips_the_checker() {
    TestDb::with(|db| async move {
        {
            let (email, contract) = (
                "g1-property@example.test",
                r#"{"kind":"property","check":"divisor_count","args":{"n":3}}"#,
            );
            let mut live = common::lesson_problem(5.0, "kp1", Vec::new());
            live.expected.answer = "9".to_owned();
            live.answer_kind = Some("expression".to_owned());
            live.expected.answer_contract =
                Some(serde_json::from_str::<cadus_core::answer::AnswerContract>(contract).unwrap());
            let user = common::lesson_learner(&db, email, live).await;
            let app = common::lesson_app(&db);

            let reply = common::answer_task_ok(
                &app,
                user,
                LESSON,
                json!({"problem_id": common::PROBLEM_ID, "answer": "-9"}),
            )
            .await;
            assert_eq!(reply["diagnosis"]["status"], json!("pending"), "{reply}");
            let text = scrape(&app).await;
            assert!(
                text.contains("cadus_diagnosis_jobs_total{result=\"ready_checker\"} 0\n"),
                "{text}"
            );
        }
    })
    .await;
}

/// An authored distractor wins over the checker rule: `14.5` is one away from
/// the key, and the authored note is the one the learner reads.
#[tokio::test]
async fn an_authored_distractor_wins_over_the_checker() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "g1-authored@example.test").await;
        seed_distractors(
            &db,
            "g1-digest-authored",
            &json!({
                "v": 1,
                "distractors": [
                    { "answer": "14.5",
                      "error_tag": "wrong-method",
                      "note": "You added the whole step twice." }
                ]
            }),
        )
        .await;

        let (status, body) = answer(&app, user, "14.5").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["diagnosis"]["status"], json!("ready"));
        assert_eq!(body["diagnosis"]["error_tags"], json!(["wrong-method"]));
        assert_eq!(
            body["diagnosis"]["prose"],
            json!("You added the whole step twice.")
        );

        let text = scrape(&app).await;
        assert!(
            text.contains("cadus_diagnosis_jobs_total{result=\"ready_checker\"} 0\n"),
            "{text}"
        );
        assert!(
            text.contains("cadus_diagnosis_jobs_total{result=\"ready_preauthored\"} 1\n"),
            "{text}"
        );
    })
    .await;
}

/// A replayed request gives the same checker field and writes no job row.
///
/// The log is seeded with a gap, as in `a_rolled_back_grade_leaves_no_job_row`:
/// the first answer takes `-2`, and the second computes `-3` and meets the
/// standing row.
#[tokio::test]
async fn a_replayed_checker_hit_is_the_same_field() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "g1-replay@example.test").await;
        seed_attempt(&db, user, 2, ATTEMPT_ID_3).await;

        let (status, first) = answer(&app, user, "-13.5").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(first["diagnosis"]["status"], json!("ready"));

        let mut scratch = stored_state(&db, user).await;
        scratch.served.insert(LESSON.to_string(), live_problem());
        put_state(&db, user, &scratch).await;

        let (status, second) = answer(&app, user, "-13.5").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(second["task_status"], json!("already_recorded"));
        assert_eq!(second["diagnosis"], first["diagnosis"]);
        assert!(jobs_of(&db, user).await.is_empty());
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The enqueue that fails
// --------------------------------------------------------------------------- //

/// A job row that does not write fails the grade, and the log keeps no
/// attempt of it.
#[tokio::test]
async fn an_enqueue_that_fails_is_500_and_writes_no_job() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "u9-enqueue-fault@example.test").await;
        seed_distractors(&db, "u9-digest-fault", &json!({"v": 1, "distractors": []})).await;
        common::fail_writes(&db, "diagnosis_jobs", "true").await;

        let (status, body) = answer(&app, user, UNKNOWN_MISS).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
        assert_eq!(body["error"]["code"], "internal_error");
        assert_eq!(jobs_of(&db, user).await.len(), 0);
    })
    .await;
}
