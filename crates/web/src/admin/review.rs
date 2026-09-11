//! The two review writes, `POST /api/admin/content/{digest}/approve` and
//! `/reject`, and the re-gate an approval runs (C6).

use axum::Json;
use axum::extract::State;
use cadus_core::curriculum::Exemplar;
use cadus_core::instruction::{
    InstructionSpec, ServedInstance, regate_with_policy, template_instances,
};
use cadus_store::content::{self, Admin, KIND_TEMPLATE};
use cadus_store::{Db, StoreError};
use serde_json::{Value, json};

use super::{
    AdminUser, BODY_MESSAGE, REASON_FIELD, REASON_LONG_MESSAGE, REASON_MAX_CHARS, REASON_MESSAGE,
    admin_path, decided,
};
use crate::AppState;
use crate::auth::body::LimitedBody;
use crate::error::ApiError;
use crate::operator::spec_of;
use crate::path::ApiPath;
use crate::state::Content;

/// `POST /api/admin/content/{digest}/approve` — approve one digest (C6).
///
/// Approval binds the immutable content digest and the currently loaded finite
/// policy fingerprint. A changed policy requires a fresh AI or optional human
/// review. Instruction approval also binds the complete eligible template bank. Repeating
/// approval under the same context preserves its first stamp.
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
    LimitedBody(body): LimitedBody,
) -> Result<Json<Value>, ApiError> {
    let admin = admin_path(&state)?;
    let found = crate::grade::store(&state, content::document(state.db.pool(), &digest))
        .await?
        .ok_or_else(super::unknown_digest)?;
    let policy = state
        .content
        .as_ref()
        .map(|loaded| loaded.policy_digest(&found.item.kp_id))
        .transpose()?
        .flatten();
    let curriculum_digest = state
        .content
        .as_ref()
        .ok_or_else(|| ApiError::internal("The current curriculum is unavailable."))?
        .curriculum_context_digest()?;
    let review_engine_digest = state
        .content
        .as_ref()
        .map(|content| content.review_engine_digest())
        .ok_or_else(|| ApiError::internal("The current review engine is unavailable."))?;
    let current = content::CurrentContext {
        policy_digest: policy.as_deref(),
        curriculum_digest,
        review_engine_digest,
    };
    let template_context = if matches!(
        found.item.kind.as_str(),
        "template" | "teach" | "hint_ladder"
    ) {
        crate::grade::store(
            &state,
            content::template_review_context(
                state.db.pool(),
                &found.item.kp_id,
                current,
                (found.item.kind == KIND_TEMPLATE).then_some(digest.as_str()),
            ),
        )
        .await?
        .0
    } else {
        None
    };
    check_approval_context(
        &body,
        policy.as_deref(),
        template_context.as_deref(),
        curriculum_digest,
        review_engine_digest,
    )?;
    let decision = crate::grade::store(
        &state,
        content::approve_current(
            Admin::new(admin),
            &digest,
            Some(authed.user.id),
            content::ApprovalContext {
                current,
                template_context_digest: template_context.as_deref(),
            },
        ),
    )
    .await?
    .ok_or_else(context_changed)?;

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
        "approved_policy_digest": policy,
        "approved_template_context_digest": matches!(
            found.item.kind.as_str(), "template" | "teach" | "hint_ladder"
        ).then_some(template_context).flatten(),
        "approved_curriculum_digest": curriculum_digest,
        "approved_review_engine_digest": review_engine_digest,
        "rejected_documents": regated,
    })))
}

#[derive(Default, serde::Deserialize)]
struct ApprovalContext {
    #[serde(default)]
    policy_digest: Option<String>,
    #[serde(default)]
    template_context_digest: Option<String>,
    #[serde(default)]
    curriculum_digest: Option<String>,
    #[serde(default)]
    review_engine_digest: Option<String>,
}

fn check_approval_context(
    body: &[u8],
    current: Option<&str>,
    template_context: Option<&str>,
    curriculum_digest: &str,
    review_engine_digest: &str,
) -> Result<(), ApiError> {
    let expected: ApprovalContext = if body.is_empty() {
        ApprovalContext::default()
    } else {
        serde_json::from_slice(body).map_err(|_| {
            ApiError::invalid_request(
                "The approval context must contain a policy digest or null.".to_owned(),
            )
        })?
    };
    if expected.policy_digest.as_deref() != current
        || expected.template_context_digest.as_deref() != template_context
        || expected.curriculum_digest.as_deref() != Some(curriculum_digest)
        || expected.review_engine_digest.as_deref() != Some(review_engine_digest)
    {
        return Err(context_changed());
    }
    Ok(())
}

fn context_changed() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::CONFLICT,
        "review_context_changed",
        "The exercise policy or template bank changed. Review the current content before approval.",
    )
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
    // A digest the table no longer holds and a document of another kind give
    // the same answer: nothing to judge again.
    let Some(found) = content::document(state.db.pool(), digest)
        .await?
        .filter(|found| found.item.kind == KIND_TEMPLATE)
    else {
        return Ok(Vec::new());
    };
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
/// - an APPROVED page or ladder retains its decision and serves only while its
///   template-context stamp matches the current bank;
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
        let policy = gate_spec
            .as_ref()
            .and_then(|spec| spec.finite.as_ref())
            .map(|finite| finite.policy);
        let Some(rejection) = regate_with_policy(&row.kind, &row.body.to_string(), &spec, policy)
        else {
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
mod tests;
