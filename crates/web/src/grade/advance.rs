//! The lesson advance (`service.advance_task`, `service.py:417-500`), its
//! close event, its remediation, and the task progress it moves.

use super::*;

/// What one recorded attempt did to its task.
pub(super) struct Advance {
    /// The `task_status` of the reply.
    pub(super) status: &'static str,
    /// The close event of a lesson, when the attempt closed one.
    pub(super) result: Option<Event>,
    /// The remediation the close triggered.
    pub(super) remediation: Vec<RemediationTriggered>,
    /// The XP the core priced, when it priced one.
    pub(super) xp: Option<f64>,
    /// The knowledge point a `kp_advance` moves the lesson to
    /// (`_next_kp`, `api.py:230-236`).
    pub(super) next_kp: Option<String>,
}

impl Advance {
    /// The answer for every attempt that closes nothing.
    pub(super) const fn carry_on() -> Self {
        Self {
            status: STATUS_CONTINUE,
            result: None,
            remediation: Vec::new(),
            xp: None,
            next_kp: None,
        }
    }

    /// The events the close appends after the attempt: the result, then the
    /// remediation, in that order.
    pub(super) fn events(&self) -> impl Iterator<Item = Event> + '_ {
        self.result.iter().cloned().chain(
            self.remediation
                .iter()
                .cloned()
                .map(Event::RemediationTriggered),
        )
    }

    /// The `remediation` entries of the reply.
    pub(super) fn remediation_view(&self) -> Vec<Value> {
        self.remediation
            .iter()
            .map(|body| {
                json!({
                    "kind": body.kind,
                    "targets": body.targets.iter().map(Slug::as_str).collect::<Vec<_>>(),
                })
            })
            .collect()
    }
}

/// Decide the task status of one attempt (`advance_task`, `service.py:417-427`).
///
/// Only a lesson advances here. A review, a drill, a quiz and a multi-step task
/// close explicitly, because their pass rule is order-sensitive over the whole
/// question set, so every non-lesson attempt is [`STATUS_CONTINUE`].
///
/// An UNGRADED attempt advances nothing (D-F2). It is not in the knowledge-point
/// sequence, it closes no lesson, and it earns no XP, because the checker gave no
/// verdict to count. The learner still takes the next problem.
pub(super) fn advance(
    graph: &Curriculum,
    cfg: &Config,
    now: Timestamp,
    attempt: &Attempt,
    prior: &[EventRow],
    history: &SessionView,
) -> Advance {
    if !decided_lesson_attempt(attempt) {
        return Advance::carry_on();
    }
    if failed_lesson_practice(attempt, prior) {
        return Advance {
            status: STATUS_TASK_FAILED,
            ..Advance::carry_on()
        };
    }
    let Some(idx) = graph.idx_of(attempt.topic.as_str()) else {
        return Advance::carry_on();
    };
    let kp_ids: Vec<String> = graph
        .knowledge_points(idx)
        .iter()
        .map(|kp| kp.id.as_str().to_string())
        .collect();
    let kp = attempt
        .kp
        .as_ref()
        .map(|slug| slug.as_str().to_string())
        .or_else(|| kp_ids.first().cloned());

    let mut sequence: Vec<bool> = prior
        .iter()
        .filter_map(|row| match &row.event {
            // An ungraded prior attempt is not evidence, so it never enters the
            // sequence the pass rule reads (D-F2).
            Event::Attempt(body)
                if body.task_id == attempt.task_id
                    && !body.outcome.is_ungraded()
                    && !body.assisted
                    && body.kp.as_ref().map(|slug| slug.as_str().to_string()) == kp =>
            {
                Some(body.correct)
            }
            _ => None,
        })
        .collect();
    sequence.push(attempt.correct);

    if kp_failed(&sequence, cfg) {
        return lesson_failed(graph, cfg, now, attempt, kp.as_deref(), history);
    }
    if !kp_passed(&sequence, cfg.lesson.pass_rule()) {
        return Advance::carry_on();
    }
    // The knowledge point passed. A lesson with a later knowledge point advances
    // to it; a lesson at its last one is over.
    let position = kp
        .as_ref()
        .and_then(|id| kp_ids.iter().position(|k| k == id));
    if let Some(at) = position.filter(|at| at + 1 < kp_ids.len()) {
        return Advance {
            status: STATUS_KP_ADVANCE,
            // `_next_kp` reads the point after the CURRENT one, and a served
            // problem that names no knowledge point names no current one.
            next_kp: attempt.kp.as_ref().and(kp_ids.get(at + 1).cloned()),
            ..Advance::carry_on()
        };
    }
    lesson_passed(cfg, now, attempt, prior, kp_ids.len())
}

/// A lesson advances on independent checker decisions.
fn decided_lesson_attempt(attempt: &Attempt) -> bool {
    attempt.task_type == TaskType::Lesson && !attempt.outcome.is_ungraded() && !attempt.assisted
}

/// A supplemental item preserves its already-recorded failed lesson result.
fn failed_lesson_practice(attempt: &Attempt, prior: &[EventRow]) -> bool {
    attempt.feedback_practice && prior.iter().any(|row| matches!(&row.event,
        Event::LessonResult(result) if !result.passed && result.topic == attempt.topic && result.session == attempt.session))
}

/// The passing lesson close and its XP (`advance_task`, the pass arm).
fn lesson_passed(
    cfg: &Config,
    now: Timestamp,
    attempt: &Attempt,
    prior: &[EventRow],
    kp_count: usize,
) -> Advance {
    let quality = attempt.work_quality;
    let count = i64::try_from(kp_count).unwrap_or(i64::MAX);
    let xp = task_xp(TaskType::Lesson, quality, cfg, count, 0, false);
    // The passing lesson is reference-assisted when ANY of its attempts was.
    let assisted = attempt.assisted
        || prior.iter().any(|row| {
            matches!(&row.event, Event::Attempt(body)
                if body.task_id == attempt.task_id && body.assisted)
        });
    Advance {
        status: STATUS_TASK_PASSED,
        result: Some(Event::LessonResult(LessonResult {
            ts: now,
            session: attempt.session.clone(),
            v: SchemaVersion::current(),
            topic: attempt.topic.clone(),
            passed: true,
            failed_at_kp: None,
            xp,
            quality_tier: quality,
            assisted,
        })),
        remediation: Vec::new(),
        xp: Some(round2(xp)),
        next_kp: None,
    }
}

/// The failed lesson close and its remediation (`_lesson_failed`).
fn lesson_failed(
    graph: &Curriculum,
    cfg: &Config,
    now: Timestamp,
    attempt: &Attempt,
    kp: Option<&str>,
    history: &SessionView,
) -> Advance {
    let quality = attempt.work_quality;
    let expected = graph
        .idx_of(attempt.topic.as_str())
        .and_then(|idx| graph.topic(idx))
        .map_or(0, |topic| topic.expected_time_secs);
    // A failing lesson answer that was implausibly fast trips the rushing
    // penalty. A passing lesson never reaches here.
    let rushing = is_rushing(attempt.correct, attempt.secs.get(), expected);
    let xp = task_xp(TaskType::Lesson, quality, cfg, 1, 0, rushing);
    let failed_at_kp = kp.and_then(|id| Slug::new(id).ok());
    let result = Event::LessonResult(LessonResult {
        ts: now,
        session: attempt.session.clone(),
        v: SchemaVersion::current(),
        topic: attempt.topic.clone(),
        passed: false,
        failed_at_kp: failed_at_kp.clone(),
        xp,
        quality_tier: quality,
        assisted: false,
    });

    // A SECOND failure at one knowledge point peels back to its key
    // prerequisites; the first one queues a plain lesson-fail remediation.
    //
    // The test reads the WHOLE-LOG map and not the open session (V2): the first
    // failure closed its lesson task, and that task stays done for the rest of
    // its own session, so the earlier `lesson_result` always stands in an
    // earlier session. The map holds the knowledge point of the event, so the
    // lookup spells the key the same way (`_topic_already_failed`).
    let repeat = history.already_failed(
        attempt.topic.as_str(),
        failed_at_kp.as_ref().map(Slug::as_str),
    );
    let remediation = if repeat {
        let queued =
            remediation_for_repeat_fail(attempt.topic.as_str(), kp.unwrap_or_default(), graph);
        triggered(now, attempt, REMEDIATION_REPEAT_FAIL, queued.targets)
    } else {
        triggered(now, attempt, REMEDIATION_LESSON_FAIL, Vec::new())
    };
    Advance {
        status: STATUS_TASK_FAILED,
        result: Some(result),
        remediation,
        xp: Some(round2(xp)),
        next_kp: None,
    }
}

/// The remediation a failed lesson queues: one event of `kind`, or none when a
/// repeat failure has no key prerequisite to peel back to.
fn triggered(
    now: Timestamp,
    attempt: &Attempt,
    kind: &str,
    targets: Vec<Slug>,
) -> Vec<RemediationTriggered> {
    if kind == REMEDIATION_REPEAT_FAIL && targets.is_empty() {
        return Vec::new();
    }
    vec![RemediationTriggered {
        ts: now,
        session: attempt.session.clone(),
        v: SchemaVersion::current(),
        kind: kind.to_string(),
        source_topic: attempt.topic.clone(),
        targets,
    }]
}

/// Move the task's progress on, and say whether the task is now closed.
///
/// A lesson closes on the core's status. Every other kind closes by count
/// parity, exactly as `api.py:1610-1614` does.
pub(super) fn task_moved_on(
    progress: &mut TaskProgress,
    task_type: TaskType,
    moved: &Advance,
) -> bool {
    if task_type == TaskType::Lesson {
        if let Some(point) = &moved.next_kp {
            progress.current_kp = Some(point.clone());
        }
        let closed = matches!(moved.status, STATUS_TASK_PASSED | STATUS_TASK_FAILED);
        progress.done = closed;
        return closed;
    }
    progress.answered = progress.answered.saturating_add(1);
    let closed = progress.total > 0 && progress.answered >= progress.total;
    progress.done = closed;
    closed
}

/// Supplemental practice preserves the original assessment count.
pub(super) fn practice_progress(
    progress: &mut TaskProgress,
    kind: TaskType,
    moved: &Advance,
    attempt: &Attempt,
    pending: bool,
) -> bool {
    if kind == TaskType::Lesson {
        let closed = task_moved_on(progress, kind, moved);
        progress.done = closed && !pending;
        return progress.done;
    }
    if !attempt.feedback_practice {
        task_moved_on(progress, kind, moved);
    }
    progress.done = !pending && progress.total > 0 && progress.answered >= progress.total;
    progress.done
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A progress row of `total` problems with `answered` of them answered.
    fn progress(total: i64, answered: i64) -> TaskProgress {
        TaskProgress {
            total,
            answered,
            ..TaskProgress::default()
        }
    }

    /// A non-lesson task closes by count parity: the answer that reaches the
    /// total closes it, and a task that names no total never closes.
    #[test]
    fn a_non_lesson_task_closes_by_count_parity() {
        let moved = Advance::carry_on();
        let mut open = progress(2, 0);
        assert!(!task_moved_on(&mut open, TaskType::Review, &moved));
        assert_eq!((open.answered, open.done), (1, false));
        assert!(task_moved_on(&mut open, TaskType::Review, &moved));
        assert_eq!((open.answered, open.done), (2, true));

        let mut unbounded = progress(0, 5);
        assert!(!task_moved_on(&mut unbounded, TaskType::Drill, &moved));
        assert_eq!((unbounded.answered, unbounded.done), (6, false));
    }

    /// A lesson closes on the core's status alone, and a `kp_advance` moves
    /// the progress row to the next knowledge point.
    #[test]
    fn a_lesson_closes_on_the_status_and_moves_its_knowledge_point() {
        let mut row = progress(0, 0);
        let advanced = Advance {
            status: STATUS_KP_ADVANCE,
            next_kp: Some("kp2".to_string()),
            ..Advance::carry_on()
        };
        assert!(!task_moved_on(&mut row, TaskType::Lesson, &advanced));
        assert_eq!(row.current_kp.as_deref(), Some("kp2"));
        assert_eq!((row.answered, row.done), (0, false));

        let closed = Advance {
            status: STATUS_TASK_FAILED,
            ..Advance::carry_on()
        };
        assert!(task_moved_on(&mut row, TaskType::Lesson, &closed));
        assert!(row.done);
    }

    /// A repeat failure with no key prerequisite queues nothing; every other
    /// close queues exactly one event of its kind.
    #[test]
    fn a_repeat_failure_with_no_target_queues_no_remediation() {
        let attempt: Attempt = serde_json::from_value(json!({
            "ts": "2026-01-01T00:00:10Z",
            "attempt_id": "t-1",
            "task_id": "t",
            "topic": "addition",
            "task_type": "lesson",
            "problem": {"text": "8 + 5.5", "expected": "13.5"},
            "given_answer": "14",
            "correct": false,
            "secs": 20,
            "work_quality": "nearly_passable",
        }))
        .unwrap();
        let now = Timestamp::from_micros(0);
        assert!(triggered(now, &attempt, REMEDIATION_REPEAT_FAIL, Vec::new()).is_empty());
        let plain = triggered(now, &attempt, REMEDIATION_LESSON_FAIL, Vec::new());
        assert_eq!(plain.len(), 1);
        assert_eq!(plain[0].kind, "lesson_fail");
        assert!(plain[0].targets.is_empty());
        let peeled = triggered(
            now,
            &attempt,
            REMEDIATION_REPEAT_FAIL,
            vec![Slug::new("subtraction").unwrap()],
        );
        assert_eq!(peeled.len(), 1);
        assert_eq!(peeled[0].kind, "repeat_fail");
        assert_eq!(peeled[0].targets.len(), 1);
        assert_eq!(
            Advance {
                remediation: peeled,
                ..Advance::carry_on()
            }
            .remediation_view(),
            vec![json!({"kind": "repeat_fail", "targets": ["subtraction"]})]
        );
    }
}
