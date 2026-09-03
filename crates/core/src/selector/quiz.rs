//! Quiz composition and cadence (PEDAGOGY 7, `selector.py:653-838`).

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Days, NaiveDate};

use crate::config::Config;
use crate::curriculum::Curriculum;
use crate::fire::has_review_history;
use crate::learner::{QuizState, TopicState};
use crate::numeric::{round_half_even_i64_saturating, to_datetime};

use super::review::float_then_id;
use super::{
    DAY_US, DIFFICULTY_TARGET, QUIZ_DIFFICULTY_TARGETS, QUIZ_MID_TARGET, QUIZ_RECENT_DAYS,
    QUIZ_RECENT_TARGET, QUIZ_RETAKE_DELAY_DAYS, QUIZ_TIME_FACTOR,
};

/// The sampler the quiz composer draws with.
///
/// 1.0 draws with `random.Random.sample`, the CPython Mersenne-Twister algorithm
/// (trap T11). 2.0 does NOT reproduce that sequence, so the draw lives behind
/// this trait: a caller supplies [`SeededSampler`], or a fixed sampler in a test,
/// or a replay sampler that re-serves the ids a `task_served` event recorded.
pub trait QuizSampler {
    /// Draw `k` distinct members of `population`, in draw order.
    ///
    /// The implementation returns `k` members when `k <= population.len()`, and
    /// never more than `population.len()`.
    fn sample(&mut self, population: &[String], k: usize) -> Vec<String>;
}

/// The default seeded sampler of 2.0.
///
/// It is a SplitMix64 generator behind a partial Fisher-Yates shuffle. The draw
/// is a pure function of the seed and the population, so a replay with the same
/// seed draws the same ids. It is NOT the CPython `random.sample` sequence
/// (trap T11), and it does not try to be: the sampled ids go on the
/// `task_served` event, and a replay reads them from the log.
#[derive(Debug, Clone)]
pub struct SeededSampler {
    /// The generator state.
    state: u64,
}

/// The odd increment of SplitMix64.
const SPLITMIX_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

impl SeededSampler {
    /// A sampler seeded with `seed`.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64 raw bits (SplitMix64).
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(SPLITMIX_GAMMA);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `bound`. The draw always asks with `bound` at least one.
    fn below(&mut self, bound: usize) -> usize {
        let scaled = (u128::from(self.next_u64()) * (bound as u128)) >> 64;
        usize::try_from(scaled)
            .unwrap_or(0)
            .min(bound.saturating_sub(1))
    }
}

impl QuizSampler for SeededSampler {
    fn sample(&mut self, population: &[String], k: usize) -> Vec<String> {
        let mut pool: Vec<String> = population.to_vec();
        let take = k.min(pool.len());
        let mut out: Vec<String> = Vec::with_capacity(take);
        for index in 0..take {
            let pick = index + self.below(pool.len() - index);
            pool.swap(index, pick);
            out.extend(pool.get(index).cloned());
        }
        out
    }
}

/// One stratified quiz question (`QuizQuestion`, `selector.py:653-658`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuizQuestion {
    /// The topic the question asks about.
    pub topic: String,
    /// The timed budget: the authored expected time times 1.5, rounded.
    pub time_budget_secs: i64,
    /// The stratum the topic came from: `recent`, `mid`, or `old`.
    pub stratum: &'static str,
}

/// A composed quiz (`QuizPlan`, `selector.py:661-673`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuizPlan {
    /// The questions, in draw order.
    pub questions: Vec<QuizQuestion>,
}

impl QuizPlan {
    /// The sampled topic ids, in draw order.
    #[must_use]
    pub fn topics(&self) -> Vec<&str> {
        self.questions
            .iter()
            .map(|question| question.topic.as_str())
            .collect()
    }

    /// The total timed budget of the quiz.
    #[must_use]
    pub fn total_time_budget_secs(&self) -> i64 {
        self.questions.iter().fold(0_i64, |total, question| {
            total.saturating_add(question.time_budget_secs)
        })
    }
}

/// An authored integer as a float. An authored count never leaves the exact
/// integer range of an `f64`.
#[expect(
    clippy::cast_precision_loss,
    reason = "an authored count never leaves the exact f64 integer range"
)]
pub(super) const fn i64_as_float(count: i64) -> f64 {
    count as f64
}

/// The per-question time budget: the authored expected time times 1.5, rounded
/// half to even (`_quiz_budget`, `selector.py:676-678`).
///
/// The input is an authored count, which the `i64` range holds, so the budget uses
/// the saturating rounding form and reports no error.
#[must_use]
pub fn quiz_budget(graph: &Curriculum, tid: &str) -> i64 {
    let seconds = graph
        .idx_of(tid)
        .and_then(|idx| graph.topic(idx))
        .map_or(0, |topic| topic.expected_time_secs);
    round_half_even_i64_saturating(i64_as_float(seconds) * QUIZ_TIME_FACTOR)
}

/// A microsecond delta as seconds, the way `timedelta.total_seconds` reads it.
#[expect(
    clippy::cast_precision_loss,
    reason = "the age key only orders topics, and 1.0 reads the same float"
)]
fn micros_as_seconds(delta_us: i64) -> f64 {
    delta_us as f64 / 1_000_000.0
}

/// Compose the stratified quiz of PEDAGOGY 7 (`quiz_composer`, `selector.py:681-754`).
///
/// The strata are 4 recent, 2 mid, and 2 old, scaled down when the learner knows
/// fewer topics than the quiz needs. A short stratum backfills from the rest, so
/// the quiz reaches its full length whenever enough topics are learned.
///
/// `learned_at` maps a topic id to the UTC microseconds it was first mastered.
/// Without it, recency falls back to the `repNum` proxy of 1.0 and no topic
/// counts as recent.
#[must_use]
pub fn quiz_composer(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    sampler: &mut dyn QuizSampler,
    learned_at: Option<&BTreeMap<String, i64>>,
) -> QuizPlan {
    let learned: Vec<String> = states
        .iter()
        .filter(|(id, state)| graph.idx_of(id).is_some() && has_review_history(state))
        .map(|(id, _)| id.clone())
        .collect();
    if learned.is_empty() {
        return QuizPlan::default();
    }
    let (recent, mid_pool, old_pool) = strata(&learned, states, t_us, learned_at);

    let questions = usize::try_from(cfg.quiz.questions).unwrap_or(0);
    let total = questions.min(learned.len());
    let want_recent = QUIZ_RECENT_TARGET.min(total);
    let want_mid = QUIZ_MID_TARGET.min(total - want_recent);
    let want_old = total - want_recent - want_mid;

    let mut picked: Vec<(String, &'static str)> = Vec::new();
    let mut used: BTreeSet<String> = BTreeSet::new();
    take_stratum(
        sampler,
        &recent,
        want_recent,
        "recent",
        &mut picked,
        &mut used,
    );
    take_stratum(sampler, &mid_pool, want_mid, "mid", &mut picked, &mut used);
    take_stratum(sampler, &old_pool, want_old, "old", &mut picked, &mut used);

    if picked.len() < total {
        let mut leftover: Vec<String> = learned
            .iter()
            .filter(|id| !used.contains(*id))
            .cloned()
            .collect();
        leftover.sort_unstable();
        let need = total - picked.len();
        take_stratum(sampler, &leftover, need, "mid", &mut picked, &mut used);
    }

    QuizPlan {
        questions: picked
            .into_iter()
            .map(|(tid, stratum)| QuizQuestion {
                time_budget_secs: quiz_budget(graph, &tid),
                topic: tid,
                stratum,
            })
            .collect(),
    }
}

/// The three strata of the learned topics: recent, mid and old, each sorted
/// oldest first (`selector.py:700-731`).
///
/// `learned_at` dates a topic; without it, recency falls back to the `repNum`
/// proxy of 1.0 and no topic counts as recent. The older half of the
/// non-recent topics is the old stratum and the newer half the mid stratum.
fn strata(
    learned: &[String],
    states: &BTreeMap<String, TopicState>,
    t_us: i64,
    learned_at: Option<&BTreeMap<String, i64>>,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let age_key = |tid: &str| -> (f64, String) {
        match learned_at.and_then(|map| map.get(tid).copied()) {
            // 1.0 keys on `-(t - learned_at).total_seconds()`, so an older topic
            // sorts first.
            Some(at) => (-micros_as_seconds(t_us.saturating_sub(at)), tid.to_owned()),
            None => (
                -states.get(tid).map_or(0.0, |state| state.rep_num),
                tid.to_owned(),
            ),
        }
    };
    let is_recent = |tid: &str| -> bool {
        learned_at
            .and_then(|map| map.get(tid).copied())
            .is_some_and(|at| t_us.saturating_sub(at) <= QUIZ_RECENT_DAYS * DAY_US)
    };

    let (mut recent, mut older): (Vec<String>, Vec<String>) =
        learned.iter().cloned().partition(|id| is_recent(id));
    recent.sort_by(|left, right| float_then_id(&age_key(left), &age_key(right)));
    older.sort_by(|left, right| float_then_id(&age_key(left), &age_key(right)));

    let half = older.len() / 2;
    let mid_pool = older.split_off(half);
    (recent, mid_pool, older)
}

/// Draw one stratum of the quiz (`take`, `selector.py:736-741`).
fn take_stratum(
    sampler: &mut dyn QuizSampler,
    pool: &[String],
    k: usize,
    stratum: &'static str,
    picked: &mut Vec<(String, &'static str)>,
    used: &mut BTreeSet<String>,
) {
    let available: Vec<String> = pool
        .iter()
        .filter(|id| !used.contains(*id))
        .cloned()
        .collect();
    let k = k.min(available.len());
    if k == 0 {
        return;
    }
    for tid in sampler.sample(&available, k) {
        used.insert(tid.clone());
        picked.push((tid, stratum));
    }
}

/// The quiz difficulty target for a high-score streak
/// (`quiz_difficulty_target`, `selector.py:757-770`).
///
/// The streak is clamped into the band range, so any value is safe.
#[must_use]
pub fn quiz_difficulty_target(high_score_streak: i64) -> &'static str {
    let last = QUIZ_DIFFICULTY_TARGETS.len().saturating_sub(1);
    let index = usize::try_from(high_score_streak.max(0))
        .unwrap_or(last)
        .min(last);
    QUIZ_DIFFICULTY_TARGETS
        .get(index)
        .copied()
        .unwrap_or(DIFFICULTY_TARGET)
}

/// The date a failed quiz becomes re-eligible
/// (`quiz_retake_available_at`, `selector.py:773-786`).
///
/// `None` when no retake is pending, or when no last-quiz date is recorded.
#[must_use]
pub fn quiz_retake_available_at(quiz_state: Option<&QuizState>) -> Option<NaiveDate> {
    let state = quiz_state?;
    if !state.retake_pending {
        return None;
    }
    let days = u64::try_from(QUIZ_RETAKE_DELAY_DAYS).unwrap_or(0);
    state.last_at?.checked_add_days(Days::new(days))
}

/// The UTC calendar date of an instant, the way 1.0 reads `t.date()` off a naive
/// UTC datetime (trap T8). `None` when the instant leaves the representable range.
#[must_use]
pub fn utc_date(t_us: i64) -> Option<NaiveDate> {
    to_datetime(t_us).ok().map(|when| when.date_naive())
}

/// Whether a quiz is due (`quiz_is_due`, `selector.py:789-838`).
///
/// A pending retake supersedes the cadence: it becomes due only after the retake
/// delay. `active_study_days` makes the day cadence advance on the days the
/// learner actually studied; without it the cadence counts calendar days.
#[must_use]
pub fn quiz_is_due(
    quiz_state: Option<&QuizState>,
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    active_study_days: Option<&[NaiveDate]>,
) -> bool {
    let learned = states
        .iter()
        .filter(|(id, state)| graph.idx_of(id).is_some() && has_review_history(state))
        .count();
    if i64::try_from(learned).unwrap_or(i64::MAX) < cfg.quiz.questions {
        return false;
    }
    let Some(state) = quiz_state else {
        return true;
    };
    let Some(today) = utc_date(t_us) else {
        return false;
    };
    if state.retake_pending {
        return quiz_retake_available_at(Some(state)).is_none_or(|at| today >= at);
    }
    let Some(last_at) = state.last_at else {
        return true;
    };
    if state.xp_since >= cfg.quiz.cadence_xp {
        return true;
    }
    if let Some(days) = active_study_days {
        let active: BTreeSet<NaiveDate> = days
            .iter()
            .copied()
            .filter(|day| last_at < *day && *day <= today)
            .collect();
        return i64::try_from(active.len()).unwrap_or(i64::MAX) >= cfg.quiz.cadence_days;
    }
    (today - last_at).num_days() >= cfg.quiz.cadence_days
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fire::testing::{T_US, graph, learned, topic};

    #[test]
    fn the_quiz_draws_its_strata_and_reads_its_cadence() {
        let cfg = Config::default();
        let ids: Vec<String> = (0..9).map(|index| format!("t{index}")).collect();
        let tree = graph(ids.iter().map(|id| topic(id, &[])).collect());
        let states: BTreeMap<String, TopicState> =
            ids.iter().map(|id| (id.clone(), learned(0.8))).collect();
        let learned_at: BTreeMap<String, i64> = ids
            .iter()
            .enumerate()
            .map(|(index, id)| {
                (
                    id.clone(),
                    T_US - i64::try_from(index).expect("small") * 4 * DAY_US,
                )
            })
            .collect();
        let mut sampler = SeededSampler::new(7);
        let plan = quiz_composer(&states, &tree, &cfg, T_US, &mut sampler, Some(&learned_at));
        assert_eq!(plan.questions.len(), 8);
        assert_eq!(plan.topics().len(), 8);
        assert_eq!(plan.total_time_budget_secs(), 8 * 45);
        let recent = plan
            .questions
            .iter()
            .filter(|q| q.stratum == "recent")
            .count();
        assert_eq!(recent, 4);
        let by_rep = quiz_composer(&states, &tree, &cfg, T_US, &mut sampler, None);
        assert!(by_rep.questions.iter().all(|q| q.stratum != "recent"));
        assert_eq!(sampler.sample(&ids, 20).len(), 9);

        assert_eq!(quiz_difficulty_target(1), QUIZ_DIFFICULTY_TARGETS[1]);
        let quiet = QuizState {
            last_at: utc_date(T_US),
            xp_since: 0,
            retake_pending: false,
        };
        assert!(!quiz_is_due(Some(&quiet), &states, &tree, &cfg, T_US, None));
        assert!(quiz_is_due(
            Some(&quiet),
            &states,
            &tree,
            &cfg,
            T_US + 9 * DAY_US,
            None
        ));
        let days = [utc_date(T_US + DAY_US).expect("a date")];
        assert!(!quiz_is_due(
            Some(&quiet),
            &states,
            &tree,
            &cfg,
            T_US + 9 * DAY_US,
            Some(&days)
        ));
        assert!(quiz_is_due(None, &states, &tree, &cfg, T_US, None));
        let retake = QuizState {
            retake_pending: true,
            ..quiet
        };
        assert_eq!(
            quiz_retake_available_at(Some(&retake)),
            utc_date(T_US + DAY_US)
        );
        assert!(!quiz_is_due(
            Some(&retake),
            &states,
            &tree,
            &cfg,
            T_US,
            None
        ));
        assert!(!quiz_is_due(
            Some(&retake),
            &states,
            &tree,
            &cfg,
            i64::MAX,
            None
        ));
        assert_eq!(quiz_budget(&tree, "ghost"), 0);
    }
}
