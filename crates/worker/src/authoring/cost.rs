//! T3 accounting: what one knowledge point cost to author (T3, T6).
//!
//! Spec: `docs/reference/authoring-and-spa-1.0-spec.md` section 2.2, paragraph
//! "T3 — authoring cost per KP", and row R3 of section 7.
//!
//! # The three facts T3 asks for
//!
//! 1. **Every attempt writes one `model_call_log` row** with
//!    `purpose = 'authoring'` and `user_id = NULL`. [`crate::model_log::write`]
//!    writes it, and [`crate::authoring::job::author_one`] calls that writer
//!    before it stores the document, so no paid call is recorded as free.
//! 2. **The stored row carries the bill.** `content_store.authoring_attempts` is
//!    the count of model calls the pass spent, and `authoring_cost_usd` is the
//!    sum of what those calls cost.
//! 3. **A knowledge point above [`ATTEMPT_ALERT`] attempts alerts.** T3 names
//!    the number: "alert when a KP exceeds 3 authoring attempts". The alert does
//!    not stop the job (spec section 2.2, step 3).
//!
//! # Why the money never becomes a float
//!
//! `usage.cost` reaches this module as the TEXT of the reply body, and both
//! money columns are `numeric(12,6)`. The sum therefore runs in Postgres, over
//! the same texts the ledger rows hold, and no step of the path parses a price
//! into an `f64`. [`spend`] is the guard in front of that cast: a text it
//! returns is a decimal number the column can hold, so the `::numeric` cast
//! inside the INSERT raises nothing and never rolls a stored document back.
//!
//! # Why the sum drops an unpriced call
//!
//! A provider that reports no `usage.cost` writes a ledger row with a NULL
//! `cost_usd`, because an unknown price is a NULL and never a guess
//! (`crate::model_log`). SQL `sum()` skips a NULL, so this module skips it too,
//! and the row's cost stays the sum of the KP's call rows in every case. A pass
//! whose calls all report no price stores a NULL cost, not a zero: zero is a
//! measurement, NULL is the absence of one. The attempt count is exact
//! regardless, so `authoring_attempts` is the honest reading when the money is
//! NULL.
//!
//! # The sum runs in Postgres
//!
//! [`job::store_pending`](crate::authoring::job::store_pending) carries this
//! expression inside its INSERT, because a query macro takes a literal and never
//! a constant. It reads the array of [`spend`] texts:
//!
//! ```sql
//! (SELECT CASE WHEN abs(sum(round(c::numeric, 6))) < 1000000
//!              THEN sum(round(c::numeric, 6)) END
//!    FROM unnest($7::text[]) AS c)
//! ```
//!
//! - NULL for an empty array, because SQL `sum()` of no row is NULL;
//! - NULL for a total the `numeric(12,6)` column cannot hold, because an
//!   overflow raises inside the transaction and would roll the stored document
//!   back;
//! - the exact total otherwise, with every term rounded to [`MONEY_SCALE`]
//!   places first, so the total equals the sum of the ledger rows of that pass.
//!
//! The integration test `authoring_cost.rs` runs the expression itself over all
//! four cases.

use cadus_model_client::Attempt;
use cadus_store::Db;

use crate::WorkerError;
use crate::authoring::prompt::Kind;
use crate::model_log::money;

/// The authoring attempts one knowledge point gets before the pass alerts (T3).
///
/// T3: "Track cost per KP; alert when a KP exceeds 3 authoring attempts." The
/// bound is the count of model calls of one pass, not of one document: a pass
/// that lands on attempt 4 alerts, and so does a pass that declines after 5.
pub const ATTEMPT_ALERT: u32 = 3;

/// [`ATTEMPT_ALERT`] as the `integer` the `authoring_attempts` column holds.
///
/// A query binds this one. The test `the_two_bounds_are_one_number` proves the
/// two constants are the same number.
const ALERT_BOUND: i32 = 3;

/// The scale of `content_store.authoring_cost_usd` and `model_call_log.cost_usd`.
///
/// Both columns are `numeric(12,6)`, so both round a price to six decimal
/// places. The sum rounds every term the same way, which is what makes the
/// stored total equal to the sum of the ledger rows term by term.
pub const MONEY_SCALE: i32 = 6;

/// One document whose authoring passed the T3 attempt bound.
///
/// The operator reads this row: the knowledge point, what it cost, and how many
/// calls it took. The A6 flag `needs_template` names a knowledge point with no
/// approved template; this one names a knowledge point that FOUGHT the gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    /// The serving key, `"<topic_id>/<kp_id>"`.
    pub kp_id: String,
    /// The `content_store.kind` of the document.
    pub kind: String,
    /// The content address of the document (C6).
    pub digest: String,
    /// The model calls the pass that stored it spent.
    pub attempts: i32,
    /// The exact text of `authoring_cost_usd`, or `None` when no call reported a
    /// price.
    pub cost_usd: Option<String>,
}

/// Whether this attempt count is above the T3 bound.
#[must_use]
pub const fn alerts(attempts: u32) -> bool {
    attempts > ATTEMPT_ALERT
}

/// The prices of one pass, one text per HTTP attempt that reported one (T6).
///
/// The order is the order of the attempts. An attempt with no price, or with a
/// price the money column cannot hold, contributes no term, exactly as its NULL
/// ledger row contributes no term to SQL `sum()`.
///
/// Every text this function returns is made of the characters of a decimal
/// number, so the `::numeric` cast of the stored sum is total.
#[must_use]
pub fn spend(attempts: &[Attempt]) -> Vec<String> {
    attempts
        .iter()
        .filter_map(|attempt| attempt.cost_usd.as_deref())
        .filter_map(money)
        .filter(|text| text.chars().all(is_decimal_char))
        .collect()
}

/// Whether this character can appear in a decimal number Postgres reads.
///
/// [`money`] proves the text parses as a finite number inside the column's
/// bound. This second pass proves the same text carries nothing else, so the
/// cast raises no `invalid input syntax for type numeric` inside the INSERT that
/// stores the document.
const fn is_decimal_char(c: char) -> bool {
    matches!(c, '0'..='9' | '.' | '+' | '-' | 'e' | 'E')
}

/// Raise the T3 operator alert for one pass, and report whether it fired.
///
/// The alert is a fact about money, so it goes out at `error` level with the
/// knowledge point, the kind, the attempts and the bill on the line. It stops
/// nothing: the pass has already stored its document or already declined.
pub fn raise(kp_id: &str, kind: Kind, attempts: u32, cost_usd: Option<&str>) -> bool {
    if !alerts(attempts) {
        return false;
    }
    tracing::error!(
        kp = kp_id,
        kind = kind.as_str(),
        attempts,
        bound = ATTEMPT_ALERT,
        cost_usd = cost_usd.unwrap_or("unknown"),
        "T3 alert: this knowledge point spent more than 3 authoring attempts"
    );
    true
}

/// Every stored document above the T3 attempt bound, dearest first (T3).
///
/// This is the operator's list. A DECLINED knowledge point is not in it, because
/// a decline stores no row: it reaches the operator as
/// [`BatchReport::declines`](crate::authoring::job::BatchReport) and as the A6
/// flag `needs_template`, and its spend is in the ledger.
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when the statement fails or the bound expires.
pub async fn alerting(db: &Db) -> Result<Vec<Alert>, WorkerError> {
    let query = sqlx::query!(
        r#"
        SELECT kp_id AS "kp_id!",
               kind AS "kind!",
               digest AS "digest!",
               authoring_attempts AS "attempts!",
               authoring_cost_usd::text AS "cost_usd?"
          FROM content_store
         WHERE authoring_attempts > $1
         ORDER BY authoring_attempts DESC, kp_id, kind, digest
        "#,
        ALERT_BOUND,
    )
    .fetch_all(db.pool());
    let rows = cadus_store::bounded(db, query).await?;
    Ok(rows
        .into_iter()
        .map(|row| Alert {
            kp_id: row.kp_id,
            kind: row.kind,
            digest: row.digest,
            attempts: row.attempts,
            cost_usd: row.cost_usd,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{ALERT_BOUND, ATTEMPT_ALERT, MONEY_SCALE, alerts, raise, spend};
    use crate::authoring::prompt::Kind;
    use cadus_model_client::{Attempt, Usage};

    /// One attempt record that reports this price. Nothing else is read.
    fn priced(cost_usd: Option<&str>) -> Attempt {
        Attempt {
            index: 0,
            max_tokens: 2_048,
            status: 200,
            latency_ms: 700,
            usage: Usage::default(),
            model_id: "qwen3.6".to_owned(),
            provider: None,
            request_id: None,
            cost_usd: cost_usd.map(str::to_owned),
        }
    }

    /// The T3 bound is 3, and the two spellings of it are one number.
    #[test]
    fn the_two_bounds_are_one_number() {
        assert_eq!(ATTEMPT_ALERT, 3);
        assert_eq!(ALERT_BOUND, 3);
        assert_eq!(i64::from(ALERT_BOUND), i64::from(ATTEMPT_ALERT));
        assert_eq!(MONEY_SCALE, 6);
    }

    /// Three attempts are inside the bound; four are above it.
    #[test]
    fn the_alert_starts_above_three_attempts() {
        assert!(!alerts(0));
        assert!(!alerts(1));
        assert!(!alerts(2));
        assert!(!alerts(3));
        assert!(alerts(4));
        assert!(alerts(5));
    }

    /// [`raise`] reports the alert it wrote, and stays quiet inside the bound.
    #[test]
    fn raise_fires_only_above_the_bound() {
        assert!(!raise("t/kp", Kind::Template, 3, Some("0.002")));
        assert!(raise("t/kp", Kind::Template, 4, Some("0.002")));
        assert!(raise("t/kp", Kind::Template, 5, None));
    }

    /// The spend of a pass keeps the exact text of every priced attempt, in
    /// order, and drops every attempt the money column cannot hold.
    #[test]
    fn the_spend_keeps_the_exact_texts_and_drops_the_unpriced() {
        let attempts = vec![
            priced(Some("0.0012")),
            priced(None),
            priced(Some("\"0.00034\"")),
            priced(Some("null")),
            priced(Some("free")),
            priced(Some("1000000")),
            priced(Some("0.5")),
        ];
        assert_eq!(
            spend(&attempts),
            vec!["0.0012".to_owned(), "0.00034".to_owned(), "0.5".to_owned()]
        );
        assert!(spend(&[]).is_empty());
        assert!(spend(&[priced(None)]).is_empty());
    }

    /// The cast the INSERT runs sees decimal characters only.
    #[test]
    fn every_kept_text_is_a_decimal_number() {
        let attempts = vec![
            priced(Some("1e-6")),
            priced(Some("+0.25")),
            priced(Some(".5")),
        ];
        for text in spend(&attempts) {
            assert!(
                text.chars()
                    .all(|c| c.is_ascii_digit() || matches!(c, '.' | '+' | '-' | 'e' | 'E')),
                "the cast would raise on {text:?}"
            );
        }
    }
}
