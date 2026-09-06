//! The multi-step integration task and the queued remediation tasks
//! (`selector.py:1044-1152`).

use std::collections::{BTreeMap, BTreeSet};

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::event::TaskType;
use crate::learner::{PendingRemediation, TopicState};
use crate::xp::is_known;

use super::review::review_mix;
use super::task::{Task, start_kp};
use super::topic_set::TopicSet;
use super::{
    DIFFICULTY_TARGET, MULTISTEP_CADENCE, MULTISTEP_MAX_COMPONENTS, REMEDIATION_CONFIRM_FAILED,
};

/// Whether the periodic multi-step cadence fires
/// (`multistep_is_due`, `selector.py:1050-1068`).
///
/// One integration task is owed per [`MULTISTEP_CADENCE`] mastered reviewable
/// topics, and `n_closed` spends the owed slots. It never fires for a learner
/// with no review history.
#[must_use]
pub const fn multistep_is_due(n_mastered_reviewable: i64, n_closed: i64) -> bool {
    n_mastered_reviewable >= MULTISTEP_CADENCE
        && n_closed < n_mastered_reviewable / MULTISTEP_CADENCE
}

/// Order the components of a multi-step task
/// (`multistep_components`, `selector.py:1055-1068`).
///
/// The order is `(in-set ancestor count, id)` ascending, so a prerequisite leads
/// its dependents. The list is capped at [`MULTISTEP_MAX_COMPONENTS`].
#[must_use]
pub fn multistep_components(candidates: &[String], graph: &Curriculum) -> Vec<String> {
    let pool: BTreeSet<String> = candidates.iter().cloned().collect();
    let mut in_pool = TopicSet::empty(graph);
    for tid in &pool {
        in_pool.insert_id(graph, tid);
    }
    let mut keyed: Vec<(usize, String)> = pool
        .into_iter()
        .map(|tid| {
            let depth = graph.idx_of(&tid).map_or(0, |idx| {
                graph
                    .ancestors(idx)
                    .into_iter()
                    .filter(|ancestor| in_pool.contains(*ancestor))
                    .count()
            });
            (depth, tid)
        })
        .collect();
    keyed.sort();
    keyed
        .into_iter()
        .take(MULTISTEP_MAX_COMPONENTS)
        .map(|(_, tid)| tid)
        .collect()
}

/// Build the multi-step integration task (`_multistep_task`, `selector.py:1071-1105`).
///
/// The recently served digests of every component are pooled, deduped, and kept
/// in component order, so every part avoids what the learner just solved.
pub(super) fn multistep_task(
    components: &[String],
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
) -> Task {
    let names: Vec<&str> = components
        .iter()
        .filter_map(|tid| graph.idx_of(tid))
        .filter_map(|idx| graph.topic(idx))
        .map(|topic| topic.name.as_str())
        .collect();
    let mut recent: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let default = TopicState::default();
    for component in components {
        for digest in &states.get(component).unwrap_or(&default).last_problems {
            if seen.insert(digest.clone()) {
                recent.push(digest.clone());
            }
        }
    }
    let n_parts = components.len();
    let joined = names.join(", ");
    Task {
        task_type: TaskType::MultiStep,
        n_problems: i64::try_from(n_parts).ok(),
        component_topics: components.to_vec(),
        difficulty_target: Some(DIFFICULTY_TARGET.to_owned()),
        recent_problem_hashes: recent,
        why: format!(
            "multi-part integration ({n_parts} parts, one per mastered skill in a novel \
             combination); compresses {n_parts} reviews into one task [{joined}]"
        ),
        ..Task::default()
    }
}

/// Build the queued remediation tasks (`_remediation_tasks`, `selector.py:1108-1152`).
///
/// A mastered target gets a remedial review; an unmastered one gets its lesson,
/// the peel-back of PEDAGOGY 8. Each target is served once, first occurrence wins.
#[must_use]
pub fn remediation_tasks(
    pending: &[PendingRemediation],
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
) -> Vec<Task> {
    let mut tasks: Vec<Task> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let default = TopicState::default();
    for item in pending {
        for target in &item.targets {
            let id = target.as_str();
            if seen.contains(id) || graph.idx_of(id).is_none() {
                continue;
            }
            seen.insert(id.to_owned());
            let state = states.get(id).unwrap_or(&default);
            let kind = &item.kind;
            // A failed confirmation always peels back to the LESSON: the topic
            // is known by inference only, and the inference just failed (D-F6).
            let relearn = kind == REMEDIATION_CONFIRM_FAILED;
            let task = if is_known(state) && !relearn {
                Task {
                    task_type: TaskType::Review,
                    topic: Some(id.to_owned()),
                    n_problems: Some(cfg.review.questions),
                    mix: review_mix(graph, id),
                    difficulty_target: Some(DIFFICULTY_TARGET.to_owned()),
                    recent_problem_hashes: state.last_problems.clone(),
                    why: format!("remediation ({kind}); remedial review"),
                    is_remediation: true,
                    ..Task::default()
                }
            } else {
                Task {
                    topic: Some(id.to_owned()),
                    start_at_kp: start_kp(graph, id, state),
                    why: format!("remediation ({kind}); peel-back lesson"),
                    is_remediation: true,
                    ..Task::default()
                }
            };
            tasks.push(task);
        }
    }
    tasks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Slug;
    use crate::fire::testing::{graph, learned, topic};

    #[test]
    fn the_integration_task_and_the_remediation_queue_name_their_topics() {
        let cfg = Config::default();
        let tree = graph(vec![
            topic("a", &[]),
            topic("b", &[("a", 0.5, false)]),
            topic("c", &[("b", 0.5, false)]),
        ]);
        let mut with_history = learned(0.5);
        with_history.last_problems = vec!["h1".to_owned(), "h2".to_owned()];
        let states: BTreeMap<String, TopicState> = [
            ("a".to_owned(), with_history.clone()),
            ("b".to_owned(), with_history),
        ]
        .into_iter()
        .collect();
        assert!(multistep_is_due(4, 0) && !multistep_is_due(4, 1));
        let ids = ["c", "b", "a"].map(str::to_owned);
        let components = multistep_components(&ids, &tree);
        assert_eq!(components, ["a", "b", "c"]);
        let task = multistep_task(&components, &states, &tree);
        assert_eq!(task.recent_problem_hashes, ["h1", "h2"]);
        assert_eq!(task.n_problems, Some(3));
        assert_eq!(task.difficulty_target.as_deref(), Some(DIFFICULTY_TARGET));
        assert_eq!(
            task.why,
            "multi-part integration (3 parts, one per mastered skill in a novel \
             combination); compresses 3 reviews into one task [a, b, c]"
        );
        assert!(task.topic.is_none() && task.task_id.is_empty());

        let targets = ["a", "c", "ghost"].map(|id| Slug::new(id).expect("a slug"));
        let mut pending = vec![PendingRemediation {
            kind: "quiz_miss".to_owned(),
            targets: targets.to_vec(),
        }];
        pending.push(PendingRemediation {
            kind: "repeat_fail".to_owned(),
            targets: targets[..1].to_vec(),
        });
        let tasks = remediation_tasks(&pending, &states, &tree, &cfg);
        let kinds: Vec<TaskType> = tasks.iter().map(|task| task.task_type).collect();
        assert_eq!(kinds, [TaskType::Review, TaskType::Lesson]);
        assert!(tasks.iter().all(|task| task.is_remediation));
        assert!(tasks.iter().all(|task| task.task_id.is_empty()));
        let review = &tasks[0];
        assert_eq!(review.mix, ["kp1"]);
        assert_eq!(review.difficulty_target.as_deref(), Some(DIFFICULTY_TARGET));
        assert_eq!(review.recent_problem_hashes, ["h1", "h2"]);
        assert_eq!(review.why, "remediation (quiz_miss); remedial review");
        let lesson = &tasks[1];
        assert_eq!(lesson.start_at_kp.as_deref(), Some("kp1"));
        assert_eq!(lesson.why, "remediation (quiz_miss); peel-back lesson");
    }
}
