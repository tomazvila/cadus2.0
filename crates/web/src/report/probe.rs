//! The writer of the `retention_probe` event (D-F11).
//!
//! The selector marks one task per session as a probe, the serve route copies the
//! marker onto `task_served`, and THIS module turns the graded attempt on that
//! task into the measurement event.
//!
//! It is a pure read of the log. The grade route calls [`probe_event`] with the
//! attempt it just recorded and the prior rows it already holds, appends what it
//! answers, and nothing else changes.
//!
//! # The provenance rules
//!
//! * `item_digest` is the [`problem_text_hash`] of the problem the learner saw,
//!   the same digest `TopicState::last_problems` carries.
//! * `exposure` is `First` only when NO earlier row of the log carries that
//!   digest. An earlier attempt or an earlier probe on the same rendered problem
//!   makes it `Repeat`, and the report then keeps the answer out of the
//!   independent evidence.
//! * `assisted` and `outcome` are copied from the attempt. An ungraded attempt
//!   still writes a probe: "the checker reached no verdict" is a fact about the
//!   probe, and the report counts it on its own line (D-F2).
//! * `policy` stamps the version and the digest of the policy the probe ran
//!   under, so a row of the report is readable beside the numbers that produced
//!   it (D-F12).

use cadus_core::config::Config;
use cadus_core::event::{Attempt, Event, Exposure, RetentionProbe, SchemaVersion, Slug, Timestamp};
use cadus_core::learner::problem_text_hash;
use cadus_store::state::EventRow;

/// The delay of the probe this task was served as, or `None` for a plain task.
fn served_as_probe(task_id: &str, prior: &[EventRow]) -> Option<(u32, Option<String>)> {
    prior.iter().rev().find_map(|row| match &row.event {
        Event::TaskServed(body) if body.task_id == task_id => body
            .probe_delay_days
            .map(|delay| (delay, body.kp.as_ref().map(|kp| kp.as_str().to_owned()))),
        _ => None,
    })
}

/// Whether an earlier row of the log already carries `digest`.
fn seen_before(digest: &str, prior: &[EventRow]) -> bool {
    prior.iter().any(|row| match &row.event {
        Event::Attempt(body) => problem_text_hash(&body.problem.text) == digest,
        Event::RetentionProbe(body) => body.item_digest.as_deref() == Some(digest),
        _ => false,
    })
}

/// The `retention_probe` event this attempt owes, or `None` (D-F11).
///
/// `prior` holds the rows BEFORE the attempt, so the exposure test never reads
/// the attempt against itself.
#[must_use]
pub fn probe_event(
    cfg: &Config,
    attempt: &Attempt,
    prior: &[EventRow],
    now: Timestamp,
) -> Option<Event> {
    let (delay_days, served_kp) = served_as_probe(&attempt.task_id, prior)?;
    let kp = attempt
        .kp
        .clone()
        .or_else(|| served_kp.as_deref().and_then(|id| Slug::new(id).ok()))?;
    // The attempt states its own digest and its own exposure when the serve path
    // recorded them (D-F9). This module falls back to the log only when it does
    // not, and it never overrides what the attempt already claims.
    let digest = attempt
        .item_digest
        .clone()
        .unwrap_or_else(|| problem_text_hash(&attempt.problem.text));
    let exposure = attempt.exposure.unwrap_or(if seen_before(&digest, prior) {
        Exposure::Repeat
    } else {
        Exposure::First
    });
    Some(Event::RetentionProbe(RetentionProbe {
        ts: now,
        session: attempt.session.clone(),
        v: SchemaVersion::current(),
        kp,
        topic: attempt.topic.clone(),
        delay_days,
        item_digest: Some(digest),
        outcome: attempt.outcome.clone(),
        assisted: attempt.assisted,
        exposure: Some(exposure),
        secs: attempt.secs,
        policy: Some(cfg.policy_version.stamp(&cfg.policy_digest())),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadus_core::event::{
        AttemptOutcome, AttemptProblem, Secs, TaskServed, TaskType, WorkQuality,
    };

    /// The task every fixture serves.
    const TASK: &str = "s1-review-1";

    /// One row of the log at `seq`.
    fn row(seq: i64, event: Event) -> EventRow {
        EventRow { seq, event }
    }

    /// One `task_served`, with the probe marker when `delay` is set.
    fn served(delay: Option<u32>) -> Event {
        Event::TaskServed(TaskServed {
            ts: Timestamp::from_micros(0),
            session: Some("s1".to_owned()),
            v: SchemaVersion::current(),
            task_id: TASK.to_owned(),
            task_type: TaskType::Review,
            topic: Some(Slug::new("t1").expect("a slug")),
            kp: Some(Slug::new("kp1").expect("a slug")),
            problems: Vec::new(),
            component_topics: Vec::new(),
            seed: None,
            probe_delay_days: delay,
            confirm: false,
        })
    }

    /// One graded attempt on `text`.
    fn attempt(text: &str, assisted: bool) -> Attempt {
        Attempt {
            ts: Timestamp::from_micros(0),
            session: Some("s1".to_owned()),
            v: SchemaVersion::current(),
            attempt_id: "a1".to_owned(),
            task_id: TASK.to_owned(),
            topic: Slug::new("t1").expect("a slug"),
            task_type: TaskType::Review,
            kp: Some(Slug::new("kp1").expect("a slug")),
            problem: AttemptProblem {
                text: text.to_owned(),
                answer_contract: None,
                expected: "3".to_owned(),
            },
            given_answer: "3".to_owned(),
            work: None,
            answer_kind: None,
            correct: true,
            outcome: AttemptOutcome::Correct,
            item_digest: None,
            item_source: None,
            exposure: None,
            timing_reliable: None,
            skills: Vec::new(),
            independent_after_feedback: false,
            secs: Secs::new(30).expect("in range"),
            error_tags: Vec::new(),
            work_quality: WorkQuality::NearlyPerfect,
            grader_note: None,
            feedback_practice: false,
            assisted,
        }
    }

    /// The probe body of an event, or a panic.
    fn probe_of(event: &Event) -> &RetentionProbe {
        match event {
            Event::RetentionProbe(body) => body,
            other => panic!("expected a retention probe, got a {}", other.type_name()),
        }
    }

    #[test]
    fn a_plain_task_writes_no_probe() {
        let prior = [row(1, served(None))];
        let event = probe_event(
            &Config::default(),
            &attempt("p1", false),
            &prior,
            Timestamp::from_micros(0),
        );
        assert!(event.is_none());
    }

    #[test]
    fn a_probe_task_writes_the_measurement_with_its_policy_stamp() {
        let cfg = Config::default();
        let prior = [row(1, served(Some(7)))];
        let event = probe_event(
            &cfg,
            &attempt("p1", false),
            &prior,
            Timestamp::from_micros(0),
        )
        .expect("a probe");
        let body = probe_of(&event);
        assert_eq!(body.delay_days, 7);
        assert_eq!(body.kp.as_str(), "kp1");
        assert_eq!(body.topic.as_str(), "t1");
        assert_eq!(body.exposure, Some(Exposure::First));
        assert_eq!(body.item_digest, Some(problem_text_hash("p1")));
        assert_eq!(
            body.policy,
            Some(format!("v1:{}", cfg.policy_digest())),
            "the row is readable only beside the policy that produced it"
        );
    }

    #[test]
    fn an_item_the_learner_answered_before_is_a_repeat() {
        let prior = [
            row(1, Event::Attempt(attempt("p1", false))),
            row(2, served(Some(30))),
        ];
        let event = probe_event(
            &Config::default(),
            &attempt("p1", false),
            &prior,
            Timestamp::from_micros(0),
        )
        .expect("a probe");
        assert_eq!(probe_of(&event).exposure, Some(Exposure::Repeat));
    }

    #[test]
    fn an_assisted_or_ungraded_answer_still_writes_its_probe() {
        let prior = [row(1, served(Some(90)))];
        let mut helped = attempt("p2", true);
        helped.outcome = AttemptOutcome::Ungraded {
            reason: "model-unavailable".to_owned(),
        };
        helped.correct = false;
        let event = probe_event(
            &Config::default(),
            &helped,
            &prior,
            Timestamp::from_micros(0),
        )
        .expect("a probe");
        let body = probe_of(&event);
        assert!(body.assisted);
        assert!(body.outcome.is_ungraded());
    }
}
