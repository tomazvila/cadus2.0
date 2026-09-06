//! Integrated evidence is local to the decided, explicitly credited knowledge points.

use crate::answer::AnswerContract;
use crate::event::{AttemptOutcome, IntegratedAttempt, KpProgress};

use super::Projector;

impl Projector<'_> {
    /// Complete one integrated task in the light index and persist only its KP evidence.
    ///
    /// A passed assisted lesson also marks its KPs passed. This narrower evidence grants
    /// no whole-topic status, FIRe propagation, ability, repetition, or XP increase.
    /// Assistance stays on the event; it never closes independent-confirmation practice.
    pub(super) fn on_integrated_attempt(&mut self, event: &IntegratedAttempt, apply_fire: bool) {
        let key = format!(
            "integrated:{}:{}",
            event.session.as_deref().unwrap_or("legacy"),
            event.task_id
        );
        if !self.completed_task_ids.insert(key) {
            return;
        }
        self.completed_tasks += 1;
        for field in event
            .steps
            .iter()
            .chain(std::iter::once(&event.final_field))
        {
            if field.outcome != AttemptOutcome::Correct || field.contract == AnswerContract::None {
                continue;
            }
            for skill in &field.skills {
                if !event.skills_credited.contains(skill) {
                    continue;
                }
                let Some((topic, kp)) = self.known_skill(skill) else {
                    continue;
                };
                if !event.assisted && !field.assisted {
                    self.last_practice.insert(skill.clone(), event.ts.micros());
                }
                if apply_fire {
                    self.topics
                        .entry(topic.to_owned())
                        .or_default()
                        .kp_progress
                        .insert(kp.to_owned(), KpProgress::Passed);
                }
            }
        }
    }

    /// Both key components resolve against the loaded curriculum, including the KP.
    fn known_skill<'a>(&self, key: &'a str) -> Option<(&'a str, &'a str)> {
        let (topic, kp) = key.split_once('/')?;
        let idx = self.graph.idx_of(topic)?;
        self.graph
            .knowledge_points(idx)
            .iter()
            .any(|point| point.id.as_str() == kp)
            .then_some((topic, kp))
    }
}

#[cfg(test)]
#[path = "integrated_tests.rs"]
mod tests;
