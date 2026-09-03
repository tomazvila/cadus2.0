//! T6: the `model_call_log` writer, one row per HTTP attempt (spec section 7).
//!
//! Requirements: T2 (only the worker and the offline authoring pipeline spend
//! tokens), T6 (every model call logs its model id, its cached and uncached
//! input tokens, its output tokens, its reasoning tokens, its latency and its
//! purpose; cost is a dashboard, not a surprise).
//!
//! # One row per HTTP attempt, not per job
//!
//! A truncation retry is two calls and two bills, so a call that retried writes
//! two rows. [`cadus_model_client::Call`] already carries one [`Attempt`] per
//! HTTP attempt; this module turns each one into a row.
//!
//! # An unmeasured call is visible as unmeasured
//!
//! Every field of the `usage` block is optional on some provider. The client
//! reads each one with a default of 0, so a reply with NO `usage` block writes a
//! row of zeros with a NULL cost. It never writes no row: a dropped row hides a
//! call that the operator paid for.
//!
//! # Who writes it
//!
//! The worker, as `cadus_admin`. `cadus_app` holds no privilege on the table and
//! none on its sequence (`docs/SCHEMA.md`, findings #5 and #12), which is why
//! the table stays outside row-level security.
//!
//! # A failed ledger write never costs the learner the diagnosis
//!
//! The caller writes the ledger BEFORE it settles the job row, so the bill is
//! recorded first. A ledger write that fails is logged at `error` level with the
//! fields of the row in the log line, and the pass goes on to settle the job:
//! the learner keeps the diagnosis that was already paid for, and the bill is
//! recoverable from the log.

use cadus_model_client::{Attempt, BACKOFF_MS, Usage};
use cadus_store::Db;
use sqlx::types::Uuid;

use crate::WorkerError;

/// The `purpose` of a diagnosis call (spec section 7).
pub const PURPOSE_DIAGNOSIS: &str = "diagnosis";

/// The `purpose` of an offline authoring call (T2). T2 names no third spender.
pub const PURPOSE_AUTHORING: &str = "authoring";

/// The first value `cost_usd numeric(12,6)` cannot hold.
///
/// The column keeps six digits before the point. A larger number raises
/// `numeric field overflow`, which rolls the whole ledger write back, so a
/// value at or above this bound is stored as NULL instead.
pub const COST_BOUND: f64 = 1_000_000.0;

/// The longest `usage.cost` text the writer accepts, in bytes.
pub const COST_MAX_CHARS: usize = 40;

/// What the ledger records about the CALLER of one model call.
///
/// The tenant and the session come from the job, not from the reply, so one row
/// joins a bill to the learner who caused it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallRecord<'a> {
    /// [`PURPOSE_DIAGNOSIS`] or [`PURPOSE_AUTHORING`].
    pub purpose: &'a str,
    /// The tenant the job belongs to. `None` for an offline authoring call.
    pub user_id: Option<Uuid>,
    /// The attempt's session. It feeds the per-session T4 count.
    pub session_id: Option<&'a str>,
}

/// The visible output tokens of one reply (spec section 7, `output_tokens`).
///
/// The column wants `usage.completion_tokens` minus reasoning WHERE THE PROVIDER
/// DOUBLE-COUNTS. Two shapes exist and the numbers themselves tell them apart:
///
/// - `reasoning <= completion`: the provider counts the hidden reasoning inside
///   `completion_tokens` (the OpenAI shape), so the visible output is the
///   difference. A reply that spent its whole budget on reasoning reports 0
///   visible tokens, which is what a truncated forced tool call is.
/// - `reasoning > completion`: the provider counts the two apart, measured live
///   at `completion_tokens = 1024` with `reasoning_tokens = 1083`. A difference
///   is meaningless there, so `completion_tokens` stands as it is.
///
/// Under both rules `output_tokens` and `reasoning_tokens` count disjoint
/// tokens, so the gross completion of a reply is their sum.
#[must_use]
pub fn visible_output(usage: &Usage) -> u32 {
    if usage.reasoning <= usage.output {
        usage.output - usage.reasoning
    } else {
        usage.output
    }
}

/// The exact money text of one attempt, or `None` when the cost is unknown.
///
/// [`Attempt::cost_usd`] is the TEXT of `usage.cost` and never a float, so the
/// value reaches `numeric(12,6)` without one binary rounding. This function is
/// the guard between that text and the column: a value the column cannot hold
/// raises inside the transaction and rolls back rows that ARE measured, so
/// anything outside the guard becomes NULL. `cost_usd` is nullable for exactly
/// this reason — an unknown cost is a NULL, never a guess.
#[must_use]
pub fn money(raw: &str) -> Option<String> {
    let text = raw.trim().trim_matches('"').trim();
    if text.is_empty() || text.len() > COST_MAX_CHARS {
        return None;
    }
    let value: f64 = text.parse().ok()?;
    if !value.is_finite() || value.abs() >= COST_BOUND {
        return None;
    }
    Some(text.to_owned())
}

/// How long before the write each attempt STARTED, in milliseconds.
///
/// The `ts` column is the call start (spec section 7) and the rows are written
/// after the last attempt, so each row carries its own age. The client waits
/// `BACKOFF_MS << index` between one attempt and the next, so a walk backwards
/// over the list adds each latency and each backoff in turn. The two rows of one
/// truncation retry then carry two different `ts` values, in the order the
/// attempts ran.
#[must_use]
pub fn started_ms_ago(attempts: &[Attempt]) -> Vec<u64> {
    let mut ages = Vec::with_capacity(attempts.len());
    let mut tail = 0_u64;
    for (position, attempt) in attempts.iter().enumerate().rev() {
        tail = tail.saturating_add(u64::from(attempt.latency_ms));
        ages.push(tail);
        if let Some(before) = position
            .checked_sub(1)
            .and_then(|prior| attempts.get(prior))
        {
            tail = tail.saturating_add(backoff_ms(before.index));
        }
    }
    ages.reverse();
    ages
}

/// The backoff the client waits after the attempt with this index.
fn backoff_ms(index: u32) -> u64 {
    BACKOFF_MS.checked_shl(index).unwrap_or(u64::MAX)
}

/// A `u32` token count as the `integer` the column holds.
fn tokens(count: u32) -> i32 {
    i32::try_from(count).unwrap_or(i32::MAX)
}

/// Write one ledger row per attempt, in one transaction (T6).
///
/// The rows of one call land together or not at all: a partial ledger reads as a
/// cheaper call than the one the operator paid for.
///
/// Returns how many rows the write appended.
///
/// # Errors
///
/// Returns [`WorkerError::Db`] when a statement fails or the transaction does
/// not commit.
pub async fn write(
    db: &Db,
    record: &CallRecord<'_>,
    attempts: &[Attempt],
) -> Result<u64, WorkerError> {
    if attempts.is_empty() {
        return Ok(0);
    }
    let ages = started_ms_ago(attempts);
    let mut tx = db.pool().begin().await?;
    let mut written = 0_u64;
    for (position, attempt) in attempts.iter().enumerate() {
        #[expect(
            clippy::cast_precision_loss,
            reason = "an age in milliseconds is far below 2**53"
        )]
        let age_secs = ages.get(position).copied().unwrap_or(0) as f64 / 1000.0;
        let cost = attempt.cost_usd.as_deref().and_then(money);
        sqlx::query!(
            r#"
            INSERT INTO model_call_log
                (ts, purpose, model_id, provider, user_id, session_id,
                 input_tokens_cached, input_tokens_uncached, output_tokens,
                 reasoning_tokens, latency_ms, cost_usd, request_id)
            VALUES (now() - make_interval(secs => $1), $2, $3, $4, $5, $6,
                    $7, $8, $9, $10, $11, $12::text::numeric, $13)
            "#,
            age_secs,
            record.purpose,
            attempt.model_id,
            attempt.provider,
            record.user_id,
            record.session_id,
            tokens(attempt.usage.input_cached),
            tokens(attempt.usage.input_uncached),
            tokens(visible_output(&attempt.usage)),
            tokens(attempt.usage.reasoning),
            tokens(attempt.latency_ms),
            cost,
            attempt.request_id,
        )
        .execute(&mut *tx)
        .await?;
        written += 1;
    }
    tx.commit().await?;
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::{money, started_ms_ago, visible_output};
    use cadus_model_client::{Attempt, Usage};

    /// One attempt record with this index and latency. Nothing else is read.
    fn attempt(index: u32, latency_ms: u32) -> Attempt {
        Attempt {
            index,
            max_tokens: 600,
            status: 200,
            latency_ms,
            usage: Usage::default(),
            model_id: "qwen3.6".to_owned(),
            provider: None,
            request_id: None,
            cost_usd: None,
        }
    }

    /// The OpenAI shape: reasoning sits INSIDE `completion_tokens`.
    #[test]
    fn a_double_counted_reply_reports_the_visible_output() {
        let usage = Usage {
            input_cached: 0,
            input_uncached: 0,
            output: 60,
            reasoning: 40,
        };
        assert_eq!(visible_output(&usage), 20);
    }

    /// A reply that spent its whole budget on reasoning shows no visible output.
    #[test]
    fn a_reply_that_is_all_reasoning_reports_no_visible_output() {
        let usage = Usage {
            input_cached: 0,
            input_uncached: 0,
            output: 1024,
            reasoning: 1024,
        };
        assert_eq!(visible_output(&usage), 0);
    }

    /// The measured live shape: the two counts are apart, so neither is a part
    /// of the other and `completion_tokens` stands.
    #[test]
    fn a_provider_that_counts_them_apart_keeps_completion_tokens() {
        let usage = Usage {
            input_cached: 0,
            input_uncached: 0,
            output: 1024,
            reasoning: 1083,
        };
        assert_eq!(visible_output(&usage), 1024);
    }

    /// The money guard: the exact text passes, and nothing the column refuses
    /// reaches it.
    #[test]
    fn the_money_guard_keeps_the_exact_text() {
        assert_eq!(money("0.00123456"), Some("0.00123456".to_owned()));
        assert_eq!(money("\"0.0012\""), Some("0.0012".to_owned()));
        assert_eq!(money("1e-6"), Some("1e-6".to_owned()));
        assert_eq!(money("null"), None);
        assert_eq!(money(""), None);
        assert_eq!(money("free"), None);
        assert_eq!(money("NaN"), None);
        assert_eq!(money("1000000"), None);
        assert_eq!(money(&"9".repeat(41)), None);
    }

    /// One attempt is as old as its own latency.
    #[test]
    fn one_attempt_is_as_old_as_its_latency() {
        assert_eq!(started_ms_ago(&[attempt(0, 900)]), vec![900]);
        assert!(started_ms_ago(&[]).is_empty());
    }

    /// Two attempts: the first one is older by its own latency plus the 500 ms
    /// backoff the client waited after it.
    #[test]
    fn a_retry_gives_the_first_attempt_the_older_stamp() {
        let ages = started_ms_ago(&[attempt(0, 700), attempt(1, 300)]);
        assert_eq!(ages, vec![1500, 300]);
    }
}
