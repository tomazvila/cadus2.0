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
//! | `POST /api/admin/content/{digest}/approve` | `{digest, status, approved_at}` |
//! | `POST /api/admin/content/{digest}/reject` | `{digest, status}`, and the reason is required |
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
//! # The rendered instances are the point
//!
//! 1.0 `scripts/review_templates.py:52,145-155` prints eight instances with
//! their computed answers, and its docstring gives the reason: the live failures
//! are obvious in the instances and invisible in the expression. The show route
//! draws the same eight, from the constant seed
//! [`GATE_SEED`](cadus_core::template::GATE_SEED), so two reviewers of one digest
//! read the same eight problems.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{FromRequestParts, Query, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use cadus_core::pool::{ProblemSource, TemplateSource};
use cadus_core::template::{GATE_SEED, from_body};
use cadus_store::content::{
    self, Admin, BANK_TARGET, Decision, KIND_TEMPLATE, LIST_LIMIT, ReviewFilter, ReviewItem,
    StoredDoc,
};
use cadus_store::{Db, StoreError};
use serde_json::{Value, json};

use crate::AppState;
use crate::auth::body::LimitedBody;
use crate::auth::guard::Authed;
use crate::auth::store_call;
use crate::error::ApiError;
use crate::operator::{FORBIDDEN, FORBIDDEN_MESSAGE, gate_json, spec_of};
use crate::path::ApiPath;
use crate::session::content;
use crate::state::Content;

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
// The queue
// --------------------------------------------------------------------------- //

/// The first [`SUMMARY_CHARS`] characters of the one line a reviewer scans.
///
/// A template body carries its `statement`, and a teach body carries its
/// `title`. A body with neither field falls back to its own JSON text, so a
/// queue line is never empty.
fn summary(body: &Value) -> String {
    let text = match body.get("statement").or_else(|| body.get("title")) {
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

/// One queue line, as JSON.
fn item_json(item: &ReviewItem) -> Value {
    json!({
        "digest": item.digest,
        "kp_id": item.kp_id,
        "kind": item.kind,
        "status": item.status,
        "authoring_attempts": item.authoring_attempts,
        "authoring_cost_usd": item.cost_usd,
        "created_at": item.created_at.to_rfc3339(),
        "summary": summary(&item.body),
        "approved_templates": item.approved_templates,
        "bank_warning": bank_warning(item.approved_templates),
    })
}

/// `GET /api/admin/content` — the review queue (C6, T3).
///
/// The three query parameters of spec section 3.2 filter the queue: `status`,
/// `kind`, and `kp`. A parameter this route does not name is ignored, and an
/// absent parameter applies no filter of that kind.
///
/// # Errors
///
/// - `401 unauthorized` — the request carries no live session.
/// - `403 forbidden` — the account is not an admin.
/// - `500 internal_error` — the statement failed or passed its bound.
pub async fn list(
    State(state): State<AppState>,
    AdminUser(_authed): AdminUser,
    Query(params): Query<BTreeMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let filter = ReviewFilter {
        status: params.get(STATUS_PARAM).map(String::as_str),
        kind: params.get(KIND_PARAM).map(String::as_str),
        kp_id: params.get(KP_PARAM).map(String::as_str),
    };
    let rows = store_call(
        &state.db,
        "admin content list",
        content::review_list(state.db.pool(), &filter),
    )
    .await?;

    Ok(Json(json!({
        "items": rows.iter().map(item_json).collect::<Vec<Value>>(),
        "bank_target": BANK_TARGET,
        "limit": LIST_LIMIT,
    })))
}

// --------------------------------------------------------------------------- //
// The document
// --------------------------------------------------------------------------- //

/// Render [`SAMPLE_INSTANCES`] instances of one template body.
///
/// The answer is the instance list and the note of an empty list. A body that
/// does not read as a template document, a document that does not compile, and a
/// document no draw satisfies each give an empty list with the reason, because a
/// reviewer must never read "no instances" with no cause beside it (A6).
///
/// The draw runs [`TemplateSource::fill`], the same call the refill worker makes,
/// so the instances a reviewer reads are instances the pool would serve. The
/// exemplars of the knowledge point join the source when the curriculum names
/// it, so the per-instance envelope check is the serving check.
fn instances_json(content: &Content, kp_id: &str, body: &Value) -> (Vec<Value>, Option<String>) {
    let text = body.to_string();
    let doc = match from_body(&text) {
        Ok(doc) => doc,
        Err(err) => {
            return (
                Vec::new(),
                Some(format!(
                    "the body does not read as a template document: {err}"
                )),
            );
        }
    };
    let source = match TemplateSource::new(kp_id, &doc) {
        Ok(source) => source,
        Err(err) => return (Vec::new(), Some(err.to_string())),
    };
    let source = match spec_of(content, kp_id) {
        Some(spec) => source.with_exemplars(spec.exemplars),
        None => source,
    };
    match source.fill(kp_id, SAMPLE_INSTANCES, GATE_SEED) {
        Ok(batch) => (
            batch
                .instances()
                .iter()
                .map(|instance| json!({"text": instance.text, "answer": instance.answer}))
                .collect(),
            None,
        ),
        Err(err) => (Vec::new(), Some(err.to_string())),
    }
}

/// `GET /api/admin/content/{digest}` — one document, its gate notes, and its
/// rendered instances (C6, A6).
///
/// The gate block and the instance list belong to a template. Every other kind
/// carries `gate: null` and an empty instance list: there is no statement to
/// render and no answer to compute.
///
/// # Errors
///
/// - `401 unauthorized` — the request carries no live session.
/// - `403 forbidden` — the account is not an admin.
/// - `404 not_found` — `content_store` holds no row with that digest.
/// - `503 curriculum_unavailable` — the process loaded no curriculum.
/// - `500 internal_error` — the statement failed or passed its bound.
pub async fn show(
    State(state): State<AppState>,
    AdminUser(_authed): AdminUser,
    ApiPath(digest): ApiPath<String>,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let found = store_call(
        &state.db,
        "admin content show",
        content::document(state.db.pool(), &digest),
    )
    .await?;
    let StoredDoc {
        item,
        review_reason,
        approved_at,
    } = found.ok_or_else(unknown_digest)?;

    let is_template = item.kind == KIND_TEMPLATE;
    let (instances, note) = if is_template {
        instances_json(content, &item.kp_id, &item.body)
    } else {
        (Vec::new(), None)
    };
    let gate = if is_template {
        gate_json(content, &item.kp_id, &item.digest, &item.body.to_string())
    } else {
        Value::Null
    };

    Ok(Json(json!({
        "digest": item.digest,
        "kp_id": item.kp_id,
        "kind": item.kind,
        "status": item.status,
        "authoring_attempts": item.authoring_attempts,
        "authoring_cost_usd": item.cost_usd,
        "created_at": item.created_at.to_rfc3339(),
        "approved_at": approved_at.map(|at| at.to_rfc3339()),
        "review_reason": review_reason,
        "approved_templates": item.approved_templates,
        "bank_warning": bank_warning(item.approved_templates),
        "summary": summary(&item.body),
        "body": item.body,
        "gate": gate,
        "instances": instances,
        "instances_note": note,
        "sample_instances": SAMPLE_INSTANCES,
    })))
}

// --------------------------------------------------------------------------- //
// The two writes
// --------------------------------------------------------------------------- //

/// `POST /api/admin/content/{digest}/approve` — approve one digest (C6).
///
/// The route reads no field of the request body. Approval binds to the digest in
/// the path and to nothing else, so an edited body is a new digest with its own
/// approval.
///
/// The call is idempotent: [`cadus_store::content::approve`] keeps the first
/// `approved_by` and the first `approved_at`, so a second approval of one digest
/// answers the first stamp.
///
/// # Errors
///
/// - `401 unauthorized` — the request carries no live session.
/// - `403 forbidden` — the account is not an admin.
/// - `404 not_found` — `content_store` holds no row with that digest.
/// - `503 admin_path_unavailable` — this deployment configured no admin
///   connection.
/// - `500 internal_error` — the statement failed or passed its bound.
pub async fn approve(
    State(state): State<AppState>,
    AdminUser(authed): AdminUser,
    ApiPath(digest): ApiPath<String>,
) -> Result<Json<Value>, ApiError> {
    let admin = admin_path(&state)?;
    let decision = decided(
        "admin content approve",
        content::approve(Admin::new(admin), &digest, Some(authed.user.id)).await,
    )?;

    Ok(Json(json!({
        "digest": decision.digest,
        "status": decision.status,
        "approved_at": decision.approved_at.map(|at| at.to_rfc3339()),
    })))
}

/// The reason of one reject body, or the `422` the route answers.
///
/// The three refusals are separate, and each one names what the caller must fix:
/// a body that is not a JSON object, an absent or blank reason, and a reason past
/// [`REASON_MAX_CHARS`]. The answer is the trimmed reason, so a reason of spaces
/// alone never reaches the table.
fn reason_of(body: &[u8]) -> Result<String, ApiError> {
    let document: Value = serde_json::from_slice(body)
        .map_err(|_| ApiError::invalid_request(BODY_MESSAGE.to_string()))?;
    if !document.is_object() {
        return Err(ApiError::invalid_request(BODY_MESSAGE.to_string()));
    }
    let reason = match document.get(REASON_FIELD) {
        Some(Value::String(reason)) => reason.trim(),
        _ => return Err(ApiError::invalid_request(REASON_MESSAGE.to_string())),
    };
    if reason.is_empty() {
        return Err(ApiError::invalid_request(REASON_MESSAGE.to_string()));
    }
    if reason.chars().count() > REASON_MAX_CHARS {
        return Err(ApiError::invalid_request(REASON_LONG_MESSAGE.to_string()));
    }
    Ok(reason.to_string())
}

/// `POST /api/admin/content/{digest}/reject` — refuse one digest, with the
/// reason the reviewer wrote (C6).
///
/// The row keeps its body and stops serving. The reason is required: 1.0 makes
/// `--reason` a required argument, and a refused document with no reason tells
/// the next author nothing.
///
/// # Errors
///
/// - `401 unauthorized` — the request carries no live session.
/// - `403 forbidden` — the account is not an admin.
/// - `404 not_found` — `content_store` holds no row with that digest.
/// - `422 invalid_request` — the body is not an object, or the reason is absent,
///   blank, or too long.
/// - `503 admin_path_unavailable` — this deployment configured no admin
///   connection.
/// - `500 internal_error` — the statement failed or passed its bound.
pub async fn reject(
    State(state): State<AppState>,
    AdminUser(_authed): AdminUser,
    ApiPath(digest): ApiPath<String>,
    LimitedBody(body): LimitedBody,
) -> Result<Json<Value>, ApiError> {
    let reason = reason_of(&body)?;
    let admin = admin_path(&state)?;
    let decision = decided(
        "admin content reject",
        content::reject(Admin::new(admin), &digest, &reason).await,
    )?;

    Ok(Json(json!({
        "digest": decision.digest,
        "status": decision.status,
    })))
}
