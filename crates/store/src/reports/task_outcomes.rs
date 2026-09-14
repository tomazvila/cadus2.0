//! Report-owned, append-only replacements of historical task results.
//!
//! Replacements live in the native regrade reason, retaining the original event
//! sequence and timestamp. Keeping the regrade in the stream also invalidates
//! cached projections through the existing regrade replay rule.
use std::collections::BTreeMap;

use cadus_core::config::Config;
use cadus_core::event::{Attempt, Event, QuizTopicResult, Regraded, TaskType, WorkQuality};
use cadus_core::fire::assess_review;
use cadus_core::projector::{apply_regrades, kp_failed, kp_passed};
use cadus_core::xp::{is_rushing, task_xp};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction, types::Uuid};

use crate::StoreError;
use crate::state::{EventRow, load_raw_events_after, load_web_state, save_web_state};

const PREFIX: &str = "cadus.report.task-outcomes.v1:";

/// Construct the native audit event and all task-result changes for one report.
pub(super) async fn complete_submission(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    packet: &Value,
    report: Uuid,
) -> Result<(Option<Event>, Value), StoreError> {
    use cadus_core::event::{AttemptOutcome, RegradedAttempt, SchemaVersion, Slug, Timestamp};
    if packet["content_only"] == true {
        return Ok((None, json!({"status":"content_only"})));
    }
    let mut original: Event = serde_json::from_value(packet["attempt"].clone())
        .map_err(|error| StoreError::Document(format!("invalid report event: {error}")))?;
    original.normalize();
    let (task_id, topic, session, attempts) = match &original {
        Event::Attempt(attempt) => {
            if attempt.correct {
                return Ok((None, json!({"status":"already_correct"})));
            }
            (
                attempt.task_id.clone(),
                attempt.topic.clone(),
                attempt.session.clone(),
                vec![RegradedAttempt {
                    attempt_id: attempt.attempt_id.clone(),
                    outcome: Some(AttemptOutcome::Correct),
                    work_quality: WorkQuality::NearlyPerfect,
                    error_tags: Vec::new(),
                    grader_note: Some(format!(
                        "Qwen report {report}; checked mathematical evidence"
                    )),
                }],
            )
        }
        Event::IntegratedAttempt(attempt) => (
            attempt.task_id.clone(),
            Slug::new(&attempt.topic).map_err(|error| StoreError::Document(error.to_string()))?,
            attempt.session.clone(),
            Vec::new(),
        ),
        Event::DiagnosticAnswer(attempt) => (
            packet["report_identity"]["task_id"]
                .as_str()
                .ok_or_else(|| {
                    StoreError::Document("diagnostic report has no task identity".into())
                })?
                .to_owned(),
            attempt.topic.clone(),
            attempt.session.clone(),
            Vec::new(),
        ),
        _ => return Err(StoreError::Document("unsupported report event".into())),
    };
    let micros = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| StoreError::Document(error.to_string()))?
        .as_micros();
    let mut correction = Event::Regraded(Regraded {
        ts: Timestamp::from_micros(
            i64::try_from(micros).map_err(|error| StoreError::Document(error.to_string()))?,
        ),
        session,
        v: SchemaVersion::current(),
        task_id,
        topic,
        attempts,
        quality_tier: None,
        xp: None,
        reason: format!("Verified problem report {report}; original submission retained"),
    });
    let summary = match original {
        Event::Attempt(_) => complete_regrade(tx, user, packet, &mut correction).await?,
        Event::IntegratedAttempt(_) => {
            complete_integrated_regrade(tx, user, packet, &mut correction).await?
        }
        Event::DiagnosticAnswer(_) => {
            super::diagnostics::complete_regrade(tx, user, packet, &mut correction).await?
        }
        _ => unreachable!("event kind was checked above"),
    };
    Ok((Some(correction), summary))
}

#[derive(Deserialize)]
struct Policy {
    config: Config,
    knowledge_points: Vec<String>,
    expected_time_secs: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Replacement {
    seq: i64,
    event: Option<Event>,
}

#[derive(Serialize, Deserialize)]
struct Repair {
    reason: String,
    replacements: Vec<Replacement>,
}

/// Price the closed assessment using every effective attempt, then freeze its
/// replacements inside the regrade that the caller appends in this transaction.
pub(super) async fn complete_regrade(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    packet: &Value,
    event: &mut Event,
) -> Result<Value, StoreError> {
    let Event::Regraded(correction) = event else {
        return Err(StoreError::Document(
            "task repair requires a regrade".into(),
        ));
    };
    let policy: Policy = serde_json::from_value(packet["task_policy"].clone())
        .map_err(|error| StoreError::Document(format!("report task policy is invalid: {error}")))?;
    let rows = load_raw_events_after(tx, user, 0).await?;
    let replacements = recalculate(&rows, &policy, correction);
    repair_quiz_scratch(tx, user, packet).await?;
    let incomplete = replacements.iter().any(|replacement| {
        replacement.event.is_none()
            && rows.iter().any(|row| {
                row.seq == replacement.seq && matches!(row.event, Event::LessonResult(_))
            })
    });
    if incomplete {
        reopen_lesson(tx, user, &rows, &policy, correction).await?;
    }
    let summary = json!({
        "status": if incomplete { "incomplete" }
            else if replacements.is_empty() { "no_closed_result" } else { "recalculated" },
        "results": replacements.iter().filter_map(|r| r.event.as_ref()).collect::<Vec<_>>(),
    });
    // Native task-price corrections and the store overlay must agree. A later
    // report always replaces the whole price, never adds a second XP award.
    for replacement in &replacements {
        match &replacement.event {
            Some(Event::LessonResult(result)) => {
                correction.quality_tier = Some(result.quality_tier);
                correction.xp = Some(result.xp);
            }
            Some(Event::ReviewResult(result)) => {
                correction.quality_tier = Some(result.quality_tier);
                correction.xp = Some(result.xp);
            }
            _ => {}
        }
    }
    let repair = Repair {
        reason: correction.reason.clone(),
        replacements,
    };
    correction.reason = format!(
        "{PREFIX}{}",
        serde_json::to_string(&repair).map_err(|error| StoreError::Document(error.to_string()))?
    );
    Ok(summary)
}

async fn repair_quiz_scratch(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    packet: &Value,
) -> Result<(), StoreError> {
    if packet["attempt"]["task_type"] != "quiz" {
        return Ok(());
    }
    let Some(mut scratch) = load_web_state(tx, user).await? else {
        return Ok(());
    };
    if scratch["session"] != packet["attempt"]["session"] {
        return Ok(());
    }
    let Some(task) = packet["attempt"]["task_id"].as_str() else {
        return Ok(());
    };
    let Some(problem) = packet["report_identity"]["problem_id"]
        .as_str()
        .or_else(|| packet["problem_id"].as_str())
    else {
        return Ok(());
    };
    let Some(answers) = scratch["quizzes"][task]["answers"].as_array_mut() else {
        return Ok(());
    };
    for answer in answers
        .iter_mut()
        .filter(|answer| answer["problem_id"] == problem)
    {
        answer["correct"] = json!(true);
        answer["outcome"] = json!("correct");
        answer["reason"] = Value::Null;
    }
    let all_correct = answers.iter().all(|answer| answer["correct"] == true);
    if all_correct {
        if let Some(pending) = scratch["feedback_practice"].as_object_mut() {
            pending.remove(task);
        }
        if let Some(progress) = scratch["tasks"].get_mut(task) {
            progress["done"] = json!(
                progress["total"].as_i64().unwrap_or(0) > 0
                    && progress["answered"].as_i64().unwrap_or(0)
                        >= progress["total"].as_i64().unwrap_or(0)
            );
        }
    }
    save_web_state(tx, user, &scratch).await
}

async fn reopen_lesson(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    rows: &[EventRow],
    policy: &Policy,
    correction: &Regraded,
) -> Result<(), StoreError> {
    let Some(mut scratch) = load_web_state(tx, user).await? else {
        return Ok(());
    };
    if scratch["session"].as_str() != correction.session.as_deref() {
        return Ok(());
    }
    let mut events: Vec<Event> = rows.iter().map(|row| row.event.clone()).collect();
    events.push(Event::Regraded(correction.clone()));
    let effective = apply_regrades(&events);
    let attempts: Vec<_> = effective
        .iter()
        .filter_map(|event| match event {
            Event::Attempt(attempt)
                if attempt.task_id == correction.task_id
                    && !attempt.assisted
                    && !attempt.feedback_practice
                    && !attempt.outcome.is_ungraded() =>
            {
                Some(attempt)
            }
            _ => None,
        })
        .collect();
    let next = policy.knowledge_points.iter().find(|kp| {
        let sequence: Vec<_> = attempts
            .iter()
            .filter(|a| {
                a.kp.as_ref()
                    .map(|id| id.as_str())
                    .or_else(|| policy.knowledge_points.first().map(String::as_str))
                    == Some(kp.as_str())
            })
            .map(|a| a.correct)
            .collect();
        !kp_passed(&sequence, policy.config.lesson.pass_rule())
    });
    if let Some(progress) = scratch["tasks"].get_mut(&correction.task_id) {
        progress["done"] = json!(false);
        progress["current_kp"] = json!(next);
    }
    // Remove a now-correct task's obsolete feedback queue; retain queues with
    // any independent incorrect evidence still requiring practice.
    if attempts.iter().all(|a| a.correct)
        && let Some(pending) = scratch["feedback_practice"].as_object_mut()
    {
        pending.remove(&correction.task_id);
    }
    save_web_state(tx, user, &scratch).await
}

/// Add a server-derived replacement to a native replay-invalidating regrade.
/// The caller owns verification and supplies the original tenant event sequence.
pub(super) fn replace_event(
    correction: &mut Event,
    seq: i64,
    replacement: Event,
) -> Result<(), StoreError> {
    let Event::Regraded(correction) = correction else {
        return Err(StoreError::Document(
            "event repair requires a regrade".into(),
        ));
    };
    let mut repair = if let Some(encoded) = correction.reason.strip_prefix(PREFIX) {
        serde_json::from_str::<Repair>(encoded)
            .map_err(|error| StoreError::Document(error.to_string()))?
    } else {
        Repair {
            reason: correction.reason.clone(),
            replacements: Vec::new(),
        }
    };
    repair.replacements.push(Replacement {
        seq,
        event: Some(replacement),
    });
    correction.reason = format!(
        "{PREFIX}{}",
        serde_json::to_string(&repair).map_err(|error| StoreError::Document(error.to_string()))?
    );
    Ok(())
}

/// Correct one verified integrated field and rebuild its stored aggregate grade.
pub(super) async fn complete_integrated_regrade(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    packet: &Value,
    correction: &mut Event,
) -> Result<Value, StoreError> {
    let seq = packet["event_seq"]
        .as_i64()
        .ok_or_else(|| StoreError::Document("integrated report has no event sequence".into()))?;
    let field = packet["report_identity"]["field"]
        .as_str()
        .ok_or_else(|| StoreError::Document("integrated report has no field identity".into()))?;
    let mut rows = load_raw_events_after(tx, user, 0).await?;
    overlay(tx, user, &mut rows).await?;
    let Some(Event::IntegratedAttempt(attempt)) =
        rows.iter().find(|row| row.seq == seq).map(|row| &row.event)
    else {
        return Err(StoreError::Document(
            "integrated report target is unavailable".into(),
        ));
    };
    let Event::Regraded(envelope) = correction else {
        return Err(StoreError::Document(
            "integrated report requires a regrade".into(),
        ));
    };
    if attempt.task_id != envelope.task_id {
        return Err(StoreError::Document(
            "integrated report task changed".into(),
        ));
    }
    let mut fixed = attempt.clone();
    correct_integrated_field(&mut fixed, field)?;
    let summary = json!({"status":"recalculated","solved":fixed.solved,"grade":fixed.grade,"xp":0});
    replace_event(correction, seq, Event::IntegratedAttempt(fixed))?;
    Ok(summary)
}

fn correct_integrated_field(
    attempt: &mut cadus_core::event::IntegratedAttempt,
    field_id: &str,
) -> Result<(), StoreError> {
    use cadus_core::event::AttemptOutcome;
    let field = if field_id == "final" {
        Some(&mut attempt.final_field)
    } else {
        attempt.steps.iter_mut().find(|field| field.id == field_id)
    }
    .ok_or_else(|| StoreError::Document("integrated report field is unavailable".into()))?;
    field.outcome = AttemptOutcome::Correct;
    attempt.solved = attempt.final_field.outcome == AttemptOutcome::Correct;
    attempt.skills_credited.clear();
    for field in attempt
        .steps
        .iter()
        .chain(std::iter::once(&attempt.final_field))
    {
        if field.outcome == AttemptOutcome::Correct {
            for skill in &field.skills {
                if !attempt.skills_credited.contains(skill) {
                    attempt.skills_credited.push(skill.clone());
                }
            }
        }
    }
    if let Some(grade) = &mut attempt.grade {
        for field in grade
            .steps
            .iter_mut()
            .chain(std::iter::once(&mut grade.final_grade))
        {
            if field.id == field_id {
                field.correct = true;
                field.ungraded = false;
                field.answered = true;
                field.notation = false;
            }
        }
        grade.correct_steps = grade.steps.iter().filter(|field| field.correct).count();
        grade.solved = attempt.solved;
        grade.ungraded = grade
            .steps
            .iter()
            .chain(std::iter::once(&grade.final_grade))
            .any(|field| field.ungraded);
        grade.skills_credited.clone_from(&attempt.skills_credited);
    }
    Ok(())
}

/// Overlay effective attempt and task fields for one tenant's requested window.
/// The original regrade rows remain visible, including above a cached cursor.
pub(crate) async fn overlay(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
    rows: &mut Vec<EventRow>,
) -> Result<(), StoreError> {
    if rows.is_empty() {
        return Ok(());
    }
    let corrections = sqlx::query_scalar::<_, sqlx::types::Json<Event>>(
        "SELECT payload FROM events WHERE user_id=$1 AND type='regraded' ORDER BY seq",
    )
    .bind(user)
    .fetch_all(&mut **tx)
    .await?;
    let corrections: Vec<Event> = corrections.into_iter().map(|item| item.0).collect();
    apply_overlay(rows, &corrections)
}

fn apply_overlay(rows: &mut Vec<EventRow>, corrections: &[Event]) -> Result<(), StoreError> {
    let mut replacements = BTreeMap::new();
    let mut attempts = BTreeMap::new();
    for event in corrections {
        let Event::Regraded(correction) = event else {
            continue;
        };
        for attempt in &correction.attempts {
            attempts.insert(
                attempt.attempt_id.as_str(),
                (correction.task_id.as_str(), attempt),
            );
        }
        if let Some(encoded) = correction.reason.strip_prefix(PREFIX) {
            let repair: Repair = serde_json::from_str(encoded)
                .map_err(|error| StoreError::Document(format!("invalid task repair: {error}")))?;
            for replacement in repair.replacements {
                replacements.insert(replacement.seq, replacement.event);
            }
        }
    }
    rows.retain_mut(|row| {
        if let Some(replacement) = replacements.get(&row.seq) {
            let Some(event) = replacement else {
                return false;
            };
            row.event = event.clone();
        }
        if let Event::Attempt(attempt) = &mut row.event
            && let Some((task, correction)) = attempts.get(attempt.attempt_id.as_str())
            && *task == attempt.task_id
        {
            attempt.work_quality = correction.work_quality;
            attempt.error_tags.clone_from(&correction.error_tags);
            attempt.grader_note.clone_from(&correction.grader_note);
            if let Some(outcome) = &correction.outcome {
                attempt.outcome = outcome.clone();
                attempt.correct = outcome == &cadus_core::event::AttemptOutcome::Correct;
            }
        }
        true
    });
    Ok(())
}

fn recalculate(rows: &[EventRow], policy: &Policy, correction: &Regraded) -> Vec<Replacement> {
    let mut events: Vec<Event> = rows.iter().map(|row| row.event.clone()).collect();
    events.push(Event::Regraded(correction.clone()));
    let effective = apply_regrades(&events);
    let attempts: BTreeMap<_, _> = effective
        .iter()
        .filter_map(|event| match event {
            Event::Attempt(attempt) => Some((attempt.attempt_id.as_str(), attempt)),
            _ => None,
        })
        .collect();
    let mut seen = Vec::new();
    let mut last_task_of_topic = BTreeMap::new();
    let mut replacements = Vec::new();
    let mut repair_remediation = false;
    for row in rows {
        if let Event::Attempt(original) = &row.event {
            last_task_of_topic.insert(original.topic.as_str(), original.task_id.as_str());
            if original.task_id == correction.task_id
                && let Some(attempt) = attempts.get(original.attempt_id.as_str())
            {
                seen.push(*attempt);
            }
            repair_remediation = false;
            continue;
        }
        let bound = last_task_of_topic.get(correction.topic.as_str()).copied()
            == Some(correction.task_id.as_str());
        let replacement = match &row.event {
            Event::LessonResult(result) if result.topic == correction.topic && bound => {
                let result = lesson_result(result, &seen, policy);
                repair_remediation = result.as_ref().is_none_or(|r| r.passed);
                Some(result.map(Event::LessonResult))
            }
            Event::ReviewResult(result)
                if result
                    .task_id
                    .as_deref()
                    .filter(|id| !id.is_empty())
                    .map_or(bound, |id| id == correction.task_id)
                    && result.topic == correction.topic =>
            {
                let originals: Vec<_> = seen
                    .iter()
                    .copied()
                    .filter(|a| !a.feedback_practice)
                    .collect();
                let evidence = assess_review(&originals, &policy.config);
                let mut fixed = result.clone();
                fixed.passed = evidence.passed;
                fixed.weighted_score = evidence.score;
                fixed.inconclusive = evidence.inconclusive;
                fixed.confirmation_skills = evidence.confirmation_skills;
                fixed.assisted = originals.iter().any(|a| a.assisted);
                fixed.quality_tier = originals
                    .last()
                    .map_or(result.quality_tier, |a| a.work_quality);
                fixed.xp = if fixed.inconclusive {
                    0.0
                } else {
                    task_xp(
                        TaskType::Review,
                        fixed.quality_tier,
                        &policy.config,
                        1,
                        0,
                        false,
                    )
                };
                repair_remediation = true;
                Some(Some(Event::ReviewResult(fixed)))
            }
            Event::QuizResult(result) if result.quiz_id == correction.task_id => {
                Some(Some(quiz_result(result, &seen, &policy.config)))
            }
            Event::RemediationTriggered(remediation)
                if repair_remediation
                    && remediation.source_topic == correction.topic
                    && (remediation.kind == "lesson_fail"
                        || remediation.kind == "repeat_fail"
                        || remediation.kind.starts_with("review_confirmation:")) =>
            {
                // Review remediation still required by the recomputed result is retained.
                let still_required = replacements
                    .iter()
                    .rev()
                    .find_map(|r: &Replacement| match &r.event {
                        Some(Event::ReviewResult(result)) => {
                            Some(result.confirmation_skills.iter().any(|skill| {
                                skill.split_once('/').is_some_and(|(_, kp)| {
                                    remediation.kind == format!("review_confirmation:{kp}")
                                })
                            }))
                        }
                        _ => None,
                    })
                    .unwrap_or(false);
                if still_required { None } else { Some(None) }
            }
            _ => {
                repair_remediation = false;
                None
            }
        };
        if let Some(event) = replacement {
            replacements.push(Replacement {
                seq: row.seq,
                event,
            });
        }
    }
    replacements
}

fn lesson_result(
    original: &cadus_core::event::LessonResult,
    attempts: &[&Attempt],
    policy: &Policy,
) -> Option<cadus_core::event::LessonResult> {
    let independent: Vec<_> = attempts
        .iter()
        .copied()
        .filter(|a| !a.assisted && !a.feedback_practice && !a.outcome.is_ungraded())
        .collect();
    let last = independent.last()?;
    let cfg = &policy.config;
    let mut failed = None;
    let mut all_passed = !policy.knowledge_points.is_empty();
    for kp in &policy.knowledge_points {
        let sequence: Vec<bool> = independent
            .iter()
            .filter(|a| {
                a.kp.as_ref()
                    .map(|id| id.as_str())
                    .or_else(|| policy.knowledge_points.first().map(String::as_str))
                    == Some(kp.as_str())
            })
            .map(|a| a.correct)
            .collect();
        let passed = kp_passed(&sequence, cfg.lesson.pass_rule());
        all_passed &= passed;
        if kp_failed(&sequence, cfg) && failed.is_none() {
            failed = Some(kp);
        }
    }
    // A correction can remove the only evidence of failure while leaving KPs
    // unanswered. Such a task has no supported completed result to project.
    if !all_passed && failed.is_none() {
        return None;
    }
    let mut fixed = original.clone();
    fixed.passed = all_passed;
    fixed.failed_at_kp = failed.and_then(|kp| cadus_core::event::Slug::new(kp).ok());
    fixed.quality_tier = last.work_quality;
    fixed.assisted = all_passed && attempts.iter().any(|a| a.assisted);
    let count = if all_passed {
        i64::try_from(policy.knowledge_points.len()).unwrap_or(i64::MAX)
    } else {
        1
    };
    fixed.xp = task_xp(
        TaskType::Lesson,
        fixed.quality_tier,
        cfg,
        count,
        0,
        !all_passed && is_rushing(last.correct, last.secs.get(), policy.expected_time_secs),
    );
    Some(fixed)
}

#[expect(
    clippy::cast_precision_loss,
    reason = "a quiz contains a bounded question batch"
)]
fn quiz_result(
    original: &cadus_core::event::QuizResult,
    attempts: &[&Attempt],
    cfg: &Config,
) -> Event {
    let originals: Vec<_> = attempts
        .iter()
        .copied()
        .filter(|a| !a.feedback_practice)
        .collect();
    let decided: Vec<_> = originals
        .iter()
        .copied()
        .filter(|a| !a.outcome.is_ungraded())
        .collect();
    let mut fixed = original.clone();
    fixed.score = decided.iter().filter(|a| a.correct).count() as f64 / decided.len().max(1) as f64;
    fixed.inconclusive = decided.len() != originals.len();
    fixed.xp = if fixed.inconclusive {
        0.0
    } else {
        task_xp(TaskType::Quiz, WorkQuality::NearlyPerfect, cfg, 0, 0, false) * fixed.score
    };
    fixed.per_topic = decided
        .iter()
        .map(|a| QuizTopicResult {
            topic: a.topic.clone(),
            correct: a.correct,
            secs: a.secs,
        })
        .collect();
    Event::QuizResult(fixed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(value: Value) -> Event {
        let mut event: Event = serde_json::from_value(value).unwrap();
        event.normalize();
        event
    }
    fn attempt(id: &str, correct: bool, kind: &str) -> Event {
        event(
            json!({"type":"attempt","ts":"2026-01-01T12:00:00Z","session":"s",
            "attempt_id":id,"task_id":"t","topic":"q","kp":"k","task_type":kind,
            "problem":{"text":"1+1","expected":"3"},"given_answer":"2","correct":correct,
            "secs":30,"work_quality":if correct {"nearly_perfect"} else {"poor"}}),
        )
    }
    fn correction(id: &str) -> Regraded {
        let Event::Regraded(result) = event(json!({"type":"regraded","ts":"2026-01-02T12:00:00Z",
            "task_id":"t","topic":"q","reason":"verified report",
            "attempts":[{"attempt_id":id,"outcome":"correct","work_quality":"nearly_perfect"}]}))
        else {
            unreachable!()
        };
        result
    }
    fn policy() -> Policy {
        Policy {
            config: Config::default(),
            knowledge_points: vec!["k".into()],
            expected_time_secs: 60,
        }
    }
    fn rows(events: Vec<Event>) -> Vec<EventRow> {
        events
            .into_iter()
            .enumerate()
            .map(|(idx, event)| EventRow {
                seq: i64::try_from(idx).unwrap() + 1,
                event,
            })
            .collect()
    }
    fn lesson() -> Event {
        event(
            json!({"type":"lesson_result","ts":"2026-01-01T12:01:00Z","session":"s",
            "topic":"q","passed":false,"failed_at_kp":"k","xp":0,"quality_tier":"poor"}),
        )
    }
    #[test]
    fn corrected_lesson_reprices_original_close_and_session_xp() {
        let mut rows = rows(vec![
            event(json!({"type":"session_start","ts":"2026-01-01T11:00:00Z","session":"s"})),
            attempt("a1", true, "lesson"),
            attempt("a2", false, "lesson"),
            lesson(),
        ]);
        let original = rows.last().unwrap().event.clone();
        let mut correction = correction("a2");
        let replacements = recalculate(&rows, &policy(), &correction);
        let Some(Event::LessonResult(result)) = &replacements[0].event else {
            panic!()
        };
        assert!(result.passed);
        assert_eq!(
            result.xp,
            task_xp(
                TaskType::Lesson,
                WorkQuality::NearlyPerfect,
                &policy().config,
                1,
                0,
                false
            )
        );
        assert_eq!(rows.last().unwrap().event, original);
        correction.reason = format!(
            "{PREFIX}{}",
            serde_json::to_string(&Repair {
                reason: "report".into(),
                replacements
            })
            .unwrap()
        );
        apply_overlay(&mut rows, &[Event::Regraded(correction.clone())]).unwrap();
        let view = crate::state::SessionView::of_log(&rows);
        assert!(view.session_xp["s"] > 0.0);
        assert!(view.lesson_failures.is_empty());
        let once: Vec<_> = rows.iter().map(|r| r.event.clone()).collect();
        apply_overlay(&mut rows, &[Event::Regraded(correction)]).unwrap();
        assert_eq!(
            once,
            rows.iter().map(|r| r.event.clone()).collect::<Vec<_>>()
        );
    }
    #[test]
    fn missing_knowledge_point_evidence_cannot_award_a_lesson_pass() {
        let rows = rows(vec![
            attempt("a1", true, "lesson"),
            attempt("a2", false, "lesson"),
            lesson(),
        ]);
        let mut policy = policy();
        policy.knowledge_points.push("unanswered".into());
        let replacements = recalculate(&rows, &policy, &correction("a2"));
        assert!(replacements[0].event.is_none());
    }
    #[test]
    fn review_recomputes_inconclusive_score_and_xp() {
        let rows = rows(vec![
            attempt("a1", true, "review"),
            attempt("a2", false, "review"),
            event(
                json!({"type":"review_result","ts":"2026-01-01T12:01:00Z","topic":"q",
                "task_id":"t","passed":false,"weighted_score":0.3,"inconclusive":true,"xp":0,"quality_tier":"poor"}),
            ),
        ]);
        let repaired = recalculate(&rows, &policy(), &correction("a2"));
        let Some(Event::ReviewResult(result)) = &repaired[0].event else {
            panic!()
        };
        assert!(result.passed);
        assert!(!result.inconclusive);
        assert_eq!(result.weighted_score, 1.0);
        assert!(result.xp > 0.0);
    }
    #[test]
    fn later_report_uses_earlier_effective_attempts_and_latest_price() {
        let mut rows = rows(vec![
            attempt("a1", false, "quiz"),
            attempt("a2", false, "quiz"),
            event(
                json!({"type":"quiz_result","ts":"2026-01-01T12:01:00Z","quiz_id":"t","score":0,"xp":0}),
            ),
        ]);
        rows.push(EventRow {
            seq: 4,
            event: Event::Regraded(correction("a1")),
        });
        let repaired = recalculate(&rows, &policy(), &correction("a2"));
        let Some(Event::QuizResult(result)) = &repaired[0].event else {
            panic!()
        };
        assert_eq!(result.score, 1.0);
        assert!(result.xp > 0.0);
        assert_eq!(result.per_topic.len(), 2);
    }
    #[test]
    fn drill_correction_preserves_its_non_scored_closure() {
        let rows = rows(vec![
            attempt("a1", false, "drill"),
            event(json!({"type":"drill_result",
            "ts":"2026-01-01T12:01:00Z","task_id":"t"})),
        ]);
        assert!(recalculate(&rows, &policy(), &correction("a1")).is_empty());
    }
    #[test]
    fn another_task_on_the_same_topic_keeps_its_close() {
        let mut other = attempt("other", false, "lesson");
        if let Event::Attempt(attempt) = &mut other {
            attempt.task_id = "other-task".into();
        }
        let rows = rows(vec![attempt("a1", false, "lesson"), other, lesson()]);
        assert!(recalculate(&rows, &policy(), &correction("a1")).is_empty());
    }

    #[test]
    fn integrated_field_repair_rebuilds_aggregate_without_crediting_other_misses() {
        let field = |id: &str, skill: &str| {
            json!({"id":id,"answer":"2","contract":cadus_core::answer::AnswerContract::Exact,
            "outcome":"incorrect","assisted":false,"skills":[skill]})
        };
        let grade = |id: &str, skill: &str| {
            json!({"id":id,"answered":true,"correct":false,
            "ungraded":false,"notation":false,"assisted":false,"skills":[skill]})
        };
        let Event::IntegratedAttempt(mut attempt) = event(json!({"type":"integrated_attempt",
            "ts":"2026-01-01T12:00:00Z","attempt_id":"i1","task_id":"t","item_id":"i",
            "item_digest":"digest","topic":"q","steps":[field("s1","q/k1")],
            "final_field":field("final","q/k2"),"solved":false,"assisted":false,
            "grade":{"item_id":"i","item_digest":"digest","method":null,"steps":[grade("s1","q/k1")],
                "final_grade":grade("final","q/k2"),"correct_steps":0,"total_steps":1,"solved":false,
                "assisted":false,"ungraded":false,"skills_credited":[],"interpretation":"value","reasoning_recorded":false}
        })) else {
            panic!()
        };
        correct_integrated_field(&mut attempt, "s1").unwrap();
        assert!(!attempt.solved);
        assert_eq!(attempt.skills_credited, ["q/k1"]);
        assert_eq!(attempt.grade.as_ref().unwrap().correct_steps, 1);
        correct_integrated_field(&mut attempt, "final").unwrap();
        assert!(attempt.solved);
        assert!(attempt.grade.as_ref().unwrap().solved);
        assert_eq!(attempt.skills_credited, ["q/k1", "q/k2"]);
        assert!(correct_integrated_field(&mut attempt, "missing").is_err());
    }

    #[test]
    fn latest_repair_overlays_original_sequence_and_keeps_replay_invalidator() {
        let mut first = Event::Regraded(correction("a1"));
        let mut replacement = lesson();
        if let Event::LessonResult(result) = &mut replacement {
            result.xp = 2.0;
        }
        replace_event(&mut first, 1, replacement.clone()).unwrap();
        let mut second = Event::Regraded(correction("a2"));
        if let Event::LessonResult(result) = &mut replacement {
            result.xp = 5.0;
        }
        replace_event(&mut second, 1, replacement).unwrap();
        let mut rows = rows(vec![lesson(), first.clone(), second.clone()]);
        apply_overlay(&mut rows, &[first, second]).unwrap();
        assert_eq!(
            rows.iter().map(|row| row.seq).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        let Event::LessonResult(result) = &rows[0].event else {
            panic!()
        };
        assert_eq!(result.xp, 5.0);
        assert!(matches!(rows[2].event, Event::Regraded(_)));
    }

    #[tokio::test]
    async fn integrated_repair_overlays_exact_submission_and_preserves_raw_fields() {
        crate::test_support::TestDb::with(|db| async move {
            let user = db.seed_user("report-integrated@example.test").await;
            let mut tx = crate::begin_tenant(&db.app, user).await.unwrap();
            crate::state::lock_web_state(&mut tx, user).await.unwrap();
            let original = event(
                json!({"type":"integrated_attempt","ts":"2026-01-01T12:00:00Z",
                "attempt_id":"integrated-a1","task_id":"t","item_id":"item","item_digest":"digest",
                "topic":"q","steps":[],"final_field":{"id":"final","answer":"2",
                    "contract":cadus_core::answer::AnswerContract::Exact,"outcome":"incorrect",
                    "assisted":false,"skills":["q/k"]},"solved":false,"assisted":false}),
            );
            crate::state::append_event(&mut tx, user, &original, Some("integrated-a1"))
                .await
                .unwrap();
            let mut correction = Event::Regraded(correction("unrelated"));
            if let Event::Regraded(body) = &mut correction {
                body.attempts.clear();
            }
            let packet = json!({"event_seq":1,"report_identity":{"field":"final"}});
            complete_integrated_regrade(&mut tx, user, &packet, &mut correction)
                .await
                .unwrap();
            crate::state::append_event(&mut tx, user, &correction, None)
                .await
                .unwrap();
            let effective = crate::state::load_events(&mut tx, user).await.unwrap();
            let Event::IntegratedAttempt(attempt) = &effective[0].event else {
                panic!()
            };
            assert!(attempt.solved);
            assert_eq!(attempt.skills_credited, ["q/k"]);
            assert_eq!(
                attempt.final_field.outcome,
                cadus_core::event::AttemptOutcome::Correct
            );
            let raw = crate::state::load_raw_events_after(&mut tx, user, 0)
                .await
                .unwrap();
            assert_eq!(raw[0].event, original);
            assert_eq!(raw.len(), 2);
            let repeated = crate::integrated::attempt(&mut tx, user, "integrated-a1")
                .await
                .unwrap()
                .unwrap();
            assert!(repeated.solved);
            assert_eq!(repeated.skills_credited, ["q/k"]);
            tx.rollback().await.unwrap();
        })
        .await;
    }
}

#[cfg(test)]
mod quiz_scratch_tests {
    use super::*;

    #[tokio::test]
    async fn quiz_repair_updates_reveal_buffer_and_removes_obsolete_practice() {
        crate::test_support::TestDb::with(|db| async move {
            let user = db.seed_user("report-quiz-buffer@example.test").await;
            let mut tx = crate::begin_tenant(&db.app,user).await.unwrap();
            crate::state::lock_web_state(&mut tx,user).await.unwrap();
            let scratch = json!({"session":"s","tasks":{"t":{"done":false,"total":2,"answered":2}},
                "quizzes":{"t":{"answers":[{"problem_id":"p1","correct":false,"outcome":"incorrect"},
                    {"problem_id":"p2","correct":true,"outcome":"correct"}]}},
                "feedback_practice":{"t":{"topic":"q","kp":"k"}}});
            save_web_state(&mut tx,user,&scratch).await.unwrap();
            let packet = json!({"attempt":{"session":"s","task_id":"t","task_type":"quiz"},
                "report_identity":{"problem_id":"p1"}});
            repair_quiz_scratch(&mut tx,user,&packet).await.unwrap();
            repair_quiz_scratch(&mut tx,user,&packet).await.unwrap();
            let repaired = load_web_state(&mut tx,user).await.unwrap().unwrap();
            assert_eq!(repaired["quizzes"]["t"]["answers"][0]["correct"],true);
            assert_eq!(repaired["quizzes"]["t"]["answers"][0]["outcome"],"correct");
            assert_eq!(repaired["quizzes"]["t"]["answers"][1],scratch["quizzes"]["t"]["answers"][1]);
            assert!(repaired["feedback_practice"].get("t").is_none());
            assert_eq!(repaired["tasks"]["t"]["done"],true);
            tx.rollback().await.unwrap();
        }).await;
    }
}
