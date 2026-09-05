//! The two review writes, `POST /api/admin/content/{digest}/approve` and
//! `/reject`, and the re-gate an approval runs (C6).

use axum::Json;
use axum::extract::State;
use cadus_core::curriculum::Exemplar;
use cadus_core::instruction::{InstructionSpec, ServedInstance, regate, template_instances};
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

    use cadus_store::test_support::TestDb;
    use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
    use sqlx::PgPool;

    use super::{Regated, regate_knowledge_point};

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
