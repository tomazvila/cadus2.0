//! The mixed review block: the questions of consecutive due reviews, served in
//! an interleaved order across their topics (Math Academy Way ch. 19, the
//! micro-interleaving of review problems).
//!
//! The plan keeps one review task per topic, and each task keeps its own
//! questions, its own attempts and its own `review_result`. The block changes
//! ONLY the order in which the already-planned questions are served: after each
//! answer the serve layer asks [`next_mixed_review`] which review of the block
//! serves next.
//!
//! The rule is a pure function of the block's state, so a resumed session and
//! a re-serve both land on the same task:
//!
//! - a review other than the one answered last, when one is open;
//! - among those, the one with the most questions left (the greedy rule that
//!   avoids two consecutive questions of one topic whenever any order can);
//! - a tie breaks on a hash of the session seed, the task id and the count left,
//!   so the order differs between sessions and never between two calls.

use crate::event::TaskType;

use super::task::Task;

/// Whether `task` joins a mixed review block.
///
/// An ordinary due or nearly-due review joins, and so does the delayed
/// retention probe (D-F11): inside a block it is one more question of the
/// interleaved sequence, so nothing marks it as a test. A remediation review,
/// a D-F6 confirmation item and an integrated assessment keep their own
/// serving.
#[must_use]
pub fn joins_mixed_review(task: &Task) -> bool {
    task.task_type == TaskType::Review
        && !task.is_remediation
        && !task.confirm
        && task.integrated_assessment_of.is_none()
        && task.integrated_item_id.is_none()
}

/// The mixed review block that holds `task_id`, in plan order.
///
/// The block is the run of consecutive joining reviews around `task_id`, read
/// over the plan with its closed tasks removed (`is_done`); `task_id` itself
/// stays in the run even when it is closed, so the answer that closes it still
/// hands on to the rest of its block. A lesson or any other task ends the run,
/// so the review throttle of the plan keeps its lessons where they stood.
///
/// The answer is empty when the run holds fewer than two reviews: a lone review
/// serves exactly as before.
#[must_use]
pub fn mixed_review_block<'plan>(
    tasks: &'plan [Task],
    task_id: &str,
    is_done: impl Fn(&str) -> bool,
) -> Vec<&'plan Task> {
    let open: Vec<&Task> = tasks
        .iter()
        .filter(|task| task.task_id == task_id || !is_done(&task.task_id))
        .collect();
    let Some(at) = open.iter().position(|task| task.task_id == task_id) else {
        return Vec::new();
    };
    if !joins_mixed_review(open[at]) {
        return Vec::new();
    }
    let start = open[..at]
        .iter()
        .rposition(|task| !joins_mixed_review(task))
        .map_or(0, |index| index + 1);
    let end = open[at..]
        .iter()
        .position(|task| !joins_mixed_review(task))
        .map_or(open.len(), |offset| at + offset);
    let block = open[start..end].to_vec();
    if block.len() < 2 { Vec::new() } else { block }
}

/// One review of a block, as the rule reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewSlot<'a> {
    /// The task id.
    pub task_id: &'a str,
    /// The questions the review still owes.
    pub remaining: i64,
    /// Whether the review is still open.
    pub open: bool,
}

/// The review of the block whose question is served next, or `None` when every
/// review is closed.
///
/// `last` is the review answered last. `seed` is the session id.
#[must_use]
pub fn next_mixed_review<'a>(
    seed: &str,
    slots: &[ReviewSlot<'a>],
    last: Option<&str>,
) -> Option<&'a str> {
    let open: Vec<&ReviewSlot<'a>> = slots.iter().filter(|slot| slot.open).collect();
    let others: Vec<&ReviewSlot<'a>> = open
        .iter()
        .copied()
        .filter(|slot| Some(slot.task_id) != last)
        .collect();
    let candidates = if others.is_empty() { open } else { others };
    candidates
        .into_iter()
        .max_by(|left, right| {
            left.remaining.cmp(&right.remaining).then_with(|| {
                // The SMALLER hash wins a tie, so the comparison is reversed.
                tie_break(seed, right).cmp(&tie_break(seed, left))
            })
        })
        .map(|slot| slot.task_id)
}

/// The order a whole block serves in when every answer follows the rule, for a
/// fresh block of `(task_id, questions)`.
///
/// It is the rule played forward, so the tests and the documentation read the
/// same sequence the serve layer produces one answer at a time.
#[must_use]
pub fn mixed_review_order(seed: &str, block: &[(&str, i64)]) -> Vec<String> {
    let mut left: Vec<i64> = block.iter().map(|(_, n)| (*n).max(0)).collect();
    let mut order = Vec::new();
    let mut last: Option<String> = None;
    loop {
        let slots: Vec<ReviewSlot<'_>> = block
            .iter()
            .zip(&left)
            .map(|((task_id, _), remaining)| ReviewSlot {
                task_id,
                remaining: *remaining,
                open: *remaining > 0,
            })
            .collect();
        let Some(next) = next_mixed_review(seed, &slots, last.as_deref()) else {
            return order;
        };
        if let Some(index) = block.iter().position(|(task_id, _)| *task_id == next) {
            left[index] -= 1;
        }
        order.push(next.to_owned());
        last = Some(next.to_owned());
    }
}

/// The FNV-1a hash of the seed, the task id and the count left. Stable across
/// builds and platforms, unlike the standard library's hasher.
fn tie_break(seed: &str, slot: &ReviewSlot<'_>) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    let parts: [&[u8]; 5] = [
        seed.as_bytes(),
        &[0xff],
        slot.task_id.as_bytes(),
        &[0xff],
        &slot.remaining.to_le_bytes(),
    ];
    for part in parts {
        for byte in part {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(PRIME);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review(id: &str) -> Task {
        Task {
            task_id: id.to_owned(),
            task_type: TaskType::Review,
            topic: Some(id.to_owned()),
            n_problems: Some(4),
            ..Task::default()
        }
    }

    fn lesson(id: &str) -> Task {
        Task {
            task_id: id.to_owned(),
            topic: Some(id.to_owned()),
            ..Task::default()
        }
    }

    fn adjacent_repeats(order: &[String]) -> usize {
        order.windows(2).filter(|pair| pair[0] == pair[1]).count()
    }

    #[test]
    fn the_order_never_repeats_a_topic_back_to_back_when_it_can_avoid_it() {
        for seed in ["s_2026-01-01a", "s_2026-03-14b", "s_x"] {
            for block in [
                vec![("a", 4), ("b", 4)],
                vec![("a", 4), ("b", 4), ("c", 4)],
                vec![("a", 4), ("b", 3)],
                vec![("a", 3), ("b", 1), ("c", 2), ("d", 4)],
            ] {
                let order = mixed_review_order(seed, &block);
                assert_eq!(adjacent_repeats(&order), 0, "{seed} {block:?} {order:?}");
                for (id, n) in &block {
                    let served = order.iter().filter(|task| task == id).count();
                    assert_eq!(served as i64, *n, "every question of {id} is served once");
                }
            }
        }
    }

    #[test]
    fn an_unavoidable_repeat_is_kept_to_the_minimum() {
        // The fewest repeats any order can reach: 2 * largest - total - 1.
        for block in [
            vec![("a", 5), ("b", 1)],
            vec![("a", 4), ("b", 2)],
            vec![("a", 4), ("b", 1), ("c", 1)],
            vec![("a", 4)],
        ] {
            let order = mixed_review_order("s", &block);
            let total: i64 = block.iter().map(|(_, n)| n).sum();
            let largest = block.iter().map(|(_, n)| *n).max().unwrap_or(0);
            let floor = (2 * largest - total - 1).max(0) as usize;
            assert_eq!(adjacent_repeats(&order), floor, "{block:?} {order:?}");
        }
        assert_eq!(
            mixed_review_order("s", &[("a", 5), ("b", 1)])[..3],
            ["a", "b", "a"]
        );
    }

    #[test]
    fn the_order_is_deterministic_per_seed_and_varies_between_seeds() {
        let block = [("a", 4), ("b", 4), ("c", 4)];
        let first = mixed_review_order("s_2026-01-01a", &block);
        assert_eq!(first, mixed_review_order("s_2026-01-01a", &block));
        let differs = ["s1", "s2", "s3", "s4", "s5", "s6"]
            .iter()
            .any(|seed| mixed_review_order(seed, &block) != first);
        assert!(differs, "the seed changes the tie-breaks");
    }

    #[test]
    fn the_rule_resumes_mid_block_on_the_same_task() {
        let block = [("a", 4), ("b", 4), ("c", 4)];
        let order = mixed_review_order("s_2026-01-01a", &block);
        // Stop after any prefix: the state alone gives the next task.
        for cut in 1..order.len() {
            let left: Vec<i64> = block
                .iter()
                .map(|(id, n)| n - order[..cut].iter().filter(|task| task == id).count() as i64)
                .collect();
            let slots: Vec<ReviewSlot<'_>> = block
                .iter()
                .zip(&left)
                .map(|((id, _), remaining)| ReviewSlot {
                    task_id: id,
                    remaining: *remaining,
                    open: *remaining > 0,
                })
                .collect();
            let next = next_mixed_review("s_2026-01-01a", &slots, Some(&order[cut - 1]));
            assert_eq!(next, Some(order[cut].as_str()), "cut {cut}");
        }
    }

    #[test]
    fn a_closed_review_is_never_chosen_and_an_empty_block_ends() {
        let slots = [
            ReviewSlot {
                task_id: "a",
                remaining: 0,
                open: false,
            },
            ReviewSlot {
                task_id: "b",
                remaining: 2,
                open: true,
            },
        ];
        assert_eq!(next_mixed_review("s", &slots, Some("b")), Some("b"));
        let closed = [ReviewSlot {
            task_id: "a",
            remaining: 0,
            open: false,
        }];
        assert_eq!(next_mixed_review("s", &closed, None), None);
    }

    #[test]
    fn the_block_is_the_run_of_plain_reviews_around_the_task() {
        let mut confirm = review("c");
        confirm.confirm = true;
        let mut remedial = review("r");
        remedial.is_remediation = true;
        let mut probe = review("p");
        probe.probe_delay_days = Some(7);
        let tasks = vec![
            remedial,
            confirm,
            review("a"),
            review("b"),
            review("d"),
            lesson("l"),
            review("e"),
            lesson("m"),
            probe.clone(),
        ];
        let ids = |block: Vec<&Task>| -> Vec<String> {
            block.iter().map(|task| task.task_id.clone()).collect()
        };
        let none = |_: &str| false;
        assert_eq!(ids(mixed_review_block(&tasks, "b", none)), ["a", "b", "d"]);
        assert!(
            mixed_review_block(&tasks, "e", none).is_empty(),
            "a lone review"
        );
        assert!(mixed_review_block(&tasks, "r", none).is_empty());
        assert!(mixed_review_block(&tasks, "c", none).is_empty());
        assert!(mixed_review_block(&tasks, "p", none).is_empty());
        assert!(mixed_review_block(&tasks, "l", none).is_empty());
        assert!(mixed_review_block(&tasks, "ghost", none).is_empty());
        // A closed review leaves the run; the asking task stays in it.
        let a_done = |id: &str| id == "a";
        assert_eq!(ids(mixed_review_block(&tasks, "b", a_done)), ["b", "d"]);
        assert_eq!(
            ids(mixed_review_block(&tasks, "a", a_done)),
            ["a", "b", "d"]
        );
        // A closed lesson joins the runs on either side of it.
        let l_done = |id: &str| id == "l";
        assert_eq!(
            ids(mixed_review_block(&tasks, "e", l_done)),
            ["a", "b", "d", "e"]
        );
        let only_b = |id: &str| id != "b";
        assert!(mixed_review_block(&tasks, "b", only_b).is_empty());
        // A retention probe beside a review is one more question of its block.
        let beside = vec![probe, review("x")];
        assert_eq!(ids(mixed_review_block(&beside, "x", none)), ["p", "x"]);
        assert_eq!(ids(mixed_review_block(&beside, "p", none)), ["p", "x"]);
    }
}
