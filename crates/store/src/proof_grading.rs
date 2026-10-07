//! The Amendment K proof-grading queue (design point 6): the enqueue, the
//! tenant-bound read, the claim, and the settle.
//!
//! The equivalence queue's shape, without a cache. The grade transaction
//! enqueues an UNGRADED written-proof attempt; the worker claims the row,
//! asks the hosted model for a per-check grading, and settles it.
//!
//! # Who writes what
//!
//! The request tier calls [`enqueue`] inside the grade transaction and reads
//! with [`job`], [`jobs`] and [`open_heads`]. It writes the learner-side
//! marks of a row ([`mark`], [`dispute`]) and the human verdict of a disputed
//! row ([`set_override`]). The worker claims with [`claim`] and settles with
//! [`settle`]. Nothing else writes the table.
//!
//! # The revision chain
//!
//! A resubmitted proof is a NEW row whose `revision_of` names the row it
//! revises, so one problem's drafts form a linked list: the root has no
//! `revision_of`, the head has no successor. Each row keeps its own verdict
//! and feedback, and `closed_at` on the head ends the chain.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::{PgExecutor, Postgres, Transaction};

use crate::StoreError;

/// `proof_grading_jobs.status` of a job no worker claimed yet.
pub const JOB_PENDING: &str = "pending";

/// `proof_grading_jobs.status` of a claimed job.
pub const JOB_RUNNING: &str = "running";

/// `proof_grading_jobs.status` of a job whose `result` stands.
pub const JOB_DONE: &str = "done";

/// `proof_grading_jobs.status` of a job the worker gave up on.
pub const JOB_FAILED: &str = "failed";

/// `proof_grading_jobs.status` of a job the per-learner daily cap refused.
pub const JOB_CAPPED: &str = "capped";

/// The `mode` of a payload that grades a written proof.
pub const MODE_PROOF: &str = "proof";

/// The `mode` of a payload that grades a written sentence: the learner's
/// answer is compared with a reference sentence (D-PR1).
pub const MODE_WRITTEN: &str = "written";

/// The version of the [`JobPayload`] document.
pub const PAYLOAD_VERSION: u32 = 1;

/// The version of the [`Grading`] result document.
pub const RESULT_VERSION: u32 = 1;

/// The `verdict` of a proof the grading accepts.
pub const VERDICT_PASS: &str = "pass";

/// The `verdict` of a proof the grading sends back.
pub const VERDICT_NEEDS_REVISION: &str = "needs_revision";

/// The `grader_note` of the `regraded` correction the background proof
/// grader appends. The request tier reads it back to apply a landed verdict
/// to the evidence of a review that closes after it.
pub const GRADER_NOTE: &str = "proof grading (background model)";

/// What the grade transaction hands the worker.
///
/// Every field the grading prompt needs: the problem, the reference solution,
/// an authored rubric when the item carries one, and the learner's text. No
/// session and no user id: the worker reads the tenant from the row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobPayload {
    /// [`PAYLOAD_VERSION`].
    pub v: u32,
    /// The task the attempt belongs to.
    pub task_id: String,
    /// The topic of the problem.
    pub topic: String,
    /// The digest of the served item.
    pub item_digest: String,
    /// The statement the learner read.
    pub problem: String,
    /// The reference solution (the worked solution sketch), or `None`.
    #[serde(default)]
    pub reference: Option<String>,
    /// The stored key, when it carries content (a proof item usually stores a
    /// placeholder such as "See the solution.").
    #[serde(default)]
    pub expected: Option<String>,
    /// The authored yes/no rubric of the item. Empty today: no item carries
    /// one yet. A non-empty rubric takes precedence over derived checks.
    #[serde(default)]
    pub rubric: Vec<String>,
    /// What the learner wrote, raw.
    pub given_answer: String,
    /// The knowledge point of the problem, inside `topic`. A lesson chain is
    /// found by `(topic, kp)`. Rows written before the revision loop carry
    /// none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kp: Option<String>,
    /// `"proof"` or `"written"` (a short answer). Rows written before the key
    /// existed carry none and read as a proof.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// The hash of the problem text; a lesson chain is found by
    /// `(topic, kp, problem_hash)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem_hash: Option<String>,
}

/// One graded check of a [`Grading`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    /// `G1`..`G5` for the general checks, `S1`.. for the problem-specific
    /// ones, `R1`.. for authored rubric items.
    pub id: String,
    /// The yes/no question, in words.
    pub text: String,
    /// Whether the check is minor (one failed minor check still passes).
    pub minor: bool,
    /// Whether the learner text meets the check.
    pub met: bool,
    /// A short quote from the learner text, or "not found".
    pub evidence: String,
    /// Whether the quote was found in the learner text (whitespace and case
    /// set aside). A met check needs a verified quote.
    #[serde(default)]
    pub quote_verified: bool,
}

/// The result document of one graded proof (`proof_grading_jobs.result`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grading {
    /// [`RESULT_VERSION`].
    pub v: u32,
    /// [`VERDICT_PASS`] or [`VERDICT_NEEDS_REVISION`].
    pub verdict: String,
    /// The checks, in the order the model answered them.
    pub checks: Vec<Check>,
    /// Two to four sentences for the learner.
    pub feedback: String,
    /// The model id that graded.
    pub model: String,
}

impl Grading {
    /// Whether the verdict is a pass.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.verdict == VERDICT_PASS
    }
}

/// `proof_grading_jobs.context` of a row written before the revision loop.
pub const CONTEXT_LEGACY: &str = "legacy";

/// `proof_grading_jobs.context` of a written proof inside a lesson: the
/// blocking loop (the knowledge point closes on a pass).
pub const CONTEXT_LESSON: &str = "lesson";

/// `proof_grading_jobs.context` of a written proof outside a lesson and a
/// quiz: graded in the background, revised from the proofs list.
pub const CONTEXT_REVIEW: &str = "review";

/// `proof_grading_jobs.context` of a quiz proof, graded after the reveal.
pub const CONTEXT_QUIZ: &str = "quiz";

/// `proof_grading_jobs.context` of a free-explanation self-check.
pub const CONTEXT_SELFCHECK: &str = "selfcheck";

/// The human verdict of a disputed row that accepts the proof.
pub const OVERRIDE_PASS: &str = "pass";

/// The human verdict of a disputed row that keeps the revision.
pub const OVERRIDE_NEEDS_REVISION: &str = "needs_revision";

/// One `proof_grading_jobs` row, as the request tier reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRow {
    /// The primary key. It is the `id` the client polls.
    pub id: Uuid,
    /// The attempt the job grades.
    pub attempt_id: String,
    /// One of [`JOB_PENDING`], [`JOB_RUNNING`], [`JOB_DONE`], [`JOB_FAILED`],
    /// [`JOB_CAPPED`].
    pub status: String,
    /// The payload the grade transaction wrote.
    pub payload: Json,
    /// The result document, `NULL` until the worker writes it.
    pub result: Option<Json>,
    /// When the grade transaction wrote the row.
    pub created_at: DateTime<Utc>,
    /// The job this draft revises, `None` for the first draft of a problem.
    pub revision_of: Option<Uuid>,
    /// How many needs-revision verdicts the chain used before this draft.
    pub revision: i32,
    /// Where the draft was written ([`CONTEXT_LESSON`] and the others).
    pub context: String,
    /// Whether this is the unaided rewrite after the revision cap.
    pub rewrite: bool,
    /// When the learner first saw the verdict.
    pub seen_at: Option<DateTime<Utc>>,
    /// When the reference solution was shown after the revision cap.
    pub revealed_at: Option<DateTime<Utc>>,
    /// When the chain closed (set on its head).
    pub closed_at: Option<DateTime<Utc>>,
    /// When the learner disputed the verdict.
    pub disputed_at: Option<DateTime<Utc>>,
    /// The learner's note on the dispute.
    pub dispute_note: Option<String>,
    /// The human verdict of a disputed row, when one stands.
    pub override_verdict: Option<String>,
}

impl JobRow {
    /// The verdict that stands for this row: the human verdict of a resolved
    /// dispute, else the model verdict of a settled grading. `None` while the
    /// job runs, and for a failed or capped job with no human verdict.
    #[must_use]
    pub fn verdict(&self) -> Option<&str> {
        if let Some(human) = self.override_verdict.as_deref() {
            return Some(human);
        }
        if self.status != JOB_DONE {
            return None;
        }
        self.result
            .as_ref()
            .and_then(|doc| doc.get("verdict"))
            .and_then(Json::as_str)
    }

    /// The model grading of a settled row.
    #[must_use]
    pub fn grading(&self) -> Option<Grading> {
        if self.status != JOB_DONE {
            return None;
        }
        self.result
            .as_ref()
            .and_then(|doc| serde_json::from_value(doc.clone()).ok())
    }

    /// The payload document, when it reads.
    #[must_use]
    pub fn job_payload(&self) -> Option<JobPayload> {
        serde_json::from_value(self.payload.clone()).ok()
    }

    /// A string field of the payload.
    #[must_use]
    pub fn payload_str(&self, key: &str) -> Option<&str> {
        self.payload.get(key).and_then(Json::as_str)
    }
}

/// The chain columns of a new row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewJob<'a> {
    /// [`CONTEXT_LESSON`] and the others.
    pub context: &'a str,
    /// The job this draft revises.
    pub revision_of: Option<Uuid>,
    /// How many needs-revision verdicts the chain used before this draft.
    pub revision: i32,
    /// Whether this is the unaided rewrite after the cap.
    pub rewrite: bool,
}

impl<'a> NewJob<'a> {
    /// The first draft of a problem in `context`.
    #[must_use]
    pub const fn first(context: &'a str) -> Self {
        Self {
            context,
            revision_of: None,
            revision: 0,
            rewrite: false,
        }
    }
}

/// One claimed row, as the worker reads it.
#[derive(Debug, Clone)]
pub struct Claimed {
    /// The primary key.
    pub id: Uuid,
    /// The tenant that submitted the attempt.
    pub user_id: Uuid,
    /// The attempt the job grades.
    pub attempt_id: String,
    /// The payload document.
    pub payload: Json,
    /// How many claims this row already used, this one included.
    pub attempts: i32,
}

/// Put one job on the queue, inside the caller's transaction.
///
/// The insert is idempotent on `(user_id, attempt_id)`: a retried grade meets
/// the standing row and answers with its id instead of a second one.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn enqueue(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    attempt_id: &str,
    payload: &Json,
    job: &NewJob<'_>,
) -> Result<Uuid, StoreError> {
    let id = sqlx::query_scalar::<_, Uuid>(
        r#"
        WITH inserted AS (
            INSERT INTO proof_grading_jobs
                   (user_id, attempt_id, payload, context, revision_of, revision, rewrite)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (user_id, attempt_id) DO NOTHING
            RETURNING id
        )
        SELECT id FROM inserted
        UNION ALL
        SELECT id FROM proof_grading_jobs
         WHERE user_id = $1 AND attempt_id = $2
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .bind(attempt_id)
    .bind(payload)
    .bind(job.context)
    .bind(job.revision_of)
    .bind(job.revision)
    .bind(job.rewrite)
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

/// The column list every row read selects, as a literal the statements
/// `concat!` (sqlx takes literal SQL only).
macro_rules! columns {
    () => {
        "id, attempt_id, status, payload, result, created_at, revision_of, revision, context, \
         rewrite, seen_at, revealed_at, closed_at, disputed_at, dispute_note, override_verdict"
    };
}

/// Read one selected row.
fn row_of(row: &sqlx::postgres::PgRow) -> Result<JobRow, sqlx::Error> {
    use sqlx::Row;
    Ok(JobRow {
        id: row.try_get("id")?,
        attempt_id: row.try_get("attempt_id")?,
        status: row.try_get("status")?,
        payload: row.try_get("payload")?,
        result: row.try_get("result")?,
        created_at: row.try_get("created_at")?,
        revision_of: row.try_get("revision_of")?,
        revision: row.try_get("revision")?,
        context: row.try_get("context")?,
        rewrite: row.try_get("rewrite")?,
        seen_at: row.try_get("seen_at")?,
        revealed_at: row.try_get("revealed_at")?,
        closed_at: row.try_get("closed_at")?,
        disputed_at: row.try_get("disputed_at")?,
        dispute_note: row.try_get("dispute_note")?,
        override_verdict: row.try_get("override_verdict")?,
    })
}

/// Read one job by its primary key, under the caller's tenant binding.
///
/// The statement names no `user_id`: the `tenant_isolation` policy scopes it,
/// so another tenant's id returns no row.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn job<'e, E>(executor: E, id: Uuid) -> Result<Option<JobRow>, StoreError>
where
    E: PgExecutor<'e>,
{
    let sql = concat!(
        "SELECT ",
        columns!(),
        " FROM proof_grading_jobs WHERE id = $1"
    );
    let row = sqlx::query(sql).bind(id).fetch_optional(executor).await?;
    Ok(row.as_ref().map(row_of).transpose()?)
}

/// Read the complete revision chain containing `id`, root first, under the
/// caller's tenant binding. The lookup follows `revision_of` in both
/// directions, so presentation limits in [`jobs`] cannot truncate the chain.
/// An id outside the caller's tenant returns `None`.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn chain_rows<'e, E>(executor: E, id: Uuid) -> Result<Option<Vec<JobRow>>, StoreError>
where
    E: PgExecutor<'e>,
{
    let sql = concat!(
        "WITH RECURSIVE ancestors(id, revision_of, depth, path) AS (",
        " SELECT id, revision_of, 0, ARRAY[id] FROM proof_grading_jobs WHERE id = $1",
        " UNION ALL",
        " SELECT parent.id, parent.revision_of, a.depth + 1, a.path || parent.id",
        " FROM proof_grading_jobs parent JOIN ancestors a ON parent.id = a.revision_of",
        " WHERE NOT parent.id = ANY(a.path)",
        "), root AS (SELECT id FROM ancestors ORDER BY depth DESC LIMIT 1),",
        " chain(id, depth, path) AS (",
        " SELECT id, 0, ARRAY[id] FROM root",
        " UNION ALL",
        " SELECT child.id, c.depth + 1, c.path || child.id FROM proof_grading_jobs child",
        " JOIN chain c ON child.revision_of = c.id WHERE NOT child.id = ANY(c.path)",
        ") SELECT j.* FROM proof_grading_jobs j JOIN chain c ON c.id = j.id ORDER BY c.depth"
    );
    let rows = sqlx::query(sql).bind(id).fetch_all(executor).await?;
    if rows.is_empty() {
        return Ok(None);
    }
    Ok(Some(rows.iter().map(row_of).collect::<Result<_, _>>()?))
}

/// Read complete chains touched by the newest `limit` jobs, plus every open
/// lesson/review chain, under the caller's tenant binding. This bounds the
/// number of historical chains shown while retaining each selected chain's
/// root and head, including older open work.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn jobs_for_list<'e, E>(executor: E, limit: i64) -> Result<Vec<JobRow>, StoreError>
where
    E: PgExecutor<'e>,
{
    let sql = concat!(
        "WITH RECURSIVE recent AS (",
        " SELECT id FROM proof_grading_jobs ORDER BY created_at DESC, id LIMIT $1",
        "), ancestor(id, revision_of, path) AS (",
        " SELECT j.id, j.revision_of, ARRAY[j.id] FROM proof_grading_jobs j",
        " JOIN recent r ON r.id = j.id",
        " UNION ALL",
        " SELECT p.id, p.revision_of, a.path || p.id FROM proof_grading_jobs p",
        " JOIN ancestor a ON p.id = a.revision_of WHERE NOT p.id = ANY(a.path)",
        "), open_head(id) AS (",
        " SELECT j.id FROM proof_grading_jobs j",
        " WHERE j.closed_at IS NULL AND j.context = ANY($2)",
        " AND NOT EXISTS (SELECT 1 FROM proof_grading_jobs s WHERE s.revision_of = j.id)",
        "), open_ancestor(id, revision_of, path) AS (",
        " SELECT j.id, j.revision_of, ARRAY[j.id] FROM proof_grading_jobs j",
        " JOIN open_head h ON h.id = j.id",
        " UNION ALL",
        " SELECT p.id, p.revision_of, a.path || p.id FROM proof_grading_jobs p",
        " JOIN open_ancestor a ON p.id = a.revision_of WHERE NOT p.id = ANY(a.path)",
        "), roots(id) AS (",
        " SELECT id FROM ancestor WHERE revision_of IS NULL",
        " UNION SELECT id FROM open_ancestor WHERE revision_of IS NULL",
        "), selected(id, depth, path) AS (",
        " SELECT r.id, 0, ARRAY[r.id] FROM roots r",
        " UNION ALL",
        " SELECT s.id, selected.depth + 1, selected.path || s.id FROM proof_grading_jobs s",
        " JOIN selected ON s.revision_of = selected.id",
        " WHERE NOT s.id = ANY(selected.path)",
        ") SELECT j.* FROM proof_grading_jobs j JOIN selected s ON s.id = j.id",
        " ORDER BY j.created_at, j.id"
    );
    let rows = sqlx::query(sql)
        .bind(limit)
        .bind(vec![CONTEXT_LESSON, CONTEXT_REVIEW])
        .fetch_all(executor)
        .await?;
    Ok(rows.iter().map(row_of).collect::<Result<_, _>>()?)
}

/// The heads of closed lesson proof chains attempted after the most recent
/// `lesson_result` for `topic`, ordered by root attempt sequence. Both the
/// event and proof rows are read under the caller's tenant binding. This
/// identifies proof points completed in the current lesson run across session
/// rollover while excluding earlier runs.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn closed_lesson_heads_after_latest_result<'e, E>(
    executor: E,
    topic: &str,
) -> Result<Vec<Uuid>, StoreError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query_scalar::<_, Uuid>(
        r#"
        WITH RECURSIVE boundary AS (
            SELECT COALESCE(MAX(seq), 0) AS seq
              FROM events
             WHERE type = 'lesson_result' AND payload->>'topic' = $1
        ), roots AS (
            SELECT j.id, e.seq
              FROM proof_grading_jobs j
              JOIN events e ON e.attempt_id = j.attempt_id
             CROSS JOIN boundary b
             WHERE j.context = 'lesson' AND j.revision_of IS NULL
               AND j.payload->>'topic' = $1
               AND e.type = 'attempt' AND e.payload->>'topic' = $1
               AND e.seq > b.seq
        ), chain(id, root_id, seq) AS (
            SELECT id, id, seq FROM roots
            UNION ALL
            SELECT child.id, chain.root_id, chain.seq
              FROM proof_grading_jobs child
              JOIN chain ON child.revision_of = chain.id
        )
        SELECT chain.id
          FROM chain
          JOIN proof_grading_jobs head ON head.id = chain.id
         WHERE head.closed_at IS NOT NULL
           AND NOT EXISTS (
               SELECT 1 FROM proof_grading_jobs successor
                WHERE successor.revision_of = head.id
           )
         ORDER BY chain.seq, chain.root_id
        "#,
    )
    .bind(topic)
    .fetch_all(executor)
    .await?;
    Ok(rows)
}

/// Every job of the caller's tenant, oldest first, at most `limit` rows.
///
/// The request tier groups them into revision chains. The per-learner daily
/// cap of the worker keeps the table small.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn jobs<'e, E>(executor: E, limit: i64) -> Result<Vec<JobRow>, StoreError>
where
    E: PgExecutor<'e>,
{
    let sql = concat!(
        "SELECT ",
        columns!(),
        " FROM (SELECT * FROM proof_grading_jobs ORDER BY created_at DESC, id LIMIT $1) recent \
         ORDER BY created_at, id"
    );
    let rows = sqlx::query(sql).bind(limit).fetch_all(executor).await?;
    Ok(rows.iter().map(row_of).collect::<Result<_, _>>()?)
}

/// The open heads of the caller's tenant: rows of `contexts` with no
/// successor and no `closed_at`, oldest first.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn open_heads<'e, E>(executor: E, contexts: &[&str]) -> Result<Vec<JobRow>, StoreError>
where
    E: PgExecutor<'e>,
{
    let contexts: Vec<String> = contexts.iter().map(|c| (*c).to_owned()).collect();
    let sql = concat!(
        "SELECT ",
        columns!(),
        " FROM proof_grading_jobs j \
          WHERE j.closed_at IS NULL AND j.context = ANY($1) \
            AND NOT EXISTS (SELECT 1 FROM proof_grading_jobs s WHERE s.revision_of = j.id) \
          ORDER BY j.created_at, j.id"
    );
    let rows = sqlx::query(sql).bind(contexts).fetch_all(executor).await?;
    Ok(rows.iter().map(row_of).collect::<Result<_, _>>()?)
}

/// The open lesson head of one written item of a knowledge point (`topic` is
/// the serving topic of the payload, `problem_hash` the
/// `problem_text_hash` of the item), when one stands.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn open_lesson_head<'e, E>(
    executor: E,
    topic: &str,
    kp: &str,
    problem_hash: &str,
) -> Result<Option<JobRow>, StoreError>
where
    E: PgExecutor<'e>,
{
    let sql = concat!(
        "SELECT ",
        columns!(),
        " FROM proof_grading_jobs j \
          WHERE j.closed_at IS NULL AND j.context = $1 \
            AND j.payload->>'topic' = $2 AND j.payload->>'kp' = $3 \
            AND (j.payload->>'problem_hash' = $4 OR j.payload->>'problem_hash' IS NULL) \
            AND NOT EXISTS (SELECT 1 FROM proof_grading_jobs s WHERE s.revision_of = j.id) \
          ORDER BY j.created_at DESC, j.id"
    );
    let rows = sqlx::query(sql)
        .bind(CONTEXT_LESSON)
        .bind(topic)
        .bind(kp)
        .bind(problem_hash)
        .fetch_all(executor)
        .await?;
    // A chain opened before the key existed carries no hash: it belongs to
    // the item whose statement hashes to the key.
    for row in &rows {
        let job = row_of(row)?;
        let hash = job
            .payload_str("problem_hash")
            .map(str::to_owned)
            .or_else(|| {
                job.payload_str("problem")
                    .map(cadus_core::learner::problem_text_hash)
            });
        if hash.as_deref() == Some(problem_hash) {
            return Ok(Some(job));
        }
    }
    Ok(None)
}

/// The `problem_hash` of every item of `(topic, kp)` whose lesson chain
/// closed under `task_id`, oldest first. A chain closes on a pass the learner
/// continued from, or on the unaided rewrite after the cap.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn closed_lesson_items<'e, E>(
    executor: E,
    topic: &str,
    kp: &str,
    task_id: &str,
) -> Result<Vec<String>, StoreError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT j.payload->>'problem_hash', j.payload->>'problem' \
           FROM proof_grading_jobs j \
          WHERE j.closed_at IS NOT NULL AND j.context = $1 \
            AND j.payload->>'topic' = $2 AND j.payload->>'kp' = $3 \
            AND j.payload->>'task_id' = $4 \
          ORDER BY j.created_at",
    )
    .bind(CONTEXT_LESSON)
    .bind(topic)
    .bind(kp)
    .bind(task_id)
    .fetch_all(executor)
    .await?;
    // A chain opened before the key existed carries its statement instead.
    let mut hashes: Vec<String> = Vec::new();
    for (hash, problem) in rows {
        let found = hash.or_else(|| {
            problem
                .as_deref()
                .map(cadus_core::learner::problem_text_hash)
        });
        if let Some(found) = found.filter(|found| !hashes.contains(found)) {
            hashes.push(found);
        }
    }
    Ok(hashes)
}

/// The job that grades `attempt_id`, under the caller's tenant binding.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn job_by_attempt<'e, E>(
    executor: E,
    attempt_id: &str,
) -> Result<Option<JobRow>, StoreError>
where
    E: PgExecutor<'e>,
{
    let sql = concat!(
        "SELECT ",
        columns!(),
        " FROM proof_grading_jobs WHERE attempt_id = $1 LIMIT 1"
    );
    let row = sqlx::query(sql)
        .bind(attempt_id)
        .fetch_optional(executor)
        .await?;
    Ok(row.as_ref().map(row_of).transpose()?)
}

/// The disputed rows that wait for a human verdict, oldest first.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn open_disputes<'e, E>(executor: E) -> Result<Vec<JobRow>, StoreError>
where
    E: PgExecutor<'e>,
{
    let sql = concat!(
        "SELECT ",
        columns!(),
        " FROM proof_grading_jobs \
          WHERE disputed_at IS NOT NULL AND override_verdict IS NULL \
          ORDER BY disputed_at, id"
    );
    let rows = sqlx::query(sql).fetch_all(executor).await?;
    Ok(rows.iter().map(row_of).collect::<Result<_, _>>()?)
}

/// Which learner-side mark [`mark`] writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// `seen_at`: the learner saw the verdict.
    Seen,
    /// `revealed_at`: the reference solution was shown after the cap.
    Revealed,
    /// `closed_at`: the chain is over.
    Closed,
}

/// Stamp one learner-side mark on one row, keeping a stamp that stands.
/// The answer is `true` when this call wrote the stamp.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn mark<'e, E>(executor: E, id: Uuid, which: Mark) -> Result<bool, StoreError>
where
    E: PgExecutor<'e>,
{
    let sql = match which {
        Mark::Seen => {
            "UPDATE proof_grading_jobs SET seen_at = now() WHERE id = $1 AND seen_at IS NULL"
        }
        Mark::Revealed => {
            "UPDATE proof_grading_jobs SET revealed_at = now() WHERE id = $1 AND revealed_at IS NULL"
        }
        Mark::Closed => {
            "UPDATE proof_grading_jobs SET closed_at = now() WHERE id = $1 AND closed_at IS NULL"
        }
    };
    let done = sqlx::query(sql).bind(id).execute(executor).await?;
    Ok(done.rows_affected() > 0)
}

/// Record the learner's dispute of one row's verdict.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn dispute<'e, E>(executor: E, id: Uuid, note: Option<&str>) -> Result<bool, StoreError>
where
    E: PgExecutor<'e>,
{
    let done = sqlx::query(
        "UPDATE proof_grading_jobs SET disputed_at = now(), dispute_note = $2 \
          WHERE id = $1 AND disputed_at IS NULL",
    )
    .bind(id)
    .bind(note)
    .execute(executor)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Record the human verdict of the row that grades `attempt_id`. The answer
/// is `true` when a row took it.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn set_override<'e, E>(
    executor: E,
    attempt_id: &str,
    verdict: &str,
) -> Result<bool, StoreError>
where
    E: PgExecutor<'e>,
{
    let done =
        sqlx::query("UPDATE proof_grading_jobs SET override_verdict = $2 WHERE attempt_id = $1")
            .bind(attempt_id)
            .bind(verdict)
            .execute(executor)
            .await?;
    Ok(done.rows_affected() > 0)
}

/// The id of the job that grades `attempt_id`, under the caller's tenant
/// binding. The row is unique per `(user_id, attempt_id)` and the
/// `tenant_isolation` policy supplies the `user_id`.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn job_of_attempt<'e, E>(
    executor: E,
    attempt_id: &str,
) -> Result<Option<Uuid>, StoreError>
where
    E: PgExecutor<'e>,
{
    let id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM proof_grading_jobs WHERE attempt_id = $1 LIMIT 1",
    )
    .bind(attempt_id)
    .fetch_optional(executor)
    .await?;
    Ok(id)
}

/// Record that the lesson owes the written item `problem_hash` of `(topic,
/// kp)`: the decided items of the point passed (D-PR1). A second record
/// changes nothing.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn owe<'e, E>(
    executor: E,
    user_id: Uuid,
    topic: &str,
    kp: &str,
    problem_hash: &str,
) -> Result<(), StoreError>
where
    E: PgExecutor<'e>,
{
    sqlx::query(
        "INSERT INTO proof_owed (user_id, topic, kp, problem_hash) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (user_id, topic, kp, problem_hash) DO NOTHING",
    )
    .bind(user_id)
    .bind(topic)
    .bind(kp)
    .bind(problem_hash)
    .execute(executor)
    .await?;
    Ok(())
}

/// Whether the lesson owes the written item `problem_hash` of `(topic, kp)`,
/// under the caller's tenant binding. A row of migration 0027 carries the
/// empty hash and stands for the point's only item.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn is_owed<'e, E>(
    executor: E,
    topic: &str,
    kp: &str,
    problem_hash: &str,
) -> Result<bool, StoreError>
where
    E: PgExecutor<'e>,
{
    let owed = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM proof_owed \
          WHERE topic = $1 AND kp = $2 AND problem_hash IN ($3, ''))",
    )
    .bind(topic)
    .bind(kp)
    .bind(problem_hash)
    .fetch_one(executor)
    .await?;
    Ok(owed)
}

/// Every knowledge point of the caller's tenant that owes a written item, as
/// `(topic, kp)`, oldest first, once per point.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn owed<'e, E>(executor: E) -> Result<Vec<(String, String)>, StoreError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT topic, kp FROM proof_owed GROUP BY topic, kp \
          ORDER BY min(created_at), topic, kp",
    )
    .fetch_all(executor)
    .await?;
    Ok(rows)
}

/// The owed item `problem_hash` of `(topic, kp)` is no longer owed: its first
/// draft opened the revision chain.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn settle_owed<'e, E>(
    executor: E,
    topic: &str,
    kp: &str,
    problem_hash: &str,
) -> Result<(), StoreError>
where
    E: PgExecutor<'e>,
{
    sqlx::query("DELETE FROM proof_owed WHERE topic = $1 AND kp = $2 AND problem_hash IN ($3, '')")
        .bind(topic)
        .bind(kp)
        .bind(problem_hash)
        .execute(executor)
        .await?;
    Ok(())
}

/// Take the oldest pending row (`FOR UPDATE SKIP LOCKED`, the D-O5 pattern).
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn claim(db: &crate::Db) -> Result<Option<Claimed>, StoreError> {
    let row = sqlx::query!(
        r#"
        WITH job AS (
          SELECT id FROM proof_grading_jobs
           WHERE status = $1
           ORDER BY created_at
           FOR UPDATE SKIP LOCKED
           LIMIT 1)
        UPDATE proof_grading_jobs d
           SET status = $2, claimed_at = now(), attempts = attempts + 1
          FROM job WHERE d.id = job.id
        RETURNING d.id AS "id!", d.user_id AS "user_id!", d.attempt_id AS "attempt_id!",
                  d.payload AS "payload!", d.attempts AS "attempts!"
        "#,
        JOB_PENDING,
        JOB_RUNNING,
    )
    .fetch_optional(db.pool())
    .await?;
    Ok(row.map(|row| Claimed {
        id: row.id,
        user_id: row.user_id,
        attempt_id: row.attempt_id,
        payload: row.payload,
        attempts: row.attempts,
    }))
}

/// Settle one claimed row into `status`, with `result` when there is one.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn settle(
    executor: impl PgExecutor<'_>,
    id: Uuid,
    status: &str,
    result: Option<&Json>,
) -> Result<(), StoreError> {
    sqlx::query!(
        r#"
        UPDATE proof_grading_jobs
           SET status = $2, result = $3, finished_at = now()
         WHERE id = $1
        "#,
        id,
        status,
        result,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// How many proof-grading jobs this learner wrote since the UTC day began.
///
/// The per-learner daily cap of Amendment K (20) reads this count.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn jobs_today<'e, E>(executor: E, user_id: Uuid) -> Result<i64, StoreError>
where
    E: PgExecutor<'e>,
{
    let count = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!"
          FROM proof_grading_jobs
         WHERE user_id = $1 AND created_at >= date_trunc('day', now())
        "#,
        user_id,
    )
    .fetch_one(executor)
    .await?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::{Check, Grading, JobPayload, MODE_WRITTEN, VERDICT_PASS};

    /// The result document round-trips through its JSON spelling.
    #[test]
    fn a_grading_round_trips() {
        let grading = Grading {
            v: 1,
            verdict: VERDICT_PASS.to_owned(),
            checks: vec![Check {
                id: "G1".to_owned(),
                text: "The claim is stated.".to_owned(),
                minor: false,
                met: true,
                evidence: "Let n be odd".to_owned(),
                quote_verified: true,
            }],
            feedback: "A complete proof.".to_owned(),
            model: "deepseek/deepseek-v4-pro".to_owned(),
        };
        let json = serde_json::to_value(&grading).unwrap();
        assert_eq!(serde_json::from_value::<Grading>(json).unwrap(), grading);
        assert!(grading.passed());
    }

    /// A payload with no rubric, no reference and no key still reads.
    #[test]
    fn a_minimal_payload_reads() {
        let payload: JobPayload = serde_json::from_value(serde_json::json!({
            "v": 1, "task_id": "t", "topic": "x", "item_digest": "abc123def456",
            "problem": "Prove it.", "given_answer": "Proof."
        }))
        .unwrap();
        assert!(payload.rubric.is_empty());
        assert_eq!(payload.reference, None);
    }

    /// A row written before the item key reads with no mode and no hash, and
    /// a written payload keeps both new fields through JSON.
    #[test]
    fn the_mode_and_the_item_key_default_and_round_trip() {
        let old: JobPayload = serde_json::from_value(serde_json::json!({
            "v": 1, "task_id": "t", "topic": "x", "item_digest": "abc123def456",
            "problem": "Prove it.", "given_answer": "Proof."
        }))
        .unwrap();
        assert_eq!(old.mode, None);
        assert_eq!(old.problem_hash, None);
        let json = serde_json::to_value(&old).unwrap();
        assert!(json.get("problem_hash").is_none());
        let written = JobPayload {
            mode: Some(MODE_WRITTEN.to_owned()),
            problem_hash: Some("0123456789ab".to_owned()),
            ..old
        };
        let json = serde_json::to_value(&written).unwrap();
        assert_eq!(json["mode"], "written");
        assert_eq!(json["problem_hash"], "0123456789ab");
        assert_eq!(serde_json::from_value::<JobPayload>(json).unwrap(), written);
    }
}
