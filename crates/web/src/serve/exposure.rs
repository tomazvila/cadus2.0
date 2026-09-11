//! Durable ordinary hand-off identity and probe-exposure helpers.

use super::*;

fn item_source(source: Source) -> ItemSource {
    match source {
        Source::Template => ItemSource::Template,
        Source::Exemplar => ItemSource::Exemplar,
        Source::Generator => ItemSource::Generator,
    }
}

fn handoff_context(request: &HandoffRequest<'_>) -> Result<(String, String), ApiError> {
    match request.row.source {
        Source::Template => request
            .row
            .generation_context
            .as_ref()
            .map(|context| {
                (
                    context.curriculum_digest.clone(),
                    context.review_engine_digest.clone(),
                )
            })
            .ok_or_else(|| ApiError::internal("The template row has no generation context.")),
        Source::Exemplar => Ok((
            request.content.curriculum_context_digest()?.to_owned(),
            request.content.review_engine_digest().to_owned(),
        )),
        Source::Generator => Err(ApiError::internal(
            "An unreviewed generated problem cannot be handed off.",
        )),
    }
}

pub(super) fn exposure_of(
    role: Option<FiniteCaseRole>,
    previously_claimed: bool,
    seen: bool,
) -> Exposure {
    if role == Some(FiniteCaseRole::TaughtRehearsal) || previously_claimed || seen {
        Exposure::Repeat
    } else {
        Exposure::First
    }
}

fn finite_case(
    graph: &Curriculum,
    target: &Target,
    row: &PoolRow,
    expected: &cadus_core::pool::PoolAnswer,
) -> Result<Option<(String, FiniteCaseRole)>, ApiError> {
    let policy = graph
        .idx_of(&target.serve)
        .and_then(|topic| graph.kp_idx_of(topic, &target.kp).map(|kp| (topic, kp)))
        .and_then(|(topic, kp)| graph.knowledge_point(topic, kp))
        .and_then(|kp| kp.finite_objective_domain.as_ref());
    let Some(policy) = policy else {
        return Ok(None);
    };
    policy
        .case_for(
            &row.problem.text,
            &expected.answer,
            expected.answer_contract.as_ref(),
        )
        .map(|case| (case.id.as_str().to_owned(), case.role))
        .map(Some)
        .ok_or_else(|| ApiError::internal("The served finite problem has no reviewed case."))
}

pub(super) struct HandoffRequest<'a> {
    pub(super) state: &'a AppState,
    pub(super) content: &'a Content,
    pub(super) user_id: Uuid,
    pub(super) task_id: &'a str,
    pub(super) target: &'a Target,
    pub(super) scratch: &'a WebState,
    pub(super) row: &'a PoolRow,
    pub(super) expected: &'a cadus_core::pool::PoolAnswer,
    pub(super) previously_claimed: bool,
    pub(super) at: Timestamp,
}

pub(super) async fn record_handoff(
    tx: &mut Transaction<'_, Postgres>,
    request: HandoffRequest<'_>,
) -> Result<(String, ProblemHandoff), ApiError> {
    let digest = problem_text_hash(&request.row.problem.text);
    let finite = finite_case(
        &request.content.curriculum,
        request.target,
        request.row,
        request.expected,
    )?;
    let identity = finite
        .as_ref()
        .map_or(HandoffIdentity::Digest(&digest), |(case_id, _)| {
            HandoffIdentity::Finite {
                kp_id: &request.target.key,
                case_id,
            }
        });
    let seen = store(request.state, handoff_seen(tx, request.user_id, identity)).await?;
    let role = finite.as_ref().map(|(_, role)| *role);
    let exposure = exposure_of(role, request.previously_claimed, seen);
    let item_source = item_source(request.row.source);
    let (source_curriculum_digest, source_review_engine_digest) = handoff_context(&request)?;
    let problem_id = Uuid::new_v4().simple().to_string();
    let handoff = ProblemHandoff {
        item_digest: digest.clone(),
        item_source,
        source_content_digest: request.row.content_digest.clone(),
        source_curriculum_digest: Some(source_curriculum_digest),
        source_review_engine_digest: Some(source_review_engine_digest),
        finite_case_id: finite.as_ref().map(|(id, _)| id.clone()),
        finite_case_role: role,
        exposure,
    };
    let event = Event::OrdinaryProblemServed(OrdinaryProblemServed {
        ts: request.at,
        session: request.scratch.session.clone(),
        v: SchemaVersion::current(),
        task_id: request.task_id.to_owned(),
        problem_id: problem_id.clone(),
        kp_id: request.target.key.clone(),
        item_digest: digest,
        item_source,
        finite_case_id: finite.map(|(id, _)| id),
        finite_case_role: role,
        exposure,
    });
    let idempotency = format!("ordinary-problem-served:{problem_id}");
    let Some(seq) = store(
        request.state,
        append_event(tx, request.user_id, &event, Some(&idempotency)),
    )
    .await?
    else {
        return Err(ApiError::internal("The problem hand-off did not record."));
    };
    // This event is projection-neutral. Advance only when it immediately
    // follows the stored cursor; every stale/missing-cache case falls back to
    // the projector so an intervening event can never be skipped.
    if !store(
        request.state,
        advance_handoff_cursor(tx, request.user_id, seq),
    )
    .await?
    {
        let input = projection_input(request.content, request.at);
        store(
            request.state,
            project_and_save(tx, request.user_id, &input, None),
        )
        .await?;
    }
    Ok((problem_id, handoff))
}

pub(super) struct ProbeCandidate<'a> {
    pub(super) state: &'a AppState,
    pub(super) user_id: Uuid,
    pub(super) graph: &'a Curriculum,
    pub(super) target: &'a Target,
    pub(super) row: &'a PoolRow,
    pub(super) previously_claimed: bool,
    pub(super) recent: &'a [String],
    pub(super) digest: &'a str,
}

pub(super) async fn repeats_probe(
    tx: &mut Transaction<'_, Postgres>,
    candidate: ProbeCandidate<'_>,
) -> Result<bool, ApiError> {
    if candidate.previously_claimed || candidate.recent.iter().any(|seen| seen == candidate.digest)
    {
        return Ok(true);
    }
    let expected = answer_of(candidate.graph, candidate.target, candidate.row);
    let finite = finite_case(candidate.graph, candidate.target, candidate.row, &expected)?;
    if finite
        .as_ref()
        .is_some_and(|(_, role)| *role == FiniteCaseRole::TaughtRehearsal)
    {
        return Ok(true);
    }
    let identity =
        finite
            .as_ref()
            .map_or(HandoffIdentity::Digest(candidate.digest), |(case_id, _)| {
                HandoffIdentity::Finite {
                    kp_id: &candidate.target.key,
                    case_id,
                }
            });
    store(
        candidate.state,
        handoff_seen(tx, candidate.user_id, identity),
    )
    .await
}

pub(super) fn repeats_feedback(feedback: Option<&Value>, digest: &str) -> bool {
    feedback.is_some_and(|value| {
        value["digest"].as_str() == Some(digest)
            || value["digests"]
                .as_array()
                .is_some_and(|seen| seen.iter().any(|d| d.as_str() == Some(digest)))
    })
}
