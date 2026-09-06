//! Evidence-aware review decisions (D-F7).
use super::grade_review;
use crate::config::Config;
use crate::event::Attempt;
use std::collections::{BTreeMap, BTreeSet};

/// A review decision and its unresolved skills.
#[derive(Debug, PartialEq)]
pub struct ReviewEvidence {
    /// True only after consistent, independent evidence.
    pub passed: bool,
    /// The positional score of decided answers.
    pub score: f64,
    /// True when more evidence is necessary.
    pub inconclusive: bool,
    /// Stable topic/KP keys for one direct confirmation each.
    pub confirmation_skills: Vec<String>,
}

/// Assess the aggregate trajectory and each exercised skill.
#[must_use]
pub fn assess_review(attempts: &[&Attempt], cfg: &Config) -> ReviewEvidence {
    let decided: Vec<bool> = attempts
        .iter()
        .filter(|a| !a.outcome.is_ungraded())
        .map(|a| a.correct && !a.assisted)
        .collect();
    let (passed, score) = grade_review(&decided, cfg);
    let last = decided.last().copied().unwrap_or(false);
    let conflict = decided.is_empty() || (score >= cfg.review.pass_weighted) != last;
    let mut skills = BTreeSet::new();
    for attempt in attempts {
        if conflict || attempt.outcome.is_ungraded() || attempt.assisted {
            skills.extend(attempt.skills.iter().cloned());
        }
    }
    let mut by_skill: BTreeMap<&str, Vec<bool>> = BTreeMap::new();
    for attempt in attempts
        .iter()
        .filter(|a| !a.outcome.is_ungraded() && !a.assisted)
    {
        for skill in &attempt.skills {
            by_skill.entry(skill).or_default().push(attempt.correct);
        }
    }
    for (skill, answers) in by_skill {
        let (skill_pass, skill_score) = grade_review(&answers, cfg);
        if (passed && !skill_pass)
            || ((skill_score >= cfg.review.pass_weighted)
                != answers.last().copied().unwrap_or(false))
        {
            skills.insert(skill.to_owned());
        }
    }
    ReviewEvidence {
        passed: passed && skills.is_empty() && !conflict,
        score,
        inconclusive: conflict || !skills.is_empty(),
        confirmation_skills: skills.into_iter().collect(),
    }
}
