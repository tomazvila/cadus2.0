//! The A4 client surface: the `diagnosis` field, the poll route, and the stream.
//!
//! Requirements: A4 (the verdict ships at once and the prose follows), C3 (every
//! tenant read runs inside `begin_tenant`), D7 (push), L2/L3 (the learner is
//! never blocked on a model), R4 and T1 (this tier does local work and database
//! I/O only; it spends no model token).
//!
//! Spec: `docs/reference/web-service-1.0-spec.md` section 2.1 (the wire shape
//! and the push paragraph), section 6.2 (what skips the call), section 9 trap
//! W14, and row U9 of section 11. Ruling D-M5-1 is binding.
//!
//! # The three states of `diagnosis`
//!
//! - [`STATUS_NOT_OFFERED`] — a correct answer or a blank one. No job row.
//! - [`STATUS_READY`] — a pre-authored distractor of the M4 template matched the
//!   learner's answer, read from `content_store` kind `diagnosis` in the SAME
//!   transaction. **No job row**: that is the difference between a bill that
//!   scales with attempts and one that scales with distinct misconceptions
//!   (spec section 6.2).
//! - [`STATUS_PENDING`] — a `diagnosis_jobs` row went in inside the grade
//!   transaction, and `id` is its primary key.
//!
//! # Why the enqueue sits inside the grade transaction
//!
//! Spec section 4.3 step 9. A grade that rolls back must leave no job row, or
//! the worker pays for an attempt the log does not hold.
//!
//! # Why the stream re-reads the row
//!
//! `LISTEN/NOTIFY` carries no row-level security and no tenant binding (trap
//! W14). The notice holds a job id and a user id, the handler keeps only the
//! notices of ITS tenant, and it re-reads the row through `begin_tenant` before
//! it writes a byte. A payload alone never reaches a client.
//!
//! # What this unit does NOT do
//!
//! The T4 knobs of spec section 6.6 (`DIAGNOSIS_CALLS_PER_SESSION` and the two
//! token bounds) belong to the worker of unit U10, so nothing here writes
//! `status = 'capped'`; the routes report the state when the worker does. The
//! vocabulary filter of spec section 6.3 runs where the tag is read: unit U9
//! filters the PRE-AUTHORED tag, and unit U10 filters the model's tags before it
//! writes `result`, so the poll route reports the stored document as it stands.

use axum::Router;
use axum::routing::get;
use cadus_core::curriculum::AnswerKind;
use cadus_core::template::{Preauthored, match_answer, read_distractors};
use cadus_store::Db;
use cadus_store::diagnosis::Notice;
use serde_json::Value;
use tokio::sync::broadcast;

use crate::AppState;

mod decide;
mod routes;

pub(crate) use decide::{Miss, Pending, decide};
pub use routes::{JobView, job_view, poll, stream};

/// The code of a diagnosis id this tenant does not own.
pub const UNKNOWN_DIAGNOSIS: &str = "unknown_diagnosis";

/// The `content_store.kind` of an authored distractor set (A4).
pub const KIND_DIAGNOSIS: &str = "diagnosis";

/// No diagnosis is owed: the answer was right, or it was blank.
pub const STATUS_NOT_OFFERED: &str = "not_offered";

/// A job row stands and the prose is on its way.
pub const STATUS_PENDING: &str = "pending";

/// The prose stands now: a pre-authored distractor, or a finished job.
pub const STATUS_READY: &str = "ready";

/// The job will produce nothing. The learner already holds the verdict, the
/// solution and the re-solve instruction, so this costs prose and nothing else.
pub const STATUS_FAILED: &str = "failed";

/// A T4 cap refused the call (spec section 6.6). Unit U10 writes it.
pub const STATUS_CAPPED: &str = "capped";

/// The `event:` name of every server-sent diagnosis frame (spec section 2.1).
pub const SSE_EVENT: &str = "diagnosis";

/// The heartbeat comment interval of the stream, in seconds (spec section 2.1).
pub const HEARTBEAT_SECS: u64 = 15;

/// A job still unfinished after this many seconds is reported
/// [`STATUS_FAILED`], never left open (spec section 2.1, last paragraph).
pub const PENDING_DEADLINE_SECS: i64 = 30;

/// How many notices the hub buffers for one slow subscriber.
///
/// A subscriber that falls further behind than this loses the notices in
/// between. It is not a lost diagnosis: the poll route is the required fallback
/// and `pending_diagnoses` holds the ids.
pub const STREAM_BACKLOG: usize = 256;
// --------------------------------------------------------------------------- //
// The push hub (D7, D-M5-1)
// --------------------------------------------------------------------------- //

/// The one fan-out point between the process `LISTEN` connection and the open
/// streams.
///
/// One connection listens; every stream subscribes here. A connection per
/// stream would give one idle learner one Postgres backend.
#[derive(Debug)]
pub struct DiagnosisHub {
    sender: broadcast::Sender<Notice>,
}

impl Default for DiagnosisHub {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagnosisHub {
    /// A hub with no listener attached. Publishing still works, so a test and a
    /// process that runs no listener behave the same way.
    #[must_use]
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(STREAM_BACKLOG);
        Self { sender }
    }

    /// Hand one notice to every open stream. The answer is the count of
    /// subscribers it reached.
    pub fn publish(&self, notice: Notice) -> usize {
        self.sender.send(notice).unwrap_or(0)
    }

    /// Open one subscription. A stream holds it for its whole life.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Notice> {
        self.sender.subscribe()
    }

    /// Read every notice the worker sends and publish it (spec section 2.1).
    ///
    /// The loop never ends by itself: `PgListener::recv` reconnects and
    /// re-`LISTEN`s after a lost connection. A payload the reader refuses is
    /// counted and dropped, because the channel carries no tenant binding and a
    /// guess would name the wrong learner (trap W14).
    ///
    /// # Errors
    ///
    /// Returns the sqlx error when the `LISTEN` cannot be established at all.
    pub async fn listen(&self, db: &Db) -> Result<(), sqlx::Error> {
        let mut listener = sqlx::postgres::PgListener::connect_with(db.pool()).await?;
        listener
            .listen(cadus_store::diagnosis::NOTIFY_CHANNEL)
            .await?;
        tracing::info!(
            channel = cadus_store::diagnosis::NOTIFY_CHANNEL,
            "cadus-web: the diagnosis listener is up"
        );
        loop {
            let notification = listener.recv().await?;
            match Notice::parse(notification.payload()) {
                Some(notice) => {
                    self.publish(notice);
                }
                None => tracing::warn!(
                    channel = cadus_store::diagnosis::NOTIFY_CHANNEL,
                    "cadus-web: a notice payload did not read; dropping it"
                ),
            }
        }
    }
}

// --------------------------------------------------------------------------- //
// The pre-authored distractor lookup (spec section 6.2)
// --------------------------------------------------------------------------- //

/// Find the authored distractor that names this answer (spec section 6.2).
///
/// The whole rule lives in `cadus_core::template::distractor`, so the gate that
/// AUTHORS a distractor list and the grade path that READS one share one
/// definition of a match and one vocabulary filter (unit R7). This function is
/// the request tier's spelling of it: it reads the stored body and hands the
/// distractors over.
///
/// The match runs through [`cadus_core::answer::check`], so `12` and `12.0`
/// name the same mistake, exactly as they name the same right answer. A
/// distractor the checker cannot decide — an unbound parameter expression, say —
/// simply never matches, so a template document under this kind is safe to read.
///
/// The authored tag goes through the vocabulary of spec section 5.3 first: a tag
/// the vocabulary lacks is dropped silently, which is the 1.0 lesson of
/// `prompts.py:529-536`. A hit with neither a surviving tag nor prose carries
/// nothing a learner can read, so it is NOT a hit and the caller enqueues.
#[must_use]
pub fn match_distractor(
    body: &Value,
    answer: &str,
    kind: AnswerKind,
    vocabulary: &[String],
) -> Option<Preauthored> {
    match_answer(&read_distractors(body), answer, kind, vocabulary)
}
/// The two A4 client routes (spec section 2.1, "New in 2.0").
///
/// `/api/diagnosis/stream` is a static segment and `/api/diagnosis/{id}` is a
/// parameter, so the router matches the static one first and `stream` is never
/// read as an id.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/diagnosis/stream", get(stream))
        .route("/api/diagnosis/{id}", get(poll))
}

#[cfg(test)]
mod tests {
    use sqlx::types::Uuid;

    use super::*;

    /// The default hub is a fresh one: publishing with no subscriber reaches
    /// nobody, and one subscriber then receives the next notice.
    #[test]
    fn a_hub_publishes_to_its_subscribers() {
        let hub = DiagnosisHub::default();
        let notice = Notice {
            job_id: Uuid::nil(),
            user_id: Uuid::nil(),
        };
        assert_eq!(hub.publish(notice.clone()), 0);
        let _receiver = hub.subscribe();
        assert_eq!(hub.publish(notice), 1);
    }
}
