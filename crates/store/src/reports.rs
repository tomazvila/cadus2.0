//! Durable report jobs and immutable, source-bound correction versions.
//! SQL uses fixed parameterized statements, matching the exposure store.
pub mod diagnostics;
pub(crate) mod task_outcomes;

/// Stable identity of mathematical content across accepted-answer repairs.
pub fn source_identity(source: &serde_json::Value) -> serde_json::Value {
    let mut identity = source.clone();
    if let Some(problem) = identity["problem"].as_object_mut() {
        problem.remove("expected");
    }
    identity["identity_version"] = serde_json::json!(2);
    identity
}

use crate::state::{append_event, lock_web_state};
use crate::{Db, StoreError, begin_tenant};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction, types::Uuid};

/// One leased report. A worker must present this lease on every write.
#[derive(Debug)]
pub struct ReportJob {
    pub id: Uuid,
    pub user_id: Uuid,
    pub lease: Uuid,
    pub input: Value,
    pub source_hash: String,
    pub attempt: i32,
}

/// Indexed, tenant-bound reconstruction of a submitted ordinary attempt.
pub async fn attempt(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    task: &str,
    problem: &str,
    attempt: &str,
) -> Result<Option<Value>, StoreError> {
    Ok(sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object('event_seq', a.seq, 'attempt', a.payload)
         FROM events a WHERE a.user_id=$1 AND ($4='' OR a.attempt_id=$4) AND a.type='attempt'
           AND a.payload->>'task_id'=$2
           AND a.payload->>'task_type' IN ('lesson','review','drill','quiz','multi-step')
           AND (SELECT h.payload->>'problem_id' FROM events h
                WHERE h.user_id=a.user_id AND h.type='ordinary_problem_served'
                  AND h.payload->>'task_id'=$2 AND h.seq<a.seq
                ORDER BY h.seq DESC LIMIT 1)=$3
         ORDER BY a.seq DESC LIMIT 1",
    )
    .bind(user)
    .bind(task)
    .bind(problem)
    .bind(attempt)
    .fetch_optional(&mut **tx)
    .await?)
}

/// Prior request, active review, or final successful review of this attempt.
pub async fn existing(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    request: Uuid,
    attempt: &str,
) -> Result<Option<Value>, StoreError> {
    Ok(sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(r) FROM problem_reports r WHERE user_id=$1
         AND (request_id=$2 OR (attempt_id=$3 AND status IN ('queued','running','completed')))
         ORDER BY (request_id=$2) DESC,created_at DESC LIMIT 1",
    )
    .bind(user)
    .bind(request)
    .bind(attempt)
    .fetch_optional(&mut **tx)
    .await?)
}

/// Bound user retries and global queue growth outside model control.
pub async fn capacity(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    attempt: &str,
) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT count(*) FILTER (WHERE created_at>now()-interval '1 day')<20
             AND count(*) FILTER (WHERE status IN ('queued','running'))<3
             AND count(*) FILTER (WHERE attempt_id=$2)<3
         FROM problem_reports WHERE user_id=$1",
    )
    .bind(user)
    .bind(attempt)
    .fetch_one(&mut **tx)
    .await?)
}

/// Insert server-reconstructed evidence. The tenant advisory lock serializes retries.
#[expect(
    clippy::too_many_arguments,
    reason = "The insert explicitly binds the seven immutable report columns and its transaction."
)]
pub async fn enqueue(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    request: Uuid,
    task: &str,
    problem: &str,
    attempt: &str,
    hash: &str,
    input: &Value,
) -> Result<Value, StoreError> {
    Ok(sqlx::query_scalar::<_, Value>(
        "INSERT INTO problem_reports(user_id,request_id,task_id,problem_id,attempt_id,source_hash,input)
         VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING to_jsonb(problem_reports)"
    ).bind(user).bind(request).bind(task).bind(problem).bind(attempt).bind(hash).bind(input)
     .fetch_one(&mut **tx).await?)
}

/// Poll exactly one tenant-owned report.
pub async fn get(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    report: Uuid,
) -> Result<Option<Value>, StoreError> {
    Ok(sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(r) FROM problem_reports r WHERE user_id=$1 AND id=$2",
    )
    .bind(user)
    .bind(report)
    .fetch_optional(&mut **tx)
    .await?)
}

/// The most recent immutable correction for this exact source.
pub async fn published(
    tx: &mut Transaction<'_, Postgres>,
    hash: &str,
) -> Result<Option<Value>, StoreError> {
    Ok(sqlx::query_scalar::<_, Value>(
        "SELECT jsonb_build_object('version',version,'body',body)
         FROM problem_corrections WHERE source_hash=$1 ORDER BY version DESC LIMIT 1",
    )
    .bind(hash)
    .fetch_optional(&mut **tx)
    .await?)
}

/// Claim one job. A shared advisory lock enforces one active Qwen job globally.
pub async fn claim(db: &Db) -> Result<Option<ReportJob>, StoreError> {
    let mut tx = db.pool().begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(1128350805, 22)")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE problem_reports SET status='failed',stage='worker_interrupted',
         result=jsonb_build_object('resolution','needs_review','message',
           'The review stopped before verification completed. You can retry this report.',
           'qwen_verdict','ambiguous','verification','unresolved',
           'grade_corrected',false,'content_published',false),
         lease=NULL,lease_until=NULL,updated_at=now()
         WHERE status='running' AND lease_until<now() AND attempt>=3",
    )
    .execute(&mut *tx)
    .await?;
    let lease = Uuid::new_v4();
    let row = sqlx::query_scalar::<_, Value>(
        "UPDATE problem_reports SET status='running',stage='starting',attempt=attempt+1,
             lease=$1,lease_until=now()+interval '2 minutes',updated_at=now()
         WHERE id=(SELECT id FROM problem_reports
             WHERE (status='queued' OR (status='running' AND lease_until<now())) AND attempt<3
             AND NOT EXISTS (SELECT 1 FROM problem_reports busy WHERE busy.status='running'
                             AND busy.lease_until>=now())
             ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT 1)
         RETURNING to_jsonb(problem_reports)",
    )
    .bind(lease)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    row.map(|row| {
        Ok(ReportJob {
            id: uuid_field(&row, "id")?,
            user_id: uuid_field(&row, "user_id")?,
            lease,
            input: row["input"].clone(),
            source_hash: row["source_hash"].as_str().unwrap_or_default().to_owned(),
            attempt: i32::try_from(row["attempt"].as_i64().unwrap_or_default())
                .map_err(|_| StoreError::Document("invalid report attempt count".into()))?,
        })
    })
    .transpose()
}

fn uuid_field(row: &Value, key: &str) -> Result<Uuid, StoreError> {
    row[key]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| StoreError::Document("invalid report identity".into()))
}

/// Renew only a still-owned, unexpired lease.
pub async fn heartbeat(db: &Db, job: &ReportJob, stage: &str) -> Result<bool, StoreError> {
    Ok(sqlx::query(
        "UPDATE problem_reports SET stage=$3,lease_until=now()+interval '2 minutes',updated_at=now()
         WHERE id=$1 AND lease=$2 AND status='running' AND lease_until>now()"
    ).bind(job.id).bind(job.lease).bind(stage).execute(db.pool()).await?.rows_affected()==1)
}

/// Append one bounded evidence step only while the lease remains live.
pub async fn record_step(
    db: &Db,
    job: &ReportJob,
    stage: &str,
    evidence: &Value,
) -> Result<bool, StoreError> {
    Ok(sqlx::query(
        "INSERT INTO problem_report_steps(report_id,stage,evidence)
         SELECT id,$3,$4 FROM problem_reports
         WHERE id=$1 AND lease=$2 AND status='running' AND lease_until>now()",
    )
    .bind(job.id)
    .bind(job.lease)
    .bind(stage)
    .bind(evidence)
    .execute(db.pool())
    .await?
    .rows_affected()
        == 1)
}

fn unresolved(message: &str) -> Value {
    json!({"resolution":"needs_review","message":message,"qwen_verdict":"ambiguous",
           "verification":"unresolved","grade_corrected":false,"content_published":false})
}

/// Commit the public decision, correction version, and native regrade atomically.
/// A stale source version or lease changes no learner history or content.
pub async fn finish(
    db: &Db,
    job: &ReportJob,
    result: &Value,
    correction: Option<&Value>,
) -> Result<bool, StoreError> {
    let mut tx = begin_tenant(db.pool(), job.user_id).await?;
    lock_web_state(&mut tx, job.user_id).await?;
    let owns = sqlx::query_scalar::<_, bool>(
        "SELECT true FROM problem_reports WHERE id=$1 AND lease=$2
         AND status='running' AND lease_until>now() FOR UPDATE",
    )
    .bind(job.id)
    .bind(job.lease)
    .fetch_optional(&mut *tx)
    .await?
    .unwrap_or(false);
    if !owns {
        return Ok(false);
    }
    let mut answer = result.clone();
    answer["grade_corrected"] = json!(false);
    answer["corrected_outcome"] = Value::Null;
    answer["content_published"] = json!(false);
    if let Some(proposed) = correction {
        let mut body = proposed.clone();
        let previous = published(&mut tx, &job.source_hash).await?;
        let version = previous
            .as_ref()
            .and_then(|p| p["version"].as_i64())
            .unwrap_or(0);
        let expected_version = job.input["correction_version"].as_i64().unwrap_or(0);
        let supported = answer["qwen_verdict"] == "correct"
            && answer["verification"] == "proved"
            && answer["resolution"] == "confirmed_issue"
            && body["verification"]["supported"] == true
            && body["verification"]["status"] == "completed"
            && body["verification"]["learner"]["status"] == "proved"
            && body["verification"]["candidate"]["status"] == "proved"
            && body["verification"]["source_hash"] == job.source_hash;
        let same_formal = previous
            .as_ref()
            .is_none_or(|p| p["body"]["formal_problem"] == body["formal_problem"]);
        if version != expected_version || !same_formal {
            answer =
                unresolved("The content changed during review. Please report the current version.");
        } else if !supported {
            answer = unresolved(
                "The report did not provide the evidence required for automatic publication.",
            );
        } else {
            let mut allowed = body["accepted_answers"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if let Some(old) = previous
                .as_ref()
                .and_then(|p| p["body"]["accepted_answers"].as_array())
            {
                for entry in old {
                    if !allowed.contains(entry) {
                        allowed.push(entry.clone());
                    }
                }
            }
            if allowed.len() > 128 {
                answer = unresolved("This correction needs a broader application-code repair.");
            } else {
                body["accepted_answers"] = Value::Array(allowed);
                let inserted = sqlx::query(
                    "INSERT INTO problem_corrections(source_hash,version,report_id,body)
                     VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
                )
                .bind(&job.source_hash)
                .bind(version + 1)
                .bind(job.id)
                .bind(&body)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                    == 1;
                if !inserted {
                    answer =
                        unresolved("A newer correction was published during review. Please retry.");
                } else {
                    let (corrected, summary) = task_outcomes::complete_submission(
                        &mut tx,
                        job.user_id,
                        &job.input,
                        job.id,
                    )
                    .await?;
                    answer["task_recalculation"] = summary;
                    if let Some(corrected) = corrected {
                        append_event(&mut tx, job.user_id, &corrected, None).await?;
                        answer["grade_corrected"] = json!(true);
                        answer["corrected_outcome"] = json!("correct");
                    }
                    answer["content_published"] = json!(true);
                }
            }
        }
    }
    let status = if answer["resolution"] == "needs_review" {
        "unresolved"
    } else {
        "completed"
    };
    sqlx::query(
        "UPDATE problem_reports SET status=$3,stage=$3,result=$4,lease=NULL,lease_until=NULL,updated_at=now()
         WHERE id=$1 AND lease=$2"
    ).bind(job.id).bind(job.lease).bind(status).bind(&answer).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}
