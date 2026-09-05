//! The regrade pre-pass (`projector.py:698-770`).

use std::collections::BTreeMap;

use crate::event::{Attempt, Event, LessonResult, Regraded, RegradedAttempt, ReviewResult};

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
    let corrections = Corrections::collect(events);
    // The task each topic's most recent attempt belonged to. It is the key that gives a
    // `lesson_result`, which carries no task id, the task it closes.
    let mut task_of_topic: BTreeMap<String, String> = BTreeMap::new();
    events
        .iter()
        .filter_map(|event| corrections.correct(event, &mut task_of_topic))
        .collect()
}

/// The corrections of a stream, keyed by the attempt and the task they name.
struct Corrections<'a> {
    by_attempt: BTreeMap<&'a str, &'a RegradedAttempt>,
    by_task: BTreeMap<&'a str, &'a Regraded>,
}

impl<'a> Corrections<'a> {
    /// Collect every `regraded` event. A later correction of the same target
    /// supersedes an earlier one.
    fn collect(events: &'a [Event]) -> Self {
        let mut by_attempt: BTreeMap<&'a str, &'a RegradedAttempt> = BTreeMap::new();
        let mut by_task: BTreeMap<&'a str, &'a Regraded> = BTreeMap::new();
        for event in events {
            let Event::Regraded(correction) = event else {
                continue;
            };
            for corrected in &correction.attempts {
                by_attempt.insert(corrected.attempt_id.as_str(), corrected);
            }
            if correction.quality_tier.is_some() || correction.xp.is_some() {
                by_task.insert(correction.task_id.as_str(), correction);
            }
        }
        Self {
            by_attempt,
            by_task,
        }
    }

    /// The corrected form of one event, or `None` for a consumed `regraded`.
    fn correct(
        &self,
        event: &Event,
        task_of_topic: &mut BTreeMap<String, String>,
    ) -> Option<Event> {
        match event {
            Event::Regraded(_) => None,
            Event::Attempt(body) => {
                task_of_topic.insert(body.topic.as_str().to_owned(), body.task_id.clone());
                Some(Event::Attempt(self.corrected_attempt(body)))
            }
            Event::LessonResult(body) => {
                // A `lesson_result` has no `task_id` field at all.
                let task_id = task_of_topic.get(body.topic.as_str()).map(String::as_str);
                Some(Event::LessonResult(self.corrected_lesson(body, task_id)))
            }
            Event::ReviewResult(body) => {
                // Python `event.task_id or task_of_topic.get(topic)`: an EMPTY task id
                // is falsy there, so it falls through to the preceding attempt.
                let task_id = body
                    .task_id
                    .as_deref()
                    .filter(|id| !id.is_empty())
                    .or_else(|| task_of_topic.get(body.topic.as_str()).map(String::as_str));
                Some(Event::ReviewResult(self.corrected_review(body, task_id)))
            }
            other => Some(other.clone()),
        }
    }

    /// The attempt with its named correction applied.
    fn corrected_attempt(&self, body: &Attempt) -> Attempt {
        let mut fixed = body.clone();
        if let Some(correction) = self.by_attempt.get(fixed.attempt_id.as_str()) {
            fixed.work_quality = correction.work_quality;
            fixed.error_tags.clone_from(&correction.error_tags);
            fixed.grader_note.clone_from(&correction.grader_note);
        }
        fixed
    }

    /// The lesson result with the correction of `task_id` applied.
    fn corrected_lesson(&self, body: &LessonResult, task_id: Option<&str>) -> LessonResult {
        let mut fixed = body.clone();
        apply_task_correction(
            &mut fixed.quality_tier,
            &mut fixed.xp,
            self.task_correction(task_id),
        );
        fixed
    }

    /// The review result with the correction of `task_id` applied.
    fn corrected_review(&self, body: &ReviewResult, task_id: Option<&str>) -> ReviewResult {
        let mut fixed = body.clone();
        apply_task_correction(
            &mut fixed.quality_tier,
            &mut fixed.xp,
            self.task_correction(task_id),
        );
        fixed
    }

    /// The correction that names `task_id`, when one does.
    fn task_correction(&self, task_id: Option<&str>) -> Option<&'a Regraded> {
        task_id.and_then(|id| self.by_task.get(id).copied())
    }
}

/// Replace the tier and the XP of a result with the values a correction carries.
fn apply_task_correction(
    quality_tier: &mut crate::event::WorkQuality,
    xp: &mut f64,
    correction: Option<&Regraded>,
) {
    let Some(correction) = correction else {
        return;
    };
    if let Some(tier) = correction.quality_tier {
        *quality_tier = tier;
    }
    if let Some(value) = correction.xp {
        *xp = value;
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::ev;
    use super::*;
    use crate::event::WorkQuality;

    #[test]
    fn a_correction_lands_on_its_attempt_and_on_the_result_that_closes_the_task() {
        let events = [
            r#"{"type":"attempt","ts":"2026-07-14T12:00:00Z","attempt_id":"a1","task_id":"t1","topic":"q","task_type":"lesson","problem":{"text":"1+1","expected":"2"},"given_answer":"2","correct":true,"secs":5,"work_quality":"poor"}"#,
            r#"{"type":"lesson_result","ts":"2026-07-14T12:01:00Z","topic":"q","passed":true,"xp":1.0,"quality_tier":"poor"}"#,
            r#"{"type":"review_result","ts":"2026-07-14T12:02:00Z","topic":"q","passed":true,"weighted_score":1.0,"xp":1.0,"quality_tier":"poor","task_id":""}"#,
            r#"{"type":"review_result","ts":"2026-07-14T12:03:00Z","topic":"r","passed":true,"weighted_score":1.0,"xp":1.0,"quality_tier":"poor"}"#,
            r#"{"type":"session_start","ts":"2026-07-14T12:04:00Z"}"#,
            r#"{"type":"regraded","ts":"2026-07-14T12:05:00Z","task_id":"t1","topic":"q","reason":"r","quality_tier":"perfect","attempts":[{"attempt_id":"a1","work_quality":"perfect"}]}"#,
        ]
        .map(ev);
        let out = apply_regrades(&events);
        let grades: Vec<(Option<WorkQuality>, f64)> = out
            .iter()
            .map(|event| match event {
                Event::Attempt(body) => (Some(body.work_quality), 0.0),
                Event::LessonResult(body) => (Some(body.quality_tier), body.xp),
                Event::ReviewResult(body) => (Some(body.quality_tier), body.xp),
                _ => (None, 0.0),
            })
            .collect();
        assert_eq!(
            grades,
            [
                (Some(WorkQuality::Perfect), 0.0),
                (Some(WorkQuality::Perfect), 1.0),
                (Some(WorkQuality::Perfect), 1.0),
                (Some(WorkQuality::Poor), 1.0),
                (None, 0.0),
            ]
        );
        assert_eq!(apply_regrades(&out), out);
    }
}
