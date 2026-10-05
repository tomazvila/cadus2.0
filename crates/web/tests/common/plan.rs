//! The curricula, the learner models, and the authored content of the task
//! route tests: what puts a review, a drill, or a component question into the
//! plan of the open session.

use std::collections::BTreeMap;

use axum::Router;
use cadus_core::curriculum::{Curriculum, PrereqEdge, Slug, Topic, review_context_digest};
use cadus_core::event::{Timestamp, TopicStatus};
use cadus_core::learner::{LearnerModel, TopicState};
use cadus_core::pool::{PoolAnswer, PoolProblem};
use cadus_store::test_support::TestDb;
use cadus_web::state::{TaskProgress, WebState};
use serde_json::{Value, json};
use sqlx::types::Uuid;

use super::{
    BASE_US, EXPECTED_ANSWER, KEY, PROBLEM_TEXT, SESSION, app_with_content, exemplar,
    exemplar_with_solution, kp, one_unit_curriculum, put_state, seed_cached_model, seed_learner,
    seed_open_session, seed_pool_row, topic,
};

/// The task id of the `addition` review.
pub const REVIEW: &str = "s_2026-01-01a-review-addition";

/// The task id of the `tables` drill (`assign_ids`: `{session}-{type}-{topic}`).
pub const DRILL: &str = "s_2026-01-01a-drill-tables";

/// The session the cadence test opens after it closes [`SESSION`].
pub const SESSION_2: &str = "s_2026-01-02a";

/// The task id the `tables` drill takes in [`SESSION_2`].
pub const DRILL_2: &str = "s_2026-01-02a-drill-tables";

/// The statement of the second authored exemplar of `addition/kp1`.
pub const EXEMPLAR_TEXT_2: &str = "Compute 9 + 4.25.";

/// The statement of the first authored exemplar of `tables/kp1`.
pub const DRILL_TEXT: &str = "Compute 8 x 7 + 0.5.";

/// The statement of the pool row the pop tests seed.
pub const POOL_TEXT: &str = "Compute 21 + 34.75.";

/// The answer of that pool row. It must never appear in an HTTP body.
pub const POOL_ANSWER: &str = "55.75";

/// The task id of the `counting` lesson of the component fixture.
pub const COUNTING_LESSON: &str = "s_2026-01-01a-lesson-counting";

/// The serving key of the component skill.
pub const COMPONENT_KEY: &str = "counting/kp1";

/// The serving key of the parent topic of the review.
pub const PARENT_KEY: &str = "addition/kp1";

/// The statement of the authored exemplar of `counting/kp1`.
pub const COMPONENT_TEXT: &str = "Count on from 3 by 4.";

/// The answer of that exemplar.
pub const COMPONENT_ANSWER: &str = "7";

/// The authored solution of that exemplar.
pub const COMPONENT_SOLUTION: &str = "Count 4, 5, 6, 7.";

/// The authored solution of the `addition/kp1` exemplar of the component fixture.
pub const PARENT_SOLUTION: &str = "Add 8 and 5.5 to reach 13.5.";

/// A drill-tagged topic. `schedule_drills` reads `drill` and offers the topic
/// once the learner masters it and stays under the automaticity bar.
pub fn drill_topic(id: &str, points: Vec<cadus_core::curriculum::KnowledgePoint>) -> Topic {
    Topic {
        drill: true,
        ..topic(id, points)
    }
}

/// The three-topic fixture of the serve route tests. `addition` authors two
/// knowledge points and two exemplars on the first one, `subtraction` authors
/// one knowledge point with no exemplar, and `tables` is the drill-tagged
/// topic of the cadence tests.
pub fn drill_curriculum() -> Curriculum {
    one_unit_curriculum(vec![
        topic(
            "addition",
            vec![
                kp(
                    "kp1",
                    vec![
                        exemplar(PROBLEM_TEXT, EXPECTED_ANSWER),
                        exemplar(EXEMPLAR_TEXT_2, "13.25"),
                    ],
                ),
                kp("kp2", vec![exemplar("Compute 40 + 2.5.", "42.5")]),
            ],
        ),
        topic("subtraction", vec![kp("kp1", vec![])]),
        drill_topic(
            "tables",
            vec![kp(
                "kp1",
                vec![
                    exemplar(DRILL_TEXT, "56.5"),
                    exemplar("Compute 6 x 7 + 0.25.", "42.25"),
                ],
            )],
        ),
    ])
}

/// The router of a serve route test, with the three-topic fixture loaded.
pub fn drill_app(db: &TestDb) -> Router {
    app_with_content(db, drill_curriculum())
}

/// The same router with the D-F5 readiness rule ON.
///
/// Every other router of these tests turns the rule off, because a test
/// database approves no document and the rule would then block every lesson.
/// This one is the rule's own fixture.
pub fn gated_app(db: &TestDb) -> Router {
    cadus_web::create_app(
        cadus_web::AppState::new(cadus_store::Db::new(
            db.app.clone(),
            cadus_store::DEFAULT_CLIENT_TIMEOUT_MS,
        ))
        .with_content(std::sync::Arc::new(cadus_web::state::Content::new(
            drill_curriculum(),
        ))),
    )
}

/// The two-topic fixture of the component tests.
///
/// `counting` is a KEY prerequisite of `addition`, so `review_mix` of `addition`
/// is `["kp1", "component:counting"]` and serve index 1 of the review draws its
/// statement from `counting` while the attempt records against `addition`.
pub fn component_curriculum() -> Curriculum {
    let mut addition = topic(
        "addition",
        vec![kp(
            "kp1",
            vec![exemplar_with_solution(
                PROBLEM_TEXT,
                EXPECTED_ANSWER,
                PARENT_SOLUTION,
            )],
        )],
    );
    addition.prerequisites = vec![PrereqEdge {
        id: Slug::new("counting").unwrap(),
        weight: 1.0,
        key: true,
    }];
    one_unit_curriculum(vec![
        topic(
            "counting",
            vec![kp(
                "kp1",
                vec![exemplar_with_solution(
                    COMPONENT_TEXT,
                    COMPONENT_ANSWER,
                    COMPONENT_SOLUTION,
                )],
            )],
        ),
        addition,
    ])
}

/// The router of a component test, with the two-topic fixture loaded.
pub fn component_app(db: &TestDb) -> Router {
    app_with_content(db, component_curriculum())
}

/// Cache a learner model whose one topic is in the learning state, with a
/// review started at `t0_us` on an interval of `interval_days`.
pub async fn seed_topic_model(
    db: &TestDb,
    user: Uuid,
    topic: &str,
    t0_us: i64,
    interval_days: f64,
) {
    let mut topics: BTreeMap<String, TopicState> = BTreeMap::new();
    topics.insert(
        topic.to_string(),
        TopicState {
            status: TopicStatus::Learning,
            rep_num: 1.0,
            memory_base: 1.0,
            t0: Some(Timestamp::from_micros(t0_us)),
            interval_days,
            ability: 0.6,
            ..TopicState::default()
        },
    );
    let model = LearnerModel {
        topics,
        ..LearnerModel::default()
    };
    seed_cached_model(db, user, &model, 1).await;
}

/// Write a learner model whose `addition` topic is a review that came due.
pub async fn seed_due_review(db: &TestDb, user: Uuid) {
    seed_topic_model(db, user, "addition", BASE_US - 400 * 86_400_000_000, 1.0).await;
}

/// Write a learner model whose `tables` topic is mastered and still under the
/// automaticity bar, which is what `schedule_drills` asks for.
pub async fn seed_drill_due(db: &TestDb, user: Uuid) {
    seed_topic_model(db, user, "tables", BASE_US, 30.0).await;
}

/// Approve one authored document for `key`, `days_ago` days before now.
///
/// `approved_template` takes the NEWEST approval, so a document approved a day
/// earlier is the one a later approval supersedes.
pub async fn seed_content_aged(
    db: &TestDb,
    key: &str,
    kind: &str,
    digest: &str,
    body: Value,
    days_ago: i32,
) {
    seed_content_aged_for(db, &drill_curriculum(), key, kind, digest, body, days_ago).await;
}

/// Approve one authored document against the exact curriculum loaded by a test.
pub async fn seed_content_aged_for(
    db: &TestDb,
    curriculum: &Curriculum,
    key: &str,
    kind: &str,
    digest: &str,
    body: Value,
    days_ago: i32,
) {
    let curriculum_digest = review_context_digest(curriculum).unwrap();
    let review_engine_digest = cadus_core::review_engine::DIGEST;
    sqlx::query(
        "INSERT INTO content_store
            (digest, kp_id, kind, body, status, approved_at,
             approved_template_context_digest, approved_curriculum_digest,
             approved_review_engine_digest)
         VALUES ($1, $2, $3, $4, 'approved', now() - make_interval(days => $5),
                 CASE WHEN $3 IN ('teach', 'hint_ladder')
                      THEN public.cadus_template_context($2, NULL, $6, $7) ELSE NULL END,
                 $6, $7)",
    )
    .bind(digest)
    .bind(key)
    .bind(kind)
    .bind(body)
    .bind(days_ago)
    .bind(curriculum_digest)
    .bind(review_engine_digest)
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Put one PENDING document for `key` into the store. A pending document is
/// not an approved one (C6), so no route serves it.
pub async fn seed_pending_content(db: &TestDb, key: &str, kind: &str, digest: &str, body: Value) {
    sqlx::query(
        "INSERT INTO content_store (digest, kp_id, kind, body, status)
         VALUES ($1, $2, $3, $4, 'pending')",
    )
    .bind(digest)
    .bind(key)
    .bind(kind)
    .bind(body)
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Approve one authored document for `key`, now.
pub async fn seed_content(db: &TestDb, key: &str, kind: &str, digest: &str, body: Value) {
    seed_content_aged(db, key, kind, digest, body, 0).await;
}

/// Approve one authored document against the exact curriculum loaded by a test.
pub async fn seed_content_for(
    db: &TestDb,
    curriculum: &Curriculum,
    key: &str,
    kind: &str,
    digest: &str,
    body: Value,
) {
    seed_content_aged_for(db, curriculum, key, kind, digest, body, 0).await;
}

/// One template row of the pool, as the draw of one document wrote it.
pub struct TemplateRow<'a> {
    /// The serving key.
    pub key: &'a str,
    /// The `content_store` document the row was drawn from, or `None`.
    pub digest: Option<&'a str>,
    /// The canonical strings the draw bound.
    pub bindings: BTreeMap<String, String>,
    /// The rendered statement.
    pub text: &'a str,
    /// The expected answer.
    pub answer: &'a str,
    /// The instance hash the D5 windows record.
    pub hash: &'a str,
}

/// Put one unclaimed template row into the pool of `user`.
pub async fn seed_template_row(db: &TestDb, user: Uuid, row: TemplateRow<'_>) {
    seed_template_row_for(db, &drill_curriculum(), user, row).await;
}

/// Put one unclaimed template row under the test's exact generation context.
pub async fn seed_template_row_for(
    db: &TestDb,
    curriculum: &Curriculum,
    user: Uuid,
    row: TemplateRow<'_>,
) {
    let curriculum_digest = review_context_digest(curriculum).unwrap();
    let problem = PoolProblem {
        v: 1,
        text: row.text.to_string(),
        bindings: row.bindings,
        seed: 7,
    };
    let expected = PoolAnswer {
        answer_contract: None,
        v: 1,
        answer: row.answer.to_string(),
    };
    sqlx::query(
        "INSERT INTO serving_pool
            (user_id, kp_id, source, content_digest, problem, expected_answer, instance_hash,
             source_curriculum_digest, source_review_engine_digest)
         VALUES ($1, $2, 'template', $3, $4::text::jsonb, $5::text::jsonb, $6, $7, $8)",
    )
    .bind(user)
    .bind(row.key)
    .bind(row.digest)
    .bind(problem.to_body().unwrap())
    .bind(expected.to_body().unwrap())
    .bind(row.hash)
    .bind(curriculum_digest)
    .bind(cadus_core::review_engine::DIGEST)
    .execute(&db.admin)
    .await
    .unwrap();
}

/// The body of the `counting/kp1` template, with the sketch its author wrote.
pub fn template_body(sketch: &str) -> Value {
    json!({
        "v": 1,
        "topic_id": "counting",
        "answer_kind": "numeric",
        "statement": "Count on from {a} by {b}.",
        "params": {
            "a": {"kind": "int", "low": 1, "high": 12},
            "b": {"kind": "int", "low": -9, "high": -1}
        },
        "answer_expr": "a + b",
        "solution_sketch": sketch
    })
}

/// The bindings of the seeded template row: `a = 8` and `b = -3`.
pub fn seeded_bindings() -> BTreeMap<String, String> {
    [("a", "8"), ("b", "-3")]
        .into_iter()
        .map(|(name, text)| (name.to_string(), text.to_string()))
        .collect()
}

/// A learner with an open session and one unclaimed pool row of `KEY`:
/// `POOL_TEXT`, whose answer is `POOL_ANSWER`, under the given curriculum
/// and review-engine digest.
pub async fn learner_with_pool_row(db: &TestDb, email: &str, digests: (&str, &str)) -> Uuid {
    let user = seed_learner(db, email).await;
    seed_open_session(db, user).await;
    seed_pool_row(db, user, KEY, POOL_TEXT, POOL_ANSWER, "hash-a", digests).await;
    user
}

/// A learner with an open session and one unclaimed drill-curriculum pool row.
pub async fn learner_with_drill_pool_row(db: &TestDb, email: &str) -> Uuid {
    let curriculum = drill_curriculum();
    let curriculum_digest = review_context_digest(&curriculum).unwrap();
    learner_with_pool_row(
        db,
        email,
        (&curriculum_digest, cadus_core::review_engine::DIGEST),
    )
    .await
}

/// A learner whose review of `addition` stands at serve index `served`, with
/// as many answers behind it, and no live problem.
pub async fn learner_at_review_index(db: &TestDb, email: &str, served: i64) -> Uuid {
    let user = seed_learner(db, email).await;
    seed_open_session(db, user).await;
    seed_due_review(db, user).await;
    let mut scratch = WebState::for_session(SESSION);
    scratch.tasks.insert(
        REVIEW.to_string(),
        TaskProgress {
            task_id: REVIEW.to_string(),
            task_type: "review".to_string(),
            total: 4,
            served,
            answered: served,
            done: false,
            current_kp: None,
        },
    );
    put_state(db, user, &scratch).await;
    user
}

/// A learner whose review stands at serve index 1: the COMPONENT question.
///
/// `review_mix("addition")` is `["kp1", "component:counting"]` and a review
/// indexes by served count, so the next serve of `REVIEW` draws from `counting`
/// and records against `addition`.
pub async fn learner_at_the_component_question(db: &TestDb, email: &str) -> Uuid {
    learner_at_review_index(db, email, 1).await
}
