//! The request body and its caps (spec section 5.7), the answer kind of a
//! served problem, and the `attempt` event one submission builds.

use super::*;

/// What the client sends. Every field is read defensively: the handler must not
/// panic on ANY body (the no-panic rule).
pub(super) struct Submission {
    /// The id of the problem the learner answers.
    pub(super) problem_id: String,
    /// The answer text. An absent field is a blank answer.
    pub(super) answer: String,
    /// The learner's written work.
    pub(super) work: Option<String>,
    /// Whether the client flags the attempt reference-assisted (H3).
    pub(super) assisted: bool,
}

/// Read the body, or the `422`/`413` that refuses it.
pub(super) fn submission(body: Option<&Value>) -> Result<Submission, ApiError> {
    let field = |name: &str| body.and_then(|value| value.get(name));
    let problem_id = field("problem_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if problem_id.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            INVALID_REQUEST,
            "answer requires a problem_id.",
        ));
    }
    let answer = text_field(field("answer"), "answer")?.unwrap_or_default();
    let work = text_field(field("work"), "work")?;
    // The caps bound the checker's CPU on hostile input (spec section 5.7). The
    // count is in CHARACTERS, so a multi-byte answer is not refused for its
    // encoding.
    if answer.chars().count() > MAX_ANSWER_CHARS
        || work
            .as_ref()
            .is_some_and(|text| text.chars().count() > MAX_WORK_CHARS)
    {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            ANSWER_TOO_LARGE,
            "answer or work exceeds the size limit.",
        ));
    }
    Ok(Submission {
        problem_id,
        answer,
        work,
        assisted: field("assisted").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// One optional text field of the body, or the `422` that refuses a non-string.
fn text_field(value: Option<&Value>, name: &str) -> Result<Option<String>, ApiError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            INVALID_REQUEST,
            format!("{name} must be a string."),
        )),
    }
}

/// `500 state_unavailable`: the stored D-S6 row names something the arena does
/// not hold, so the request cannot be graded honestly.
pub(super) fn broken_state(reason: &str) -> ApiError {
    tracing::error!(reason = %reason, "answer: the served problem is not gradable");
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        STATE_UNAVAILABLE,
        "The stored session state is not usable for this answer.",
    )
}

/// The answer kind of a served problem, in the checker's spelling.
pub(super) fn answer_kind(served: &ServedProblem) -> Option<AnswerKind> {
    match served.answer_kind.as_deref() {
        Some("numeric") => Some(AnswerKind::Numeric),
        Some("expression") => Some(AnswerKind::Expression),
        Some("multi-step") => Some(AnswerKind::MultiStep),
        Some("proof") => Some(AnswerKind::Proof),
        _ => None,
    }
}

/// The event spelling of an answer kind.
const fn event_kind(kind: AnswerKind) -> cadus_core::event::AnswerKind {
    match kind {
        AnswerKind::Numeric => cadus_core::event::AnswerKind::Numeric,
        AnswerKind::Expression => cadus_core::event::AnswerKind::Expression,
        AnswerKind::MultiStep => cadus_core::event::AnswerKind::MultiStep,
        AnswerKind::Proof => cadus_core::event::AnswerKind::Proof,
    }
}

/// The 1-based position of THIS attempt inside its task, read from the log.
///
/// It counts the `attempt` events the task already holds and adds one. An id of
/// the task is `{task_id}-{n}`, so the count takes the ids that carry the
/// `{task_id}-` prefix AND a digit after it. A topic id may hold a hyphen, so
/// one task id can be the prefix of another; the digit keeps the attempts of
/// task `{task}-{other}` out of the count of task `{task}`. The H3 re-solve id
/// `{task_id}-{n}-rework` passes both tests, so a re-solve counts as the one
/// attempt it records.
///
/// `events` is the OPEN SESSION's window, and a task id carries its session id,
/// so every attempt of the task is inside it (F15).
///
/// The caller holds the tenant's advisory lock and reads the log inside the same
/// transaction as the INSERT, so no second request of this tenant computes the
/// same number.
pub(super) fn attempt_index(events: &[EventRow], task_id: &str) -> i64 {
    let prefix = format!("{task_id}-");
    let recorded = events
        .iter()
        .filter(|row| match &row.event {
            Event::Attempt(attempt) => attempt
                .attempt_id
                .strip_prefix(&prefix)
                .is_some_and(|rest| rest.starts_with(|first: char| first.is_ascii_digit())),
            _ => false,
        })
        .count();
    i64::try_from(recorded)
        .unwrap_or(i64::MAX)
        .saturating_add(1)
}

/// The attempt the log already holds under `attempt_id`.
///
/// Step 6 appends nothing when that row stands, so this is the "state read" the
/// [`STATUS_ALREADY_RECORDED`] reply answers with.
pub(super) fn stored_attempt<'a>(events: &'a [EventRow], attempt_id: &str) -> Option<&'a Attempt> {
    events.iter().find_map(|row| match &row.event {
        Event::Attempt(attempt) if attempt.attempt_id == attempt_id => Some(attempt),
        _ => None,
    })
}

/// The verdict one submission got, with the clock beside it.
pub(super) struct Graded<'a> {
    /// The deterministic verdict.
    pub(super) grade: &'a Grade,
    /// The verdict's tags and the timing tags, in that order.
    pub(super) error_tags: &'a [String],
    /// The server-measured solve time.
    pub(super) secs: i64,
    /// The answer grammar the checker read.
    pub(super) kind: AnswerKind,
    /// Whether the attempt is reference-assisted (H3).
    pub(super) assisted: bool,
}

/// Build the `attempt` event of one submission (`_graded_answer`), and the
/// JSON form the H3 stash keeps of it.
///
/// The stash is built here, beside the event, so one function owns the two
/// spellings of one attempt and the route never serializes on its own.
pub(super) fn build_attempt(
    task: &Task,
    served: &ServedProblem,
    submitted: &Submission,
    graded: &Graded<'_>,
    session: Option<&str>,
    now: Timestamp,
    index: i64,
) -> Result<(Attempt, Value), ApiError> {
    let Some(topic) = served.topic.as_deref().and_then(|id| Slug::new(id).ok()) else {
        return Err(broken_state("the served problem names no topic"));
    };
    let secs = Secs::new(graded.secs).map_err(|err| broken_state(&format!("secs: {err}")))?;
    let attempt = Attempt {
        ts: now,
        session: session.map(str::to_string),
        v: SchemaVersion,
        attempt_id: format!("{}-{index}", task.task_id),
        task_id: task.task_id.clone(),
        topic,
        kp: served.kp.as_deref().and_then(|id| Slug::new(id).ok()),
        task_type: task.task_type,
        problem: AttemptProblem {
            text: served.text.clone(),
            expected: served.expected.answer.clone(),
        },
        given_answer: submitted.answer.clone(),
        work: submitted.work.clone().filter(|text| !text.is_empty()),
        answer_kind: Some(event_kind(graded.kind)),
        correct: graded.grade.correct,
        secs,
        error_tags: graded.error_tags.to_vec(),
        work_quality: graded.grade.work_quality,
        grader_note: Some(GRADER_NOTE.to_string()),
        assisted: graded.assisted,
    };
    let stash = serde_json::to_value(&attempt)
        .map_err(|err| broken_state(&format!("the attempt does not serialize: {err}")))?;
    Ok((attempt, stash))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    /// A served problem of `kind`, read from its D-S6 document.
    fn served(kind: Option<&str>) -> ServedProblem {
        serde_json::from_value(json!({
            "problem_id": "p1",
            "task_id": "s-lesson-addition",
            "topic": "addition",
            "kp": "kp1",
            "answer_kind": kind,
            "text": "Compute 8 + 5.5.",
            "expected": {"v": 1, "answer": "13.5"},
            "started_at": 0.0,
        }))
        .unwrap()
    }

    /// The lesson task the served problem belongs to.
    fn lesson() -> Task {
        Task {
            task_id: "s-lesson-addition".to_string(),
            task_type: TaskType::Lesson,
            topic: Some("addition".to_string()),
            n_problems: None,
            mix: Vec::new(),
            difficulty_target: None,
            recent_problem_hashes: Vec::new(),
            start_at_kp: None,
            time_budget_secs: None,
            why: String::new(),
            gap_fill: false,
            gap_return_to: None,
            component_topics: Vec::new(),
            is_remediation: false,
            nearly_due: false,
        }
    }

    /// One wrong numeric submission, `secs` seconds after the hand-off.
    fn miss() -> Submission {
        Submission {
            problem_id: "p1".to_string(),
            answer: "14".to_string(),
            work: Some(String::new()),
            assisted: false,
        }
    }

    /// The four spellings the D-S6 row uses, and the absent one.
    #[test]
    fn the_answer_kind_reads_the_checker_spelling() {
        assert_eq!(
            answer_kind(&served(Some("numeric"))),
            Some(AnswerKind::Numeric)
        );
        assert_eq!(
            answer_kind(&served(Some("expression"))),
            Some(AnswerKind::Expression)
        );
        assert_eq!(
            answer_kind(&served(Some("multi-step"))),
            Some(AnswerKind::MultiStep)
        );
        assert_eq!(answer_kind(&served(Some("proof"))), Some(AnswerKind::Proof));
        assert_eq!(answer_kind(&served(Some("essay"))), None);
        assert_eq!(answer_kind(&served(None)), None);
    }

    /// The event spelling is the same word for every kind.
    #[test]
    fn the_event_kind_keeps_the_word() {
        use cadus_core::event::AnswerKind as Wire;
        assert_eq!(event_kind(AnswerKind::Numeric), Wire::Numeric);
        assert_eq!(event_kind(AnswerKind::Expression), Wire::Expression);
        assert_eq!(event_kind(AnswerKind::MultiStep), Wire::MultiStep);
        assert_eq!(event_kind(AnswerKind::Proof), Wire::Proof);
    }

    /// A negative solve time is not an event field, a problem with no topic
    /// names no attempt topic, and an instant outside the wire range does not
    /// serialize: each one is `state_unavailable`, never a panic.
    #[test]
    fn an_attempt_the_event_grammar_refuses_is_state_unavailable() {
        let grade = Grade {
            correct: false,
            work_quality: WorkQuality::NearlyPassable,
            error_tags: Vec::new(),
        };
        let mut graded = Graded {
            grade: &grade,
            error_tags: &[],
            secs: -1,
            kind: AnswerKind::Numeric,
            assisted: false,
        };
        let refused = build_attempt(
            &lesson(),
            &served(Some("numeric")),
            &miss(),
            &graded,
            Some("s"),
            Timestamp::from_micros(0),
            1,
        );
        assert_eq!(refused.err().map(|err| err.code), Some(STATE_UNAVAILABLE));

        graded.secs = 20;
        let mut orphan = served(Some("numeric"));
        orphan.topic = None;
        let refused = build_attempt(
            &lesson(),
            &orphan,
            &miss(),
            &graded,
            Some("s"),
            Timestamp::from_micros(0),
            1,
        );
        assert_eq!(refused.err().map(|err| err.code), Some(STATE_UNAVAILABLE));

        let refused = build_attempt(
            &lesson(),
            &served(Some("numeric")),
            &miss(),
            &graded,
            Some("s"),
            Timestamp::from_micros(i64::MAX),
            1,
        );
        assert_eq!(refused.err().map(|err| err.code), Some(STATE_UNAVAILABLE));

        let (attempt, stash) = build_attempt(
            &lesson(),
            &served(Some("numeric")),
            &miss(),
            &graded,
            None,
            Timestamp::from_micros(0),
            3,
        )
        .unwrap();
        assert_eq!(attempt.attempt_id, "s-lesson-addition-3");
        assert_eq!(attempt.session, None);
        // An empty work field is no work at all.
        assert_eq!(attempt.work, None);
        assert_eq!(stash["attempt_id"], "s-lesson-addition-3");
    }
}
