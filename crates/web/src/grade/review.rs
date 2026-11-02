//! Review close and targeted confirmation (D-F7).
use super::*;
use cadus_core::curriculum::Curriculum;
use cadus_core::event::ReviewResult;
use cadus_core::fire::assess_review;
use cadus_core::learner::problem_text_hash;
use std::collections::BTreeSet;

/// Whether a review needs two distinct correct problems (D-RV1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Variety {
    /// A review of fewer than two questions cannot hold two problems.
    NotRequired,
    /// The topic serves fewer than two distinct authored problems.
    Waived,
    /// The rule applies.
    Required,
}

/// The number of distinct problems among the decided right answers.
///
/// The key of an attempt is its `item_digest`, or the hash of its problem text
/// when the row has no digest.
fn distinct_correct_items(attempts: &[&Attempt]) -> usize {
    attempts
        .iter()
        .filter(|a| !a.feedback_practice && !a.outcome.is_ungraded() && a.correct && !a.assisted)
        .map(|a| {
            a.item_digest
                .clone()
                .unwrap_or_else(|| problem_text_hash(&a.problem.text))
        })
        .collect::<BTreeSet<_>>()
        .len()
}

/// The number of distinct authored exemplar texts the review mix can serve.
///
/// A `component:x` entry names the topic `x`. Any other entry belongs to the
/// topic of the task. A topic that the curriculum lacks counts 0.
fn servable_items(graph: &Curriculum, topic: Option<&str>, mix: &[String]) -> usize {
    let mut topics: BTreeSet<&str> = BTreeSet::new();
    if let Some(topic) = topic {
        topics.insert(topic);
    }
    for entry in mix {
        match entry.strip_prefix("component:") {
            Some(component) => topics.insert(component),
            None => topic.is_some_and(|topic| topics.insert(topic)),
        };
    }
    let mut texts: BTreeSet<&str> = BTreeSet::new();
    for id in topics {
        if let Some(idx) = graph.idx_of(id) {
            for kp in graph.knowledge_points(idx) {
                texts.extend(kp.exemplars.iter().map(|e| e.problem.as_str()));
            }
        }
    }
    texts.len()
}

/// Which variety rule applies to this review.
fn variety(task: &Task, graph: &Curriculum, cfg: &Config) -> Variety {
    if task.n_problems.unwrap_or(cfg.review.questions) < 2 {
        Variety::NotRequired
    } else if servable_items(graph, task.topic.as_deref(), &task.mix) < 2 {
        Variety::Waived
    } else {
        Variety::Required
    }
}

/// The attempts with the background proof grader's landed verdicts applied.
///
/// Only the grader's own corrections apply here (their `grader_note` is
/// [`cadus_store::proof_grading::GRADER_NOTE`]); every other correction keeps
/// the path it had.
fn proof_verdicts(prior: &[EventRow], attempts: &[&Attempt]) -> Vec<Attempt> {
    let mut verdicts: std::collections::BTreeMap<&str, &cadus_core::event::RegradedAttempt> =
        std::collections::BTreeMap::new();
    for row in prior {
        if let Event::Regraded(correction) = &row.event {
            for corrected in &correction.attempts {
                if corrected.grader_note.as_deref() == Some(cadus_store::proof_grading::GRADER_NOTE)
                {
                    verdicts.insert(corrected.attempt_id.as_str(), corrected);
                }
            }
        }
    }
    attempts
        .iter()
        .map(|attempt| {
            let mut fixed = (*attempt).clone();
            if let Some(corrected) = verdicts.get(attempt.attempt_id.as_str())
                && fixed.outcome.is_ungraded()
                && let Some(outcome) = &corrected.outcome
            {
                fixed.correct = *outcome == AttemptOutcome::Correct;
                fixed.outcome = outcome.clone();
                fixed.work_quality = corrected.work_quality;
            }
            fixed
        })
        .collect()
}

/// Close a complete review from its recorded evidence.
pub(super) fn close_review(
    task: &Task,
    attempt: &Attempt,
    prior: &[EventRow],
    cfg: &Config,
    graph: &Curriculum,
) -> Advance {
    let needs_practice = !attempt.outcome.is_ungraded() && (!attempt.correct || attempt.assisted);
    if task.task_type != TaskType::Review
        || needs_practice
        || (attempt.feedback_practice && attempt.outcome.is_ungraded())
    {
        return Advance::carry_on();
    }
    let mut attempts: Vec<&Attempt> = prior
        .iter()
        .filter_map(|row| match &row.event {
            Event::Attempt(body) if body.task_id == task.task_id && !body.feedback_practice => {
                Some(body)
            }
            _ => None,
        })
        .collect();
    if !attempt.feedback_practice {
        attempts.push(attempt);
    }
    // A written proof the background grader already graded counts with its
    // verdict: a pass as correct, a needs-revision as a decided miss (D-PR1).
    let corrected = proof_verdicts(prior, &attempts);
    let attempts: Vec<&Attempt> = corrected.iter().collect();
    if i64::try_from(attempts.len()).unwrap_or(i64::MAX)
        < task.n_problems.unwrap_or(cfg.review.questions)
    {
        return Advance::carry_on();
    }
    let mut evidence = assess_review(&attempts, cfg);
    let variety = variety(task, graph, cfg);
    // D-RV1: two right answers on one problem are not evidence of transfer.
    let needs_variety =
        variety == Variety::Required && evidence.passed && distinct_correct_items(&attempts) < 2;
    if needs_variety {
        evidence.passed = false;
        evidence.inconclusive = true;
        let skills: BTreeSet<String> = attempts
            .iter()
            .filter(|a| !a.outcome.is_ungraded() && a.correct && !a.assisted)
            .flat_map(|a| a.skills.iter().cloned())
            .collect();
        evidence.confirmation_skills = skills.into_iter().collect();
    }
    let quality = attempts
        .last()
        .map_or(attempt.work_quality, |last| last.work_quality);
    let xp = if evidence.inconclusive {
        0.0
    } else {
        task_xp(TaskType::Review, quality, cfg, 1, 0, false)
    };
    let remediation = evidence
        .confirmation_skills
        .iter()
        .filter_map(|skill| {
            let (topic, kp) = skill.split_once('/')?;
            Some(RemediationTriggered {
                ts: attempt.ts,
                session: attempt.session.clone(),
                v: SchemaVersion::current(),
                kind: format!("review_confirmation:{kp}"),
                source_topic: attempt.topic.clone(),
                targets: vec![Slug::new(topic).ok()?],
            })
        })
        .collect();
    Advance {
        status: if needs_variety {
            STATUS_TASK_NEEDS_VARIETY
        } else if evidence.inconclusive {
            "task_inconclusive"
        } else if evidence.passed {
            STATUS_TASK_PASSED
        } else {
            STATUS_TASK_FAILED
        },
        result: Some(Event::ReviewResult(ReviewResult {
            ts: attempt.ts,
            session: attempt.session.clone(),
            v: SchemaVersion::current(),
            topic: attempt.topic.clone(),
            passed: evidence.passed,
            weighted_score: evidence.score,
            xp,
            quality_tier: quality,
            assisted: attempts.iter().any(|a| a.assisted),
            task_id: Some(task.task_id.clone()),
            inconclusive: evidence.inconclusive,
            confirmation_skills: evidence.confirmation_skills,
            variety_waived: variety == Variety::Waived,
        })),
        remediation,
        xp: Some(round2(xp)),
        next_kp: None,
        proof_due: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An inline curriculum: `solo` holds one exemplar, `pair` holds two.
    fn graph() -> Curriculum {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("cadus-variety-{}-{n}", std::process::id()));
        let course = root.join("calculus-1");
        std::fs::create_dir_all(&course).expect("the fixture directory");
        std::fs::write(
            root.join("courses.yaml"),
            "courses:\n  - id: calculus-1\n    name: Calculus 1\n    order: 1\n",
        )
        .expect("the fixture catalog");
        let topic = |id: &str, problems: &[&str], needs: &str| {
            let exemplars: String = problems
                .iter()
                .map(|p| {
                    format!(
                        "          - problem: '{p}'\n            answer: \"1\"\n            solution_sketch: 'one'\n"
                    )
                })
                .collect();
            let edge = if needs.is_empty() {
                "[]".to_owned()
            } else {
                format!("[{{id: {needs}, weight: 0.5, key: true}}]")
            };
            let key = if needs.is_empty() {
                "[]".to_owned()
            } else {
                format!("[{needs}]")
            };
            format!(
                "  - id: {id}\n    name: Topic {id}\n    core: true\n    difficulty: 0.45\n    drill: false\n    answer_kind: numeric\n    expected_time_secs: 90\n    prerequisites: {edge}\n    knowledge_points:\n      - id: kp1\n        name: Point\n        key_prerequisites: {key}\n        exemplars:\n{exemplars}        constraints: \"c\"\n"
            )
        };
        let body = format!(
            "unit: variety\ncourse: calculus-1\nmodule: \"M\"\ntopics:\n{}{}",
            topic("solo", &["Add 1 and 1."], ""),
            topic("pair", &["Add 2 and 2.", "Add 3 and 3."], "solo"),
        );
        std::fs::write(course.join("00-unit.yaml"), body).expect("the fixture file");
        let (graph, _findings) =
            cadus_core::curriculum::load_curriculum(&root).expect("the fixture loads");
        std::fs::remove_dir_all(&root).expect("the fixture is removed");
        graph
    }

    /// A review attempt: `n`, the digest, the verdict, and the text.
    fn attempt(n: usize, digest: Option<&str>, correct: bool, text: &str) -> Attempt {
        let raw = include_str!("../../tests/fixtures/tp23-attempt.json");
        let mut attempt: Attempt = serde_json::from_str(raw).expect("the fixture decodes");
        attempt.attempt_id = format!("t-review-{n}");
        attempt.task_id = "t-review".to_owned();
        attempt.task_type = TaskType::Review;
        attempt.correct = correct;
        attempt.outcome = if correct {
            AttemptOutcome::Correct
        } else {
            AttemptOutcome::Incorrect
        };
        attempt.item_digest = digest.map(str::to_owned);
        attempt.problem.text = text.to_owned();
        attempt.skills = vec!["pair/kp1".to_owned()];
        attempt.feedback_practice = false;
        attempt
    }

    fn task(topic: &str, mix: &[&str], n_problems: i64) -> Task {
        Task {
            task_id: "t-review".to_owned(),
            task_type: TaskType::Review,
            topic: Some(topic.to_owned()),
            n_problems: Some(n_problems),
            mix: mix.iter().map(|m| (*m).to_owned()).collect(),
            ..Task::default()
        }
    }

    /// Close the review whose answers are `answers` (digest, correct); the last
    /// one is the answer that closes it.
    fn close(task: &Task, answers: &[(&str, bool)]) -> Advance {
        let mut all: Vec<Attempt> = answers
            .iter()
            .enumerate()
            .map(|(n, (digest, ok))| attempt(n, Some(digest), *ok, &format!("text {digest}")))
            .collect();
        let last = all.pop().expect("one answer");
        let prior: Vec<EventRow> = all
            .into_iter()
            .zip(1..)
            .map(|(body, seq)| EventRow {
                seq,
                event: Event::Attempt(body),
            })
            .collect();
        close_review(task, &last, &prior, &Config::default(), &graph())
    }

    fn body(moved: &Advance) -> &ReviewResult {
        match moved.result.as_ref().expect("a close event") {
            Event::ReviewResult(body) => body,
            other => panic!("not a review result: {other:?}"),
        }
    }

    #[test]
    fn two_distinct_correct_problems_pass() {
        let t = task("pair", &[], 4);
        let moved = close(&t, &[("a", true), ("b", true), ("a", true), ("c", true)]);
        assert_eq!(moved.status, STATUS_TASK_PASSED);
        assert!(body(&moved).passed);
        assert!(!body(&moved).variety_waived);
    }

    #[test]
    fn two_right_answers_on_one_problem_do_not_pass() {
        let t = task("pair", &[], 4);
        let moved = close(&t, &[("a", true), ("a", true), ("a", true), ("a", true)]);
        assert_eq!(moved.status, STATUS_TASK_NEEDS_VARIETY);
        let result = body(&moved);
        assert!(!result.passed);
        assert!(result.inconclusive);
        assert!(result.xp.abs() < f64::EPSILON);
        assert_eq!(result.confirmation_skills, vec!["pair/kp1".to_owned()]);
        assert!(!result.variety_waived);
    }

    #[test]
    fn a_miss_then_two_right_on_one_problem_follows_the_pass_rule() {
        let t = task("pair", &[], 4);
        let one = close(&t, &[("a", false), ("b", true), ("b", true), ("b", true)]);
        assert_eq!(one.status, STATUS_TASK_NEEDS_VARIETY);
        let two = close(&t, &[("a", false), ("b", true), ("c", true), ("c", true)]);
        assert_eq!(two.status, STATUS_TASK_PASSED);
        assert!(body(&two).passed);
    }

    #[test]
    fn a_failed_pass_rule_stays_what_the_core_decided() {
        let t = task("pair", &[], 4);
        let moved = close(&t, &[("a", false), ("a", false), ("a", false), ("a", true)]);
        assert_eq!(moved.status, "task_inconclusive");
        assert!(!body(&moved).passed);
    }

    #[test]
    fn a_none_digest_falls_back_to_the_text_hash() {
        let a = attempt(0, None, true, "text one");
        let b = attempt(1, None, true, "text two");
        let c = attempt(2, None, true, "text one");
        assert_eq!(distinct_correct_items(&[&a, &b]), 2);
        assert_eq!(distinct_correct_items(&[&a, &c]), 1);
    }

    #[test]
    fn a_one_item_topic_is_waived() {
        let t = task("solo", &[], 4);
        let moved = close(&t, &[("a", true), ("a", true), ("a", true), ("a", true)]);
        assert_eq!(moved.status, STATUS_TASK_PASSED);
        assert!(body(&moved).passed);
        assert!(body(&moved).variety_waived);
    }

    #[test]
    fn a_one_question_review_needs_no_variety() {
        let t = task("pair", &[], 1);
        let moved = close(&t, &[("a", true)]);
        assert_eq!(moved.status, STATUS_TASK_PASSED);
        assert!(!body(&moved).variety_waived);
        let json = serde_json::to_string(body(&moved)).expect("the body writes");
        assert!(!json.contains("variety_waived"));
    }

    #[test]
    fn servable_items_counts_component_topics() {
        let graph = graph();
        assert_eq!(servable_items(&graph, Some("solo"), &[]), 1);
        assert_eq!(
            servable_items(&graph, Some("solo"), &["component:pair".to_owned()]),
            3
        );
        assert_eq!(servable_items(&graph, Some("missing"), &[]), 0);
    }

    // ---- D-L5: the lesson rules on a review, and the exit of the loop --------

    /// The rows of earlier answers (digest, correct, in the corrective loop).
    fn rows(answers: &[(&str, bool, bool)]) -> Vec<EventRow> {
        answers
            .iter()
            .enumerate()
            .map(|(n, (digest, ok, practice))| {
                let mut body = attempt(n, Some(digest), *ok, &format!("text {digest}"));
                body.feedback_practice = *practice;
                EventRow {
                    seq: i64::try_from(n).expect("a small index") + 1,
                    event: Event::Attempt(body),
                }
            })
            .collect()
    }

    /// Run the review rules for the answer after `earlier`.
    fn rules(earlier: &[(&str, bool, bool)], now: (&str, bool, bool)) -> (Advance, Option<Value>) {
        let t = task("pair", &[], 4);
        let prior = rows(earlier);
        let mut current = attempt(
            earlier.len(),
            Some(now.0),
            now.1,
            &format!("text {}", now.0),
        );
        current.feedback_practice = now.2;
        let graph = graph();
        let cfg = Config::default();
        let closed = close_review(&t, &current, &prior, &cfg, &graph);
        let moved = review_rules(&t, &current, &prior, &cfg, &graph, closed);
        let teach = teach_first(&t, &current, &prior, &moved);
        (moved, teach)
    }

    /// Rule 2: two misses in a row on a review give the in-sitting prerequisite
    /// step. The same pair gives the teach page of the point (rule 3).
    #[test]
    fn two_misses_in_a_row_on_a_review_queue_the_prerequisite_and_the_teach_page() {
        let (first, teach) = rules(&[], ("a", false, false));
        assert!(first.remediation.is_empty() && teach.is_none());
        let (second, teach) = rules(&[("a", false, false)], ("b", false, true));
        assert_eq!(second.status, STATUS_CONTINUE);
        assert_eq!(second.remediation.len(), 1);
        assert_eq!(second.remediation[0].kind, REMEDIATION_IN_SITTING_PREREQ);
        assert_eq!(
            second.remediation[0].targets,
            vec![Slug::new("solo").expect("a slug")]
        );
        let page = teach.expect("the second miss owes the teach page");
        assert_eq!(page["topic"], "pair");
        assert_eq!(page["kp"], "kp1");
    }

    /// The pair fires once: a third miss in a row queues nothing more.
    #[test]
    fn the_second_miss_fires_once() {
        let (third, teach) = rules(
            &[("a", false, false), ("b", false, true)],
            ("c", false, true),
        );
        assert!(third.remediation.is_empty());
        assert!(teach.is_none());
    }

    /// Rule 4: a corrective loop ends after two failed fresh items. The review
    /// result stands with `passed: false` and the task closes.
    #[test]
    fn two_failed_fresh_items_close_the_loop_with_a_failed_result() {
        let (moved, teach) = rules(
            &[("a", false, false), ("b", false, true)],
            ("c", false, true),
        );
        assert!(teach.is_none());
        // The loop limit is three misses in a row: the first miss and two fresh items.
        assert_eq!(moved.status, STATUS_TASK_FAILED);
        let result = body(&moved);
        assert!(!result.passed && !result.inconclusive);
        assert_eq!(result.task_id.as_deref(), Some("t-review"));
        assert!(moved.remediation.is_empty());
    }

    /// An unassisted right answer ends the run, and a right fresh item keeps the
    /// loop open.
    #[test]
    fn a_right_answer_ends_the_run_of_misses() {
        let (moved, _) = rules(
            &[("a", false, false), ("b", true, true)],
            ("c", false, false),
        );
        assert_ne!(moved.status, STATUS_TASK_FAILED);
        assert!(moved.result.is_none());
        assert_eq!(
            trailing_misses(
                &rows(&[("a", false, false), ("b", true, false)]),
                &attempt(2, Some("c"), false, "text c")
            ),
            1
        );
    }

    /// Rule 2: `fail_after` counts the misses of one point on a review.
    #[test]
    fn misses_reach_fail_after_on_a_review() {
        let cfg = Config::default();
        let fail_after = usize::try_from(cfg.lesson.fail_after).expect("a small count");
        // `fail_after - 1` misses stand, each apart from the next by a right answer.
        let mut padded: Vec<(&str, bool, bool)> = Vec::new();
        for n in 0..fail_after - 1 {
            padded.push(("x", false, false));
            if n + 1 < fail_after - 1 {
                padded.push(("y", true, false));
            }
        }
        let (moved, _) = rules(&padded, ("z", false, false));
        assert_eq!(moved.status, STATUS_TASK_FAILED);
        assert!(!body(&moved).passed);
    }

    /// A lesson and a review read the same sequence, and an ungraded answer
    /// stays out of it.
    #[test]
    fn an_ungraded_answer_is_not_a_miss_in_the_sequence() {
        let mut quiet = attempt(0, Some("a"), false, "text a");
        quiet.outcome = AttemptOutcome::Ungraded {
            reason: "no verdict".to_owned(),
        };
        let prior = vec![EventRow {
            seq: 1,
            event: Event::Attempt(quiet),
        }];
        let current = attempt(1, Some("b"), false, "text b");
        assert_eq!(trailing_misses(&prior, &current), 1);
        assert_eq!(kp_sequence(&prior, &current, Some("kp1")), vec![false]);
    }
}
