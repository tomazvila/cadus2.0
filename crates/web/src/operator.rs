//! `GET /api/operator/flags` — the A6 operator view, for an admin account only.
//!
//! Requirements: A6 (a fallback is explicit, never silent), C6 (approval binds
//! to the content digest), C3 (every tenant read runs inside `begin_tenant`),
//! R4 (the handler does local CPU work and database I/O only), L6 (the route
//! calls no model).
//!
//! Spec: `docs/reference/web-service-1.0-spec.md` section 11, unit U12.
//! `docs/SELF_HOST.md`, section "The operator flags (A6, C6)", names the seven
//! fields of one row and the cure for each of the two faults.
//!
//! # What the answer holds
//!
//! Two blocks:
//!
//! - `flags` — one row per knowledge point, from
//!   [`cadus_store::pool::operator_flags`].
//! - `gate` — the [`Verified::notes`](cadus_core::template::Verified::notes) of
//!   the approved template of a knowledge point. The core writes no log (R3), so
//!   the gate hands its skipped checks to its caller and the caller reports them.
//!   The refill worker drops them today, and this route is where an operator
//!   reads them.
//!
//! # Two bounds, both explicit
//!
//! 1. **The tenant.** `serving_pool` is under row-level security, so the row
//!    counts are the counts of the CALLING account (C3). `content_store` is
//!    curriculum content and stands outside row-level security, so
//!    `approved_templates` and `needs_template` are deployment-wide.
//!    `docs/SELF_HOST.md` states the same split.
//! 2. **The gate.** One `gate` call walks up to
//!    [`GATE_SAMPLES`](cadus_core::template::GATE_SAMPLES) instances, which is
//!    about 10 ms of CPU per template on the build box. The route therefore
//!    gates at most [`GATE_NOTE_LIMIT`] templates per request, in `kp_id` order,
//!    and sets `gate_truncated` when it left some template ungated. A `kp` query
//!    parameter scopes the whole answer to one knowledge point, so an operator
//!    reads the notes of any single template with one request.
//!
//! An empty `notes` list is the good case: the gate skipped no check.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use cadus_core::pool::split_kp_key;
use cadus_core::template::{GateSpec, gate_body};
use cadus_store::begin_tenant;
use cadus_store::pool::{KpFlag, approved_template};
use serde_json::{Value, json};

use crate::AppState;
use crate::auth::guard::current_user;
use crate::auth::store_call;
use crate::error::ApiError;
use crate::session::content;
use crate::state::Content;

/// The code of a request from an account that is not an admin.
pub const FORBIDDEN: &str = "forbidden";

/// The message of that refusal.
pub const FORBIDDEN_MESSAGE: &str = "This route serves an admin account only.";

/// The query parameter that scopes the answer to one serving key.
pub const KP_PARAM: &str = "kp";

/// The templates one request gates, at most.
///
/// One `gate` call draws up to `GATE_SAMPLES` instances and solves each one, so
/// it costs about 10 ms on the build box. Twenty of them is about 200 ms of CPU,
/// which an operator screen carries and a learner never waits on: no L\* line
/// covers this route (`docs/reference/l1-budget.md`, section 2.1, the route
/// table).
pub const GATE_NOTE_LIMIT: usize = 20;

/// One `flags` row, as JSON.
fn flag_json(flag: &KpFlag) -> Value {
    json!({
        "kp_id": flag.kp_id,
        "approved_templates": flag.approved_templates,
        "pool_depth": flag.pool_depth,
        "last_source": flag.last_source.map(|source| source.as_str()),
        "last_exemplar_at": flag.last_exemplar_at.map(|at| at.to_rfc3339()),
        "needs_template": flag.needs_template,
        "source_exhausted": flag.source_exhausted,
    })
}

/// Run the gate over one approved template body and report what it learned.
///
/// A knowledge point the loaded curriculum does not name has no answer kind and
/// no exemplars, so the gate cannot run on it. The row then carries
/// `gated: false`, which is the same report the refill worker logs for that
/// case. Silence is what A6 refuses.
pub(crate) fn gate_json(content: &Content, kp_id: &str, digest: &str, body: &str) -> Value {
    let Some(spec) = spec_of(content, kp_id) else {
        return json!({
            "kp_id": kp_id,
            "digest": digest,
            "gated": false,
            "reason": "the curriculum does not name this knowledge point",
            "notes": Vec::<String>::new(),
        });
    };
    match gate_body(body, &spec) {
        Ok((_, verified)) => json!({
            "kp_id": kp_id,
            "digest": digest,
            "gated": true,
            "exhaustive": verified.exhaustive,
            "instances_checked": verified.instances_checked,
            "notes": verified.notes,
        }),
        Err(rejection) => json!({
            "kp_id": kp_id,
            "digest": digest,
            "gated": true,
            "rejected": {"code": rejection.code, "message": rejection.message},
            "notes": Vec::<String>::new(),
        }),
    }
}

/// The gate specification of one serving key, when the curriculum names it.
///
/// The answer kind belongs to the topic and the exemplars belong to the
/// knowledge point, which is the pair `cadus_worker::refill` builds for the same
/// call.
pub(crate) fn spec_of<'a>(content: &'a Content, kp_id: &str) -> Option<GateSpec<'a>> {
    let graph = &content.curriculum;
    let (topic_id, point_id) = split_kp_key(kp_id)?;
    let topic_idx = graph.idx_of(topic_id)?;
    let kp_idx = graph.kp_idx_of(topic_idx, point_id)?;
    // Both indexes came from the graph one line above, so both reads find
    // their row.
    graph
        .topic(topic_idx)
        .zip(graph.knowledge_point(topic_idx, kp_idx))
        .map(|(topic, kp)| GateSpec {
            answer_kind: topic.answer_kind,
            exemplars: &kp.exemplars,
        })
}

/// Serve the A6 operator view (admin only).
///
/// # Errors
///
/// - `401 unauthorized` — no session, or a session the guard refuses.
/// - `403 forbidden` — a live session whose account is not an admin.
/// - `503 curriculum_unavailable` — the process loaded no curriculum.
/// - `500 internal_error` — a statement failed or passed its bound.
pub async fn flags(
    State(state): State<AppState>,
    Query(params): Query<BTreeMap<String, String>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let authed = current_user(&state, &headers).await?;
    if !authed.user.is_admin {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            FORBIDDEN,
            FORBIDDEN_MESSAGE,
        ));
    }
    let content = content(&state)?;
    let wanted = params.get(KP_PARAM).map(String::as_str);

    let mut tx = store_call(
        &state.db,
        "operator flags: tenant bind",
        begin_tenant(state.db.pool(), authed.user.id),
    )
    .await?;
    let rows = store_call(
        &state.db,
        "operator flags: read",
        cadus_store::pool::operator_flags(&mut *tx),
    )
    .await?;
    let rows: Vec<KpFlag> = match wanted {
        Some(kp_id) => rows.into_iter().filter(|row| row.kp_id == kp_id).collect(),
        None => rows,
    };

    // The gate loop reads at most GATE_NOTE_LIMIT approved templates, in the
    // `kp_id` order `operator_flags` already returns. A knowledge point with no
    // approved template has no document to gate, so it is not a candidate.
    let mut gate = Vec::new();
    let mut truncated = false;
    for row in rows.iter().filter(|row| row.approved_templates > 0) {
        if gate.len() == GATE_NOTE_LIMIT {
            truncated = true;
            break;
        }
        let found = store_call(
            &state.db,
            "operator flags: template read",
            approved_template(&mut *tx, &row.kp_id),
        )
        .await?;
        // The row counted an approved template, so the read finds one.
        gate.extend(
            found.map(|template| gate_json(content, &row.kp_id, &template.digest, &template.body)),
        );
    }

    // The transaction is read-only, so the drop rolls it back, releases the
    // binding, and needs no second round trip.
    drop(tx);

    Ok(Json(json!({
        "flags": rows.iter().map(flag_json).collect::<Vec<Value>>(),
        "gate": gate,
        "gate_limit": GATE_NOTE_LIMIT,
        "gate_truncated": truncated,
    })))
}
