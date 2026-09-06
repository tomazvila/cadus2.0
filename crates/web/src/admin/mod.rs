//! `/api/admin/content*` — the C6 review surface (M6 R5).
//!
//! Requirements: C6 (a human approves a document, and the approval binds to the
//! digest), A6 (a skipped check is explicit, never silent), R4 (the handler does
//! local CPU work and database I/O only), L6 (the route calls no model), T3 (the
//! attempts and the money of one authored document reach the reviewer).
//!
//! Spec: `docs/reference/authoring-and-spa-1.0-spec.md` section 3.2, and row R5
//! of section 7. The four routes are the table of section 3.2:
//!
//! | Method and path | What it answers |
//! |---|---|
//! | `GET /api/admin/content` | the review queue, with the T3 numbers and the bank warning |
//! | `GET /api/admin/content/{digest}` | the body, the gate notes, and [`SAMPLE_INSTANCES`] rendered instances |
//! | `POST /api/admin/content/{digest}/approve` | `{digest, status, approved_at, rejected_documents}` |
//! | `POST /api/admin/content/{digest}/reject` | `{digest, status}`, and the reason is required |
//! | `GET /api/admin/ungraded` | the ungraded attempts of the learner (D-F2) |
//! | `POST /api/admin/ungraded/{attempt_id}/regrade` | the human verdict of one ungraded attempt |
//!
//! # The admin gate
//!
//! [`AdminUser`] reads the [`Authed`] that the M5 tenant layer wrote into the
//! request extensions, and it refuses every account whose `users.is_admin` is
//! false. The runtime role holds no grant on that column (`docs/SCHEMA.md`,
//! "The M5 auth contract"), so the flag is trustworthy. A request with no live
//! session is `401 unauthorized`, and a live session that is not an admin is
//! `403 forbidden`, on all four routes.
//!
//! # The two connections
//!
//! `cadus_app` holds SELECT on `content_store` and nothing else
//! (`docs/SCHEMA.md`, finding #14), so this module reads and writes through two
//! different handles:
//!
//! - the two READS take [`AppState::db`], the tenant-tier pool. `content_store`
//!   holds curriculum content, it stands outside row-level security, and the
//!   read needs no tenant binding;
//! - the two WRITES take [`AppState::admin`], the explicit admin path of
//!   [`cadus_store::content::Admin`]. A deployment that configures no admin
//!   connection answers [`ADMIN_PATH_UNAVAILABLE`] with `503`, because a review
//!   write that fails at the server with SQLSTATE 42501 tells the reviewer
//!   nothing.
//!
//! # An approval judges the documents beside it
//!
//! Approving a TEMPLATE changes the material the knowledge point serves, so the
//! approve route runs the teach gate and the hint gate again over every PENDING
//! page and ladder of that knowledge point ([`regate_knowledge_point`]). One the
//! gate now refuses moves to `rejected`, with the gate's own message as the
//! reason, and `rejected_documents` names it in the answer. An APPROVED page or
//! ladder is never touched: a human passed it, and this route does not undo a
//! human verdict (the FIX2-M6-A ruling, part 3).
//!
//! # The rendered instances are the point
//!
//! 1.0 `scripts/review_templates.py:52,145-155` prints eight instances with
//! their computed answers, and its docstring gives the reason: the live failures
//! are obvious in the instances and invisible in the expression. The show route
//! draws the same eight, from the constant seed
//! [`GATE_SEED`](cadus_core::template::GATE_SEED), so two reviewers of one digest
//! read the same eight problems.

use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use cadus_store::content::{BANK_TARGET, Decision, ReviewItem};
use cadus_store::{Db, StoreError};
use serde_json::{Map, Value, json};

use crate::AppState;
use crate::auth::guard::Authed;
use crate::error::ApiError;
use crate::operator::{FORBIDDEN, FORBIDDEN_MESSAGE};

mod queue;
mod review;
mod ungraded;

pub use queue::{list, show};
pub use review::{Regated, approve, reject};
pub use ungraded::{list_ungraded, regrade_ungraded};

/// The count of instances the show route renders (1.0 `SAMPLE_INSTANCES`,
/// `scripts/review_templates.py:52`).
pub const SAMPLE_INSTANCES: usize = 8;

/// The count of characters of the body summary one queue line carries.
///
/// 1.0 `cmd_list` prints the first 64 characters of the statement
/// (`scripts/review_templates.py:83-113`). The cut runs on characters and not on
/// bytes, so a statement with a multi-byte character keeps a valid string.
pub const SUMMARY_CHARS: usize = 64;

/// The query parameter that selects one review status.
pub const STATUS_PARAM: &str = "status";

/// The query parameter that selects one document kind.
pub const KIND_PARAM: &str = "kind";

/// The query parameter that selects one serving key.
pub const KP_PARAM: &str = "kp";

/// The query parameter that selects one page of the queue.
///
/// The queue read is capped at [`LIST_LIMIT`] rows, and one answer of spec
/// section 3.2 needs more rows than one page holds: the T3 bill of the operator
/// screen prices EVERY stored document, of every status. `page` is zero-based,
/// and page `n` skips `n * LIST_LIMIT` rows of the order the first page was cut
/// from, so a caller that reads pages until one comes back short has read the
/// whole table.
pub const PAGE_PARAM: &str = "page";

/// The highest page this route serves.
///
/// A page number is free text from a caller, and an `OFFSET` on a large number
/// is a scan the reader throws away. These pages hold 20,000 documents between
/// them, which is more than the deployments this build sizes for, and the bound
/// keeps one careless URL out of the planner.
pub const MAX_PAGE: i64 = 99;

/// The message of a `page` that is not a page.
fn page_message() -> String {
    format!("The page must be a whole number from 0 to {MAX_PAGE}.")
}

/// The zero-based page one query string asks for.
///
/// An absent parameter is page 0, so a caller that names no page reads the queue
/// exactly as it did before this parameter existed.
///
/// Every other bad value is refused. A `page=two` that the route silently reads
/// as 0 answers the FIRST 200 rows to a caller that asked for a later page, and
/// a bill added up from that answer is wrong with nothing on screen to say so,
/// which is the silent skip A6 forbids.
///
/// # Errors
///
/// - `422 invalid_request` — the value is empty, is not a whole number, is
///   negative, or is past [`MAX_PAGE`].
pub fn page_of(raw: Option<&str>) -> Result<i64, ApiError> {
    let Some(text) = raw else {
        return Ok(0);
    };
    let page: i64 = text
        .parse()
        .map_err(|_| ApiError::invalid_request(page_message()))?;
    if !(0..=MAX_PAGE).contains(&page) {
        return Err(ApiError::invalid_request(page_message()));
    }
    Ok(page)
}

/// The field of the reject body that carries the reason.
pub const REASON_FIELD: &str = "reason";

/// The most characters a rejection reason holds.
///
/// The reason is free text from a reviewer, and it goes into a column with no
/// length of its own. This bound keeps one careless paste out of the table.
pub const REASON_MAX_CHARS: usize = 1_000;

/// The message of a reject with no usable reason.
///
/// 1.0 makes `--reason` a required argument of `cmd_reject`
/// (`scripts/review_templates.py:201-214`), so a refused document always carries
/// the reason it was refused for.
pub const REASON_MESSAGE: &str =
    "A rejection needs a reason: send a JSON object with a non-empty \"reason\" string.";

/// The message of a reject whose reason is past [`REASON_MAX_CHARS`].
pub const REASON_LONG_MESSAGE: &str = "The rejection reason is too long.";

/// The message of a request body that does not read as a JSON object.
pub const BODY_MESSAGE: &str = "The request body must be a JSON object.";

/// The code of a deployment that configured no admin connection.
///
/// The two review writes need `cadus_admin`, and this process holds only the
/// runtime role. The answer is `503` and not `500`: nothing is broken, and the
/// operator cures it with one environment variable.
pub const ADMIN_PATH_UNAVAILABLE: &str = "admin_path_unavailable";

/// The message of that refusal.
pub const ADMIN_PATH_MESSAGE: &str =
    "This deployment configured no admin database connection, so the review writes are closed.";

/// The message of a digest that `content_store` does not hold.
pub const UNKNOWN_DIGEST_MESSAGE: &str = "No stored document carries that digest.";

/// The message of a request that carries no live session.
pub const NO_SESSION_MESSAGE: &str =
    "This route needs a session. Send the session cookie or a bearer token.";

// --------------------------------------------------------------------------- //
// The admin gate
// --------------------------------------------------------------------------- //

/// The identity of an admin request.
///
/// The extractor reads the [`Authed`] that
/// [`tenant_layer`](crate::auth::layer::tenant_layer) wrote. It opens no
/// statement of its own, so the four routes cost one credential read between
/// them and not two.
///
/// The two refusals are the refusals of spec section 3.2: `401 unauthorized`
/// with no live session, and `403 forbidden` for a live session on an account
/// that is not an admin.
#[derive(Debug, Clone)]
pub struct AdminUser(pub Authed);

impl<S: Sync> FromRequestParts<S> for AdminUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let authed = parts
            .extensions
            .get::<Authed>()
            .cloned()
            .ok_or_else(|| ApiError::unauthorized(NO_SESSION_MESSAGE))?;
        if !authed.user.is_admin {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                FORBIDDEN,
                FORBIDDEN_MESSAGE,
            ));
        }
        Ok(Self(authed))
    }
}

/// The admin connection of this deployment, or `503`.
fn admin_path(state: &AppState) -> Result<&Db, ApiError> {
    state.admin.as_ref().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            ADMIN_PATH_UNAVAILABLE,
            ADMIN_PATH_MESSAGE,
        )
    })
}

/// The answer of an attempt id the log does not hold as an ungraded attempt.
fn unknown_ungraded() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        crate::error::NOT_FOUND,
        ungraded::NOT_UNGRADED_MESSAGE,
    )
}

/// The answer of a digest the table does not hold.
fn unknown_digest() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        crate::error::NOT_FOUND,
        UNKNOWN_DIGEST_MESSAGE,
    )
}

/// Map one review write onto the section 2 envelope.
///
/// [`StoreError::NotFound`] is the answer of an absent digest, and it becomes
/// `404`. Every other store failure becomes `500 internal_error`, and its text
/// reaches the log alone: a store message can carry a value of the row.
fn decided(step: &'static str, answer: Result<Decision, StoreError>) -> Result<Decision, ApiError> {
    match answer {
        Ok(decision) => Ok(decision),
        Err(StoreError::NotFound { .. }) => Err(unknown_digest()),
        Err(err) => {
            tracing::error!(error = %err, step, "admin content: a review write failed");
            Err(ApiError::internal(step))
        }
    }
}

// --------------------------------------------------------------------------- //
// The queue line
// --------------------------------------------------------------------------- //

/// The first [`SUMMARY_CHARS`] characters of the one line a reviewer scans.
///
/// Each kind names its own headline field, and unit R6 fixed the two that were
/// open: a template body carries `statement`, a teach page carries `concept`
/// (`cadus_core::instruction::TeachPage`), and a hint ladder carries its widest
/// rung as `hints[0]` (`cadus_core::instruction::HintLadder`). Both instruction
/// documents deny an unknown field, so neither one can carry a headline field of
/// its own. The queue therefore reads the fields the documents have.
///
/// A body with none of the three falls back to its own JSON text, so a queue
/// line is never empty.
fn summary(body: &Value) -> String {
    let headline = body
        .get("statement")
        .or_else(|| body.get("concept"))
        .or_else(|| body.get("hints").and_then(|rungs| rungs.get(0)));
    let text = match headline {
        Some(Value::String(text)) => text.clone(),
        _ => body.to_string(),
    };
    text.chars().take(SUMMARY_CHARS).collect()
}

/// Whether the bank of this knowledge point is below the 1.0 target.
///
/// 1.0 prints the note when `0 < approved < BANK_TARGET`
/// (`scripts/review_templates.py:100-112`). 2.0 flags a bank of zero too: spec
/// section 3.2 says "fewer than 3 approved templates", and a knowledge point
/// with no approved template is the worst case of the same fault.
const fn bank_warning(approved_templates: i64) -> bool {
    approved_templates < BANK_TARGET
}

/// The fields of one document that the queue line and the show route share.
pub(super) fn item_fields(item: &ReviewItem) -> Map<String, Value> {
    // The pairs collect straight into a `Map`, so the answer is an object by
    // construction and there is no non-object case to fall back from.
    [
        ("digest", json!(item.digest)),
        ("kp_id", json!(item.kp_id)),
        ("kind", json!(item.kind)),
        ("status", json!(item.status)),
        ("authoring_attempts", json!(item.authoring_attempts)),
        ("authoring_cost_usd", json!(item.cost_usd)),
        ("created_at", json!(item.created_at.to_rfc3339())),
        ("summary", json!(summary(&item.body))),
        ("approved_templates", json!(item.approved_templates)),
        ("bank_warning", json!(bank_warning(item.approved_templates))),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value))
    .collect()
}

/// One queue line, as JSON.
pub(super) fn item_json(item: &ReviewItem) -> Value {
    Value::Object(item_fields(item))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use axum::http::StatusCode;

    use super::{MAX_PAGE, page_message, page_of};

    /// An absent `page` is page 0, so a caller that names none reads the queue
    /// as it read it before the parameter existed.
    #[test]
    fn an_absent_page_is_the_first_page() {
        assert_eq!(page_of(None), Ok(0));
    }

    /// A whole number inside the bound is the page, and the bound holds at both
    /// ends.
    #[test]
    fn a_whole_number_inside_the_bound_is_the_page() {
        assert_eq!(MAX_PAGE, 99);
        assert_eq!(page_of(Some("0")), Ok(0));
        assert_eq!(page_of(Some("1")), Ok(1));
        assert_eq!(page_of(Some("99")), Ok(99));
    }

    /// Every other value is refused, and NONE of them reads as page 0: a bill
    /// added up from the first page of a request for a later page is wrong with
    /// nothing on screen to say so (A6).
    #[test]
    fn a_value_that_is_not_a_page_is_refused() {
        for raw in ["", " ", "two", "1.5", "-1", "100", "1e2", "0x1"] {
            let refused = page_of(Some(raw)).expect_err("this value is not a page");
            assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(refused.message, page_message());
        }
    }

    // ----------------------------------------------------------------------- //
    // The re-gate an approval runs (the FIX2-M6-A ruling, part 3)
}
