//! `GET /api/admin/content`, the review queue, and
//! `GET /api/admin/content/{digest}`, one document with its rendered
//! instances (C6, A6, T3).

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Query, State};
use cadus_core::pool::{ProblemSource, TemplateSource};
use cadus_core::template::{GATE_SEED, from_body};
use cadus_store::content::{self, KIND_TEMPLATE, LIST_LIMIT, ReviewFilter, StoredDoc};
use serde_json::{Value, json};

use super::{
    AdminUser, BANK_TARGET, KIND_PARAM, KP_PARAM, PAGE_PARAM, SAMPLE_INSTANCES, STATUS_PARAM,
    item_fields, item_json, page_of, unknown_digest,
};
use crate::AppState;
use crate::auth::store_call;
use crate::error::ApiError;
use crate::operator::{gate_json, spec_of};
use crate::path::ApiPath;
use crate::session::content;
use crate::state::Content;

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
    let page = page_of(params.get(PAGE_PARAM).map(String::as_str))?;
    let filter = ReviewFilter {
        status: params.get(STATUS_PARAM).map(String::as_str),
        kind: params.get(KIND_PARAM).map(String::as_str),
        kp_id: params.get(KP_PARAM).map(String::as_str),
        offset: page * LIST_LIMIT,
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

    let mut fields = item_fields(&item);
    fields.extend(
        json!({
            "approved_at": approved_at.map(|at| at.to_rfc3339()),
            "review_reason": review_reason,
            "body": item.body,
            "gate": gate,
            "instances": instances,
            "instances_note": note,
            "sample_instances": SAMPLE_INSTANCES,
        })
        .as_object()
        .cloned()
        .unwrap_or_default(),
    );
    Ok(Json(Value::Object(fields)))
}
