//! The `content_store` reads and the one write of the batch loop (C6, T3).
//!
//! [`store_pending`] is the one place a verified document reaches the table,
//! and [`slots_taken`], [`stale_slots`] and [`served_instances`] are the reads
//! the loop takes before its first model call.

use std::fmt::Write as _;

use cadus_core::instruction::{ServedInstance, template_instances};
use cadus_store::Db;
use cadus_store::content::{
    Admin, KIND_TEMPLATE, NewDocument, Verdict, insert_pending, refresh_prompt_digest, verdict,
};
use serde_json::Value;

use super::{NO_REVIEW_REASON, SAME_BODY, STATUS_APPROVED, STATUS_PENDING, STATUS_REJECTED};
use crate::WorkerError;
use crate::authoring::job::document_digest;
use crate::authoring::prompt::{self, Kind};

/// Every instance the templates of one knowledge point serve (M6 review,
/// findings F2, F15 and F25; M6 review 2, finding V1).
///
/// The two instruction gates refuse a document that gives a served answer away.
/// The exemplars are one source of those answers; the templates are the other,
/// and they are the source the serve path reads first (A6 serves the exemplars
/// only when no template is approved). This read supplies the second source, and
/// [`InstructionSpec::instance_answers`] carries it into the gate.
///
/// # Why a PENDING template counts
///
/// `cadus-worker author` writes all four kinds in ONE process, template first
/// (`crate::authoring::prompt::KINDS`), and every kind enters `content_store` as
/// `pending`. A read of the `approved` rows alone therefore answered an EMPTY
/// list for every knowledge point of a fresh curriculum, and the hint gate of
/// that pass judged the ladder against the exemplars alone (M6 review 2, finding
/// V1). A pending template is the material the reviewer is about to approve, so
/// it gates the ladder and the page authored beside it.
///
/// A `rejected` template is not read: a human refused that body, and it serves
/// nothing.
///
/// `kp_id` is the serving key `cadus_core::pool::kp_key` writes.
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when the statement fails or the bound expires.
pub async fn served_instances(db: &Db, kp_id: &str) -> Result<Vec<ServedInstance>, WorkerError> {
    let query = sqlx::query_scalar!(
        r#"
        SELECT body::text AS "body!"
          FROM content_store
         WHERE kp_id = $1 AND kind = $2 AND status IN ($3, $4)
         ORDER BY approved_at DESC NULLS LAST, created_at DESC, digest
        "#,
        kp_id,
        KIND_TEMPLATE,
        STATUS_APPROVED,
        STATUS_PENDING,
    )
    .fetch_all(db.pool());
    let bodies: Vec<String> = cadus_store::bounded(db, query).await?;
    let mut served: Vec<ServedInstance> = Vec::new();
    for body in &bodies {
        for instance in template_instances(body) {
            if !served.contains(&instance) {
                served.push(instance);
            }
        }
    }
    Ok(served)
}

/// How many slots of one knowledge point and kind are occupied (C6).
///
/// An approved row serves; a pending row waits for a human. Both occupy a slot,
/// so a nightly pass never floods the review queue with a second copy of a
/// document nobody has read yet (1.0 `problem_templates.py:1352-1374`). A
/// `rejected` row occupies nothing: a human refused that body, and the knowledge
/// point still needs a document.
///
/// The pass that follows a rejection therefore runs again, and it can reproduce
/// the refused body. That pass stores nothing, and [`author_one`] answers
/// [`Outcome::Rejected`] with the reviewer's reason: a re-author of a refused
/// body is a decline, never a store (M6 review finding F6).
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when the statement fails or the bound expires.
pub async fn slots_taken(db: &Db, kp_id: &str, kind: Kind) -> Result<i64, WorkerError> {
    let query = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!"
          FROM content_store
         WHERE kp_id = $1 AND kind = $2 AND status IN ($3, $4)
        "#,
        kp_id,
        kind.as_str(),
        STATUS_APPROVED,
        STATUS_PENDING,
    )
    .fetch_one(db.pool());
    Ok(cadus_store::bounded(db, query).await?)
}

/// How many slots of one knowledge point and kind an OLDER prompt wrote (C6).
///
/// Spec section 2.2, "Prompt digest": 1.0 folds the prompt digest into the cache
/// key, so a prompt edit retires every stored template
/// (`problem_templates.py:1129-1163`). 2.0 keeps the digest on the row, because
/// the C6 approval binds to the content. A prompt edit therefore does not
/// unapprove anything; it marks the row for re-authoring, and this count is that
/// mark.
///
/// A row with a NULL `prompt_digest` is NOT stale. NULL means "the prompt is not
/// recorded", which every row written before migration `0012` carries, and a
/// pass that read NULL as stale would re-author the whole bank on the first run
/// after the deployment (M6 review finding F4).
///
/// The count covers `approved` AND `pending` rows, exactly as [`slots_taken`]
/// does, so [`author_one`] subtracts one count from the other and reads the
/// slots the CURRENT prompt holds.
///
/// A re-author that reproduces the identical body clears the mark of its row:
/// [`store_pending`] stamps the current prompt on the held row, so the count
/// drops by one and the next pass makes no call (M6 review finding V3).
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when the statement fails or the bound expires.
pub async fn stale_slots(db: &Db, kp_id: &str, kind: Kind) -> Result<i64, WorkerError> {
    let current = prompt::prompt_digest(kind);
    let query = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!"
          FROM content_store
         WHERE kp_id = $1 AND kind = $2 AND status IN ($3, $4)
           AND prompt_digest IS NOT NULL AND prompt_digest <> $5
        "#,
        kp_id,
        kind.as_str(),
        STATUS_APPROVED,
        STATUS_PENDING,
        current,
    )
    .fetch_one(db.pool());
    Ok(cadus_store::bounded(db, query).await?)
}

/// One approved row an older prompt wrote (spec section 2.2, "Prompt digest").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleRow {
    /// The serving key `"<topic_id>/<kp_id>"` of the row.
    pub kp_id: String,
    /// The kind of the row.
    pub kind: Kind,
    /// The content address of the row. The approval still binds to it.
    pub digest: String,
    /// The prompt digest the row carries.
    pub prompt_digest: String,
}

/// Every APPROVED row of these kinds that an older prompt wrote (C6).
///
/// This is the read behind `cadus-worker author --stale`. It lists the approved
/// rows alone, because those are the rows a learner is served: a `pending` row
/// an older prompt wrote is already in front of a reviewer, and the reviewer
/// reads the body and not the prompt.
///
/// The order is the kind of [`KINDS`](crate::authoring::prompt::KINDS), then the
/// serving key, then the digest, so two runs over one table print one text.
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when a statement fails or the bound expires.
pub async fn stale_rows(db: &Db, kinds: &[Kind]) -> Result<Vec<StaleRow>, WorkerError> {
    let mut rows = Vec::new();
    for kind in kinds {
        let current = prompt::prompt_digest(*kind);
        let query = sqlx::query!(
            r#"
            SELECT kp_id AS "kp_id!", digest AS "digest!", prompt_digest AS "prompt_digest!"
              FROM content_store
             WHERE kind = $1 AND status = $2
               AND prompt_digest IS NOT NULL AND prompt_digest <> $3
             ORDER BY kp_id, digest
            "#,
            kind.as_str(),
            STATUS_APPROVED,
            current,
        )
        .fetch_all(db.pool());
        for row in cadus_store::bounded(db, query).await? {
            rows.push(StaleRow {
                kp_id: row.kp_id,
                kind: *kind,
                digest: row.digest,
                prompt_digest: row.prompt_digest,
            });
        }
    }
    Ok(rows)
}

/// The stale rows, as the text `cadus-worker author --stale` prints on stdout.
///
/// One line names the columns, one line follows per row, and the last line
/// counts them. Every field is fixed, so an operator reads the text and a test
/// asserts it.
#[must_use]
pub fn render_stale(rows: &[StaleRow]) -> String {
    let mut text = String::from("stale documents\nkp_id kind digest prompt_digest\n");
    for row in rows {
        let _ = writeln!(
            text,
            "{} {} {} {}",
            row.kp_id,
            row.kind.as_str(),
            row.digest,
            row.prompt_digest
        );
    }
    let _ = writeln!(text, "stale: rows {}", rows.len());
    text
}

/// The money of one authoring pass, as the text `content_store` stores (T3).
///
/// The sum runs in Postgres and every term rounds to [`MONEY_SCALE`](crate::authoring::cost::MONEY_SCALE) first, so
/// the total matches the sum of the ledger rows of the same calls and no step
/// passes through a float (`crate::authoring::cost`).
///
/// The answer is `None` when the pass reported no price at all, and `None` when
/// the total is too large for `numeric(12,6)`. An overflow inside the INSERT
/// would raise and lose the document, so the guard runs here and the row keeps
/// its attempt count with a NULL bill.
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when the statement fails or the bound expires.
async fn total(db: &Db, spend: &[String]) -> Result<Option<String>, WorkerError> {
    let query = sqlx::query_scalar!(
        r#"
        SELECT (CASE WHEN abs(sum(round(c::numeric, 6))) < 1000000
                     THEN sum(round(c::numeric, 6)) END)::text AS "total?"
          FROM unnest($1::text[]) AS c
        "#,
        spend,
    )
    .fetch_one(db.pool());
    Ok(cadus_store::bounded(db, query).await?)
}

/// What the insert of one authored document did (C6, T3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    /// The content address of the document: [`document_digest`] of the
    /// knowledge point, the kind and the body.
    pub digest: String,
    /// `true` when the row is new, `false` when the table already held the
    /// digest.
    pub inserted: bool,
    /// What the table already holds for the digest, when the row is not new.
    ///
    /// `None` when the row is new. A `rejected` status here is the reviewer's
    /// refusal of this exact body (M6 review finding F6).
    pub verdict: Option<Verdict>,
    /// `true` when the held row named an older prompt and now names the current
    /// one (M6 review finding V3).
    ///
    /// `false` for a new row, for a row that already named the current prompt,
    /// for a row with no prompt stamp, and for a row a reviewer refused.
    pub refreshed: bool,
    /// The exact text of `authoring_cost_usd` on the new row.
    ///
    /// `None` in two cases: the row is not new, or no call of the pass reported
    /// a price.
    pub cost_usd: Option<String>,
}

impl Stored {
    /// Whether the held row is one a reviewer REFUSED (C6).
    ///
    /// `false` for a new row and for a held row that is `pending` or
    /// `approved`.
    #[must_use]
    pub fn refused(&self) -> bool {
        self.verdict
            .as_ref()
            .is_some_and(|held| held.status == STATUS_REJECTED)
    }

    /// The decline reason of a pass that reproduced a body a reviewer refused
    /// (C6).
    ///
    /// The text leads with [`SAME_BODY`] and carries the reviewer's own reason,
    /// so the decline record gives the operator the reason of the refusal.
    #[must_use]
    pub fn same_body_reason(&self) -> String {
        let reason = self
            .verdict
            .as_ref()
            .and_then(|held| held.review_reason.as_deref())
            .unwrap_or(NO_REVIEW_REASON);
        format!("{SAME_BODY}: {reason}")
    }
}

/// Insert one verified document as `pending`, with its bill (C6, T3; spec
/// section 2.2, step 4).
///
/// The insert itself is [`cadus_store::content::insert_pending`], the one write
/// path of `content_store` (unit R4). The worker connects as `cadus_admin`, and
/// `cadus_app` holds SELECT only on the table (`docs/SCHEMA.md`, finding #14),
/// so the request tier can never write a row that already carries
/// `status = 'approved'`. [`Admin::new`] names that connection at the call site.
///
/// `attempts` is the count of model calls the pass spent, and `spend` is the
/// price of each of those calls that reported one
/// ([`cost::spend`](crate::authoring::cost::spend)). [`total`] sums the prices
/// in Postgres, so the money reaches `numeric(12,6)` with no float step
/// (`crate::authoring::cost`, section "The sum runs in Postgres").
///
/// [`Stored::inserted`] is `true` when the row is new. A digest the table
/// already holds is not an error and not a rewrite: the document is the same
/// document of the same knowledge point, and a human may have rejected it
/// already, so `ON CONFLICT DO NOTHING` leaves that verdict AND the earlier
/// row's accounting alone. The second pass paid for its own calls, and the
/// ledger holds that spend.
///
/// A collision then reads [`cadus_store::content::verdict`] and reports it in
/// [`Stored::verdict`], so the caller tells a duplicate document from a body a
/// reviewer refused. A row another transaction wrote and did not commit yet is
/// invisible to that read, which answers `None`; the caller then reports a
/// duplicate, which is what a `pending` collision is.
///
/// # The prompt stamp of a collision (M6 review finding V3)
///
/// A collision that is not a refusal stamps the CURRENT prompt on the held row
/// ([`cadus_store::content::refresh_prompt_digest`]), and [`Stored::refreshed`]
/// reports the stamp. The re-author of a stale row reproduced the identical
/// body, so the current prompt writes that body and the row is no longer stale.
///
/// The stamp is what ends the re-authoring loop. Without it the held row kept
/// the old stamp, [`stale_slots`] counted it on every run, [`author_one`]
/// re-authored it on every run, and the operator paid for one model call per run
/// forever.
///
/// A row a reviewer REFUSED keeps its stamp: that collision is a decline, the
/// knowledge point still needs a document, and a `rejected` row occupies no slot
/// and is never stale (C6, M6 review finding F6).
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when the statement fails or the bound expires,
/// and [`WorkerError::Config`] when the verified body does not read as JSON.
pub async fn store_pending(
    db: &Db,
    kp_id: &str,
    kind: Kind,
    body: &str,
    attempts: u32,
    spend: &[String],
) -> Result<Stored, WorkerError> {
    let document: Value = serde_json::from_str(body).map_err(unreadable)?;
    let cost_usd = total(db, spend).await?;
    let digest = document_digest(kp_id, kind, body);
    let stamp = prompt::prompt_digest(kind);
    let doc = NewDocument {
        digest: &digest,
        kp_id,
        kind: kind.as_str(),
        body: &document,
        authoring_attempts: attempts,
        cost_usd: cost_usd.as_deref(),
        // Spec section 2.2, "Prompt digest": the row names the prompt that
        // wrote it, so a prompt edit marks the row for re-authoring and never
        // unapproves it (M6 review finding F4).
        prompt_digest: Some(&stamp),
    };
    let inserted = insert_pending(Admin::new(db), &doc).await?;
    let mut stored = Stored {
        digest,
        inserted,
        verdict: None,
        refreshed: false,
        cost_usd: None,
    };
    if inserted {
        stored.cost_usd = cost_usd;
        return Ok(stored);
    }
    stored.verdict = verdict(db, &stored.digest).await?;
    if !stored.refused() {
        stored.refreshed = refresh_prompt_digest(Admin::new(db), &stored.digest, &stamp).await?;
    }
    Ok(stored)
}

/// The error of a verified body that does not read as JSON.
fn unreadable(err: serde_json::Error) -> WorkerError {
    WorkerError::Config(format!("the verified body does not read: {err}"))
}

#[cfg(test)]
mod tests {
    use super::{NO_REVIEW_REASON, SAME_BODY, Stored, unreadable};
    use cadus_store::content::Verdict;

    /// One `Stored` of a collision with a row of this status and reason.
    fn held(status: &str, review_reason: Option<&str>) -> Stored {
        Stored {
            digest: "sha256:0000000000000000".to_owned(),
            inserted: false,
            verdict: Some(Verdict {
                status: status.to_owned(),
                review_reason: review_reason.map(str::to_owned),
            }),
            refreshed: false,
            cost_usd: None,
        }
    }

    /// F6: the decline reason of a reproduced body carries the reviewer's own
    /// words, and a row with no reason still reads as a sentence.
    #[test]
    fn the_same_body_reason_carries_the_reviewer_words() {
        assert_eq!(
            held("rejected", Some("the statement asks for two answers")).same_body_reason(),
            "a reviewer refused this exact body already: the statement asks for two answers"
        );
        assert_eq!(
            held("rejected", None).same_body_reason(),
            "a reviewer refused this exact body already: the row carries no reason"
        );
        assert_eq!(SAME_BODY, "a reviewer refused this exact body already");
        assert_eq!(NO_REVIEW_REASON, "the row carries no reason");
    }

    /// A held row is refused when a reviewer rejected it, and only then.
    #[test]
    fn a_held_row_is_refused_when_its_status_is_rejected() {
        assert!(held("rejected", None).refused());
        assert!(!held("pending", None).refused());
        assert!(!held("approved", None).refused());
        let new = Stored {
            verdict: None,
            inserted: true,
            ..held("pending", None)
        };
        assert!(!new.refused());
    }

    /// A body that does not read is a configuration error with the serde text.
    #[test]
    fn an_unreadable_body_is_a_configuration_error() {
        let err = serde_json::from_str::<serde_json::Value>("{").expect_err("the text is cut");
        assert!(
            unreadable(err)
                .to_string()
                .starts_with("configuration error: the verified body does not read: "),
        );
    }
}
