//! Approved instruction is handed off before independent whole-item application.
use super::*;
use crate::state::WebState;
use cadus_store::content::KIND_TEACH;

pub(crate) fn required(content: &Content, task: &Task) -> bool {
    content.cfg.readiness.enforce
        && task.integrated_assessment_of.is_none()
        && for_task(content, task).is_some()
}

pub(super) fn check(content: &Content, task: &Task, scratch: &WebState) -> Result<(), ApiError> {
    if required(content, task) && !scratch.integrated_instruction.contains_key(&task.task_id) {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "instruction_required",
            "Read the worked example before starting this integrated problem.",
        ));
    }
    Ok(())
}

pub(crate) async fn teach(
    state: &AppState,
    content: &Content,
    user_id: Uuid,
    task: &Task,
    scratch: &mut WebState,
    mut tx: Transaction<'static, Postgres>,
) -> Result<Json<Value>, ApiError> {
    let item = for_task(content, task).ok_or_else(no_item)?;
    if task.integrated_assessment_of.is_some() {
        return Err(no_instruction());
    }
    let skill = item
        .skills()
        .into_iter()
        .next()
        .ok_or_else(no_instruction)?;
    let key = skill.to_string();
    let policy = content.policy_digest(&key)?;
    let doc = store(
        state,
        cadus_store::content::approved_document_current(
            &mut *tx,
            &key,
            KIND_TEACH,
            content.review_context(policy.as_deref())?,
        ),
    )
    .await?
    .ok_or_else(no_instruction)?;
    let page: cadus_core::instruction::TeachPage =
        serde_json::from_value(doc.body).map_err(|_| no_instruction())?;
    scratch
        .integrated_instruction
        .insert(task.task_id.clone(), key.clone());
    crate::session::write_state(&state.db, &mut tx, user_id, scratch).await?;
    tx.commit().await.map_err(db_failed)?;
    Ok(Json(
        json!({"kp":key, "concept":page.concept, "worked_example":page.worked_example}),
    ))
}

fn no_instruction() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        "no_instruction",
        "This integrated task needs an approved worked example.",
    )
}
