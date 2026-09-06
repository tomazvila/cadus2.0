//! The per-event handlers of the fold (`projector.py:197-482`).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::event::{
    Attempt, DiagnosticAnswer, DiagnosticPlaced, Enrolled, LessonResult, ProfileReset, QuizResult,
    RemediationTriggered, ReviewResult, Slug, TaskServed, Timestamp, TopicStatus, WorkQuality,
};
use crate::fire::{difficulty, initial_ability, interval_for, py_min, speed_for};
use crate::learner::{LAST_PROBLEMS_WINDOW, TopicState, problem_text_hash};
use crate::selector::REMEDIATION_CONFIRM_FAILED;

use super::{
    ABILITY_SEED_PRIOR, INFERRED_SEED_BIAS, PLACEMENT_MEMORY_BASE, PLACEMENT_REPNUM_CAP, Projector,
    clamp01, refreshed_repnum,
};

impl Projector<'_> {
    /// `enrolled` (`projector.py:197-211`): remember the course, then stamp `floor` on
    /// every still-untouched topic of its mastery floor.
    pub(super) fn on_enrolled(&mut self, event: &Enrolled, apply_fire: bool) {
        let graph = self.graph;
        self.enrolled_course = Some(event.course.as_str().to_owned());
        if !apply_fire {
            return;
        }
        // 1.0 iterates a set here. Sorting by id removes the class of bug (trap T5).
        let mut floor: Vec<&str> = graph
            .mastery_floor(event.course.as_str())
            .unwrap_or_default()
            .into_iter()
            .map(|idx| graph.id_of(idx))
            .collect();
        floor.sort_unstable();
        floor.dedup();
        for tid in floor {
            let mut state = self.topics.get(tid).cloned().unwrap_or_default();
            if state.status == TopicStatus::Untouched {
                state.status = TopicStatus::Floor;
                self.topics.insert(tid.to_owned(), state);
            }
        }
    }

    /// `attempt` (`projector.py:213-224`): the dedup window, then the peel-back.
    ///
    /// The projector NEVER reads `work_quality` here, nor `error_tags`, `secs`,
    /// `assisted`, `work`, `answer_kind`, or `kp`. It reads the problem text and
    /// `correct` and nothing else. FIRe fires from the task results, not from an
    /// attempt.
    pub(super) fn on_attempt(&mut self, event: &Attempt, apply_fire: bool) {
        if !apply_fire {
            return;
        }
        let topic = event.topic.as_str();
        let mut state = self.topics.get(topic).cloned().unwrap_or_default();
        state
            .last_problems
            .push(problem_text_hash(&event.problem.text));
        let overflow = state
            .last_problems
            .len()
            .saturating_sub(LAST_PROBLEMS_WINDOW);
        state.last_problems.drain(..overflow);
        self.topics.insert(topic.to_owned(), state);
        if !event.correct {
            self.peel_back_conditional(topic);
        }
    }

    /// `lesson_result` (`projector.py:226-242`).
    pub(super) fn on_lesson_result(&mut self, event: &LessonResult, ts: i64, apply_fire: bool) {
        let topic = event.topic.as_str();
        self.xp_events.push((ts, event.xp));
        self.last_practice.insert(topic.to_owned(), ts);
        if event.passed && !self.learned_at.contains_key(topic) {
            self.learned_at.insert(topic.to_owned(), ts);
            self.completions.push((ts, topic.to_owned()));
        }
        if !apply_fire {
            return;
        }
        if event.passed {
            self.apply_fire_result(topic, true, event.quality_tier, ts, event.assisted);
            self.set_status(topic, TopicStatus::Learning);
            self.mark_kps_passed(topic);
        } else {
            self.mark_lesson_fail(topic, event.failed_at_kp.as_ref(), ts);
        }
    }

    /// `task_served` (D-F6). It reads the confirmation marker and nothing else.
    ///
    /// The marker opens one confirmation item. The review result of the same
    /// topic closes it, so a topic never carries two open items.
    pub(super) fn on_task_served(&mut self, event: &TaskServed) {
        if !event.confirm {
            return;
        }
        self.confirm_tasks.insert(event.task_id.clone());
        if let Some(topic) = event.topic.as_ref() {
            self.confirm_topics.insert(topic.as_str().to_owned());
        }
    }

    /// Close the open confirmation item of one review result, if there is one
    /// (D-F6).
    ///
    /// The `task_id` binds the pair. A result with no `task_id` binds through
    /// the topic, which is the rule `ReviewResult::task_id` already states.
    fn take_confirmation(&mut self, event: &ReviewResult) -> bool {
        let topic = event.topic.as_str();
        let matched = match event.task_id.as_deref() {
            Some(task_id) => self.confirm_tasks.remove(task_id),
            None => self.confirm_topics.contains(topic),
        };
        if matched {
            self.confirm_topics.remove(topic);
        }
        matched
    }

    /// The light indices of a closed confirmation item (D-F6).
    ///
    /// A PASSED item completes the topic, the way a passed lesson completes it.
    /// A FAILED item queues the peel-back lesson of the book, p.377. The trigger
    /// instant is one microsecond after the review, because the failed review
    /// itself is practice and practice at the trigger instant closes a queue
    /// entry.
    fn index_confirmation(&mut self, event: &ReviewResult, ts: i64) {
        let topic = event.topic.as_str();
        if event.passed {
            if !self.learned_at.contains_key(topic) {
                self.learned_at.insert(topic.to_owned(), ts);
                self.completions.push((ts, topic.to_owned()));
            }
        } else {
            self.remediation.push((
                ts.saturating_add(1),
                REMEDIATION_CONFIRM_FAILED.to_owned(),
                vec![event.topic.clone()],
            ));
        }
    }

    /// `review_result` (`projector.py:244-254`). It changes no status: a review on an
    /// untouched topic leaves it untouched while it writes `repNum` and `t0`.
    ///
    /// D-F6 adds ONE status change: a passed confirmation item moves the topic
    /// from `Placed` or `Floor` to `Learning`. A failed one keeps the status.
    pub(super) fn on_review_result(&mut self, event: &ReviewResult, ts: i64, apply_fire: bool) {
        let topic = event.topic.as_str();
        self.xp_events.push((ts, event.xp));
        self.last_practice.insert(topic.to_owned(), ts);
        let confirmation = self.take_confirmation(event);
        if confirmation {
            self.index_confirmation(event, ts);
        }
        if !apply_fire {
            return;
        }
        self.apply_fire_result(topic, event.passed, event.quality_tier, ts, event.assisted);
        if confirmation && event.passed {
            self.set_status(topic, TopicStatus::Learning);
        }
    }

    /// `quiz_result` (`projector.py:256-282`). Every question applies FIRe, pass or
    /// miss; a row that names a topic outside the curriculum is skipped.
    pub(super) fn on_quiz_result(&mut self, event: &QuizResult, ts: i64, apply_fire: bool) {
        let graph = self.graph;
        self.xp_events.push((ts, event.xp));
        self.quiz_last_ts = Some(ts);
        self.quiz_retake_pending = event.score < self.cfg.quiz.retake_below;
        for row in &event.per_topic {
            if row.correct && graph.idx_of(row.topic.as_str()).is_some() {
                self.last_practice.insert(row.topic.as_str().to_owned(), ts);
            }
        }
        if !apply_fire {
            return;
        }
        for row in &event.per_topic {
            if graph.idx_of(row.topic.as_str()).is_none() {
                continue;
            }
            let quality = if row.correct {
                WorkQuality::NearlyPerfect
            } else {
                WorkQuality::Poor
            };
            self.apply_fire_result(row.topic.as_str(), row.correct, quality, ts, false);
        }
    }

    /// `remediation_triggered` (`projector.py:284-287`). Ungated by `apply_fire`.
    pub(super) fn on_remediation(&mut self, event: &RemediationTriggered, ts: i64) {
        let graph = self.graph;
        let targets: Vec<Slug> = event
            .targets
            .iter()
            .filter(|target| graph.idx_of(target.as_str()).is_some())
            .cloned()
            .collect();
        self.remediation.push((ts, event.kind.clone(), targets));
    }

    /// `diagnostic_answer` (`projector.py:289-299`). A light index: tallied on every
    /// replay, so an incrementally applied placement sees the answers before it.
    pub(super) fn on_diagnostic_answer(&mut self, event: &DiagnosticAnswer) {
        self.diag_answers
            .entry(event.topic.as_str().to_owned())
            .or_default()
            .push((event.correct, event.weight.get()));
    }

    /// `diagnostic_placed` (`projector.py:301-371`).
    ///
    /// The per-session answer tally is consumed AND reset UNCONDITIONALLY, ahead of the
    /// `apply_fire` gate, so a light replay and a full replay stay consistent.
    pub(super) fn on_diagnostic_placed(
        &mut self,
        event: &DiagnosticPlaced,
        ts: i64,
        apply_fire: bool,
    ) {
        let diag_answers = std::mem::take(&mut self.diag_answers);
        if !apply_fire {
            return;
        }
        if event.refresh {
            self.refresh_placement(event, ts, &diag_answers);
            return;
        }
        let graph = self.graph;
        let conditional: BTreeSet<&str> = event.conditional.iter().map(Slug::as_str).collect();
        // `balances` keeps the JSON key order of 1.0's dict (trap T6).
        let placed: Vec<(String, f64)> = event
            .balances
            .iter()
            .filter(|(tid, balance)| graph.idx_of(tid.as_str()).is_some() && **balance > 0.0)
            .map(|(tid, balance)| (tid.clone(), *balance))
            .collect();
        let answered: BTreeSet<&str> = placed
            .iter()
            .filter(|(tid, _)| {
                diag_answers
                    .get(tid)
                    .is_some_and(|answers| !answers.is_empty())
            })
            .map(|(tid, _)| tid.as_str())
            .collect();

        // Pass 1: a directly answered topic seeds from its own correct/speed evidence.
        for (tid, balance) in &placed {
            if !answered.contains(tid.as_str()) {
                continue;
            }
            let ability = diag_answers.get(tid).map_or(ABILITY_SEED_PRIOR, |answers| {
                self.ability_from_answers(answers)
            });
            self.seed_placed(tid, *balance, ability, ts, &conditional);
        }
        // Pass 2: an inferred topic falls back to the now-populated neighborhood, low.
        for (tid, balance) in &placed {
            if answered.contains(tid.as_str()) {
                continue;
            }
            let nbr = initial_ability(tid, graph, &self.topics, self.cfg);
            self.seed_placed(
                tid,
                *balance,
                clamp01(nbr * INFERRED_SEED_BIAS),
                ts,
                &conditional,
            );
        }
    }

    /// Place one topic at `balance` with the given ability (`projector.py:345-360`).
    fn seed_placed(
        &mut self,
        tid: &str,
        balance: f64,
        ability: f64,
        ts: i64,
        conditional: &BTreeSet<&str>,
    ) {
        let rep = py_min(balance, PLACEMENT_REPNUM_CAP);
        let mut state = self.topics.get(tid).cloned().unwrap_or_default();
        state.status = TopicStatus::Placed;
        state.rep_num = rep;
        state.memory_base = PLACEMENT_MEMORY_BASE;
        state.t0 = Some(Timestamp::from_micros(ts));
        state.interval_days = interval_for(rep, self.cfg);
        state.ability = ability;
        state.speed = speed_for(ability, difficulty(self.graph, tid), self.cfg);
        state.conditional = conditional.contains(tid);
        self.topics.insert(tid.to_owned(), state);
    }

    /// A REFRESH diagnostic (`projector.py:373-422`).
    ///
    /// A refresh AVERAGES its evidence into the current state, so it also DEMOTES
    /// a stale topic. The H2 guard keeps it from PROMOTING never-learned material: a
    /// non-positive balance on an untouched topic is skipped.
    fn refresh_placement(
        &mut self,
        event: &DiagnosticPlaced,
        ts: i64,
        diag_answers: &BTreeMap<String, Vec<(bool, f64)>>,
    ) {
        let graph = self.graph;
        let conditional: BTreeSet<&str> = event.conditional.iter().map(Slug::as_str).collect();
        // `balances` keeps the JSON key order of 1.0's dict (trap T6).
        let rows: Vec<(String, f64)> = event
            .balances
            .iter()
            .map(|(tid, balance)| (tid.clone(), *balance))
            .collect();
        for (tid, balance) in rows {
            if graph.idx_of(tid.as_str()).is_none() {
                continue;
            }
            let old = self.topics.get(&tid).cloned().unwrap_or_default();
            if balance <= 0.0 && old.status == TopicStatus::Untouched {
                continue;
            }
            let new_rep = refreshed_repnum(old.rep_num, balance);
            // `on_diagnostic_answer` creates a list with its first entry, so a
            // present list holds at least one answer.
            let ability = match diag_answers.get(&tid) {
                Some(answers) => {
                    let fresh = self.ability_from_answers(answers);
                    clamp01((old.ability + fresh) / 2.0)
                }
                None => old.ability,
            };
            let mut next = old;
            next.status = TopicStatus::Placed;
            next.rep_num = new_rep;
            next.memory_base = PLACEMENT_MEMORY_BASE;
            next.t0 = Some(Timestamp::from_micros(ts));
            next.interval_days = interval_for(new_rep, self.cfg);
            next.ability = ability;
            next.speed = speed_for(ability, difficulty(graph, &tid), self.cfg);
            next.conditional = conditional.contains(tid.as_str());
            self.topics.insert(tid, next);
        }
    }

    /// `profile_reset` (`projector.py:424-435`): a clean slate for each named topic.
    pub(super) fn on_profile_reset(&mut self, event: &ProfileReset, apply_fire: bool) {
        if !apply_fire {
            return;
        }
        let graph = self.graph;
        for tid in &event.topics {
            if graph.idx_of(tid.as_str()).is_some() {
                self.topics
                    .insert(tid.as_str().to_owned(), TopicState::default());
            }
        }
    }

    /// The ability seed of a directly answered topic (`projector.py:437-450`).
    ///
    /// Each answer is a weight-scaled step from the neutral prior toward 1 (correct)
    /// or 0 (incorrect). The accumulation is one running value, the 1.0 `+=`.
    fn ability_from_answers(&self, answers: &[(bool, f64)]) -> f64 {
        let alpha = self.cfg.ability.ewma_alpha;
        let mut ability = ABILITY_SEED_PRIOR;
        for &(correct, weight) in answers {
            let target = if correct { 1.0 } else { 0.0 };
            ability += alpha * weight * (target - ability);
        }
        clamp01(ability)
    }

    /// The conditional peel-back (`projector.py:452-482`).
    ///
    /// A MISSED attempt on `topic` peels every conditionally placed topic `C` with
    /// `topic` in `{C} + direct-prerequisites(C)` — equivalently the candidates are
    /// `{topic} + direct-dependents(topic)`. Peeling HALVES the credit: `repNum` is
    /// halved, the interval follows, the flag clears, and the status stays `placed`.
    /// The instant is unused; the halved interval alone brings the review forward.
    fn peel_back_conditional(&mut self, topic: &str) {
        let graph = self.graph;
        // 1.0 iterates a set of candidates here (trap T5).
        let mut candidates: BTreeSet<&str> = BTreeSet::new();
        candidates.insert(topic);
        if let Some(idx) = graph.idx_of(topic) {
            for dependent in graph.dependents(idx) {
                candidates.insert(graph.id_of(dependent));
            }
        }
        for cid in candidates {
            let Some(state) = self.topics.get(cid) else {
                continue;
            };
            if !state.conditional {
                continue;
            }
            let new_rep = state.rep_num / 2.0;
            let mut next = state.clone();
            next.rep_num = new_rep;
            next.interval_days = interval_for(new_rep, self.cfg);
            next.conditional = false;
            self.topics.insert(cid.to_owned(), next);
        }
    }
}
