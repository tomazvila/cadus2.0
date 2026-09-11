//! The store half of the audit: the curriculum facts, plus the approved
//! documents, give the [`Readiness`] of every knowledge point.

use std::collections::BTreeMap;
use std::fmt;

use crate::instruction::{KIND_HINT_LADDER, KIND_TEACH};
use crate::pool::kp_key;

use super::facts::{KpFacts, ReadinessIndex};
use super::{Blocker, ContentIndex, PRACTICE_MINIMUM, Readiness};

/// The eligibility question the selector asks (D-F5).
///
/// The selector never builds a readiness set of its own: it reads this trait,
/// so a test hands it a fake and the web hands it the real one.
pub trait ReadinessGate: fmt::Debug {
    /// The blockers that stop a LESSON at this topic and knowledge point. An
    /// empty answer means the lesson serves.
    fn lesson_blockers(&self, topic_id: &str, kp_id: &str) -> Vec<Blocker>;

    /// Whether the topic has at least one practicable knowledge point. A review
    /// and a quiz need this and nothing more.
    fn topic_practicable(&self, topic_id: &str) -> bool;
}

/// The readiness of every knowledge point of one curriculum, at one moment.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReadinessSet {
    /// Serving key to its readiness, in key order.
    per_kp: BTreeMap<String, Readiness>,
    /// Topic id to its serving keys, in authored order.
    topic_keys: BTreeMap<String, Vec<String>>,
}

impl ReadinessIndex {
    /// Add the approved documents of the store and answer the readiness of
    /// every knowledge point.
    ///
    /// The pass runs twice over the knowledge points. The first settles the six
    /// conditions one knowledge point answers alone; the second reads the first
    /// to answer `prerequisites_ok`, which asks about OTHER topics.
    #[must_use]
    pub fn resolve<C: ContentIndex + ?Sized>(&self, content: &C) -> ReadinessSet {
        let mut per_kp: BTreeMap<String, Readiness> = BTreeMap::new();
        let mut topic_keys: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for facts in self.facts() {
            topic_keys
                .entry(facts.topic_id.clone())
                .or_default()
                .push(facts.kp_key.clone());
            per_kp.insert(facts.kp_key.clone(), one_readiness(facts, content));
        }
        let practicable: BTreeMap<String, bool> = topic_keys
            .iter()
            .map(|(topic_id, keys)| {
                let ok = keys
                    .iter()
                    .filter_map(|key| per_kp.get(key))
                    .any(Readiness::serves_review);
                (topic_id.clone(), ok)
            })
            .collect();
        for facts in self.facts() {
            let ok = self
                .prerequisites(&facts.topic_id)
                .iter()
                .all(|prereq| practicable.get(prereq).copied().unwrap_or(false));
            if let Some(readiness) = per_kp.get_mut(&facts.kp_key) {
                readiness.prerequisites_ok = ok;
            }
        }
        ReadinessSet { per_kp, topic_keys }
    }
}

impl ReadinessSet {
    /// The readiness of one serving key.
    #[must_use]
    pub fn get(&self, kp_key: &str) -> Option<&Readiness> {
        self.per_kp.get(kp_key)
    }

    /// Every knowledge point, in serving-key order.
    pub fn all(&self) -> impl Iterator<Item = &Readiness> {
        self.per_kp.values()
    }

    /// The knowledge points of one topic, in authored order.
    #[must_use]
    pub fn topic(&self, topic_id: &str) -> Vec<&Readiness> {
        self.topic_keys
            .get(topic_id)
            .map(|keys| keys.iter().filter_map(|key| self.per_kp.get(key)).collect())
            .unwrap_or_default()
    }

    /// The topic ids, in id order.
    pub fn topics(&self) -> impl Iterator<Item = &String> {
        self.topic_keys.keys()
    }

    /// The count of knowledge points.
    #[must_use]
    pub fn len(&self) -> usize {
        self.per_kp.len()
    }

    /// Whether the set holds no knowledge point.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.per_kp.is_empty()
    }
}

impl ReadinessGate for ReadinessSet {
    /// A knowledge point the set does not name blocks nothing. The selector
    /// reads `start_at_kp`, which the arena wrote, so the miss means the plan
    /// and the index came from two different trees.
    fn lesson_blockers(&self, topic_id: &str, kp_id: &str) -> Vec<Blocker> {
        self.get(&kp_key(topic_id, kp_id))
            .map(Readiness::lesson_blockers)
            .unwrap_or_default()
    }

    fn topic_practicable(&self, topic_id: &str) -> bool {
        let keys = self.topic_keys.get(topic_id);
        // A topic the set does not name serves as it did before this rule.
        keys.is_none_or(|keys| {
            keys.iter()
                .filter_map(|key| self.per_kp.get(key))
                .any(Readiness::serves_review)
        })
    }
}

/// The readiness of one knowledge point, with `prerequisites_ok` left `true`
/// for the second pass to settle.
fn one_readiness<C: ContentIndex + ?Sized>(facts: &KpFacts, content: &C) -> Readiness {
    let approved_templates = content.approved_templates(&facts.kp_key);
    let practice_items = facts.practice_items(content);
    Readiness {
        kp_key: facts.kp_key.clone(),
        teachable: content.has_approved(&facts.kp_key, KIND_TEACH),
        practicable: practice_items >= PRACTICE_MINIMUM,
        assessable: facts.held_out.is_some(),
        hints: content.has_approved(&facts.kp_key, KIND_HINT_LADDER),
        solutions: facts.solutions,
        prerequisites_ok: true,
        visual_needed: facts.visual_needed,
        // Unit f9 gives the renderer, so an authored visual that passes
        // `VisualSpec::validate` is a present visual. A visual the check refuses
        // counts as absent, and the knowledge point stays blocked.
        visual_present: facts.valid_visuals > 0,
        broken_visuals: facts.broken_visuals,
        decidable_exemplars: facts.decidable.len(),
        approved_templates,
        practice_items,
    }
}
