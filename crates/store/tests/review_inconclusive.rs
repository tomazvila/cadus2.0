//! An inconclusive review closes its task without session XP.
#![allow(clippy::unwrap_used)]
use cadus_core::event::Event;
use cadus_store::state::SessionView;
use serde_json::json;

#[test]
fn inconclusive_close_keeps_session_xp_neutral() {
    let mut view = SessionView::default();
    view.open_sessions.insert("s".to_owned());
    for (index, inconclusive) in [true, false].into_iter().enumerate() {
        let event = Event::from_json(&json!({
            "type":"review_result", "ts":"2026-09-06T12:00:00Z", "topic":"topic",
            "task_id":format!("review-{index}"), "passed":!inconclusive, "inconclusive":inconclusive,
            "weighted_score":0.5, "quality_tier":"perfect", "xp":5.0,
        }).to_string()).unwrap();
        view.apply(i64::try_from(index).unwrap() + 1, &event);
        assert!(view.closed_task_ids.contains(&format!("review-{index}")));
        assert_eq!(
            view.session_xp.get("s").copied().unwrap_or_default(),
            if inconclusive { 0.0 } else { 5.0 }
        );
    }
}
