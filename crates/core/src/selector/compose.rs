//! `compose_session`: the ordered session plan of PEDAGOGY 5
//! (`selector.py:1235-1481`).

use std::collections::{BTreeMap, BTreeSet};

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::event::TaskType;
use crate::fire::has_review_history;
use crate::learner::TopicState;

use super::compress::compress_with;
use super::context::SessionContext;
use super::frontier::Frontier;
use super::gap_fill::is_course_complete;
use super::interleave::{SlotKind, arrange_lessons, assign_ids, interleave};
use super::multistep::{multistep_components, multistep_is_due, multistep_task, remediation_tasks};
use super::quiz::{QuizSampler, quiz_composer, quiz_is_due};
use super::reserve::reserve_open_plan;
use super::review::{due_reviews, nearly_due, order_lessons_with};
use super::task::{
    SessionPlan, Task, drill_task, knockout_count, lesson_task, quiz_task, review_task,
    schedule_drills,
};
use super::topic_set::ReachCache;
use super::{MULTISTEP_ENABLED, MULTISTEP_MIN_COMPONENTS};

/// The review side of one composition: the due list, the compression, and the
/// interleaving inputs.
struct ReviewPlan {
    /// The due reviews, sorted.
    due: Vec<String>,
    /// Per covering task, the sorted due topics it knocks out.
    knockouts: BTreeMap<String, Vec<String>>,
    /// The reviews to serve, in priority order.
    review_topics: Vec<String>,
    /// The lessons to serve, in serve order.
    lessons: Vec<String>,
    /// The reviews served before their due date.
    nearly_served: BTreeSet<String>,
}

impl ReviewPlan {
    /// The reviews and lessons of a composition, before the remediation and
    /// multi-step dedup.
    fn new(
        states: &BTreeMap<String, TopicState>,
        graph: &Curriculum,
        cfg: &Config,
        t_us: i64,
        ctx: &SessionContext<'_>,
        front: &Frontier,
        cache: &mut ReachCache<'_>,
    ) -> Self {
        let due = due_reviews(states, graph, cfg, t_us, ctx.test_prep_topics);
        let available_set: BTreeSet<String> = front.available.iter().cloned().collect();
        let comp = compress_with(&due, states, graph, cfg, t_us, Some(&available_set), cache);
        let surviving_set: BTreeSet<String> = comp.surviving.iter().cloned().collect();
        let mut review_topics = comp.surviving.clone();
        review_topics.sort_by(|left, right| {
            knockout_count(&comp.knockouts, right)
                .cmp(&knockout_count(&comp.knockouts, left))
                .then_with(|| left.cmp(right))
        });
        let nearly = nearly_due(states, graph, cfg, t_us);

        let nearly_served: BTreeSet<String>;
        let mut lessons: Vec<String> = Vec::new();
        if front.blocked_until.is_some() {
            // The frontier is blocked: serve no lesson, bring the nearly-due forward.
            nearly_served = nearly
                .iter()
                .filter(|tid| !surviving_set.contains(*tid))
                .cloned()
                .collect();
            review_topics.extend(
                nearly
                    .iter()
                    .filter(|tid| nearly_served.contains(*tid))
                    .cloned(),
            );
        } else {
            let nearly_set: BTreeSet<String> = nearly.iter().cloned().collect();
            // `comp.knockouts` is a sorted map, so its keys already come back sorted.
            let nearly_knockers: Vec<String> = comp
                .knockouts
                .keys()
                .filter(|key| nearly_set.contains(*key) && !surviving_set.contains(*key))
                .cloned()
                .collect();
            nearly_served = nearly_knockers.iter().cloned().collect();
            review_topics.extend(nearly_knockers);
            let review_targets: BTreeSet<String> =
                due.iter().chain(nearly.iter()).cloned().collect();
            let ranked = order_lessons_with(
                &front.available,
                graph,
                &review_targets,
                &front.course_topics,
                cache,
            );
            lessons = arrange_lessons(&ranked, graph);
        }
        Self {
            due,
            knockouts: comp.knockouts,
            review_topics,
            lessons,
            nearly_served,
        }
    }
}

/// The multi-step task of a composition, or `None` when none is owed. The
/// components leave `review_topics`, because the task absorbs their reviews.
fn multistep_plan(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    ctx: &SessionContext<'_>,
    due: &[String],
    review_topics: &mut Vec<String>,
) -> Option<Task> {
    let multistep_id = format!("{}-{}", ctx.session_id, TaskType::MultiStep.as_str());
    if ctx.closed_task_ids.contains(&multistep_id) {
        return None;
    }
    let open_components = ctx.open_multistep_components.unwrap_or(&[]);
    if !open_components.is_empty() {
        // The session already served an open multi-step task. Re-serve it with
        // the EXACT components the `task_served` event recorded, so the close
        // credits the same topics.
        review_topics.retain(|tid| !open_components.contains(tid));
        return Some(multistep_task(open_components, states, graph));
    }
    let n_reviewable = i64::try_from(
        states
            .iter()
            .filter(|(id, state)| graph.idx_of(id).is_some() && has_review_history(state))
            .count(),
    )
    .unwrap_or(i64::MAX);
    if !(MULTISTEP_ENABLED && multistep_is_due(n_reviewable, ctx.multistep_closed)) {
        return None;
    }
    let default = TopicState::default();
    let due_set: BTreeSet<&String> = due.iter().collect();
    let candidates: Vec<String> = review_topics
        .iter()
        .filter(|tid| {
            due_set.contains(*tid) && has_review_history(states.get(*tid).unwrap_or(&default))
        })
        .cloned()
        .collect();
    if candidates.len() < MULTISTEP_MIN_COMPONENTS {
        return None;
    }
    let components = multistep_components(&candidates, graph);
    review_topics.retain(|tid| !components.contains(tid));
    Some(multistep_task(&components, states, graph))
}

/// The inputs of one interleaved slot's task.
struct SlotInputs<'a> {
    states: &'a BTreeMap<String, TopicState>,
    graph: &'a Curriculum,
    cfg: &'a Config,
    ctx: &'a SessionContext<'a>,
    front: &'a Frontier,
    reviews: &'a ReviewPlan,
}

impl SlotInputs<'_> {
    /// The review or lesson task of one slot.
    fn task(&self, kind: SlotKind, tid: &str) -> Task {
        match kind {
            SlotKind::Review => review_task(
                tid,
                self.states,
                self.graph,
                self.cfg,
                &self.reviews.knockouts,
                self.reviews.nearly_served.contains(tid),
                self.front.blocked_until.is_some(),
            ),
            SlotKind::Lesson => lesson_task(
                tid,
                self.states,
                self.graph,
                &self.front.course_topics,
                &self.reviews.knockouts,
                self.ctx.gap_fill_chain.is_some(),
                self.ctx.gap_return_to,
            ),
        }
    }
}

/// Compose the ordered session plan of PEDAGOGY 5 (`compose_session`, `selector.py:1235-1481`).
///
/// The priority order is: the remediation queue, the compressed due reviews, the
/// frontier lessons by importance, the multi-step task, the quiz, then the
/// drills — with the review throttle, the lesson ratio, and the module
/// interleaving of PEDAGOGY 5.
///
/// With [`SessionContext::open_plan`] set it re-serves that plan minus the tasks
/// that are no longer valid, keeping their order and their ids.
#[must_use]
pub fn compose_session(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    sampler: &mut dyn QuizSampler,
    ctx: &SessionContext<'_>,
) -> SessionPlan {
    if let Some(open) = ctx.open_plan {
        return reserve_open_plan(open, states, graph, cfg, t_us, ctx);
    }
    let mut cache = ReachCache::new(graph);
    let front = Frontier::new(states, graph, cfg, t_us, ctx.course_id, ctx.gap_fill_chain);
    let course_complete = ctx.gap_fill_chain.is_none()
        && is_course_complete(states, graph, cfg, ctx.course_id, Some(&front.known));

    // Priority 1: the remediation queue. It is computed first so the review and
    // lesson lists can dedupe against it.
    let remediation = remediation_tasks(ctx.pending_remediation, states, graph, cfg);
    let remediation_topics: BTreeSet<String> = remediation
        .iter()
        .filter_map(|task| task.topic.clone())
        .collect();

    let reviews = ReviewPlan::new(states, graph, cfg, t_us, ctx, &front, &mut cache);
    // A remediation task supersedes the same topic's review or lesson.
    let mut review_topics = reviews.review_topics.clone();
    review_topics.retain(|tid| !remediation_topics.contains(tid));
    let mut lessons_ordered = reviews.lessons.clone();
    lessons_ordered.retain(|tid| !remediation_topics.contains(tid));

    // The multi-step integration task absorbs several due reviews.
    let multistep = multistep_plan(states, graph, ctx, &reviews.due, &mut review_topics);

    let seq = interleave(&review_topics, &lessons_ordered, cfg);
    let slots = SlotInputs {
        states,
        graph,
        cfg,
        ctx,
        front: &front,
        reviews: &reviews,
    };
    let mut tasks: Vec<Task> = remediation;
    tasks.extend(seq.iter().map(|(kind, tid)| slots.task(*kind, tid)));
    tasks.extend(multistep);

    let quiz_due = quiz_is_due(
        ctx.quiz_state,
        states,
        graph,
        cfg,
        t_us,
        ctx.active_study_days,
    );
    if quiz_due {
        let plan = quiz_composer(states, graph, cfg, t_us, sampler, ctx.learned_at);
        if !plan.questions.is_empty() {
            tasks.push(quiz_task(&plan, ctx.quiz_high_score_streak));
        }
    }

    for tid in schedule_drills(states, graph, t_us, ctx.last_drill_at) {
        tasks.push(drill_task(&tid, cfg));
    }

    if let Some(limit) = ctx.n {
        tasks.truncate(limit);
    }
    assign_ids(&mut tasks, ctx.session_id);

    front.plan(ctx.session_id, tasks, quiz_due, course_complete, &seq, cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Slug;
    use crate::fire::testing::{T_US, graph, learned, topic};
    use crate::learner::PendingRemediation;
    use crate::selector::SeededSampler;

    /// A root with eight due dependents, one nearly-due knocker of `r2`, and
    /// one plain lesson.
    fn tree() -> Curriculum {
        let mut plain = topic("lesson", &[("root", 0.3, false)]);
        plain.core = false;
        let mut drill = topic("drill", &[]);
        drill.drill = true;
        let mut topics = vec![topic("root", &[]), drill];
        topics.extend((0..8).map(|index| topic(&format!("r{index}"), &[("root", 0.3, false)])));
        topics.push(topic("knocker", &[("r2", 0.9, true)]));
        topics.push(plain);
        graph(topics)
    }

    /// The learned states: the root, eight due reviews, the knocker nearly due.
    fn states() -> BTreeMap<String, TopicState> {
        let mut states: BTreeMap<String, TopicState> = BTreeMap::new();
        states.insert("root".to_owned(), learned(0.9));
        states.insert("knocker".to_owned(), learned(0.55));
        for index in 0..8 {
            states.insert(format!("r{index}"), learned(0.5));
        }
        states
    }

    /// Compose over `tree` and `states` with `ctx`, seed 1, at `T`.
    fn compose(ctx: &SessionContext<'_>) -> SessionPlan {
        let cfg = Config::default();
        compose_session(
            &states(),
            &tree(),
            &cfg,
            T_US,
            &mut SeededSampler::new(1),
            ctx,
        )
    }

    #[test]
    fn the_plan_serves_remediation_reviews_lessons_and_the_multistep_task() {
        let pending = vec![PendingRemediation {
            kind: "quiz_miss".to_owned(),
            targets: vec![Slug::new("r0").expect("a slug")],
        }];
        let chain: BTreeSet<String> = ["lesson".to_owned()].into();
        let mut ctx = SessionContext::default()
            .with_pending_remediation(&pending)
            .with_multistep(0, &NO_CLOSED);
        ctx.gap_fill_chain = Some(&chain);
        ctx.gap_return_to = Some("top");
        let plan = compose(&ctx);
        let kinds: Vec<TaskType> = plan.tasks.iter().map(|task| task.task_type).collect();
        assert_eq!(kinds[0], TaskType::Review);
        assert!(plan.tasks[0].is_remediation);
        assert!(plan.tasks.iter().any(|task| task.gap_fill));
        assert!(plan.tasks.iter().any(|task| task.nearly_due));
        assert!(
            plan.tasks
                .iter()
                .any(|task| task.task_type == TaskType::MultiStep)
        );
        assert!(!plan.course_complete && plan.quiz_due);
        assert!(
            plan.tasks
                .iter()
                .any(|task| task.task_type == TaskType::Quiz)
        );

        let open = ["r1".to_owned(), "r2".to_owned()];
        let mut reserve = SessionContext::default().with_limit(Some(2));
        reserve.open_multistep_components = Some(&open);
        let short = compose(&reserve);
        assert_eq!(short.tasks.len(), 2);
        let closed: BTreeSet<String> = ["s-multi-step".to_owned()].into();
        let done = compose(&SessionContext::default().with_multistep(0, &closed));
        assert!(
            done.tasks
                .iter()
                .all(|task| task.task_type != TaskType::MultiStep)
        );
        // An open plan re-serves through the reserve path.
        let again = compose(&SessionContext::default().with_open_plan(Some(&done)));
        assert_eq!(again.tasks.len(), done.tasks.len());
    }

    #[test]
    fn too_few_due_reviews_owe_no_multistep_task_and_a_delay_blocks_the_frontier() {
        let cfg = Config::default();
        let tree = tree();
        // Five reviewable topics owe one task, but only two are due. The quiz
        // ran today, so none is due, and the fresh drill topic gets its drill.
        let mut few = states();
        for index in 2..8 {
            few.insert(format!("r{index}"), learned(0.9));
        }
        few.insert("drill".to_owned(), learned(0.9));
        let quiet = crate::learner::QuizState {
            last_at: crate::selector::utc_date(T_US),
            xp_since: 0,
            retake_pending: false,
        };
        let ctx = SessionContext::default().with_quiz_state(Some(&quiet));
        let plan = compose_session(&few, &tree, &cfg, T_US, &mut SeededSampler::new(1), &ctx);
        assert!(!plan.quiz_due);
        assert!(
            plan.tasks
                .iter()
                .all(|task| task.task_type != TaskType::MultiStep)
        );
        assert!(
            plan.tasks
                .iter()
                .any(|task| task.task_type == TaskType::Drill)
        );
        // The one frontier lesson failed half a day ago: no lesson is available,
        // and the frontier reopens one retry delay after the failure.
        let mut failed = TopicState {
            t0: Some(crate::event::Timestamp::from_micros(T_US)),
            ..TopicState::default()
        };
        failed
            .kp_progress
            .insert("kp1".to_owned(), crate::event::KpProgress::FailedOnce);
        let mut blocked = few;
        blocked.insert("lesson".to_owned(), failed);
        let front = Frontier::new(&blocked, &tree, &cfg, T_US, None, None);
        assert!(front.available.is_empty());
        assert_eq!(
            front.blocked_until,
            Some(T_US + cfg.lesson.retry_delay_days * 86_400_000_000)
        );
    }

    #[test]
    fn a_zero_question_quiz_is_due_and_serves_nothing() {
        let mut cfg = Config::default();
        cfg.quiz.questions = 0;
        let tree = tree();
        let none: BTreeMap<String, TopicState> = BTreeMap::new();
        let ctx = SessionContext::default();
        let plan = compose_session(&none, &tree, &cfg, T_US, &mut SeededSampler::new(1), &ctx);
        assert!(plan.quiz_due);
        assert!(
            plan.tasks
                .iter()
                .all(|task| task.task_type == TaskType::Lesson)
        );
        let front = Frontier::new(&none, &tree, &cfg, T_US, Some("c"), None);
        assert_eq!(front.available, ["drill", "root"]);
        assert_eq!(front.blocked_until, None);
    }

    /// No closed task ids.
    static NO_CLOSED: BTreeSet<String> = BTreeSet::new();
}
