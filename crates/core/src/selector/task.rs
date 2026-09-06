//! The served task, the session plan, and the builders of one review, lesson,
//! quiz or drill task (`model.py:628-683`, `selector.py:845-1041`).

use std::collections::BTreeMap;

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::event::{KpProgress, TaskType};
use crate::learner::TopicState;
use crate::retention::ProbePlan;

use super::DIFFICULTY_TARGET;
use super::quiz::{QuizPlan, quiz_difficulty_target};
use super::review::review_mix;
use super::topic_set::TopicSet;

/// One served task (`Task`, `model.py:628-672`).
///
/// `topic` is the topic id of a single-topic task. It is `None` for a quiz and
/// for a multi-step task, which span several topics; those list their topics in
/// `mix` and `component_topics`.
///
/// `why` is display prose for the client. NOTHING reads a scheduling decision
/// back out of it: [`Task::is_remediation`] and [`Task::nearly_due`] are the
/// typed facts the re-serve keys on.
#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    /// The content-stable id, assigned by [`assign_ids`].
    pub task_id: String,
    /// The kind of task.
    pub task_type: TaskType,
    /// The topic id, for a single-topic task.
    pub topic: Option<String>,
    /// The number of problems the task serves.
    pub n_problems: Option<i64>,
    /// The review question mix, or the sampled quiz topics.
    pub mix: Vec<String>,
    /// The difficulty target of a review, a quiz, or a multi-step task.
    pub difficulty_target: Option<String>,
    /// The recently served problem digests to avoid (Hard Rule 4).
    pub recent_problem_hashes: Vec<String>,
    /// The knowledge point a lesson resumes at.
    pub start_at_kp: Option<String>,
    /// The timed budget of a quiz or a drill.
    pub time_budget_secs: Option<i64>,
    /// The display prose. It is never parsed.
    pub why: String,
    /// Whether a lesson fills an out-of-course prerequisite gap.
    pub gap_fill: bool,
    /// The course a gap-fill lesson returns to.
    pub gap_return_to: Option<String>,
    /// The ordered component topics of a multi-step task.
    pub component_topics: Vec<String>,
    /// Whether the task serves the remediation queue.
    pub is_remediation: bool,
    /// Whether a review is served before its due date.
    pub nearly_due: bool,
    /// Whether the review confirms an inferred topic (D-F6).
    ///
    /// The serve route copies the marker onto `task_served`, and the fold reads
    /// it back to move a passed topic to `Learning`.
    pub confirm: bool,
    /// The delay, in days, of the retention probe this task serves (D-F11).
    ///
    /// `None` is an ordinary task. The serve route copies the marker onto
    /// `task_served`, so the fold counts the probe of the SESSION at the serve and
    /// not at the answer; the grade route writes the `retention_probe` event.
    pub probe_delay_days: Option<u32>,
    /// The knowledge point the retention probe tests (D-F11).
    pub probe_kp: Option<String>,
}

impl Default for Task {
    /// The 1.0 field defaults. `task_type` has no 1.0 default, so the port picks
    /// the most common one, and the lesson builders rely on it. `task_id` stays
    /// empty until [`assign_ids`] names the task.
    fn default() -> Self {
        Self {
            task_id: String::new(),
            task_type: TaskType::Lesson,
            topic: None,
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
            confirm: false,
            probe_delay_days: None,
            probe_kp: None,
        }
    }
}

/// The knowledge point a lesson resumes at (`_start_kp`, `selector.py:910-916`).
///
/// It is the first knowledge point not yet passed, or the first one when every
/// point has passed. A topic that authors no knowledge point gives `None`.
#[must_use]
pub fn start_kp(graph: &Curriculum, tid: &str, state: &TopicState) -> Option<String> {
    let idx = graph.idx_of(tid)?;
    let kps = graph.knowledge_points(idx);
    for kp in kps {
        if state.kp_progress.get(kp.id.as_str()) != Some(&KpProgress::Passed) {
            return Some(kp.id.as_str().to_owned());
        }
    }
    kps.first().map(|kp| kp.id.as_str().to_owned())
}

/// The knocked-out count of a covering task.
pub(super) fn knockout_count(knockouts: &BTreeMap<String, Vec<String>>, tid: &str) -> usize {
    knockouts.get(tid).map_or(0, Vec::len)
}

/// Build a review task (`_review_task`, `selector.py:919-975`).
pub(super) fn review_task(
    tid: &str,
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    knockouts: &BTreeMap<String, Vec<String>>,
    nearly: bool,
    frontier_blocked: bool,
) -> Task {
    let n_ko = knockout_count(knockouts, tid);
    let mut why = if nearly {
        let mut text = "nearly-due review".to_owned();
        if frontier_blocked {
            text.push_str(" (brought forward; frontier blocked)");
        }
        text
    } else {
        "due review".to_owned()
    };
    if n_ko > 0 {
        why.push_str(&format!(
            "; knocks out {n_ko} other due topic(s) via encompassing"
        ));
    }
    Task {
        nearly_due: nearly,
        ..review_shell(tid, states, graph, cfg.review.questions, why)
    }
}

/// The common shape of every review task: the topic, the question count, the
/// mix, the difficulty target, and the anti-repeat digests.
///
/// The due review, the remedial review and the D-F6 confirmation item all build
/// on it, and each one adds its own marker.
pub(super) fn review_shell(
    tid: &str,
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    n_problems: i64,
    why: String,
) -> Task {
    let default = TopicState::default();
    let state = states.get(tid).unwrap_or(&default);
    Task {
        task_type: TaskType::Review,
        topic: Some(tid.to_owned()),
        n_problems: Some(n_problems),
        mix: review_mix(graph, tid),
        difficulty_target: Some(DIFFICULTY_TARGET.to_owned()),
        recent_problem_hashes: state.last_problems.clone(),
        why,
        ..Task::default()
    }
}

/// Build one delayed retention probe (D-F11).
///
/// It is a review task of ONE problem, and it carries the marker the serve route
/// and the grade route read. `recent_problem_hashes` holds the LIFETIME exposure
/// index, so the draw refuses every item the learner ever met: a probe on a seen
/// item measures familiarity and no delayed recall.
pub(super) fn probe_task(
    plan: &ProbePlan,
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    seen: Vec<String>,
) -> Task {
    let why = format!(
        "retention probe; the lesson passed {} days ago and this item is new",
        plan.elapsed_days
    );
    Task {
        probe_delay_days: Some(plan.delay_days),
        probe_kp: Some(plan.kp.clone()),
        recent_problem_hashes: seen,
        ..review_shell(&plan.topic, states, graph, PROBE_PROBLEMS, why)
    }
}

/// The number of problems one retention probe serves.
pub const PROBE_PROBLEMS: i64 = 1;

/// Build a frontier lesson task (`_lesson_task`, `selector.py:978-1010`).
pub(super) fn lesson_task(
    tid: &str,
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    course_topics: &TopicSet,
    knockouts: &BTreeMap<String, Vec<String>>,
    gap_fill: bool,
    gap_return_to: Option<&str>,
) -> Task {
    let n_dep = graph.idx_of(tid).map_or(0, |idx| {
        graph
            .descendants(idx)
            .into_iter()
            .filter(|dependent| course_topics.contains(*dependent))
            .count()
    });
    let n_ko = knockout_count(knockouts, tid);
    let mut parts: Vec<String> = if gap_fill {
        let mut parts = vec!["gap-fill lesson".to_owned()];
        if let Some(course) = gap_return_to {
            parts.push(format!("unblocks {course}"));
        }
        parts
    } else {
        vec![
            "frontier lesson".to_owned(),
            format!("{n_dep} in-course dependents"),
        ]
    };
    if graph
        .idx_of(tid)
        .and_then(|idx| graph.topic(idx))
        .is_some_and(|topic| topic.core)
    {
        parts.push("core".to_owned());
    }
    let mut why = parts.join(", ");
    if n_ko > 0 {
        why.push_str(&format!("; knocks out {n_ko} due review(s)"));
    }
    let default = TopicState::default();
    Task {
        topic: Some(tid.to_owned()),
        start_at_kp: start_kp(graph, tid, states.get(tid).unwrap_or(&default)),
        why,
        gap_fill,
        gap_return_to: gap_return_to.map(ToOwned::to_owned),
        ..Task::default()
    }
}

/// Build the quiz task (`_quiz_task`, `selector.py:1013-1030`).
pub(super) fn quiz_task(plan: &QuizPlan, difficulty_streak: i64) -> Task {
    let n_recent = plan
        .questions
        .iter()
        .filter(|question| question.stratum == "recent")
        .count();
    let n_questions = plan.questions.len();
    Task {
        task_type: TaskType::Quiz,
        n_problems: i64::try_from(n_questions).ok(),
        mix: plan
            .questions
            .iter()
            .map(|question| format!("topic:{}", question.topic))
            .collect(),
        difficulty_target: Some(quiz_difficulty_target(difficulty_streak).to_owned()),
        time_budget_secs: Some(plan.total_time_budget_secs()),
        why: format!("quiz due; {n_questions} questions ({n_recent} recent, all-history)"),
        ..Task::default()
    }
}

/// Build a drill task (`_drill_task`, `selector.py:1033-1041`).
pub(super) fn drill_task(tid: &str, cfg: &Config) -> Task {
    let target = cfg.drill.target_secs;
    Task {
        task_type: TaskType::Drill,
        topic: Some(tid.to_owned()),
        n_problems: Some(cfg.drill.questions),
        time_budget_secs: Some(target.saturating_mul(cfg.drill.questions)),
        why: format!("automaticity drill; target {target}s/question"),
        ..Task::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fire::testing::{T_US, graph, knowledge_point, learned, topic};
    use crate::selector::QUIZ_DIFFICULTY_TARGETS;
    use crate::selector::quiz::QuizQuestion;
    use crate::selector::{
        remediation_for_quiz_miss, remediation_for_repeat_fail, schedule_drills,
    };

    #[test]
    fn the_builders_write_the_1_0_prose_and_the_drills_follow_the_bar() {
        let cfg = Config::default();
        let mut two = topic("two", &[("one", 0.9, true)]);
        two.knowledge_points.push(knowledge_point("kp2", &["one"]));
        let mut drill = topic("drill", &[]);
        drill.drill = true;
        let tree = graph(vec![topic("one", &[]), two, drill]);
        let mut passed = learned(0.5);
        passed
            .kp_progress
            .insert("kp1".to_owned(), KpProgress::Passed);
        let mut one = learned(0.5);
        one.last_problems = vec!["h1".to_owned()];
        let states: BTreeMap<String, TopicState> = [
            ("one".to_owned(), one),
            ("two".to_owned(), passed.clone()),
            ("drill".to_owned(), learned(0.9)),
        ]
        .into_iter()
        .collect();
        assert_eq!(start_kp(&tree, "two", &passed).as_deref(), Some("kp2"));
        passed
            .kp_progress
            .insert("kp2".to_owned(), KpProgress::Passed);
        assert_eq!(start_kp(&tree, "two", &passed).as_deref(), Some("kp1"));
        assert_eq!(start_kp(&tree, "ghost", &passed), None);

        let knockouts: BTreeMap<String, Vec<String>> =
            [("two".to_owned(), vec!["one".to_owned()])].into();
        let review = review_task("two", &states, &tree, &cfg, &knockouts, true, true);
        assert_eq!(
            review.why,
            "nearly-due review (brought forward; frontier blocked); \
             knocks out 1 other due topic(s) via encompassing"
        );
        assert_eq!(review.mix, ["kp1", "kp2", "component:one"]);
        assert_eq!(review.difficulty_target.as_deref(), Some(DIFFICULTY_TARGET));
        assert!(review.nearly_due && review.task_id.is_empty());
        let due = review_task("one", &states, &tree, &cfg, &knockouts, false, false);
        assert_eq!(due.why, "due review");
        assert_eq!(due.recent_problem_hashes, ["h1"]);
        let scope = TopicSet::empty(&tree);
        let gap = lesson_task("two", &states, &tree, &scope, &knockouts, true, Some("top"));
        assert_eq!(
            gap.why,
            "gap-fill lesson, unblocks top, core; knocks out 1 due review(s)"
        );
        assert!(gap.gap_fill && gap.task_type == TaskType::Lesson);
        assert_eq!(gap.gap_return_to.as_deref(), Some("top"));
        assert_eq!(gap.start_at_kp.as_deref(), Some("kp2"));
        let no_return = lesson_task("two", &states, &tree, &scope, &knockouts, true, None);
        assert!(no_return.gap_fill && !no_return.why.contains("unblocks"));
        let plain = lesson_task("one", &states, &tree, &scope, &knockouts, false, None);
        assert_eq!(plain.why, "frontier lesson, 0 in-course dependents, core");
        assert_eq!(quiz_task(&QuizPlan::default(), 0).n_problems, Some(0));
        let drill = drill_task("drill", &cfg);
        assert_eq!(drill.task_type, TaskType::Drill);
        assert_eq!(drill.why, "automaticity drill; target 6s/question");
        assert_eq!(drill.time_budget_secs, Some(120));

        assert_eq!(schedule_drills(&states, &tree, T_US, None), ["drill"]);
        let mut fresh = states.clone();
        fresh.insert("drill".to_owned(), TopicState::default());
        assert!(schedule_drills(&fresh, &tree, T_US, None).is_empty());
        fresh.remove("drill");
        assert!(schedule_drills(&fresh, &tree, T_US, None).is_empty());
        let recent: BTreeMap<String, i64> = [("drill".to_owned(), T_US)].into();
        assert!(schedule_drills(&states, &tree, T_US, Some(&recent)).is_empty());

        assert!(remediation_for_quiz_miss("").is_err());
        assert_eq!(
            quiz_task(&three_question_plan(), 1)
                .difficulty_target
                .as_deref(),
            Some(QUIZ_DIFFICULTY_TARGETS[1])
        );
        assert_eq!(
            remediation_for_repeat_fail("two", "kp2", &tree)
                .targets
                .len(),
            1
        );
        assert!(
            remediation_for_repeat_fail("two", "kp9", &tree)
                .targets
                .is_empty()
        );
        assert!(
            remediation_for_repeat_fail("ghost", "kp1", &tree)
                .targets
                .is_empty()
        );
    }

    /// One recent question and two mid questions of 45 s each.
    fn three_question_plan() -> QuizPlan {
        QuizPlan {
            questions: ["a", "b", "c"]
                .into_iter()
                .enumerate()
                .map(|(index, id)| QuizQuestion {
                    topic: id.to_owned(),
                    time_budget_secs: 45,
                    stratum: if index == 0 { "recent" } else { "mid" },
                })
                .collect(),
        }
    }

    #[test]
    fn the_quiz_task_counts_the_recent_questions_and_sums_the_budget() {
        let quiz = quiz_task(&three_question_plan(), 0);
        assert_eq!(quiz.why, "quiz due; 3 questions (1 recent, all-history)");
        assert_eq!(quiz.mix, ["topic:a", "topic:b", "topic:c"]);
        assert_eq!(
            quiz.difficulty_target.as_deref(),
            Some(QUIZ_DIFFICULTY_TARGETS[0])
        );
        assert_eq!(quiz.time_budget_secs, Some(135));
        assert_eq!(quiz.n_problems, Some(3));
        assert!(quiz.topic.is_none() && quiz.task_id.is_empty());
    }
}
