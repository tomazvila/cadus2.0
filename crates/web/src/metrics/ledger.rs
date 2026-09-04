//! The three ledger-backed series of spec section 7: what one scrape reads
//! from the two tables the worker writes, and how it renders them.

use std::collections::BTreeMap;

use cadus_store::{Db, bounded};

use super::{
    DIAGNOSIS_ENQUEUED, DIAGNOSIS_METRIC, DIAGNOSIS_PREAUTHORED, DIAGNOSIS_STATUSES,
    LEDGER_READ_BOUND, MODEL_LATENCY_METRIC, MODEL_TOKENS_METRIC, PURPOSES, TOKEN_KINDS, escape,
};

/// The token, latency and call totals of one `purpose` (T6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurposeTotals {
    /// `'diagnosis'` or `'authoring'`.
    pub purpose: String,
    /// `input_tokens_cached`, summed.
    pub cached: i64,
    /// `input_tokens_uncached`, summed.
    pub uncached: i64,
    /// `output_tokens`, summed.
    pub output: i64,
    /// `reasoning_tokens`, summed.
    pub reasoning: i64,
    /// `latency_ms`, summed.
    pub latency_ms: i64,
    /// How many HTTP attempts the ledger holds for this purpose.
    pub calls: i64,
}

/// What one scrape reads from the two tables the worker writes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LedgerTotals {
    /// One entry per `purpose` in `model_call_log`.
    pub model_calls: Vec<PurposeTotals>,
    /// One entry per `status` in `diagnosis_jobs`, as `(status, rows)`.
    pub jobs: Vec<(String, i64)>,
}

/// Read the ledger totals under [`LEDGER_READ_BOUND`].
///
/// `None` means the read failed or ran past the bound. The scrape then renders
/// the request series alone: a gap in a counter is a gap, and zeros read as a
/// counter reset that never happened.
pub async fn ledger_totals(db: &Db) -> Option<LedgerTotals> {
    match tokio::time::timeout(LEDGER_READ_BOUND, read_totals(db)).await {
        Ok(totals) => totals,
        Err(_) => {
            tracing::warn!(
                bound_ms = LEDGER_READ_BOUND.as_millis(),
                "metrics: the ledger read ran past its bound"
            );
            None
        }
    }
}

/// Read the ledger totals through the two SECURITY DEFINER aggregates.
async fn read_totals(db: &Db) -> Option<LedgerTotals> {
    let calls = sqlx::query!(
        r#"
        SELECT purpose AS "purpose!", input_cached AS "cached!",
               input_uncached AS "uncached!", output_tokens AS "output!",
               reasoning_tokens AS "reasoning!", latency_ms AS "latency_ms!",
               calls AS "calls!"
          FROM model_call_totals()
        "#
    )
    .fetch_all(db.pool());
    let calls = match bounded(db, calls).await {
        Ok(rows) => rows,
        Err(err) => {
            tracing::warn!(error = %err, "metrics: the model-call totals did not read");
            return None;
        }
    };

    let jobs =
        sqlx::query!(r#"SELECT status AS "status!", jobs AS "jobs!" FROM diagnosis_job_totals()"#)
            .fetch_all(db.pool());
    let jobs = match bounded(db, jobs).await {
        Ok(rows) => rows,
        Err(err) => {
            tracing::warn!(error = %err, "metrics: the diagnosis-job totals did not read");
            return None;
        }
    };

    Some(LedgerTotals {
        model_calls: calls
            .into_iter()
            .map(|row| PurposeTotals {
                purpose: row.purpose,
                cached: row.cached,
                uncached: row.uncached,
                output: row.output,
                reasoning: row.reasoning,
                latency_ms: row.latency_ms,
                calls: row.calls,
            })
            .collect(),
        jobs: jobs.into_iter().map(|row| (row.status, row.jobs)).collect(),
    })
}

/// Render the three ledger-backed series of spec section 7.
///
/// `preauthored` is the one label of [`DIAGNOSIS_METRIC`] that no row carries.
/// Every `purpose` of [`PURPOSES`] renders even at zero, and a purpose the
/// ledger holds beyond that list renders too, so a new spender is visible the
/// day it first spends.
#[must_use]
pub fn render_ledger(totals: &LedgerTotals, preauthored: u64) -> String {
    let mut out = String::new();

    let mut jobs: BTreeMap<&str, i64> = BTreeMap::new();
    for (status, count) in &totals.jobs {
        *jobs.entry(status.as_str()).or_default() += *count;
    }
    let enqueued: i64 = jobs.values().sum();

    out.push_str(&format!(
        "# HELP {DIAGNOSIS_METRIC} Async diagnosis jobs, by result.\n\
         # TYPE {DIAGNOSIS_METRIC} counter\n\
         {DIAGNOSIS_METRIC}{{result=\"{DIAGNOSIS_PREAUTHORED}\"}} {preauthored}\n\
         {DIAGNOSIS_METRIC}{{result=\"{DIAGNOSIS_ENQUEUED}\"}} {enqueued}\n"
    ));
    for status in DIAGNOSIS_STATUSES {
        out.push_str(&format!(
            "{DIAGNOSIS_METRIC}{{result=\"{status}\"}} {}\n",
            jobs.get(status).copied().unwrap_or(0)
        ));
    }

    let mut purposes: BTreeMap<&str, PurposeTotals> = BTreeMap::new();
    for purpose in PURPOSES {
        purposes.insert(
            purpose,
            PurposeTotals {
                purpose: purpose.to_string(),
                cached: 0,
                uncached: 0,
                output: 0,
                reasoning: 0,
                latency_ms: 0,
                calls: 0,
            },
        );
    }
    for row in &totals.model_calls {
        purposes.insert(row.purpose.as_str(), row.clone());
    }

    out.push_str(&format!(
        "# HELP {MODEL_TOKENS_METRIC} Model tokens spent, by purpose and kind.\n\
         # TYPE {MODEL_TOKENS_METRIC} counter\n"
    ));
    for (purpose, row) in &purposes {
        let label = escape(purpose);
        for (kind, count) in
            TOKEN_KINDS
                .iter()
                .zip([row.cached, row.uncached, row.output, row.reasoning])
        {
            out.push_str(&format!(
                "{MODEL_TOKENS_METRIC}{{purpose=\"{label}\",kind=\"{kind}\"}} {count}\n"
            ));
        }
    }

    out.push_str(&format!(
        "# HELP {MODEL_LATENCY_METRIC} Model-call wall clock in seconds, by purpose. One \
         observation per HTTP attempt.\n\
         # TYPE {MODEL_LATENCY_METRIC} summary\n"
    ));
    for (purpose, row) in &purposes {
        let label = escape(purpose);
        #[expect(
            clippy::cast_precision_loss,
            reason = "a latency sum in milliseconds is far below 2**53"
        )]
        let seconds = row.latency_ms as f64 / 1000.0;
        out.push_str(&format!(
            "{MODEL_LATENCY_METRIC}_sum{{purpose=\"{label}\"}} {seconds}\n\
             {MODEL_LATENCY_METRIC}_count{{purpose=\"{label}\"}} {}\n",
            row.calls
        ));
    }
    out
}
