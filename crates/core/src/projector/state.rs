//! The FIRe core of one graded result and the assembly of the derived model
//! (`projector.py:486-682`).

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;

use crate::event::{KpProgress, Slug, Timestamp, TopicStatus, WorkQuality};
use crate::fire::{
    AttemptResult, ability_update, apply_attempt_checked, difficulty, initial_ability, speed_for,
};
use crate::learner::{
    LearnerModel, PendingRemediation, QuizState, TopicState, VelocityState, XpState,
};
use crate::numeric::{local_day_in, neumaier_sum, round_half_even_i64, to_datetime};
use crate::xp::{
    VELOCITY_WINDOW_DAYS, VelocityInput, compute_velocity_state, current_streak, daily_totals,
};

use super::{PROJECTOR_VERSION, Projector, ProjectorError, clamp01};

impl Projector<'_> {
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
    pub(super) fn apply_fire_result(
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
        match apply_attempt_checked(&states, &attempt, graph, cfg, t_us) {
            Ok((new_states, _props)) => self.topics = new_states,
            Err(error) => {
                self.failure = Some(ProjectorError::NonFinite {
                    topic: error.topic,
                    event_index: self.applied,
                });
            }
        }
    }

    /// Stamp a lesson failure (`projector.py:544-568`).
    ///
    /// It writes `t0` and marks the knowledge point, with NO FIRe propagation: a
    /// never-learned topic must not penalize its untouched dependents. Every knowledge
    /// point BEFORE the failed one that carries no verdict yet becomes `passed`. The
    /// status does not change.
    pub(super) fn mark_lesson_fail(&mut self, topic: &str, failed_at_kp: Option<&Slug>, t_us: i64) {
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
    pub(super) fn mark_kps_passed(&mut self, topic: &str) {
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
    pub(super) fn set_status(&mut self, topic: &str, status: TopicStatus) {
        let mut state = self.topics.get(topic).cloned().unwrap_or_default();
        if state.status != status {
            state.status = status;
            self.topics.insert(topic.to_owned(), state);
        }
    }

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
                    let key = kind
                        .strip_prefix("review_confirmation:")
                        .map_or_else(|| target.as_str().to_owned(), |kp| format!("{target}/{kp}"));
                    self.last_practice
                        .get(&key)
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
        for (_, skill, due) in &self.feedback_confirmations {
            if self.completed_tasks < *due {
                continue;
            }
            if let Some((topic, kp)) = skill.split_once('/')
                && let Ok(topic) = Slug::new(topic)
            {
                let kind = format!("review_confirmation:{kp}");
                if seen.insert((kind.clone(), vec![topic.as_str().to_owned()])) {
                    out.push(PendingRemediation {
                        kind,
                        targets: vec![topic],
                    });
                }
            }
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
    /// Returns [`ProjectorError::Time`] for an instant `chrono` does not represent, or
    /// [`ProjectorError::OutOfRange`] for an `xp_since` outside the `i64` range.
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
            xp_since: round_half_even_i64(xp_since)?,
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
    /// Returns [`ProjectorError::Time`] for an instant `chrono` does not represent, or
    /// [`ProjectorError::OutOfRange`] for a total outside the `i64` range.
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
            total: round_half_even_i64(total)?,
            today: round_half_even_i64(daily.get(&today).copied().unwrap_or(0.0))?,
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
            cfg: self.cfg,
        };
        compute_velocity_state(&input).map_err(ProjectorError::Time)
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
    /// from a default one. The three tallies assemble in the order velocity, quiz,
    /// XP, and the first one that fails is the error.
    ///
    /// # Errors
    ///
    /// Returns the failure the fold met ([`ProjectorError::NonFinite`]), or
    /// [`ProjectorError::Time`] for an unrepresentable instant,
    /// [`ProjectorError::OutOfRange`] for an XP total outside the `i64` range, or
    /// [`ProjectorError::Serialize`] when the config hash preimage does not build.
    pub fn finalize(&self, now: Timestamp) -> Result<LearnerModel, ProjectorError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let t_ref = self.last_ts.unwrap_or_else(|| now.micros());
        let velocity = self.velocity_state(t_ref)?;
        let quiz = self.quiz_state()?;
        let xp = self.xp_state(t_ref)?;
        let topics: BTreeMap<String, TopicState> = self
            .topics
            .iter()
            .filter(|(_, state)| !state.is_default())
            .map(|(tid, state)| (tid.clone(), state.clone()))
            .collect();
        let config_hash = self.cfg.config_hash().map_err(serialize_error);
        config_hash.map(|config_hash| LearnerModel {
            built_from_ts: Some(now),
            topics,
            xp,
            quiz,
            velocity,
            pending_remediation: self.pending_remediation(),
            ungraded: self.ungraded.clone(),
            retention: self.retention.clone(),
            config_hash: Some(config_hash),
            projector_version: Some(PROJECTOR_VERSION),
            through_seq: None,
        })
    }
}

/// The error of a config hash or a model that does not serialize.
pub(super) fn serialize_error(error: serde_json::Error) -> ProjectorError {
    ProjectorError::Serialize(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::super::tests::{ev, tree};
    use super::*;
    use crate::config::Config;
    use crate::event::{Event, QuizResult, ReviewResult, SchemaVersion, SessionStart, TopicStatus};
    use crate::fire::testing::{DAY_US, T_US};
    use crate::numeric::{OutOfRangeError, TimeError};

    /// A quiz result at `ts_us` with no question and no XP.
    fn quiz_at(ts_us: i64) -> Event {
        Event::QuizResult(QuizResult {
            inconclusive: false,
            ts: Timestamp::from_micros(ts_us),
            session: None,
            v: SchemaVersion::current(),
            quiz_id: "z".to_owned(),
            score: 1.0,
            per_topic: Vec::new(),
            xp: 0.0,
        })
    }

    /// A passed review of `q` at `ts_us` worth `xp`.
    fn review_at(ts_us: i64, xp: f64) -> Event {
        Event::ReviewResult(ReviewResult {
            ts: Timestamp::from_micros(ts_us),
            session: None,
            v: SchemaVersion::current(),
            topic: Slug::new("q").expect("a slug"),
            passed: true,
            weighted_score: 1.0,
            xp,
            quality_tier: WorkQuality::Perfect,
            assisted: false,
            task_id: None,
            inconclusive: false,
            confirmation_skills: Vec::new(),
        })
    }

    /// The `OutOfRange` error of a rounded `-1e308`.
    fn huge_negative() -> ProjectorError {
        ProjectorError::OutOfRange(OutOfRangeError {
            value: "-1e+308".to_owned(),
        })
    }

    #[test]
    fn the_assembly_reports_the_first_tally_that_fails() {
        let tree = tree();
        let cfg = Config::default();
        let now = Timestamp::from_micros(0);
        let mut quiz = Projector::new(&tree, &cfg);
        quiz.apply(&quiz_at(i64::MIN), true);
        let too_early = ProjectorError::Time(TimeError::TimestampOutOfRange(i64::MIN));
        assert_eq!(quiz.quiz_state().unwrap_err(), too_early);
        assert_eq!(quiz.finalize(now).unwrap_err(), too_early);

        let mut velocity = Projector::new(&tree, &cfg);
        velocity.apply(
            &ev(r#"{"type":"enrolled","ts":"2026-07-14T12:00:00Z","course":"c"}"#),
            true,
        );
        assert!(velocity.velocity_state(i64::MAX).is_err());
        assert!(velocity.velocity_state(0).is_ok());
        velocity.apply(
            &Event::SessionStart(SessionStart {
                ts: Timestamp::from_micros(i64::MAX),
                session: None,
                v: SchemaVersion::current(),
            }),
            true,
        );
        assert!(velocity.finalize(now).is_err());

        let mut huge = Projector::new(&tree, &cfg);
        huge.apply(&review_at(T_US, -1e308), true);
        assert_eq!(huge.quiz_state().unwrap_err(), huge_negative());
        assert!(huge.xp_state(i64::MAX).is_err());
        assert_eq!(huge.xp_state(0).unwrap_err(), huge_negative());

        // An XP record with no calendar day fails the daily totals, and the
        // fold as a whole, once the quiz and the velocity pass.
        let mut early = Projector::new(&tree, &cfg);
        early.apply(&review_at(i64::MIN, 1.0), true);
        assert_eq!(early.xp_state(0).unwrap_err(), too_early);
        assert!(early.finalize(now).is_err());

        // A decay outside the finite range stops the fold, and the assembly
        // reports that failure ahead of every tally.
        let mut broken = Projector::new(&tree, &cfg);
        broken.apply(
            &ev(r#"{"type":"lesson_result","ts":"2060-01-01T09:00:00Z","topic":"q","passed":true,"quality_tier":"perfect"}"#),
            true,
        );
        broken.apply(&review_at(T_US, 1.0), true);
        assert_eq!(
            broken.finalize(now).unwrap_err(),
            ProjectorError::NonFinite {
                topic: "q".to_owned(),
                event_index: 1,
            }
        );

        // A finite whole-log total beside a reference day out of range.
        let mut today = Projector::new(&tree, &cfg);
        today.apply(&review_at(T_US, 1e308), true);
        today.apply(&review_at(T_US + DAY_US, -1e308), true);
        today.apply(&quiz_at(T_US + DAY_US + 1), true);
        assert_eq!(today.xp_state(T_US + DAY_US).unwrap_err(), huge_negative());
    }

    #[test]
    fn a_result_seeds_an_untouched_topic_once_and_stamps_a_failed_lesson() {
        let tree = tree();
        let cfg = Config::default();
        let mut proj = Projector::new(&tree, &cfg);
        proj.apply_fire_result("q", true, WorkQuality::Perfect, 0, false);
        proj.apply_fire_result("q", false, WorkQuality::Poor, 1, false);
        assert!(proj.topics()["q"].ability > 0.0);
        proj.mark_lesson_fail("ghost", None, 2);
        proj.mark_lesson_fail("q", Some(&Slug::new("kp1").expect("a slug")), 3);
        assert_eq!(
            proj.topics()["q"].kp_progress["kp1"],
            KpProgress::FailedOnce
        );
        proj.mark_kps_passed("q");
        proj.set_status("q", TopicStatus::Learning);
        proj.set_status("q", TopicStatus::Learning);
        assert_eq!(proj.topics()["q"].kp_progress["kp1"], KpProgress::Passed);
        assert!(proj.pending_remediation().is_empty());
    }
}
