//! The long-log fixture (F15, F18): 200 sessions of 100 events, the folded
//! model, the pool, and the two request transactions the benchmarks time.

use cadus_core::config::Config;
use cadus_core::curriculum::Curriculum;
use cadus_core::event::{
    Event, SchemaVersion, SessionEnd, SessionStart, Slug, TaskServed, TaskType, Timestamp,
};
use cadus_core::pool::{Avoid, Ring, TaskMemory};
use cadus_core::projector::ProjectionInput;
use cadus_store::pool::{NewInstance, insert_batch, pop_with_ring_tx};
use cadus_store::state::{
    EventRow, append_event, load_events_after, load_web_state, lock_web_state, project_and_save,
    project_current, save_web_state,
};
use cadus_store::test_support::TestDb;
use cadus_store::{StoreError, begin_tenant};
use serde_json::{Value as Json, json};
use sqlx::PgPool;
use sqlx::types::chrono::{DateTime, Utc};
use uuid::Uuid;

use super::bench::{bench_instance, full_windows, seed_web_state};
use super::events::{BASE_US, attempt_row};

/// One day, in microseconds. Session `i` runs on day `i`.
pub const DAY_US: i64 = 86_400_000_000;

/// The topic of every seeded attempt.
pub const TOPIC: &str = "adding-integers";

/// The serving key of the pool rows.
pub const KP_ID: &str = "adding-integers/kp1";

/// The sessions the fixture log carries.
pub const SESSIONS: usize = 200;

/// The events one CLOSED session carries: one start, 98 attempts, one end.
pub const EVENTS_PER_SESSION: usize = 100;

/// The events the one OPEN session carries: one start and 99 attempts.
pub const OPEN_SESSION_EVENTS: usize = 100;

/// The events the whole fixture log carries.
///
/// 199 closed sessions of [`EVENTS_PER_SESSION`], plus the open session.
pub const SEEDED_EVENTS: usize = (SESSIONS - 1) * EVENTS_PER_SESSION + OPEN_SESSION_EVENTS;

/// The unclaimed rows the fixture puts in the pool.
pub const POOL_DEPTH: usize = 64;

/// The seed the refill batch records in every `problem` document.
pub const BATCH_SEED: u64 = 20_260_829;

/// The untimed transactions that warm the connection and the plan cache, under
/// `CADUS_BENCH`.
pub const BENCH_WARMUPS: usize = 10;

/// The timed transactions of each half, under `CADUS_BENCH`.
pub const BENCH_SAMPLES: usize = 100;

/// The samples a plain `cargo test` takes. It gates the COUNTS and not the
/// clock, and three samples pin every count the benchmarks assert.
pub const COUNTING_SAMPLES: usize = 3;

/// The p95 budget of one serve transaction, in nanoseconds: the 100 ms Postgres
/// segment of L1 (`docs/reference/l1-budget.md` section 2).
pub const SERVE_P95_BUDGET_NS: u128 = 100_000_000;

/// The p95 budget of one grade transaction, in nanoseconds: the 150 ms Postgres
/// segment of L2 (`docs/reference/l1-budget.md` section 3).
pub const GRADE_P95_BUDGET_NS: u128 = 150_000_000;

/// The session id of day `day`: `s_<date>a`.
pub fn session_id(day: usize) -> String {
    let stamp = DateTime::<Utc>::from_timestamp_micros(BASE_US + (day as i64) * DAY_US)
        .expect("the fixture instant is representable");
    format!("s_{}a", stamp.date_naive())
}

/// The task id of the one review task of day `day`.
pub fn task_id(day: usize) -> String {
    format!("{}-review-{TOPIC}", session_id(day))
}

/// One graded attempt of day `day`, at minute `minute` of that day.
pub fn attempt_event(day: usize, minute: usize, attempt_id: &str) -> Event {
    attempt_row(
        Timestamp::from_micros(BASE_US + (day as i64) * DAY_US + (minute as i64) * 60_000_000),
        &session_id(day),
        &task_id(day),
        TOPIC,
        attempt_id,
        format!("Compute ${minute} + 5$."),
        (minute + 5).to_string(),
    )
}

/// The whole fixture log, in `seq` order, as `(attempt_id, event)` pairs.
pub fn fixture_log() -> Vec<(Option<String>, Event)> {
    let mut out: Vec<(Option<String>, Event)> = Vec::with_capacity(SEEDED_EVENTS);
    for day in 0..SESSIONS {
        let session = session_id(day);
        out.push((
            None,
            Event::SessionStart(SessionStart {
                ts: Timestamp::from_micros(BASE_US + (day as i64) * DAY_US),
                session: Some(session.clone()),
                v: SchemaVersion,
            }),
        ));
        let last = day + 1 == SESSIONS;
        let attempts = if last {
            OPEN_SESSION_EVENTS - 1
        } else {
            EVENTS_PER_SESSION - 2
        };
        for index in 0..attempts {
            let id = format!("{}-{index}", task_id(day));
            out.push((Some(id.clone()), attempt_event(day, index + 1, &id)));
        }
        if !last {
            out.push((
                None,
                Event::SessionEnd(SessionEnd {
                    ts: Timestamp::from_micros(
                        BASE_US + (day as i64) * DAY_US + (EVENTS_PER_SESSION as i64) * 60_000_000,
                    ),
                    session: Some(session),
                    v: SchemaVersion,
                    xp_earned: 0.0,
                    minutes: 60.0,
                }),
            ));
        }
    }
    out
}

/// Insert the whole fixture log in ONE statement, on the admin pool.
///
/// The seed is not measured, so it takes the bulk path: 20,000 round trips of
/// `append_event` would make the fixture the slowest part of the run.
pub async fn seed_log(pool: &PgPool, user: Uuid) {
    let log = fixture_log();
    assert_eq!(
        log.len(),
        SEEDED_EVENTS,
        "the fixture builder and SEEDED_EVENTS disagree"
    );
    let mut seqs: Vec<i64> = Vec::with_capacity(log.len());
    let mut stamps: Vec<DateTime<Utc>> = Vec::with_capacity(log.len());
    let mut types: Vec<String> = Vec::with_capacity(log.len());
    let mut sessions: Vec<Option<String>> = Vec::with_capacity(log.len());
    let mut attempt_ids: Vec<Option<String>> = Vec::with_capacity(log.len());
    let mut payloads: Vec<Json> = Vec::with_capacity(log.len());
    for (index, (attempt_id, event)) in log.iter().enumerate() {
        seqs.push(index as i64 + 1);
        stamps.push(
            DateTime::<Utc>::from_timestamp_micros(event.ts().micros())
                .expect("the fixture instant is representable"),
        );
        types.push(event.type_name().to_string());
        sessions.push(event.session().map(str::to_string));
        attempt_ids.push(attempt_id.clone());
        payloads.push(serde_json::to_value(event).expect("the event serializes"));
    }
    sqlx::query!(
        r#"
        INSERT INTO events (user_id, seq, ts, type, session_id, v, attempt_id, payload)
        SELECT $1, line.seq, line.ts, line.type, line.session_id, 1, line.attempt_id, line.payload
        FROM UNNEST($2::bigint[], $3::timestamptz[], $4::text[], $5::text[], $6::text[],
                    $7::jsonb[])
             AS line(seq, ts, type, session_id, attempt_id, payload)
        "#,
        user,
        &seqs[..],
        &stamps[..],
        &types[..],
        &sessions[..] as &[Option<String>],
        &attempt_ids[..] as &[Option<String>],
        &payloads[..]
    )
    .execute(pool)
    .await
    .expect("the fixture log inserts");
}

/// Seed one user, the whole log, the folded model, the D-S6 row, and the pool.
pub async fn seed(db: &TestDb, graph: &Curriculum, cfg: &Config) -> (Uuid, Ring, TaskMemory) {
    let user = db.seed_user("bench-long-log@example.test").await;
    seed_log(&db.admin, user).await;

    let mut tx = begin_tenant(&db.admin, user)
        .await
        .expect("the seed transaction starts");
    let input = ProjectionInput::new(graph, cfg, Timestamp::from_micros(BASE_US));
    project_and_save(&mut tx, user, &input, None)
        .await
        .expect("the seed fold saves");
    tx.commit().await.expect("the seed transaction commits");

    let rows: Vec<NewInstance> = (0..POOL_DEPTH)
        .map(|index| bench_instance(index, BATCH_SEED))
        .collect();
    let inserted = insert_batch(&db.admin, user, KP_ID, &rows)
        .await
        .expect("the pool inserts");
    assert_eq!(inserted, POOL_DEPTH as u64, "insert_batch skipped a digest");

    let (ring, task) = full_windows();
    seed_web_state(&db.admin, user, &ring, &task).await;
    (user, ring, task)
}

/// The count of event rows the log of `user` holds.
pub async fn log_len(pool: &PgPool, user: Uuid) -> i64 {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM events WHERE user_id = $1"#,
        user
    )
    .fetch_one(pool)
    .await
    .expect("the log length reads")
}

/// What one measured transaction read.
pub struct Read {
    /// The pool row the transaction claimed.
    pub claimed: Uuid,
    /// The count of event rows the OPEN read decoded. It is the length of the
    /// open session's window, not the length of the log.
    pub rows: usize,
    /// Whether the fold took the full-replay branch.
    pub replayed: bool,
    /// The `seq` an appended event took, or 0 when the transaction appended
    /// none.
    pub seq: i64,
    /// The `seq` the fold reached.
    pub folded_through: i64,
}

/// The read `cadus_web::serve::open` performs, statement for statement.
///
/// The advisory lock, `project_current`, the OPEN SESSION's own event window,
/// and the D-S6 document. No step reads the whole log. The answer carries the
/// window itself, because the serve route reads it for the tasks this session
/// already served (D-M5-8).
async fn open_once(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user: Uuid,
    input: &ProjectionInput<'_>,
) -> Result<(Vec<EventRow>, i64, Json), StoreError> {
    lock_web_state(tx, user).await?;
    let projection = project_current(tx, user, input).await?;
    let after = projection
        .view
        .session_start_seq
        .unwrap_or(0)
        .saturating_sub(1);
    let events = load_events_after(tx, user, after).await?;
    let doc = load_web_state(tx, user).await?.unwrap_or_else(|| json!({}));
    assert!(
        !projection.replayed,
        "the open read took the full-replay branch, so the fixture cache is stale"
    );
    Ok((events, projection.through_seq, doc))
}

/// Whether this session already served `task` (`serve::served_task_ids`).
fn already_served(events: &[EventRow], session: &str, task: &str) -> bool {
    events.iter().any(|row| match &row.event {
        Event::TaskServed(body) => body.session.as_deref() == Some(session) && body.task_id == task,
        _ => false,
    })
}

/// The `task_served` event the FIRST serve of the task of day `day` appends
/// (D-M5-8).
fn task_served_event(day: usize) -> Event {
    Event::TaskServed(TaskServed {
        ts: Timestamp::from_micros(BASE_US + (day as i64) * DAY_US),
        session: Some(session_id(day)),
        v: SchemaVersion,
        task_id: task_id(day),
        task_type: TaskType::Review,
        topic: Some(Slug::new(TOPIC).expect("the topic slug")),
        kp: None,
        problems: Vec::new(),
        component_topics: Vec::new(),
        seed: None,
    })
}

/// The D-O1 serve transaction, as `cadus_web::serve::serve` runs it.
///
/// The FIRST serve of the task appends `task_served` and folds and saves in the
/// same transaction (M5 review 2, findings V1 and V8). The append is idempotent
/// per task id, so every later serve of the run finds the line in the window,
/// appends nothing, and leaves the cursor on the head of the log. That is the
/// hand-off the learner repeats: one task takes 20 questions and one cadence
/// line.
pub async fn serve_once(
    pool: &PgPool,
    user: Uuid,
    input: &ProjectionInput<'_>,
    avoid: &Avoid<'_>,
) -> Result<Read, StoreError> {
    let mut tx = begin_tenant(pool, user).await?;
    let (events, through, doc) = open_once(&mut tx, user, input).await?;
    let rows = events.len();
    let claimed = pop_with_ring_tx(&mut tx, user, KP_ID, avoid)
        .await?
        .claimed
        .expect("the pool holds an unclaimed row");

    let day = SESSIONS - 1;
    let (seq, replayed, folded_through) =
        if already_served(&events, &session_id(day), &task_id(day)) {
            (0, false, through)
        } else {
            let seq = append_event(&mut tx, user, &task_served_event(day), None)
                .await?
                .expect("the cadence event carries no attempt id, so the append writes");
            let projection = project_and_save(&mut tx, user, input, None).await?;
            (seq, projection.replayed, projection.through_seq)
        };

    save_web_state(&mut tx, user, &doc).await?;
    tx.commit().await?;
    Ok(Read {
        claimed: claimed.row.id,
        rows,
        replayed,
        seq,
        folded_through,
    })
}

/// The D-O2 grade transaction, as `cadus_web::grade::answer` runs it.
pub async fn grade_once(
    pool: &PgPool,
    user: Uuid,
    input: &ProjectionInput<'_>,
    attempt_id: &str,
    avoid: &Avoid<'_>,
) -> Result<Read, StoreError> {
    let mut tx = begin_tenant(pool, user).await?;
    let (events, _, doc) = open_once(&mut tx, user, input).await?;
    let rows = events.len();

    let event = attempt_event(SESSIONS - 1, OPEN_SESSION_EVENTS, attempt_id);
    let seq = append_event(&mut tx, user, &event, Some(attempt_id))
        .await?
        .expect("the attempt is new, so the append returns a seq");
    let projection = project_and_save(&mut tx, user, input, None).await?;
    let claimed = pop_with_ring_tx(&mut tx, user, KP_ID, avoid)
        .await?
        .claimed
        .expect("the pool holds an unclaimed row");
    save_web_state(&mut tx, user, &doc).await?;
    tx.commit().await?;

    Ok(Read {
        claimed: claimed.row.id,
        rows,
        replayed: projection.replayed,
        seq,
        folded_through: projection.through_seq,
    })
}
