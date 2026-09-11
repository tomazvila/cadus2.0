//! Event-derived instruction/application/delayed-transfer schedule.
use super::IntegratedSet;
use crate::event::{IntegratedAttempt, IntegratedServed, TaskType, Timestamp};
use crate::selector::Task;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A first delayed transfer assessment, one week after independent application.
pub const DELAY_DAYS: u32 = 7;
const DAY_US: i64 = 86_400_000_000;

/// An assessment stays pinned through reload, completion, and receipt retry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assessment {
    pub session: String,
    pub task_id: String,
    pub source: String,
    pub item: String,
    pub completed: bool,
}

/// Small replayable index; planning reads no lifetime event query.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JourneyState {
    pub learned: BTreeMap<String, Timestamp>,
    pub seen: BTreeSet<String>,
    pub assessments: BTreeMap<String, Assessment>,
}

impl JourneyState {
    pub fn is_empty(&self) -> bool {
        self.learned.is_empty() && self.seen.is_empty() && self.assessments.is_empty()
    }
    pub fn served(&mut self, event: &IntegratedServed) {
        self.seen.insert(event.item_digest.clone());
        if let (Some(source), Some(session)) = (&event.assessment_of, &event.session) {
            self.pin(source, session, &event.task_id, &event.item_id, false);
        }
    }
    pub fn attempted(&mut self, event: &IntegratedAttempt) {
        if let (Some(source), Some(session)) = (&event.assessment_of, &event.session) {
            self.pin(source, session, &event.task_id, &event.item_id, true);
        } else if event.instruction_kp.is_some()
            && event.solved
            && !event.assisted
            && !event.final_field.outcome.is_ungraded()
        {
            self.learned
                .entry(event.item_id.clone())
                .or_insert(event.ts);
        }
        self.seen.insert(event.item_digest.clone());
    }
    fn pin(&mut self, source: &str, session: &str, task: &str, item: &str, completed: bool) {
        let held = self
            .assessments
            .entry(source.to_owned())
            .or_insert(Assessment {
                session: session.to_owned(),
                task_id: task.to_owned(),
                source: source.to_owned(),
                item: item.to_owned(),
                completed,
            });
        held.completed |= completed;
    }
    /// Keep this session's pinned receipt reachable, otherwise draw a new unseen item.
    pub fn task(&self, items: &IntegratedSet, session: &str, now: Timestamp) -> Option<Task> {
        if let Some(held) = self
            .assessments
            .values()
            .find(|held| held.session == session)
        {
            return self.as_task(items, held);
        }
        for (source, learned) in &self.learned {
            if self.assessments.contains_key(source)
                || now.micros().saturating_sub(learned.micros()) < i64::from(DELAY_DAYS) * DAY_US
            {
                continue;
            }
            let Some(original) = items.get(source) else {
                continue;
            };
            let components: Vec<String> = original
                .component_topics
                .iter()
                .map(ToString::to_string)
                .collect();
            let Some(fresh) = items.items().iter().find(|item| {
                item.course == original.course
                    && item.covers(&components)
                    && !self.seen.contains(&item.digest())
            }) else {
                continue;
            };
            return self.as_task(
                items,
                &Assessment {
                    session: session.to_owned(),
                    task_id: format!("{session}-integrated-assessment-{source}"),
                    source: source.clone(),
                    item: fresh.id.to_string(),
                    completed: false,
                },
            );
        }
        None
    }
    fn as_task(&self, items: &IntegratedSet, assessment: &Assessment) -> Option<Task> {
        let item = items.get(&assessment.item)?;
        Some(Task {
            task_id: assessment.task_id.clone(),
            task_type: TaskType::MultiStep,
            component_topics: item
                .component_topics
                .iter()
                .map(ToString::to_string)
                .collect(),
            n_problems: Some(1),
            integrated_item_id: Some(assessment.item.clone()),
            integrated_assessment_of: Some(assessment.source.clone()),
            probe_delay_days: Some(DELAY_DAYS),
            why: "Delayed independent application on a fresh integrated scenario.".to_owned(),
            ..Task::default()
        })
    }
}

#[cfg(test)]
#[path = "journey_tests.rs"]
mod tests;
