//! The fold: the event log replayed into the learner model (R5, D3, D4, spec section 4).
//!
//! The projector is the ONE writer of the derived learner model. It is a deterministic
//! fold: the same events, curriculum, and config always give the same model, so a full
//! rebuild and an incremental resume agree byte for byte, apart from `built_from_ts`.
//!
//! Two paths, one code path
//! ------------------------
//! [`Projector::apply`] folds one event. Its `apply_fire` flag decides whether the
//! expensive per-topic FIRe state changes:
//!
//! - [`project`] applies every event with `apply_fire = true`.
//! - [`project_incremental`] seeds the topic states from a CACHED model, replays the
//!   prior events with `apply_fire = false` (the cheap indices only), then applies the
//!   new events with `apply_fire = true`.
//!
//! Everything except the FIRe topic states — XP, streak, velocity, quiz cadence,
//! pending remediation, the mastery dates — is a cheap tally over event fields. The
//! fold always rebuilds those from the whole stream, so a cache never holds them.
//!
//! Time
//! ----
//! Velocity, "today", and the streak read `t_ref`, the LAST event's instant, never a
//! wall clock (trap T10). Only `built_from_ts` carries `now`, and the parity blob drops
//! that field.
//!
//! Iteration order
//! ---------------
//! Wherever 1.0 iterates a Python set, this port iterates the same ids SORTED (trap
//! T5): the mastery floor of `enrolled` and the peel-back candidates. Wherever 1.0
//! relies on dict insertion order, this port keeps that order (trap T6): the `balances`
//! map of `diagnostic_placed` and the ability delta map of the FIRe update.
//!
//! No handler panics. A malformed event never reaches the fold, because
//! [`crate::event::Event::from_json`] rejects it with an error value first; an event
//! that names a topic outside the curriculum is skipped exactly where 1.0 skips it.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use chrono_tz::Tz;
use serde_json::Value;

use crate::config::Config;
use crate::curriculum::{Curriculum, python_repr_f64, sha256_hex};
use crate::event::{
    Attempt, DiagnosticAnswer, DiagnosticPlaced, Enrolled, Event, KpProgress, LessonResult,
    ProfileReset, QuizResult, Regraded, RegradedAttempt, RemediationTriggered, ReviewResult, Slug,
    Timestamp, TopicStatus, WorkQuality,
};
use crate::fire::{
    AttemptResult, ability_update, apply_attempt, clamp, difficulty, initial_ability, interval_for,
    py_min, speed_for,
};
use crate::learner::{
    LAST_PROBLEMS_WINDOW, LearnerModel, PendingRemediation, QuizState, TopicState, VelocityState,
    XpState, problem_text_hash,
};
use crate::numeric::{
    TimeError, local_day_in, neumaier_sum, resolve_timezone, round_half_even_i64, to_datetime,
};
use crate::xp::{
    VELOCITY_WINDOW_DAYS, VelocityInput, compute_velocity_state, current_streak, daily_totals,
};

/// The projection-logic version stamped into the model (`projector.py:89`).
///
/// A stale cache is detected with it: 1 to 2 for the methodology fixes, 2 to 3 for
/// [`apply_regrades`]. ANY change to the fold bumps this number.
pub const PROJECTOR_VERSION: i64 = 3;

/// The neutral prior a placed topic's diagnostic answers fold onto (`projector.py:98`).
pub const ABILITY_SEED_PRIOR: f64 = 0.5;

/// The discount on an INFERRED placed topic's neighborhood ability (`projector.py:105`).
pub const INFERRED_SEED_BIAS: f64 = 0.9;

/// The placement rep-credit cap, `repNum = min(balance, 4)` (`diagnostic.py:71`).
pub const PLACEMENT_REPNUM_CAP: f64 = 4.0;

/// A freshly placed topic is fully remembered now (`diagnostic.py:76`).
pub const PLACEMENT_MEMORY_BASE: f64 = 1.0;

/// The daily XP goal the 1.0 entry points default to (`projector.py:779`).
pub const DEFAULT_XP_GOAL: i64 = 40;

/// What the fold reports instead of a panic.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProjectorError {
    /// A time zone name or an instant that `chrono` does not represent.
    #[error("{0}")]
    Time(#[from] TimeError),
    /// The config or the model did not serialize.
    #[error("serialize: {0}")]
    Serialize(String),
}

/// A refreshed topic's `repNum`: the mean of the current one and the refresh evidence
/// (`diagnostic.py:318-326`).
///
/// A positive balance implies `min(balance, 4)` reps; a non-positive balance implies
/// zero, so the mean pulls a stale topic DOWN.
#[must_use]
pub fn refreshed_repnum(old_rep: f64, balance: f64) -> f64 {
    let refresh_rep = if balance > 0.0 {
        py_min(balance, PLACEMENT_REPNUM_CAP)
    } else {
        0.0
    };
    (old_rep + refresh_rep) / 2.0
}

/// `projector._clamp01`: Python `max(0.0, min(1.0, x))` (`projector.py:694-695`).
fn clamp01(x: f64) -> f64 {
    clamp(x, 0.0, 1.0)
}

// --------------------------------------------------------------------------- //
// The accumulator
// --------------------------------------------------------------------------- //

/// A mutable fold accumulator over the event log (`projector.py:131-682`).
#[derive(Debug, Clone)]
pub struct Projector<'a> {
    graph: &'a Curriculum,
    cfg: &'a Config,
    zone: Tz,
    goal: i64,

    /// The FIRe topic states: the expensive part, seeded from a cache on resume.
    topics: BTreeMap<String, TopicState>,

    // The light indices. The fold always rebuilds them from the whole stream.
    enrolled_course: Option<String>,
    xp_events: Vec<(i64, f64)>,
    completions: Vec<(i64, String)>,
    learned_at: BTreeMap<String, i64>,
    quiz_last_ts: Option<i64>,
    quiz_retake_pending: bool,
    remediation: Vec<(i64, String, Vec<Slug>)>,
    last_practice: BTreeMap<String, i64>,
    diag_answers: BTreeMap<String, Vec<(bool, f64)>>,
    last_ts: Option<i64>,
}

impl<'a> Projector<'a> {
    /// A fold over `graph` and `cfg`, in UTC, with the 1.0 default XP goal.
    #[must_use]
    pub fn new(graph: &'a Curriculum, cfg: &'a Config) -> Self {
        Self {
            graph,
            cfg,
            zone: Tz::UTC,
            goal: DEFAULT_XP_GOAL,
            topics: BTreeMap::new(),
            enrolled_course: None,
            xp_events: Vec::new(),
            completions: Vec::new(),
            learned_at: BTreeMap::new(),
            quiz_last_ts: None,
            quiz_retake_pending: false,
            remediation: Vec::new(),
            last_practice: BTreeMap::new(),
            diag_answers: BTreeMap::new(),
            last_ts: None,
        }
    }

    /// Set the time zone of the day boundary. `None` means UTC (trap T9).
    ///
    /// # Errors
    ///
    /// Returns [`TimeError::UnknownTimezone`] when the name is not in the database.
    pub fn with_timezone(mut self, tz: Option<&str>) -> Result<Self, TimeError> {
        self.zone = resolve_timezone(tz)?;
        Ok(self)
    }

    /// Set the daily XP goal the streak compares against.
    #[must_use]
    pub const fn with_goal(mut self, goal: i64) -> Self {
        self.goal = goal;
        self
    }

    /// Seed the FIRe topic states from a cached model (the incremental resume).
    #[must_use]
    pub fn with_cached_topics(mut self, topics: BTreeMap<String, TopicState>) -> Self {
        self.topics = topics;
        self
    }

    /// The topic states as they stand.
    #[must_use]
    pub const fn topics(&self) -> &BTreeMap<String, TopicState> {
        &self.topics
    }

    /// The instant of the last event folded, or `None` before the first one.
    #[must_use]
    pub const fn last_ts(&self) -> Option<i64> {
        self.last_ts
    }

    // -- the public fold ---------------------------------------------------- //

    /// Fold one event (`projector.py:161-194`).
    ///
    /// `apply_fire` gates every per-topic FIRe write, so a resume replays history for
    /// its light indices without running FIRe again.
    ///
    /// A `regraded` event never arrives here: [`apply_regrades`] consumes it ahead of
    /// the fold and hands the corrected events over instead. `task_served`,
    /// `session_start`, `session_end`, `anki_card_created`, `config_changed`, and
    /// `curriculum_changed` carry no derived state, so they are no-ops.
    pub fn apply(&mut self, event: &Event, apply_fire: bool) {
        let ts = event.ts().micros();
        if self.last_ts.is_none_or(|last| ts > last) {
            self.last_ts = Some(ts);
        }

        match event {
            Event::Enrolled(body) => self.on_enrolled(body, apply_fire),
            Event::Attempt(body) => self.on_attempt(body, apply_fire),
            Event::LessonResult(body) => self.on_lesson_result(body, ts, apply_fire),
            Event::ReviewResult(body) => self.on_review_result(body, ts, apply_fire),
            Event::QuizResult(body) => self.on_quiz_result(body, ts, apply_fire),
            Event::RemediationTriggered(body) => self.on_remediation(body, ts),
            Event::DiagnosticAnswer(body) => self.on_diagnostic_answer(body),
            Event::DiagnosticPlaced(body) => self.on_diagnostic_placed(body, ts, apply_fire),
            Event::ProfileReset(body) => self.on_profile_reset(body, apply_fire),
            Event::SessionStart(_)
            | Event::SessionEnd(_)
            | Event::TaskServed(_)
            | Event::Regraded(_)
            | Event::AnkiCardCreated(_)
            | Event::ConfigChanged(_)
            | Event::CurriculumChanged(_) => {}
        }
    }

    // -- per-event handlers ------------------------------------------------- //

    /// `enrolled` (`projector.py:197-211`): remember the course, then stamp `floor` on
    /// every still-untouched topic of its mastery floor.
    fn on_enrolled(&mut self, event: &Enrolled, apply_fire: bool) {
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
    fn on_attempt(&mut self, event: &Attempt, apply_fire: bool) {
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
    fn on_lesson_result(&mut self, event: &LessonResult, ts: i64, apply_fire: bool) {
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

    /// `review_result` (`projector.py:244-254`). It changes no status: a review on an
    /// untouched topic leaves it untouched while it writes `repNum` and `t0`.
    fn on_review_result(&mut self, event: &ReviewResult, ts: i64, apply_fire: bool) {
        let topic = event.topic.as_str();
        self.xp_events.push((ts, event.xp));
        self.last_practice.insert(topic.to_owned(), ts);
        if !apply_fire {
            return;
        }
        self.apply_fire_result(topic, event.passed, event.quality_tier, ts, event.assisted);
    }

    /// `quiz_result` (`projector.py:256-282`). Every question applies FIRe, pass or
    /// miss; a row that names a topic outside the curriculum is skipped.
    fn on_quiz_result(&mut self, event: &QuizResult, ts: i64, apply_fire: bool) {
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
    fn on_remediation(&mut self, event: &RemediationTriggered, ts: i64) {
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
    fn on_diagnostic_answer(&mut self, event: &DiagnosticAnswer) {
        self.diag_answers
            .entry(event.topic.as_str().to_owned())
            .or_default()
            .push((event.correct, event.weight.get()));
    }

    /// `diagnostic_placed` (`projector.py:301-371`).
    ///
    /// The per-session answer tally is consumed AND reset UNCONDITIONALLY, ahead of the
    /// `apply_fire` gate, so a light replay and a full replay stay consistent.
    fn on_diagnostic_placed(&mut self, event: &DiagnosticPlaced, ts: i64, apply_fire: bool) {
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
            let ability = match diag_answers.get(&tid) {
                Some(answers) if !answers.is_empty() => {
                    let fresh = self.ability_from_answers(answers);
                    clamp01((old.ability + fresh) / 2.0)
                }
                _ => old.ability,
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
    fn on_profile_reset(&mut self, event: &ProfileReset, apply_fire: bool) {
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

    // -- the FIRe core ------------------------------------------------------ //

    /// One graded result on `topic` (`projector.py:486-542`).
    ///
    /// 1. Copy the states and read the current one.
    /// 2. FIRST-TOUCH SEED, only while the status is `untouched` AND the ability is
    ///    still the default 0: seed the ability from the neighborhood and set the
    ///    speed from it. The guard is NOT `t0 is None`: a lesson-fail stamp writes
    ///    `t0` on an untouched topic, and the seed must still run. The ability guard
    ///    also stops a repeat event from discarding an accumulated average.
    /// 3. Apply the ability deltas IN MAP ORDER — the attempted topic first, then the
    ///    neighbors by sorted id (trap T6) — clamping each to `[0, 1]` and computing
    ///    the speed again.
    /// 4. Run the memory, `repNum`, and interval update with the implicit propagation.
    fn apply_fire_result(
        &mut self,
        topic: &str,
        passed: bool,
        quality: WorkQuality,
        t_us: i64,
        assisted: bool,
    ) {
        let graph = self.graph;
        let cfg = self.cfg;
        let mut states = self.topics.clone();
        let mut current = states.get(topic).cloned().unwrap_or_default();
        if current.status == TopicStatus::Untouched && current.ability == 0.0 {
            let seed = initial_ability(topic, graph, &states, cfg);
            current.ability = seed;
            current.speed = speed_for(seed, difficulty(graph, topic), cfg);
        }
        states.insert(topic.to_owned(), current);

        let deltas = ability_update(&states, topic, passed, graph, cfg);
        for (other, delta) in deltas {
            let mut base = states.get(&other).cloned().unwrap_or_default();
            let new_ability = clamp01(base.ability + delta);
            base.ability = new_ability;
            base.speed = speed_for(new_ability, difficulty(graph, &other), cfg);
            states.insert(other, base);
        }

        let attempt = AttemptResult::new(topic, passed, quality).with_assisted(assisted);
        let (new_states, _props) = apply_attempt(&states, &attempt, graph, cfg, t_us);
        self.topics = new_states;
    }

    /// Stamp a lesson failure (`projector.py:544-568`).
    ///
    /// It writes `t0` and marks the knowledge point, with NO FIRe propagation: a
    /// never-learned topic must not penalize its untouched dependents. Every knowledge
    /// point BEFORE the failed one that carries no verdict yet becomes `passed`. The
    /// status does not change.
    fn mark_lesson_fail(&mut self, topic: &str, failed_at_kp: Option<&Slug>, t_us: i64) {
        let graph = self.graph;
        let mut state = self.topics.get(topic).cloned().unwrap_or_default();
        let mut kp_progress = state.kp_progress.clone();
        match (graph.idx_of(topic), failed_at_kp) {
            (Some(idx), Some(failed)) => {
                for kp in graph.knowledge_points(idx) {
                    if kp.id.as_str() == failed.as_str() {
                        break;
                    }
                    kp_progress
                        .entry(kp.id.as_str().to_owned())
                        .or_insert(KpProgress::Passed);
                }
                let prior = kp_progress.get(failed.as_str()).copied();
                let next = if matches!(
                    prior,
                    Some(KpProgress::FailedOnce | KpProgress::FailedTwice)
                ) {
                    KpProgress::FailedTwice
                } else {
                    KpProgress::FailedOnce
                };
                kp_progress.insert(failed.as_str().to_owned(), next);
            }
            (None, Some(failed)) => {
                kp_progress.insert(failed.as_str().to_owned(), KpProgress::FailedOnce);
            }
            (_, None) => {}
        }
        state.t0 = Some(Timestamp::from_micros(t_us));
        state.kp_progress = kp_progress;
        self.topics.insert(topic.to_owned(), state);
    }

    /// Mark every knowledge point of a passed lesson (`projector.py:570-579`).
    fn mark_kps_passed(&mut self, topic: &str) {
        let graph = self.graph;
        let Some(idx) = graph.idx_of(topic) else {
            return;
        };
        let mut state = self.topics.get(topic).cloned().unwrap_or_default();
        for kp in graph.knowledge_points(idx) {
            state
                .kp_progress
                .insert(kp.id.as_str().to_owned(), KpProgress::Passed);
        }
        self.topics.insert(topic.to_owned(), state);
    }

    /// Set a topic's status (`projector.py:581-584`).
    fn set_status(&mut self, topic: &str, status: TopicStatus) {
        let mut state = self.topics.get(topic).cloned().unwrap_or_default();
        if state.status != status {
            state.status = status;
            self.topics.insert(topic.to_owned(), state);
        }
    }

    // -- assembly ----------------------------------------------------------- //

    /// The still-open remediation queue (`projector.py:596-614`).
    ///
    /// A trigger stays open while a target has no practice at or after the trigger
    /// instant. The comparison is STRICTLY BEFORE. An entry with no open target is
    /// dropped, and a repeated `(kind, targets)` pair keeps its FIRST occurrence.
    #[must_use]
    pub fn pending_remediation(&self) -> Vec<PendingRemediation> {
        let mut out: Vec<PendingRemediation> = Vec::new();
        let mut seen: BTreeSet<(String, Vec<String>)> = BTreeSet::new();
        for (ts, kind, targets) in &self.remediation {
            let open: Vec<Slug> = targets
                .iter()
                .filter(|target| {
                    self.last_practice
                        .get(target.as_str())
                        .is_none_or(|practiced| practiced < ts)
                })
                .cloned()
                .collect();
            if open.is_empty() {
                continue;
            }
            let key = (
                kind.clone(),
                open.iter().map(|item| item.as_str().to_owned()).collect(),
            );
            if !seen.insert(key) {
                continue;
            }
            out.push(PendingRemediation {
                kind: kind.clone(),
                targets: open,
            });
        }
        out
    }

    /// The quiz cadence (`projector.py:616-625`).
    ///
    /// `xp_since` accumulates NAIVELY, the 1.0 `+=` (trap T2). Do not compensate it.
    /// `last_at` is the UTC calendar date of the last quiz, not a local one.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectorError::Time`] for an instant `chrono` does not represent.
    pub fn quiz_state(&self) -> Result<QuizState, ProjectorError> {
        let mut xp_since = 0.0_f64;
        for &(ts, xp) in &self.xp_events {
            if self.quiz_last_ts.is_none_or(|last| ts > last) {
                xp_since += xp;
            }
        }
        let last_at: Option<NaiveDate> = match self.quiz_last_ts {
            Some(ts) => Some(to_datetime(ts)?.date_naive()),
            None => None,
        };
        Ok(QuizState {
            last_at,
            xp_since: round_half_even_i64(xp_since),
            retake_pending: self.quiz_retake_pending,
        })
    }

    /// The XP totals against the reference instant (`projector.py:627-636`).
    ///
    /// The whole-log total is a `sum()` site, so it is compensated (trap T1); the
    /// per-day map is the naive loop of [`daily_totals`] (trap T2). Both integers come
    /// from Python `round()`, which is half-even (trap T3).
    ///
    /// # Errors
    ///
    /// Returns [`ProjectorError::Time`] for an instant `chrono` does not represent.
    pub fn xp_state(&self, t_ref: i64) -> Result<XpState, ProjectorError> {
        let total = self.total_xp();
        let daily = daily_totals(&self.xp_events, self.zone)?;
        let today = local_day_in(t_ref, self.zone)?;
        #[expect(
            clippy::cast_precision_loss,
            reason = "a daily XP goal is far below 2**53"
        )]
        let goal = self.goal as f64;
        Ok(XpState {
            total: round_half_even_i64(total),
            today: round_half_even_i64(daily.get(&today).copied().unwrap_or(0.0)),
            goal: self.goal,
            streak_days: current_streak(&daily, goal, today),
        })
    }

    /// The pace over the trailing window (`projector.py:638-651`).
    ///
    /// A learner with no `enrolled` event has a DEFAULT velocity: 1.0 returns
    /// `VelocityState()` before it computes anything.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectorError::Time`] for an instant `chrono` does not represent.
    pub fn velocity_state(&self, t_ref: i64) -> Result<VelocityState, ProjectorError> {
        let Some(course_id) = self.enrolled_course.as_deref() else {
            return Ok(VelocityState::default());
        };
        let input = VelocityInput {
            states: &self.topics,
            graph: self.graph,
            course_id: Some(course_id),
            xp_entries: &self.xp_events,
            completions: &self.completions,
            total_xp: self.total_xp(),
            t_us: t_ref,
            zone: self.zone,
            window_days: VELOCITY_WINDOW_DAYS,
        };
        Ok(compute_velocity_state(&input)?)
    }

    /// The whole-log XP total. A `sum()` site, so it is compensated (trap T1).
    fn total_xp(&self) -> f64 {
        let values: Vec<f64> = self.xp_events.iter().map(|&(_, xp)| xp).collect();
        neumaier_sum(&values)
    }

    /// Assemble the derived learner model (`projector.py:653-682`).
    ///
    /// `t_ref` is the LAST event's instant, or `now` on an empty log. Velocity,
    /// "today", and the streak read `t_ref`; only `built_from_ts` carries `now`.
    /// The emitted `topics` map is sorted by id and holds only the states that differ
    /// from a default one.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectorError::Time`] for an unrepresentable instant, or
    /// [`ProjectorError::Serialize`] when the config hash preimage does not build.
    pub fn finalize(&self, now: Timestamp) -> Result<LearnerModel, ProjectorError> {
        let t_ref = self.last_ts.unwrap_or_else(|| now.micros());
        let topics: BTreeMap<String, TopicState> = self
            .topics
            .iter()
            .filter(|(_, state)| !state.is_default())
            .map(|(tid, state)| (tid.clone(), state.clone()))
            .collect();
        let config_hash = self
            .cfg
            .config_hash()
            .map_err(|error| ProjectorError::Serialize(error.to_string()))?;
        Ok(LearnerModel {
            built_from_ts: Some(now),
            topics,
            xp: self.xp_state(t_ref)?,
            quiz: self.quiz_state()?,
            velocity: self.velocity_state(t_ref)?,
            pending_remediation: self.pending_remediation(),
            config_hash: Some(config_hash),
            projector_version: Some(PROJECTOR_VERSION),
            through_seq: None,
        })
    }
}

// --------------------------------------------------------------------------- //
// Regrades
// --------------------------------------------------------------------------- //

/// The stream with every `regraded` correction folded in (`projector.py:698-770`).
///
/// The event log is append-only, so a grade recorded in error is superseded by a later
/// event and never edited. This pre-pass does the substitution once, ahead of the fold,
/// and returns a stream in which the corrected values are simply THE values. No handler
/// ever sees a `regraded` event.
///
/// Two substitutions happen:
///
/// - an `attempt` whose `attempt_id` a correction names takes that correction's
///   `work_quality`, `error_tags` (replaced wholesale, order kept), and `grader_note`;
/// - the `lesson_result` or `review_result` that CLOSES the corrected task takes its
///   `quality_tier` and `xp`, when the correction carries them.
///
/// A `review_result` carries a `task_id` and matches on it. A `lesson_result` carries
/// none, so it matches the task of the MOST RECENT PRECEDING attempt on its topic —
/// exact by construction, because a lesson close follows that task's last attempt. A
/// result with no preceding attempt on its topic is left alone.
///
/// A later correction of the same target supersedes an earlier one. With no correction
/// in the stream the input comes back unchanged, so a second pass over the output is
/// the identity: [`apply_regrades`] is idempotent.
#[must_use]
pub fn apply_regrades(events: &[Event]) -> Vec<Event> {
    let mut by_attempt: BTreeMap<&str, &RegradedAttempt> = BTreeMap::new();
    let mut by_task: BTreeMap<&str, &Regraded> = BTreeMap::new();
    for event in events {
        let Event::Regraded(correction) = event else {
            continue;
        };
        // A later correction of the same target supersedes an earlier one.
        for corrected in &correction.attempts {
            by_attempt.insert(corrected.attempt_id.as_str(), corrected);
        }
        if correction.quality_tier.is_some() || correction.xp.is_some() {
            by_task.insert(correction.task_id.as_str(), correction);
        }
    }
    if by_attempt.is_empty() && by_task.is_empty() {
        return events.to_vec();
    }

    let mut out: Vec<Event> = Vec::with_capacity(events.len());
    // The task each topic's most recent attempt belonged to. It is the key that gives a
    // `lesson_result`, which carries no task id, the task it closes.
    let mut task_of_topic: BTreeMap<String, String> = BTreeMap::new();
    for event in events {
        match event {
            Event::Regraded(_) => continue,
            Event::Attempt(body) => {
                task_of_topic.insert(body.topic.as_str().to_owned(), body.task_id.clone());
                let mut fixed = body.clone();
                if let Some(correction) = by_attempt.get(fixed.attempt_id.as_str()) {
                    fixed.work_quality = correction.work_quality;
                    fixed.error_tags.clone_from(&correction.error_tags);
                    fixed.grader_note.clone_from(&correction.grader_note);
                }
                out.push(Event::Attempt(fixed));
            }
            Event::LessonResult(body) => {
                // A `lesson_result` has no `task_id` field at all.
                let task_id = task_of_topic.get(body.topic.as_str()).map(String::as_str);
                let mut fixed = body.clone();
                if let Some(correction) = task_id.and_then(|id| by_task.get(id)) {
                    if let Some(tier) = correction.quality_tier {
                        fixed.quality_tier = tier;
                    }
                    if let Some(xp) = correction.xp {
                        fixed.xp = xp;
                    }
                }
                out.push(Event::LessonResult(fixed));
            }
            Event::ReviewResult(body) => {
                // Python `event.task_id or task_of_topic.get(topic)`: an EMPTY task id
                // is falsy there, so it falls through to the preceding attempt.
                let task_id = body
                    .task_id
                    .as_deref()
                    .filter(|id| !id.is_empty())
                    .or_else(|| task_of_topic.get(body.topic.as_str()).map(String::as_str));
                let mut fixed = body.clone();
                if let Some(correction) = task_id.and_then(|id| by_task.get(id)) {
                    if let Some(tier) = correction.quality_tier {
                        fixed.quality_tier = tier;
                    }
                    if let Some(xp) = correction.xp {
                        fixed.xp = xp;
                    }
                }
                out.push(Event::ReviewResult(fixed));
            }
            other => out.push(other.clone()),
        }
    }
    out
}

// --------------------------------------------------------------------------- //
// Entry points
// --------------------------------------------------------------------------- //

/// The inputs both entry points share.
///
/// The 1.0 signature is `project(events, graph, cfg, now=, tz=, goal=)`
/// (`projector.py:773-791`); this struct carries the same five.
#[derive(Debug, Clone, Copy)]
pub struct ProjectionInput<'a> {
    /// The curriculum the fold reads.
    pub graph: &'a Curriculum,
    /// The scheduler config the fold reads.
    pub cfg: &'a Config,
    /// The wall-clock instant of the BUILD. Only `built_from_ts` carries it.
    pub now: Timestamp,
    /// The time zone of the day boundary. `None` means UTC.
    pub tz: Option<&'a str>,
    /// The daily XP goal the streak compares against.
    pub goal: i64,
}

impl<'a> ProjectionInput<'a> {
    /// The 1.0 defaults: UTC and a goal of 40.
    #[must_use]
    pub const fn new(graph: &'a Curriculum, cfg: &'a Config, now: Timestamp) -> Self {
        Self {
            graph,
            cfg,
            now,
            tz: None,
            goal: DEFAULT_XP_GOAL,
        }
    }

    /// Set the time zone name.
    #[must_use]
    pub const fn with_timezone(mut self, tz: Option<&'a str>) -> Self {
        self.tz = tz;
        self
    }

    /// Set the daily XP goal.
    #[must_use]
    pub const fn with_goal(mut self, goal: i64) -> Self {
        self.goal = goal;
        self
    }
}

/// Full replay (`projector.py:773-791`): fold every event with FIRe applied.
///
/// # Errors
///
/// Returns [`ProjectorError`] for an unknown time zone, an unrepresentable instant, or
/// a config that does not serialize.
pub fn project(
    events: &[Event],
    input: &ProjectionInput<'_>,
) -> Result<LearnerModel, ProjectorError> {
    let mut proj = Projector::new(input.graph, input.cfg)
        .with_timezone(input.tz)?
        .with_goal(input.goal);
    for event in apply_regrades(events) {
        proj.apply(&event, true);
    }
    proj.finalize(input.now)
}

/// Incremental projection (`projector.py:794-832`).
///
/// The FIRe topic states are seeded from `cached`, the prior events replay for their
/// light indices only, and the new events apply with FIRe. The result equals [`project`]
/// over the whole stream without running the FIRe math on every earlier event.
///
/// Corrections fold over the WHOLE stream, never over each half: a `regraded` event in
/// `new` supersedes grades in `prior`, and the prior half is not inert, because its
/// result XP is tallied with FIRe off. The halves are then split again on the corrected
/// stream, which is shorter by exactly the corrections it consumed.
///
/// # Errors
///
/// Returns [`ProjectorError`] for an unknown time zone, an unrepresentable instant, or
/// a config that does not serialize.
pub fn project_incremental(
    cached: &LearnerModel,
    prior: &[Event],
    new: &[Event],
    input: &ProjectionInput<'_>,
) -> Result<LearnerModel, ProjectorError> {
    let mut whole: Vec<Event> = Vec::with_capacity(prior.len() + new.len());
    whole.extend_from_slice(prior);
    whole.extend_from_slice(new);
    let corrected = apply_regrades(&whole);
    let fresh = new
        .iter()
        .filter(|event| !matches!(event, Event::Regraded(_)))
        .count();
    let split = corrected.len().saturating_sub(fresh);

    let mut proj = Projector::new(input.graph, input.cfg)
        .with_timezone(input.tz)?
        .with_goal(input.goal)
        .with_cached_topics(cached.topics.clone());
    for (position, event) in corrected.iter().enumerate() {
        proj.apply(event, position >= split);
    }
    proj.finalize(input.now)
}

// --------------------------------------------------------------------------- //
// The parity blob
// --------------------------------------------------------------------------- //

/// The bytes the parity oracle compares (spec section 9).
///
/// Canonical JSON of the model with `built_from_ts` REMOVED, because that field alone
/// carries wall clock (trap T10): sorted keys, compact separators, non-ASCII text
/// unescaped, and NO trailing newline. 1.0 builds the same bytes with
/// `json.dumps(payload, sort_keys=True, separators=(",",":"), ensure_ascii=False)`.
///
/// A float takes the Python `repr` text through [`python_repr_f64`], not the
/// `serde_json` text: the two spell an exponent differently, so `1e-05` reads `1e-5`
/// there and the digest diverges. A `serde_json` map is a `BTreeMap`, so its keys
/// come out in Rust `String` order, which is byte order, which equals the Python
/// code-point order for UTF-8 (trap T18).
///
/// # Errors
///
/// Returns [`ProjectorError::Serialize`] when the model does not serialize, which
/// happens only for a timestamp outside the representable range.
pub fn canonical_blob(model: &LearnerModel) -> Result<String, ProjectorError> {
    let mut value = serde_json::to_value(model)
        .map_err(|error| ProjectorError::Serialize(error.to_string()))?;
    if let Some(object) = value.as_object_mut() {
        object.remove("built_from_ts");
    }
    let mut out = String::new();
    render_json(&value, &mut out);
    Ok(out)
}

/// The lowercase hex SHA-256 of [`canonical_blob`].
///
/// # Errors
///
/// Returns [`ProjectorError::Serialize`] when the blob does not build.
pub fn blob_digest(model: &LearnerModel) -> Result<String, ProjectorError> {
    Ok(sha256_hex(canonical_blob(model)?.as_bytes()))
}

/// Write one JSON value the way `json.dumps` with compact separators writes it.
fn render_json(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => {
            if number.is_f64() {
                match number.as_f64() {
                    Some(item) => out.push_str(&python_repr_f64(item)),
                    None => out.push_str("null"),
                }
            } else {
                out.push_str(&number.to_string());
            }
        }
        Value::String(text) => out.push_str(&json_string(text)),
        Value::Array(items) => {
            out.push('[');
            for (position, item) in items.iter().enumerate() {
                if position > 0 {
                    out.push(',');
                }
                render_json(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (position, (key, item)) in map.iter().enumerate() {
                if position > 0 {
                    out.push(',');
                }
                out.push_str(&json_string(key));
                out.push(':');
                render_json(item, out);
            }
            out.push('}');
        }
    }
}

/// One JSON string, escaped the way `json.dumps(ensure_ascii=False)` escapes it.
fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_default()
}
