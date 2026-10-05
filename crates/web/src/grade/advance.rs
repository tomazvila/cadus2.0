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
    /// The proof-gated knowledge point whose decided items just passed: the
    /// lesson serves its written proof next (D-PR1).
    pub(super) proof_due: Option<String>,
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
            proof_due: None,
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
/// An UNGRADED attempt is never evidence (D-F2): it stays out of the
/// knowledge-point sequence, because the checker gave no verdict to count. The
/// one exception is the teach-only knowledge point of note 84 b, which has no
/// decidable item at all: there the unassisted self-check answer completes the
/// point (see [`self_check_completion`]), and at the last point the lesson
/// closes with its standing XP. Otherwise the learner takes the next problem.
pub(super) fn advance(
    graph: &Curriculum,
    cfg: &Config,
    now: Timestamp,
    attempt: &Attempt,
    prior: &[EventRow],
    history: &SessionView,
    readiness: Option<&ReadinessSet>,
) -> Advance {
    if !decided_lesson_attempt(attempt) {
        return self_check_completion(graph, cfg, now, attempt, prior, readiness);
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

/// The note-84 (b) completion arm: the teach-only knowledge point.
///
/// An ALL-`none` knowledge point serves self-check rows whose every attempt
/// grades UNGRADED, so the pass rule never sees evidence for it (the staging
/// walk of 2026-09-24: topic `limits-graphical-numerical`, kp3 — 3,720
/// ungraded attempts, zero `lesson_result`). When the learner answered the
/// self-check row of the CURRENT point unassisted and the readiness set shows
/// every point at or before it either passed or teach-only, the point
/// completes with this attempt: a later point advances, the last one closes
/// the lesson. Never runs for a decided attempt.
fn self_check_completion(
    graph: &Curriculum,
    cfg: &Config,
    now: Timestamp,
    attempt: &Attempt,
    prior: &[EventRow],
    readiness: Option<&ReadinessSet>,
) -> Advance {
    let Some(readiness) = readiness else {
        return Advance::carry_on();
    };
    if attempt.task_type != TaskType::Lesson || attempt.assisted || attempt.feedback_practice {
        return Advance::carry_on();
    }
    let Some(idx) = graph.idx_of(attempt.topic.as_str()) else {
        return Advance::carry_on();
    };
    let points = graph.knowledge_points(idx);
    let Some(slug) = attempt.kp.as_ref() else {
        return Advance::carry_on();
    };
    let Some(at) = points
        .iter()
        .position(|point| point.id.as_str() == slug.as_str())
    else {
        return Advance::carry_on();
    };
    // The teach-only check: the CURRENT point serves no verdict item at all,
    // and the points before it already passed (their evidence is in the log).
    let teach_only = readiness
        .topic(attempt.topic.as_str())
        .iter()
        .filter(|row| row.kp_key.ends_with(slug.as_str()))
        .all(|row| row.decidable_exemplars == 0);
    if !teach_only {
        return Advance::carry_on();
    }
    if at + 1 < points.len() {
        return Advance {
            status: STATUS_KP_ADVANCE,
            next_kp: points.get(at + 1).map(|point| point.id.as_str().to_owned()),
            ..Advance::carry_on()
        };
    }
    lesson_passed(cfg, now, attempt, prior, points.len())
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

/// What a passing lesson close reads about the answer that closed it.
pub(super) struct CloseOf<'a> {
    /// The session the close credits.
    pub(super) session: Option<String>,
    /// The topic of the lesson.
    pub(super) topic: Slug,
    /// The lesson task.
    pub(super) task_id: &'a str,
    /// The tier the XP is priced at.
    pub(super) quality: WorkQuality,
    /// Whether the closing answer was reference-assisted.
    pub(super) assisted: bool,
}

/// The passing lesson close and its XP (`advance_task`, the pass arm).
fn lesson_passed(
    cfg: &Config,
    now: Timestamp,
    attempt: &Attempt,
    prior: &[EventRow],
    kp_count: usize,
) -> Advance {
    let close = CloseOf {
        session: attempt.session.clone(),
        topic: attempt.topic.clone(),
        task_id: &attempt.task_id,
        quality: attempt.work_quality,
        assisted: attempt.assisted,
    };
    lesson_passed_with(cfg, now, &close, prior, kp_count)
}

/// The passing lesson close of [`CloseOf`].
fn lesson_passed_with(
    cfg: &Config,
    now: Timestamp,
    close: &CloseOf<'_>,
    prior: &[EventRow],
    kp_count: usize,
) -> Advance {
    let quality = close.quality;
    let count = i64::try_from(kp_count).unwrap_or(i64::MAX);
    let xp = task_xp(TaskType::Lesson, quality, cfg, count, 0, false);
    // The passing lesson is reference-assisted when ANY of its attempts was.
    let assisted = close.assisted
        || prior.iter().any(|row| {
            matches!(&row.event, Event::Attempt(body)
                if body.task_id == close.task_id && body.assisted)
        });
    Advance {
        status: STATUS_TASK_PASSED,
        result: Some(Event::LessonResult(LessonResult {
            ts: now,
            session: close.session.clone(),
            v: SchemaVersion::current(),
            topic: close.topic.clone(),
            passed: true,
            failed_at_kp: None,
            xp,
            quality_tier: quality,
            assisted,
        })),
        remediation: Vec::new(),
        xp: Some(round2(xp)),
        next_kp: None,
        proof_due: None,
    }
}

/// The tier of a knowledge point closed by a passed proof: the tier the
/// background grader prices a pass at.
pub(super) const PROOF_PASS_TIER: WorkQuality = WorkQuality::NearlyPerfect;

/// The tier of a knowledge point closed by a proof that passed after one or
/// more revisions: below a first-try pass (D-PR1).
pub(super) const PROOF_REVISED_TIER: WorkQuality = WorkQuality::Passable;

/// The tier of a knowledge point closed by the unaided rewrite after the
/// revision cap: below a pass, so the close earns less XP (D-PR1).
pub(super) const PROOF_ASSISTED_TIER: WorkQuality = WorkQuality::Passable;

/// Close the proof-gated knowledge point `kp` of `close.topic` (D-PR1): a
/// later point advances the lesson to it, the last point passes the lesson
/// with its XP.
pub(super) fn proof_close(
    graph: &Curriculum,
    cfg: &Config,
    now: Timestamp,
    close: &CloseOf<'_>,
    kp: &str,
    prior: &[EventRow],
) -> Advance {
    let points = graph
        .idx_of(close.topic.as_str())
        .map(|idx| graph.knowledge_points(idx))
        .unwrap_or_default();
    let at = points.iter().position(|point| point.id.as_str() == kp);
    if let Some(at) = at.filter(|at| at + 1 < points.len()) {
        return Advance {
            status: STATUS_KP_ADVANCE,
            next_kp: points.get(at + 1).map(|point| point.id.as_str().to_owned()),
            ..Advance::carry_on()
        };
    }
    lesson_passed_with(cfg, now, close, prior, points.len())
}

/// Hold a lesson that just passed the decided items of a proof-gated point
/// until its written proof closes (D-PR1).
///
/// `moved` is the advance of a DECIDED lesson answer. When it passes `kp`
/// (a `kp_advance` or the lesson pass) and `kp` authors a written proof, the
/// lesson carries on and serves the proof instead; the close waits for the
/// proof's chain.
pub(super) fn hold_for_proof(graph: &Curriculum, attempt: &Attempt, moved: Advance) -> Advance {
    let passed_kp = matches!(moved.status, STATUS_KP_ADVANCE | STATUS_TASK_PASSED)
        && moved
            .result
            .as_ref()
            .is_none_or(|event| matches!(event, Event::LessonResult(result) if result.passed));
    if attempt.task_type != TaskType::Lesson || !passed_kp {
        return moved;
    }
    let Some(kp) = attempt.kp.as_ref().map(Slug::as_str) else {
        return moved;
    };
    if crate::proof_grading::lesson::proof_exemplar(graph, attempt.topic.as_str(), kp).is_none() {
        return moved;
    }
    Advance {
        proof_due: Some(kp.to_owned()),
        ..Advance::carry_on()
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
        proof_due: None,
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

    /// The topic-23 stall (probe 2, 2026-09-24): a `characteristic-polynomial`
    /// kp1 lesson attempt that the deterministic checker graded correct, with a
    /// graded same-task kp1 attempt before it, must pass the `2consec|3of4` rule
    /// and advance the lesson to kp2. The staging probe recorded 4,000 correct
    /// kp1 answers over 50 task restarts and never closed one lesson.
    #[test]
    fn the_topic23_lesson_passes_on_two_correct_kp1_answers() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
        let (graph, _findings) =
            cadus_core::curriculum::load_curriculum(&root).expect("the tree loads");
        let cfg = Config::default();
        let raw = include_str!("../../tests/fixtures/tp23-attempt.json");
        let attempt: Attempt = serde_json::from_str(raw).expect("the fixture decodes");
        let mut prior_body = attempt.clone();
        prior_body.attempt_id = "s_2026-09-24i-lesson-characteristic-polynomial-0".to_owned();
        prior_body.ts = attempt.ts;
        let prior = vec![EventRow {
            seq: 1,
            event: Event::Attempt(prior_body),
        }];
        let moved = advance(
            &graph,
            &cfg,
            attempt.ts,
            &attempt,
            &prior,
            &SessionView::default(),
            Some(&ReadinessSet::default()),
        );
        assert_eq!(
            moved.status, STATUS_KP_ADVANCE,
            "two correct kp1 answers pass 2consec"
        );
        assert_eq!(moved.next_kp.as_deref(), Some("kp2"));
    }

    /// The topic-23/kp3 self-check stall (staging, 2026-09-24): a lesson whose
    /// LAST knowledge point is all-`kind: none` serves self-check rows whose
    /// every attempt grades UNGRADED; the pass rule never sees evidence. The
    /// note-84 (b) completion arm must close the lesson on the ungraded
    /// self-check answer of the last point.
    #[test]
    fn the_teach_only_last_point_completes_on_its_self_check_answer() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
        let (graph, _findings) =
            cadus_core::curriculum::load_curriculum(&root).expect("the tree loads");
        let cfg = Config::default();
        // The shape of the staging learner: an ungraded self-check attempt on
        // the last knowledge point, unassisted, no feedback practice.
        let raw = include_str!("../../tests/fixtures/tp23-attempt.json");
        let mut attempt: Attempt = serde_json::from_str(raw).expect("the fixture decodes");
        attempt.topic = cadus_core::event::Slug::new("limits-graphical-numerical").expect("a slug");
        attempt.kp = Some(cadus_core::event::Slug::new("kp3").expect("a slug"));
        attempt.outcome = cadus_core::event::AttemptOutcome::Ungraded {
            reason: "the item has no deterministic answer contract".to_owned(),
        };
        attempt.correct = false;
        // The readiness of the real tree: kp3 is the all-`none` point.
        let index = cadus_core::readiness::ReadinessIndex::build(&graph);
        let ready = index.resolve(&cadus_core::readiness::EmptyContent);
        let moved = advance(
            &graph,
            &cfg,
            attempt.ts,
            &attempt,
            &[],
            &SessionView::default(),
            Some(&ready),
        );
        assert_eq!(
            moved.status, STATUS_TASK_PASSED,
            "the teach-only last point completes the lesson on its self-check answer"
        );
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
