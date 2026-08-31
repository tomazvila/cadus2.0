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

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{FromRequestParts, Query, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use cadus_core::curriculum::Exemplar;
use cadus_core::instruction::{InstructionSpec, ServedInstance, regate, template_instances};
use cadus_core::pool::{ProblemSource, TemplateSource};
use cadus_core::template::{GATE_SEED, from_body};
use cadus_store::content::{
    self, Admin, BANK_TARGET, Decision, KIND_TEMPLATE, LIST_LIMIT, ReviewFilter, ReviewItem,
    STATUS_APPROVED, StoredDoc,
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

/// One page of the review queue, newest first.
///
/// WHY THIS STATEMENT LIVES IN THE WEB CRATE. [`content::review_list`] answers
/// the first [`LIST_LIMIT`] rows and takes no offset, so a caller cannot reach
/// the row after the two hundredth, and the T3 bill of the operator screen
/// prices every stored document. The fix unit that added [`PAGE_PARAM`] (the M6
/// review of 2026-08-30, unit FIX-M6-E) may open no file under `crates/store`,
/// so the paged read sits beside its caller. Fold it back into
/// [`content::review_list`] — one `offset` field on [`ReviewFilter`] — when the
/// store is next opened, and delete this function.
///
/// The order is the order of [`content::review_list`]: `created_at` descending
/// with the digest as the tie break. One order for every page is what makes the
/// walk whole — page `n + 1` starts on the row page `n` stopped before — so this
/// route reads every page through this one statement, page 0 included.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
async fn review_page(
    db: &Db,
    filter: &ReviewFilter<'_>,
    offset: i64,
) -> Result<Vec<ReviewItem>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT c.digest AS "digest!", c.kp_id AS "kp_id!", c.kind AS "kind!",
               c.status AS "status!", c.authoring_attempts AS "authoring_attempts!",
               c.authoring_cost_usd::text AS "cost_usd?",
               c.created_at AS "created_at!", c.body AS "body!",
               (SELECT count(*) FROM content_store a
                 WHERE a.kp_id = c.kp_id AND a.kind = $4 AND a.status = $5)
                 AS "approved_templates!"
        FROM content_store c
        WHERE ($1::text IS NULL OR c.status = $1)
          AND ($2::text IS NULL OR c.kind = $2)
          AND ($3::text IS NULL OR c.kp_id = $3)
        ORDER BY c.created_at DESC, c.digest
        LIMIT $6 OFFSET $7
        "#,
        filter.status,
        filter.kind,
        filter.kp_id,
        KIND_TEMPLATE,
        STATUS_APPROVED,
        LIST_LIMIT,
        offset,
    )
    .fetch_all(db.pool())
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| ReviewItem {
            digest: row.digest,
            kp_id: row.kp_id,
            kind: row.kind,
            status: row.status,
            authoring_attempts: row.authoring_attempts,
            cost_usd: row.cost_usd,
            created_at: row.created_at,
            body: row.body,
            approved_templates: row.approved_templates,
        })
        .collect())
}

/// `GET /api/admin/content` — the review queue (C6, T3).
///
/// The three query parameters of spec section 3.2 filter the queue: `status`,
/// `kind`, and `kp`. A parameter this route does not name is ignored, and an
/// absent parameter applies no filter of that kind.
///
/// [`PAGE_PARAM`] reads the queue past [`LIST_LIMIT`] rows. The reply carries
/// `limit`, so a caller asks for the next page while a page comes back full and
/// stops on the first short one.
///
/// # Errors
///
/// - `401 unauthorized` — the request carries no live session.
/// - `403 forbidden` — the account is not an admin.
/// - `422 invalid_request` — `page` is not a page. See [`page_of`].
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
    let page = page_of(params.get(PAGE_PARAM).map(String::as_str))?;
    let rows = store_call(
        &state.db,
        "admin content list",
        review_page(&state.db, &filter, page * LIST_LIMIT),
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
/// `rejected_documents` names every pending page and ladder the re-gate moved to
/// `rejected`, and it is an empty list when the approval refuses nothing. It is
/// `null` when the re-gate itself failed, so a check that did not run is on the
/// screen (A6).
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

    // The approval changed the material this knowledge point serves, so every
    // pending page and ladder of it is judged again (the FIX2-M6-A ruling, part
    // 3). A failure of the re-gate does not undo the approval and does not fail
    // the request: the answer carries `null` instead of a list, so a skipped
    // check is on the screen and never silent (A6).
    let regated = match regate_after_approval(&state, admin, &digest).await {
        Ok(list) => Value::Array(list.iter().map(Regated::to_json).collect()),
        Err(err) => {
            tracing::error!(error = %err, digest, "admin content approve: the re-gate did not run");
            Value::Null
        }
    };

    Ok(Json(json!({
        "digest": decision.digest,
        "status": decision.status,
        "approved_at": decision.approved_at.map(|at| at.to_rfc3339()),
        "rejected_documents": regated,
    })))
}

/// One document the re-gate moved to `rejected` (C6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Regated {
    /// The content address of the document.
    pub digest: String,
    /// The kind of the document: `teach` or `hint_ladder`.
    pub kind: String,
    /// The gate message the row now carries as its `review_reason`.
    pub reason: String,
}

impl Regated {
    /// The queue line of one re-gated document.
    fn to_json(&self) -> Value {
        json!({"digest": self.digest, "kind": self.kind, "reason": self.reason})
    }
}

/// Judge the pending instruction documents of one knowledge point again, after
/// an approval of one of its templates (C6).
///
/// A document of any other kind changes no answer set, so the function reads one
/// row and stops.
///
/// # Errors
///
/// Returns the [`StoreError`] of the read or of a rejection write.
async fn regate_after_approval(
    state: &AppState,
    admin: &Db,
    digest: &str,
) -> Result<Vec<Regated>, StoreError> {
    let Some(found) = content::document(state.db.pool(), digest).await? else {
        return Ok(Vec::new());
    };
    if found.item.kind != KIND_TEMPLATE {
        return Ok(Vec::new());
    }
    regate_knowledge_point(
        &state.db,
        admin,
        state.content.as_deref(),
        &found.item.kp_id,
    )
    .await
}

/// Re-run the two instruction gates over one knowledge point, and refuse every
/// pending page or ladder that now gives a served answer away (C6).
///
/// # Why an approval judges other documents
///
/// The gate of an authoring pass reads the material of that pass. A template
/// authored, or approved, after a ladder was stored is material that ladder was
/// never judged against, so a give-away rung no pass read stands `pending`
/// in the review queue and one click serves it (M6 review 2, the FIX2-M6-A
/// ruling, part 3).
///
/// # What it touches
///
/// - a PENDING page or ladder the gate now refuses moves to `rejected`, and the
///   gate's own message is the `review_reason` the queue shows;
/// - an APPROVED page or ladder is not read: a human passed it, and this
///   function does not undo a human verdict;
/// - a template is not judged at all: `cadus_core::instruction::regate` answers
///   [`None`] for every kind but the two instruction kinds.
///
/// A deployment with no curriculum loaded judges against the rendered instances
/// alone. The exemplars live in the curriculum tree, and a gate over the
/// instances is a smaller check, never a wrong one.
///
/// # Errors
///
/// Returns the [`StoreError`] of the read or of a rejection write.
async fn regate_knowledge_point(
    db: &Db,
    admin: &Db,
    content: Option<&Content>,
    kp_id: &str,
) -> Result<Vec<Regated>, StoreError> {
    let rows = content::regate_rows(db.pool(), kp_id).await?;
    let mut instances: Vec<ServedInstance> = Vec::new();
    for row in &rows {
        if row.kind != KIND_TEMPLATE {
            continue;
        }
        for instance in template_instances(&row.body.to_string()) {
            if !instances.contains(&instance) {
                instances.push(instance);
            }
        }
    }
    let gate_spec = content.and_then(|loaded| spec_of(loaded, kp_id));
    let none: &[Exemplar] = &[];
    let spec = InstructionSpec {
        exemplars: gate_spec.as_ref().map_or(none, |spec| spec.exemplars),
        instance_answers: instances,
    };

    let mut regated: Vec<Regated> = Vec::new();
    for row in &rows {
        let Some(rejection) = regate(&row.kind, &row.body.to_string(), &spec) else {
            continue;
        };
        content::reject(Admin::new(admin), &row.digest, &rejection.message).await?;
        tracing::warn!(
            kp_id,
            digest = row.digest,
            kind = row.kind,
            reason = rejection.message,
            "admin content approve: the served material now gives this document away"
        );
        regated.push(Regated {
            digest: row.digest.clone(),
            kind: row.kind.clone(),
            reason: rejection.message,
        });
    }
    Ok(regated)
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use axum::http::StatusCode;
    use cadus_store::test_support::TestDb;
    use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
    use sqlx::PgPool;

    use super::{MAX_PAGE, Regated, page_message, page_of, regate_knowledge_point};

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
    // ----------------------------------------------------------------------- //

    /// The serving key of the knowledge point under test.
    const KP: &str = "perfect-squares/squares";

    /// A template document of `KP`. It renders `Compute $9^{2}$.` with the
    /// answer 81, among the twelve squares of 1 to 12.
    const TEMPLATE: &str = r#"{"v":1,"topic_id":"perfect-squares","answer_kind":"numeric","statement":"Compute ${a}^{{2}}$.","params":{"a":{"kind":"int","low":1,"high":12}},"answer_expr":"a**2","samples":[{"params":{"a":1},"expected":"1"},{"params":{"a":12},"expected":"144"}],"space_size":12}"#;

    /// A ladder whose only rung states 81, the answer of a rendered instance.
    const GIVE_AWAY: &str = r#"{"hints": ["For a base of 9 the product is 81."]}"#;

    /// The gate sentence that ladder earns.
    const GIVE_AWAY_REASON: &str = "rung 0 reads 'For a base of 9 the product is 81.', which \
names the answer '81' this knowledge point serves — a hint is a question, never the final step \
(Hard Rule 3)";

    /// A teach page that works a problem the template never renders.
    const CLEAN_PAGE: &str = r#"{"concept": "Squaring multiplies a number by itself.",
        "worked_example": {"problem": "Compute $15^2$.",
        "steps": ["Write the base twice.", "The product is 225."]}}"#;

    /// Seed one `content_store` row.
    async fn seed(pool: &PgPool, digest: &str, kind: &str, status: &str, body: &str) {
        sqlx::query(
            "INSERT INTO content_store (digest, kp_id, kind, body, status)
             VALUES ($1, $2, $3, $4::jsonb, $5)",
        )
        .bind(digest)
        .bind(KP)
        .bind(kind)
        .bind(body)
        .bind(status)
        .execute(pool)
        .await
        .unwrap();
    }

    /// The status and the review reason of one row.
    async fn state_of(pool: &PgPool, digest: &str) -> (String, Option<String>) {
        sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT status, review_reason FROM content_store WHERE digest = $1",
        )
        .bind(digest)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// FIX2-M6-A, part 3. Approving a template judges the PENDING pages and
    /// ladders of that knowledge point again.
    ///
    /// A ladder authored before any template existed was gated with an empty
    /// instance set, so a rung that states a rendered answer stands `pending` and
    /// one click serves it. The re-gate moves it to `rejected`, with the gate's
    /// own sentence as the reason. The approved ladder beside it is not touched:
    /// a human passed that one, and this path does not undo a human verdict.
    #[tokio::test]
    async fn the_re_gate_refuses_a_pending_ladder_the_new_material_gives_away() {
        TestDb::with(|db| async move {
            let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
            seed(
                &db.admin,
                "sha256:the-template",
                "template",
                "approved",
                TEMPLATE,
            )
            .await;
            seed(
                &db.admin,
                "sha256:the-ladder",
                "hint_ladder",
                "pending",
                GIVE_AWAY,
            )
            .await;
            seed(
                &db.admin,
                "sha256:passed",
                "hint_ladder",
                "approved",
                GIVE_AWAY,
            )
            .await;
            seed(&db.admin, "sha256:the-page", "teach", "pending", CLEAN_PAGE).await;

            let regated = regate_knowledge_point(&handle, &handle, None, KP)
                .await
                .expect("the re-gate runs");

            assert_eq!(
                regated,
                vec![Regated {
                    digest: "sha256:the-ladder".to_owned(),
                    kind: "hint_ladder".to_owned(),
                    reason: GIVE_AWAY_REASON.to_owned(),
                }]
            );
            assert_eq!(
                state_of(&db.admin, "sha256:the-ladder").await,
                ("rejected".to_owned(), Some(GIVE_AWAY_REASON.to_owned()))
            );
            // A human passed this one. The re-gate never reads it.
            assert_eq!(
                state_of(&db.admin, "sha256:passed").await,
                ("approved".to_owned(), None)
            );
            // The page names no served answer, so it waits for its reviewer.
            assert_eq!(
                state_of(&db.admin, "sha256:the-page").await,
                ("pending".to_owned(), None)
            );
            // A template is not an instruction document, so it is not judged.
            assert_eq!(
                state_of(&db.admin, "sha256:the-template").await,
                ("approved".to_owned(), None)
            );
        })
        .await;
    }

    /// The same ladder with no template on the knowledge point is left alone:
    /// nothing serves 81, so the ladder gives nothing away. The stored TEMPLATE is
    /// what makes the difference, and this test is the control.
    #[tokio::test]
    async fn the_re_gate_refuses_nothing_when_no_template_serves() {
        TestDb::with(|db| async move {
            let handle = Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
            seed(
                &db.admin,
                "sha256:the-ladder",
                "hint_ladder",
                "pending",
                GIVE_AWAY,
            )
            .await;

            let regated = regate_knowledge_point(&handle, &handle, None, KP)
                .await
                .expect("the re-gate runs");

            assert_eq!(regated, Vec::new());
            assert_eq!(
                state_of(&db.admin, "sha256:the-ladder").await,
                ("pending".to_owned(), None)
            );
        })
        .await;
    }
}
