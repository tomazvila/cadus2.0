//! The module interleaving, the review throttle, the constraint report, and
//! the task ids (`selector.py:1160-1228`, `selector.py:1484-1519`).

use std::collections::BTreeMap;

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::numeric::round_dp;

use super::plan::Constraints;
use super::review::count_as_float;
use super::task::Task;

/// One slot of the interleaved sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    /// A review of a learned topic.
    Review,
    /// A frontier lesson.
    Lesson,
}

/// Reorder the importance-ranked lessons across their modules
/// (`_arrange_lessons`, `selector.py:1160-1187`).
///
/// At each step it takes from a module other than the last one, preferring the
/// module with the most lessons left; a tie prefers the module that appeared
/// first. It falls back to a same-module lesson only when nothing else remains.
#[must_use]
pub fn arrange_lessons(lessons: &[String], graph: &Curriculum) -> Vec<String> {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut module_order: Vec<String> = Vec::new();
    for tid in lessons {
        let module = graph
            .idx_of(tid)
            .map_or_else(String::new, |idx| graph.module_of(idx).to_owned());
        if !groups.contains_key(&module) {
            module_order.push(module.clone());
        }
        groups.entry(module).or_default().push(tid.clone());
    }

    let mut result: Vec<String> = Vec::with_capacity(lessons.len());
    let mut last: Option<String> = None;
    while let Some((module, lesson)) = take_next(&module_order, &mut groups, last.as_deref()) {
        result.push(lesson);
        last = Some(module);
    }
    result
}

/// Take the next lesson: from the fullest module other than `last`, or from
/// `last` itself when nothing else remains. `None` once every group is empty.
fn take_next(
    module_order: &[String],
    groups: &mut BTreeMap<String, Vec<String>>,
    last: Option<&str>,
) -> Option<(String, String)> {
    let non_empty: Vec<&String> = module_order
        .iter()
        .filter(|module| groups.get(*module).is_some_and(|group| !group.is_empty()))
        .collect();
    let mut avail: Vec<&String> = non_empty
        .iter()
        .copied()
        .filter(|module| Some(module.as_str()) != last)
        .collect();
    if avail.is_empty() {
        avail = non_empty;
    }
    // Python `max` keeps the FIRST maximum of `(len(group), -order_index)`.
    // Walking `avail` in module order makes `-order_index` descend, so a
    // strict `>` on the length alone reproduces it.
    let mut best: Option<&String> = None;
    let mut best_len = 0_usize;
    for module in avail {
        let len = groups.get(module).map_or(0, Vec::len);
        if best.is_none() || len > best_len {
            best = Some(module);
            best_len = len;
        }
    }
    let module = best?;
    let lesson = groups.get_mut(module).map(|group| group.remove(0));
    lesson.map(|lesson| (module.clone(), lesson))
}

/// Interleave reviews and lessons (`_interleave`, `selector.py:1190-1219`).
///
/// Reviews come first by priority, but the throttle forces a lesson once
/// `selector.max_reviews_per_lesson` reviews have run without one. A lesson
/// resets the counter.
#[must_use]
pub fn interleave(reviews: &[String], lessons: &[String], cfg: &Config) -> Vec<(SlotKind, String)> {
    let mut seq: Vec<(SlotKind, String)> = Vec::with_capacity(reviews.len() + lessons.len());
    let mut reviews = reviews.iter().peekable();
    let mut lessons = lessons.iter().peekable();
    let mut reviews_since_lesson = 0_i64;
    let max_run = cfg.selector.max_reviews_per_lesson;
    while reviews.peek().is_some() || lessons.peek().is_some() {
        let force_lesson = lessons.peek().is_some() && reviews_since_lesson >= max_run;
        match reviews.next_if(|_| !force_lesson) {
            Some(tid) => {
                seq.push((SlotKind::Review, tid.clone()));
                reviews_since_lesson += 1;
            }
            None => {
                // A lesson remains here: the throttle forced one, or every
                // review is served and the loop condition holds a lesson.
                seq.extend(lessons.next().map(|tid| (SlotKind::Lesson, tid.clone())));
                reviews_since_lesson = 0;
            }
        }
    }
    seq
}

/// The longest run of consecutive reviews (`_max_review_run`, `selector.py:1222-1228`).
fn max_review_run(seq: &[(SlotKind, String)]) -> i64 {
    let mut run = 0_i64;
    let mut best = 0_i64;
    for &(kind, _) in seq {
        run = if kind == SlotKind::Review { run + 1 } else { 0 };
        best = best.max(run);
    }
    best
}

/// The throttle and ratio report (`_constraints`, `selector.py:1499-1519`).
pub(super) fn constraints_of(
    seq: &[(SlotKind, String)],
    lessons_available: bool,
    cfg: &Config,
) -> Constraints {
    let n_review = seq
        .iter()
        .filter(|&&(kind, _)| kind == SlotKind::Review)
        .count();
    let n_lesson = seq.len() - n_review;
    let denom = seq.len();
    let ratio = if denom == 0 {
        0.0
    } else {
        count_as_float(n_lesson) / count_as_float(denom)
    };
    Constraints {
        lesson_ratio_ok: !lessons_available || denom == 0 || ratio >= cfg.selector.lesson_ratio_min,
        lesson_ratio: round_dp(ratio, 4),
        throttle_ok: !lessons_available
            || max_review_run(seq) <= cfg.selector.max_reviews_per_lesson,
        reviews: i64::try_from(n_review).unwrap_or(i64::MAX),
        lessons: i64::try_from(n_lesson).unwrap_or(i64::MAX),
    }
}

/// Assign the content-stable task ids (`_assign_ids`, `selector.py:1484-1496`).
///
/// The id is `{session}-{task_type}-{topic}`, or `{session}-{task_type}` for a
/// task that spans several topics. It is never positional: a recomposed plan
/// must not recycle a served id onto a different topic.
pub fn assign_ids(tasks: &mut [Task], session_id: &str) {
    for task in tasks {
        let kind = task.task_type.as_str();
        task.task_id = match task.topic.as_deref() {
            Some(topic) => format!("{session_id}-{kind}-{topic}"),
            None => format!("{session_id}-{kind}"),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::TaskType;
    use crate::fire::testing::{ladder, topic};

    #[test]
    fn the_sequence_alternates_modules_and_throttles_the_reviews() {
        let cfg = Config::default();
        let tree = ladder(&[
            ("f", &[], vec![topic("fa", &[])]),
            (
                "n",
                &[],
                vec![topic("na", &[]), topic("nb", &[]), topic("nc", &[])],
            ),
        ]);
        let lessons = ["fa", "na", "nb", "nc", "ghost"].map(str::to_owned);
        assert_eq!(
            arrange_lessons(&lessons, &tree),
            ["na", "fa", "nb", "ghost", "nc"]
        );
        let reviews = ["r0", "r1", "r2", "r3"].map(str::to_owned);
        let seq = interleave(&reviews, &lessons[..1], &cfg);
        let kinds: Vec<SlotKind> = seq.iter().map(|(kind, _)| *kind).collect();
        assert_eq!(
            kinds,
            [
                SlotKind::Review,
                SlotKind::Review,
                SlotKind::Review,
                SlotKind::Lesson,
                SlotKind::Review
            ]
        );
        let report = constraints_of(&seq, true, &cfg);
        assert!(report.throttle_ok && report.reviews == 4 && report.lessons == 1);
        assert!(constraints_of(&[], false, &cfg).lesson_ratio_ok);
        let mut tasks = vec![
            Task {
                task_type: TaskType::Quiz,
                ..Task::default()
            },
            Task {
                topic: Some("r0".to_owned()),
                task_type: TaskType::Review,
                ..Task::default()
            },
        ];
        assign_ids(&mut tasks, "s");
        assert_eq!(tasks[0].task_id, "s-quiz");
        assert_eq!(tasks[1].task_id, "s-review-r0");
    }

    /// Four reviews in a row, then one lesson.
    fn long_run() -> Vec<(SlotKind, String)> {
        let mut seq: Vec<(SlotKind, String)> = (0..4)
            .map(|index| (SlotKind::Review, format!("r{index}")))
            .collect();
        seq.push((SlotKind::Lesson, "l".to_owned()));
        seq
    }

    #[test]
    fn a_run_of_four_reviews_breaks_the_throttle_only_when_a_lesson_is_available() {
        let cfg = Config::default();
        let report = constraints_of(&long_run(), true, &cfg);
        assert!(!report.throttle_ok);
        assert!(!report.lesson_ratio_ok);
        assert_eq!(report.lesson_ratio, 0.2);
        assert_eq!((report.reviews, report.lessons), (4, 1));

        let idle = constraints_of(&long_run(), false, &cfg);
        assert!(idle.throttle_ok && idle.lesson_ratio_ok);
        assert_eq!(idle.lesson_ratio, 0.2);

        let empty = constraints_of(&[], true, &cfg);
        assert!(empty.throttle_ok && empty.lesson_ratio_ok);
        assert_eq!(
            (empty.lesson_ratio, empty.reviews, empty.lessons),
            (0.0, 0, 0)
        );
    }
}
