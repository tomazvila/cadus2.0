//! Re-serving an open plan minus the tasks that are no longer valid
//! (`selector.py:1527-1736`).

use std::collections::{BTreeMap, BTreeSet};

use crate::event::TaskType;
use crate::fire::{ReviewState, review_state};
use crate::learner::TopicState;
use crate::xp::{is_inferred, is_known};
use crate::{config::Config, curriculum::Curriculum};

use super::context::SessionContext;
use super::frontier::Frontier;
use super::gap_fill::is_course_complete;
use super::interleave::SlotKind;
use super::multistep::remediation_tasks;
use super::quiz::quiz_is_due;
use super::review::in_retry_delay;
use super::task::{SessionPlan, Task, schedule_drills};
use super::topic_set::TopicSet;

/// The inputs [`task_still_valid`] tests a queued task against.
#[derive(Debug, Clone, Copy)]
pub struct ValidityContext<'a> {
    /// The topics the remediation queue still names.
    pub pending_targets: &'a BTreeSet<String>,
    /// The frontier of the course scope.
    pub frontier_topics: &'a TopicSet,
    /// Whether a quiz is due.
    pub quiz_due: bool,
    /// The topics a drill may serve.
    pub drill_eligible: &'a BTreeSet<String>,
    /// The topics classified at the raised test-prep due threshold.
    pub test_prep_topics: &'a BTreeSet<String>,
    /// The task ids already closed this session.
    pub closed_task_ids: &'a BTreeSet<String>,
}

/// Whether a queued task should still be served (`_task_still_valid`, `selector.py:1527-1605`).
///
/// A task drops when the reason it was queued no longer holds. A nearly-due
/// review stays valid across the whole `{due, nearly_due}` band, because memory
/// only decays: a strict `due` test would drop it the moment it crossed the due
/// line, and would silently take the reviews it knocks out with it.
#[must_use]
pub fn task_still_valid(
    task: &Task,
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    ctx: &ValidityContext<'_>,
) -> bool {
    let validity = Validity {
        states,
        graph,
        cfg,
        t_us,
        ctx,
    };
    match task.task_type {
        TaskType::Quiz => ctx.quiz_due,
        TaskType::MultiStep => validity.multistep_open(task),
        _ => task
            .topic
            .as_deref()
            .is_some_and(|topic_id| validity.single_topic_valid(task, topic_id)),
    }
}

/// The inputs of one validity test, bundled for the per-kind helpers.
struct Validity<'a> {
    states: &'a BTreeMap<String, TopicState>,
    graph: &'a Curriculum,
    cfg: &'a Config,
    t_us: i64,
    ctx: &'a ValidityContext<'a>,
}

impl Validity<'_> {
    /// The review band of `tid` at the composition instant.
    fn band(&self, tid: &str, state: &TopicState) -> ReviewState {
        review_state(
            state,
            self.t_us,
            self.cfg,
            self.ctx.test_prep_topics.contains(tid),
        )
    }

    /// Whether an open multi-step task still has a due component.
    fn multistep_open(&self, task: &Task) -> bool {
        if self.ctx.closed_task_ids.contains(&task.task_id) {
            return false;
        }
        let default = TopicState::default();
        task.component_topics.iter().any(|component| {
            let state = self.states.get(component).unwrap_or(&default);
            self.band(component, state) == ReviewState::Due
        })
    }

    /// Whether a single-topic task still has its reason to be served.
    fn single_topic_valid(&self, task: &Task, topic_id: &str) -> bool {
        if task.is_remediation {
            return self.ctx.pending_targets.contains(topic_id);
        }
        match task.task_type {
            // A confirmation stands while the topic is still inferred (D-F6).
            // The review band never opens for a placed topic, so the band test
            // would drop the item on the first re-serve.
            TaskType::Review if task.confirm => self.states.get(topic_id).is_some_and(is_inferred),
            TaskType::Review => self.review_valid(task, topic_id),
            TaskType::Lesson => self.lesson_valid(topic_id),
            TaskType::Drill => self.ctx.drill_eligible.contains(topic_id),
            _ => true,
        }
    }

    /// Whether a review is still in its band: due, or the whole band for a
    /// review served before its due date.
    fn review_valid(&self, task: &Task, topic_id: &str) -> bool {
        let Some(state) = self.states.get(topic_id) else {
            return false;
        };
        let current = self.band(topic_id, state);
        if task.nearly_due {
            matches!(current, ReviewState::Due | ReviewState::NearlyDue)
        } else {
            current == ReviewState::Due
        }
    }

    /// Whether a lesson is still on the frontier and open to a retry.
    fn lesson_valid(&self, topic_id: &str) -> bool {
        let default = TopicState::default();
        let state = self.states.get(topic_id).unwrap_or(&default);
        self.ctx.frontier_topics.contains_id(self.graph, topic_id)
            && !is_known(state)
            && !in_retry_delay(state, self.cfg, self.t_us)
    }
}

/// Re-serve an open plan minus the tasks that are no longer valid
/// (`_reserve_open_plan`, `selector.py:1630-1736`).
///
/// The surviving tasks keep their order and their ids, so the queue is stable.
/// Remediation queued after the plan was composed is prepended, because
/// PEDAGOGY 5 serves remediation first.
#[must_use]
pub fn reserve_open_plan(
    open_plan: &SessionPlan,
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    ctx: &SessionContext<'_>,
) -> SessionPlan {
    let front = Frontier::new(states, graph, cfg, t_us, ctx.course_id, None);
    let course_complete = is_course_complete(states, graph, cfg, ctx.course_id, Some(&front.known));

    let pending_targets: BTreeSet<String> = ctx
        .pending_remediation
        .iter()
        .flat_map(|item| item.targets.iter())
        .map(|target| target.as_str().to_owned())
        .collect();
    let quiz_due = quiz_is_due(
        ctx.quiz_state,
        states,
        graph,
        cfg,
        t_us,
        ctx.active_study_days,
    );
    let drill_eligible: BTreeSet<String> = schedule_drills(states, graph, t_us, ctx.last_drill_at)
        .into_iter()
        .collect();
    let validity = ValidityContext {
        pending_targets: &pending_targets,
        frontier_topics: &front.topics,
        quiz_due,
        drill_eligible: &drill_eligible,
        test_prep_topics: ctx.test_prep_topics,
        closed_task_ids: ctx.closed_task_ids,
    };

    let kept: Vec<Task> = open_plan
        .tasks
        .iter()
        .filter(|task| task_still_valid(task, states, graph, cfg, t_us, &validity))
        .cloned()
        .collect();
    let fresh = remediation_tasks(ctx.pending_remediation, states, graph, cfg);
    let mut kept = prepend_remediation(kept, fresh, &open_plan.session);

    if let Some(limit) = ctx.n {
        kept.truncate(limit);
    }

    let seq: Vec<(SlotKind, String)> = kept
        .iter()
        .filter_map(|task| match (task.task_type, task.topic.as_ref()) {
            (TaskType::Lesson, Some(topic)) => Some((SlotKind::Lesson, topic.clone())),
            (TaskType::Review, Some(topic)) => Some((SlotKind::Review, topic.clone())),
            _ => None,
        })
        .collect();

    front.plan(
        &open_plan.session,
        kept,
        quiz_due,
        course_complete,
        &seq,
        cfg,
    )
}

/// Prepend the remediation queued after the plan was composed, so it is not
/// hidden until the next session. Existing tasks keep their ids and order, and
/// a remediation task supersedes the same topic's plain review or lesson.
fn prepend_remediation(kept: Vec<Task>, fresh: Vec<Task>, session: &str) -> Vec<Task> {
    let mut covered: BTreeSet<String> = kept.iter().filter_map(|task| task.topic.clone()).collect();
    let mut out: Vec<Task> = Vec::with_capacity(kept.len() + fresh.len());
    for mut task in fresh {
        // A remediation task always names its topic.
        let topic = task.topic.clone().unwrap_or_default();
        if covered.contains(&topic) {
            continue;
        }
        task.task_id = format!("{session}-rem-{topic}");
        covered.insert(topic);
        out.push(task);
    }
    out.extend(kept);

    let rem_topics: BTreeSet<String> = out
        .iter()
        .filter(|task| task.is_remediation)
        .filter_map(|task| task.topic.clone())
        .collect();
    out.retain(|task| {
        task.is_remediation
            || !task
                .topic
                .as_ref()
                .is_some_and(|id| rem_topics.contains(id))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fire::testing::{T_US, graph, learned, topic};
    use crate::learner::PendingRemediation;
    use crate::selector::remediation_for_quiz_miss;

    /// A review task on `topic`, remediation or not.
    fn review(topic: &str, remediation: bool) -> Task {
        Task {
            task_id: format!("s-review-{topic}"),
            task_type: TaskType::Review,
            topic: Some(topic.to_owned()),
            is_remediation: remediation,
            ..Task::default()
        }
    }

    #[test]
    fn the_reserve_keeps_the_valid_tasks_and_prepends_fresh_remediation() {
        let cfg = Config::default();
        let tree = graph(["a", "b", "c", "d"].map(|id| topic(id, &[])).to_vec());
        let states: BTreeMap<String, TopicState> = [
            ("a".to_owned(), learned(0.5)),
            ("b".to_owned(), learned(0.9)),
        ]
        .into_iter()
        .collect();
        let quiz = Task {
            task_type: TaskType::Quiz,
            ..Task::default()
        };
        let multistep = Task {
            task_id: "s-multi-step".to_owned(),
            task_type: TaskType::MultiStep,
            component_topics: vec!["a".to_owned()],
            ..Task::default()
        };
        let lesson = Task {
            task_type: TaskType::Lesson,
            topic: Some("c".to_owned()),
            ..Task::default()
        };
        let drill = Task {
            task_type: TaskType::Drill,
            topic: Some("a".to_owned()),
            ..Task::default()
        };
        let diagnostic = Task {
            task_type: TaskType::Diagnostic,
            topic: Some("b".to_owned()),
            ..Task::default()
        };
        let topicless = Task {
            task_type: TaskType::Lesson,
            ..Task::default()
        };
        let nearly = |topic: &str| Task {
            task_id: format!("s-nearly-{topic}"),
            nearly_due: true,
            ..review(topic, false)
        };
        let open = SessionPlan {
            session: "s".to_owned(),
            tasks: vec![
                review("a", true),
                review("a", false),
                review("b", false),
                review("ghost", false),
                nearly("a"),
                nearly("b"),
                quiz,
                multistep,
                lesson,
                drill,
                diagnostic,
                topicless,
            ],
            ..SessionPlan::default()
        };
        let pending: Vec<PendingRemediation> = ["a", "c", "d"]
            .map(|id| remediation_for_quiz_miss(id).expect("a slug"))
            .to_vec();
        let ctx = SessionContext::default().with_pending_remediation(&pending);
        let plan = reserve_open_plan(&open, &states, &tree, &cfg, T_US, &ctx);
        let ids: Vec<&str> = plan
            .tasks
            .iter()
            .map(|task| task.task_id.as_str())
            .collect();
        // `c` is covered by the kept lesson, `d` is fresh, and the plain review
        // of `a` yields to its remediation.
        assert_eq!(ids, ["s-rem-d", "s-review-a", "s-multi-step", "", ""]);
        assert!(plan.tasks[0].is_remediation && plan.tasks[1].is_remediation);
        assert!(!plan.quiz_due);

        let capped = SessionContext::default()
            .with_pending_remediation(&pending)
            .with_limit(Some(2));
        let short = reserve_open_plan(&open, &states, &tree, &cfg, T_US, &capped);
        assert_eq!(short.tasks.len(), 2);

        let closed: BTreeSet<String> = ["s-multi-step".to_owned()].into();
        let done = SessionContext::default().with_multistep(0, &closed);
        let later = reserve_open_plan(&open, &states, &tree, &cfg, T_US, &done);
        assert!(
            later
                .tasks
                .iter()
                .all(|task| task.task_type != TaskType::MultiStep)
        );
    }
}
