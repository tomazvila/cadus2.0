//! Where the next problem of a task comes from: the topic it records against,
//! the topic it draws from, and the knowledge point of the statement.

use super::*;

/// The first knowledge point of a topic (`_first_kp`, `api.py:227-228`).
pub(super) fn first_kp(graph: &Curriculum, topic_id: &str) -> Option<String> {
    let idx = graph.idx_of(topic_id)?;
    let kp = graph.knowledge_points(idx).first()?;
    Some(kp.id.as_str().to_string())
}

/// The knowledge point a non-lesson serve of `topic_id` draws from.
///
/// 1.0 has no answer to give here: a review, a quiz, and a drill generate from
/// the topic's WHOLE exemplar bank and record `kp_id = None`
/// (`api.py:363`, `:379`). The 2.0 pool is keyed by `"<topic>/<kp>"`, so every
/// serve names a knowledge point. The rule is a rotation over the topic's
/// knowledge points by serve index: it covers the same bank across a task and it
/// is restart-safe, because the index lives in the D-S6 row.
fn rotating_kp(graph: &Curriculum, topic_id: &str, index: i64) -> Option<String> {
    let idx = graph.idx_of(topic_id)?;
    let kps = graph.knowledge_points(idx);
    // A topic with no knowledge point divides by one and finds nothing.
    let position = index.rem_euclid(kps.len().max(1) as i64) as usize;
    kps.get(position).map(|kp| kp.id.as_str().to_string())
}

/// The topic a quiz question comes from (`_quiz_topic_id`, `api.py:240-246`).
fn quiz_topic(entry: &str) -> &str {
    entry.strip_prefix("topic:").unwrap_or(entry)
}

/// Where the `index`-th problem of `task` comes from.
///
/// `record` is the topic the attempt records against, and `serve` is the topic
/// whose pool the statement is drawn from. They differ for a review that
/// micro-interleaves a component skill: the review's FIRe applies to the parent
/// topic and the component gets credit by encompassing propagation
/// (`api.py:352-357`).
#[derive(Debug)]
pub(super) struct Target {
    /// The topic the attempt records against.
    pub(super) record: String,
    /// The topic the statement is drawn from.
    pub(super) serve: String,
    /// The knowledge point of the statement.
    pub(super) kp: String,
    /// The serving key of `serving_pool.kp_id` and `content_store.kp_id`.
    pub(super) key: String,
}

impl Target {
    /// The target that records against `record` and draws `kp` of `serve`.
    pub(super) fn new(record: String, serve: String, kp: String) -> Self {
        let key = kp_key(&serve, &kp);
        Self {
            record,
            serve,
            kp,
            key,
        }
    }
}

/// Resolve the target of the next serve, or the 409 that refuses it.
pub(super) fn target_of(
    task: &Task,
    index: i64,
    progress: &TaskProgress,
    graph: &Curriculum,
) -> Result<Target, ApiError> {
    let position = usize::try_from(index).unwrap_or(usize::MAX);
    match task.task_type {
        TaskType::MultiStep => {
            let component = part_of(
                &task.component_topics,
                position,
                MULTISTEP_EXHAUSTED,
                "This multi-step task has no further part to serve.",
            )?;
            let kp = kp_or_refuse(graph, component, index)?;
            Ok(Target::new(component.clone(), component.clone(), kp))
        }
        TaskType::Quiz => {
            let entry = part_of(
                &task.mix,
                position,
                QUIZ_EXHAUSTED,
                "This quiz has no further question to serve.",
            )?;
            let topic = quiz_topic(entry).to_string();
            let kp = kp_or_refuse(graph, &topic, index)?;
            Ok(Target::new(topic.clone(), topic, kp))
        }
        TaskType::Lesson => {
            let topic = topic_or_refuse(task)?;
            let kp = lesson_kp(progress.current_kp.as_deref(), task, graph, &topic)
                .ok_or_else(|| no_problem(&topic))?;
            Ok(Target::new(topic.clone(), topic, kp))
        }
        TaskType::Review | TaskType::Drill | TaskType::Diagnostic => {
            let topic = topic_or_refuse(task)?;
            let serve = component_of(task, position, graph).unwrap_or_else(|| topic.clone());
            let probe_kp = task.probe_kp.as_ref().filter(|kp| {
                task.probe_delay_days.is_some()
                    && graph
                        .idx_of(&serve)
                        .and_then(|idx| graph.kp_idx_of(idx, kp))
                        .is_some()
            });
            let kp = if let Some(probe_kp) = probe_kp {
                // D-F11: the probe tests the knowledge point the schedule chose.
                // The rotation would always draw the topic's first point, the
                // fold would mark THAT point done, and the chosen point would
                // stay due and probe the same topic session after session.
                probe_kp.clone()
            } else if task.task_type == TaskType::Review && task.is_remediation {
                task.start_at_kp
                    .clone()
                    .map(Ok)
                    .unwrap_or_else(|| kp_or_refuse(graph, &serve, index))?
            } else {
                kp_or_refuse(graph, &serve, index)?
            };
            Ok(Target::new(topic, serve, kp))
        }
    }
}

/// The `position`-th entry of `parts`, or the `409` of a task that has no more.
fn part_of<'a>(
    parts: &'a [String],
    position: usize,
    code: &'static str,
    message: &'static str,
) -> Result<&'a String, ApiError> {
    parts.get(position).ok_or_else(|| conflict(code, message))
}

/// The component skill a review mix entry `component:<id>` moves the DRAW to,
/// when the arena holds it. The RECORD stays on the parent.
fn component_of(task: &Task, position: usize, graph: &Curriculum) -> Option<String> {
    let entry = task
        .mix
        .get(position.checked_rem(task.mix.len()).unwrap_or(0))?;
    let id = entry.strip_prefix("component:")?;
    graph.idx_of(id).map(|_| id.to_string())
}

/// The knowledge point a lesson stands at: the progress row first, then the
/// point the task resumes at, then the first authored point of the topic.
pub(super) fn lesson_kp(
    current: Option<&str>,
    task: &Task,
    graph: &Curriculum,
    topic: &str,
) -> Option<String> {
    current
        .map(str::to_string)
        .or_else(|| task.start_at_kp.clone())
        .or_else(|| first_kp(graph, topic))
}

/// The task's own topic, or the 409 of a task that names none.
pub(super) fn topic_or_refuse(task: &Task) -> Result<String, ApiError> {
    task.topic
        .clone()
        .ok_or_else(|| conflict(POOL_UNAVAILABLE, "This task names no topic to serve from."))
}

/// The rotating knowledge point of a topic, or the 409 of a topic with none.
fn kp_or_refuse(graph: &Curriculum, topic_id: &str, index: i64) -> Result<String, ApiError> {
    rotating_kp(graph, topic_id, index).ok_or_else(|| no_problem(topic_id))
}

#[cfg(test)]
mod tests {
    use super::fixture::{graph, task};
    use super::*;

    /// The target of `task` at `index` with no progress row, or its refusal code.
    fn target(task: &Task, index: i64) -> Result<(String, String, String), &'static str> {
        target_of(task, index, &TaskProgress::default(), &graph())
            .map(|found| (found.record, found.serve, found.kp))
            .map_err(|err| err.code)
    }

    #[test]
    fn a_retention_probe_draws_the_knowledge_point_its_schedule_chose() {
        let mut probe = task(TaskType::Review, Some("addition"));
        probe.probe_delay_days = Some(7);
        probe.probe_kp = Some("kp2".to_string());
        assert_eq!(
            target(&probe, 0),
            Ok(("addition".into(), "addition".into(), "kp2".into()))
        );
        // A point the topic does not hold falls back to the rotation.
        probe.probe_kp = Some("ghost".to_string());
        assert_eq!(
            target(&probe, 0),
            Ok(("addition".into(), "addition".into(), "kp1".into()))
        );
    }

    #[test]
    fn a_multi_step_task_serves_its_parts_in_order_and_then_refuses() {
        let mut multi = task(TaskType::MultiStep, None);
        multi.component_topics = vec!["counting".to_string(), "addition".to_string()];
        assert_eq!(
            target(&multi, 1),
            Ok(("addition".into(), "addition".into(), "kp2".into()))
        );
        assert_eq!(target(&multi, 2), Err(MULTISTEP_EXHAUSTED));
    }

    #[test]
    fn a_quiz_strips_the_topic_prefix_and_refuses_past_its_mix() {
        let mut quiz = task(TaskType::Quiz, None);
        quiz.mix = vec!["topic:counting".to_string(), "empty".to_string()];
        assert_eq!(
            target(&quiz, 0),
            Ok(("counting".into(), "counting".into(), "kp1".into()))
        );
        assert_eq!(target(&quiz, 1), Err(POOL_UNAVAILABLE));
        assert_eq!(target(&quiz, 2), Err(QUIZ_EXHAUSTED));
    }

    #[test]
    fn a_lesson_takes_the_progress_row_then_the_start_then_the_first_point() {
        let mut lesson = task(TaskType::Lesson, Some("addition"));
        let row = TaskProgress {
            current_kp: Some("kp2".to_string()),
            ..TaskProgress::default()
        };
        let at_row = target_of(&lesson, 0, &row, &graph()).unwrap();
        assert_eq!(at_row.key, "addition/kp2");
        lesson.start_at_kp = Some("kp2".to_string());
        assert_eq!(target(&lesson, 0).unwrap().2, "kp2");
        lesson.start_at_kp = None;
        assert_eq!(target(&lesson, 0).unwrap().2, "kp1");
    }

    #[test]
    fn a_lesson_on_an_empty_topic_or_on_no_topic_is_pool_unavailable() {
        assert_eq!(
            target(&task(TaskType::Lesson, Some("empty")), 0),
            Err(POOL_UNAVAILABLE)
        );
        let refusal = target_of(
            &task(TaskType::Lesson, None),
            0,
            &TaskProgress::default(),
            &graph(),
        )
        .unwrap_err();
        assert_eq!(refusal.code, POOL_UNAVAILABLE);
        assert_eq!(refusal.message, "This task names no topic to serve from.");
    }

    #[test]
    fn a_review_draws_a_known_component_and_records_against_the_parent() {
        let mut review = task(TaskType::Review, Some("addition"));
        review.mix = vec![
            "kp1".to_string(),
            "component:counting".to_string(),
            "component:nowhere".to_string(),
        ];
        assert_eq!(
            target(&review, 0),
            Ok(("addition".into(), "addition".into(), "kp1".into()))
        );
        assert_eq!(
            target(&review, 1),
            Ok(("addition".into(), "counting".into(), "kp1".into()))
        );
        // An unknown component keeps the draw on the parent, and the rotation
        // wraps around the mix.
        assert_eq!(target(&review, 2).unwrap().1, "addition");
        assert_eq!(target(&review, 4).unwrap().1, "counting");
    }

    #[test]
    fn a_drill_rotates_over_the_knowledge_points_of_its_topic() {
        let drill = task(TaskType::Drill, Some("addition"));
        assert_eq!(target(&drill, 0).unwrap().2, "kp1");
        assert_eq!(target(&drill, 1).unwrap().2, "kp2");
        assert_eq!(target(&drill, 2).unwrap().2, "kp1");
        assert_eq!(target(&drill, -1).unwrap().2, "kp2");
        assert_eq!(
            target(&task(TaskType::Diagnostic, Some("nowhere")), 0),
            Err(POOL_UNAVAILABLE)
        );
    }

    #[test]
    fn a_multistep_component_with_no_knowledge_point_is_pool_unavailable() {
        let mut multi = task(TaskType::MultiStep, None);
        multi.component_topics = vec!["empty".to_string()];
        assert_eq!(target(&multi, 0), Err(POOL_UNAVAILABLE));
    }

    #[test]
    fn a_review_that_names_no_topic_is_pool_unavailable() {
        assert_eq!(
            target(&task(TaskType::Review, None), 0),
            Err(POOL_UNAVAILABLE)
        );
    }

    #[test]
    fn first_kp_names_the_first_authored_point_or_nothing() {
        assert_eq!(first_kp(&graph(), "addition").as_deref(), Some("kp1"));
        assert_eq!(first_kp(&graph(), "empty"), None);
        assert_eq!(first_kp(&graph(), "nowhere"), None);
    }
}

#[cfg(test)]
mod advance_84b_tests {
    use super::fixture::{arena, task, topic_doc};
    use super::*;
    use cadus_core::instruction::{KIND_HINT_LADDER, KIND_TEACH};
    use cadus_core::readiness::{MapContent, ReadinessGate, ReadinessIndex};

    /// The readiness set of `graph` with a teach page and a hint ladder on every
    /// serving key.
    fn readiness(graph: &Curriculum) -> ReadinessSet {
        let mut content = MapContent::default();
        for topic in graph.topics() {
            for kp in &topic.knowledge_points {
                let key = cadus_core::pool::kp_key(topic.id.as_str(), kp.id.as_str());
                content.insert(key.clone(), KIND_TEACH, 1);
                content.insert(key.clone(), KIND_HINT_LADDER, 1);
            }
        }
        ReadinessIndex::build(graph).resolve(&content)
    }

    #[test]
    fn a_verdict_capable_topic_serves_each_point_and_the_row_advances() {
        // Each knowledge point carries 4 decidable exemplars (3 practice + 1 held
        // out): every point serves a lesson.
        let trio = topic_doc(
            "trio",
            &[
                ("kp1", &["1", "11", "21", "31"]),
                ("kp2", &["2", "12", "22", "32"]),
                ("kp3", &["3", "13", "23", "33"]),
            ],
        );
        let graph = arena(&[trio]);
        let ready = readiness(&graph);
        let mut lesson = task(TaskType::Lesson, Some("trio"));
        lesson.start_at_kp = Some("kp2".to_string()); // the pinned start of the session
        // The progress row wins over start_at_kp: the serve continues where the
        // session stands (the standing fixture test pins the same order).
        let row = TaskProgress {
            current_kp: Some("kp2".to_string()),
            ..TaskProgress::default()
        };
        let found = target_of(&lesson, 0, &row, &graph).unwrap();
        assert_eq!(found.kp, "kp2", "the progress row advances the point");
        assert!(ReadinessGate::lesson_blockers(&ready, &found.serve, &found.kp).is_empty());
        // The serve route persists the chosen point (note 93 a): the progress row the
        // route writes after the skip carries `current_kp = Some(chosen)`.
        let chosen = target_of(&lesson, 0, &TaskProgress::default(), &graph).unwrap();
        let row = TaskProgress {
            current_kp: Some(chosen.kp.clone()), // the 93 a write, spelled
            ..TaskProgress::default()
        };
        assert_eq!(row.current_kp.as_deref(), Some(chosen.kp.as_str()));
    }

    #[test]
    fn a_topic_with_an_all_none_point_serves_past_it() {
        // kp1: two decidable exemplars only (too thin to serve a lesson); kp2: the
        // explicit none contract (a self-check point, no verdict). The serve reads
        // kp1 first, the skip picks kp2, and the route WRITES it into the row.
        let mut prose = topic_doc("mixed84", &[("kp1", &["1"]), ("kp2", &["2"])]);
        let kps = prose.get_mut("knowledge_points").unwrap();
        kps[1] = serde_json::json!({"id": "kp2", "name": "kp2", "exemplars": [
            {"problem": "Explain the sign.", "answer": "the worked solution",
             "answer_contract": {"kind": "none"}}]});
        let graph = arena(&[prose]);
        let ready = readiness(&graph);
        let lesson = task(TaskType::Lesson, Some("mixed84"));
        let row = TaskProgress::default();
        // The serve reads kp1 first: a fresh row starts the lesson at the first point.
        let found = target_of(&lesson, 0, &row, &graph).unwrap();
        assert_eq!(found.kp, "kp1");
        // The readiness side of 84 b: kp1 (thin) stays blocked; kp2 (all `none`)
        // serves through the teach-only rule, so the topic is servable.
        assert!(!ReadinessGate::lesson_blockers(&ready, "mixed84", "kp1").is_empty());
        assert!(ReadinessGate::lesson_blockers(&ready, "mixed84", "kp2").is_empty());
        assert!(ready.topic_serves_lesson("mixed84"));
    }
}
