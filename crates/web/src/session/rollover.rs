//! The day rollover: a session left open on an earlier day closes, and the
//! next request of the learner studies in a new session.
//!
//! A session ends when the client runs its plan out or presses Exit. Nothing
//! else closes one, so a learner who closes the tab keeps the same session for
//! weeks, and every per-session rule — one retention probe per session, the
//! quiz cadence, the session XP — then reads a single endless session.
//!
//! The rollover runs inside the locked routes that already write the log
//! (`POST /api/session/start` and every route that opens through
//! [`crate::serve::open`]). It writes NEW events only (C2): a `session_end` for
//! the stale session and, on the serve path, a `session_start` for the new one.
//!
//! # The rules
//!
//! A session is stale when ALL of these hold:
//!
//! 1. it started on an earlier calendar day than today, both read in the
//!    configured `timezone` (UTC when none is configured, trap T9);
//! 2. its newest event is at least [`ROLLOVER_IDLE_US`] old, so a learner who
//!    studies across midnight keeps the session until they pause;
//! 3. no quiz is half answered, unless the session started
//!    [`QUIZ_HOLD_DAYS`] or more days ago. A quiz runs on one clock, and
//!    cutting it at midnight scores half a quiz.
//!
//! # A problem served yesterday and answered today
//!
//! The rollover DISCARDS the stale serve. The D-S6 row clears with the session,
//! as `POST /api/session/end` clears it, so the answer meets
//! `409 session_rolled_over` and records no attempt. Grading it would put an
//! attempt into a session that already ended, where no session XP credits it,
//! and its `secs` would be the hours between the serve and the submit. The
//! hand-off of the item stays in the log, so the item counts as seen and no
//! retention probe reuses it.
//!
//! # XP, streak and daily goal
//!
//! The daily XP, the streak and the daily goal are folded from the result
//! events by their own local day, so the rollover changes none of them. The
//! `session_end` carries the stale session's XP as the log already credits it.

use cadus_core::event::{Event, SchemaVersion, SessionEnd, SessionStart, Timestamp};
use cadus_core::numeric::local_day;
use cadus_store::state::{
    EventRow, SessionView, clear_web_state, load_events_after, project_and_save,
};
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, NaiveDate, Utc};

use super::store::{Tx, append, projection_input, store};
use crate::AppState;
use crate::error::ApiError;
use crate::state::{Content, WebState};

/// The code of a request that arrived for a session the day rollover closed.
pub const SESSION_ROLLED_OVER: &str = "session_rolled_over";

/// The pause, in microseconds, a stale session must show before it rolls over.
pub const ROLLOVER_IDLE_US: i64 = 30 * 60 * 1_000_000;

/// The age, in calendar days, at which a half-answered quiz no longer holds
/// its session open.
pub const QUIZ_HOLD_DAYS: i64 = 2;

/// What the rollover rule reads about the open session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenSession {
    /// The instant of the `session_start` that opened it.
    pub started_us: i64,
    /// The instant of its newest event.
    pub last_us: i64,
    /// Whether a quiz of the session holds answers and is not finished.
    pub quiz_running: bool,
}

impl OpenSession {
    /// The facts of the open session from its event window and its D-S6 row.
    ///
    /// `events` is the window that starts AT the `session_start`. A window that
    /// holds no row answers `None`, and the rollover then leaves the session.
    #[must_use]
    pub fn of(events: &[EventRow], scratch: &WebState, session: &str) -> Option<Self> {
        let started_us = events.iter().find_map(|row| match &row.event {
            Event::SessionStart(body) if body.session.as_deref() == Some(session) => {
                Some(body.ts.micros())
            }
            _ => None,
        })?;
        let last_us = events
            .iter()
            .map(|row| row.event.ts().micros())
            .max()
            .unwrap_or(started_us)
            .max(started_us);
        let quiz_running = scratch.session.as_deref() == Some(session)
            && scratch
                .tasks
                .values()
                .any(|task| task.task_type == "quiz" && task.answered > 0 && !task.done);
        Some(Self {
            started_us,
            last_us,
            quiz_running,
        })
    }
}

/// The local calendar day of `us` in `tz`, with UTC for an unknown zone.
fn day_of(us: i64, tz: Option<&str>) -> Option<NaiveDate> {
    local_day(us, tz).or_else(|_| local_day(us, None)).ok()
}

/// The instant whose UTC date is the local day of `now_us` in `tz`.
///
/// [`SessionView::new_session_id`] names a session by the UTC date of the
/// instant it gets. Handing it noon of the LOCAL day makes the id carry the day
/// the rollover rule reads, so the session that opens "today" is named today.
#[must_use]
pub fn session_day(now_us: i64, tz: Option<&str>) -> DateTime<Utc> {
    day_of(now_us, tz)
        .and_then(|day| day.and_hms_opt(12, 0, 0))
        .map(|noon| noon.and_utc())
        .or_else(|| DateTime::<Utc>::from_timestamp_micros(now_us))
        .unwrap_or_default()
}

/// Whether the open session is stale at `now_us`. See the module rules.
#[must_use]
pub fn is_stale(open: &OpenSession, now_us: i64, tz: Option<&str>) -> bool {
    let (Some(today), Some(started)) = (day_of(now_us, tz), day_of(open.started_us, tz)) else {
        return false;
    };
    if started >= today || now_us.saturating_sub(open.last_us) < ROLLOVER_IDLE_US {
        return false;
    }
    !open.quiz_running || (today - started).num_days() >= QUIZ_HOLD_DAYS
}

/// One rollover decision and its writes, over the caller's locked transaction.
pub(crate) struct Rollover<'a> {
    /// The process state.
    pub(crate) state: &'a AppState,
    /// The curriculum and the config, for the time zone and the fold.
    pub(crate) content: &'a Content,
    /// The learner.
    pub(crate) user_id: Uuid,
    /// The session view of the head of the log.
    pub(crate) view: &'a SessionView,
    /// The open session.
    pub(crate) session: &'a str,
    /// The D-S6 row of the learner.
    pub(crate) scratch: &'a WebState,
    /// The instant of the request.
    pub(crate) now: Timestamp,
}

impl Rollover<'_> {
    /// Read the window of the open session and decide whether it rolls over.
    /// A process that runs no rollover answers `false` and reads nothing.
    pub(crate) async fn is_due(&self, tx: &mut Tx) -> Result<bool, ApiError> {
        if !self.state.day_rollover {
            return Ok(false);
        }
        let after = self.view.session_start_seq.unwrap_or(0).saturating_sub(1);
        let events = store(self.state, load_events_after(tx, self.user_id, after)).await?;
        let tz = self.content.cfg.timezone.as_deref();
        Ok(OpenSession::of(&events, self.scratch, self.session)
            .is_some_and(|open| is_stale(&open, self.now.micros(), tz)))
    }

    /// Append the `session_end` of the stale session and clear the D-S6 row.
    ///
    /// The minutes are the active seconds the session's row accumulated, which
    /// is the value `POST /api/session/end` sends when the client names none.
    pub(crate) async fn end(&self, tx: &mut Tx) -> Result<(), ApiError> {
        let active = if self.scratch.session.as_deref() == Some(self.session) {
            self.scratch.active_secs
        } else {
            0.0
        };
        let event = Event::SessionEnd(SessionEnd {
            ts: self.now,
            session: Some(self.session.to_owned()),
            v: SchemaVersion::current(),
            xp_earned: self.view.xp_in_session(self.session),
            minutes: (active / 60.0 * 100.0).round() / 100.0,
        });
        append(self.state, tx, self.user_id, &event).await?;
        store(self.state, clear_web_state(tx, self.user_id)).await
    }

    /// End the stale session, open the next session of today, and fold both
    /// events. The answer is the new session id; the caller commits.
    pub(crate) async fn roll(&self, tx: &mut Tx) -> Result<String, ApiError> {
        self.end(tx).await?;
        let day = session_day(self.now.micros(), self.content.cfg.timezone.as_deref());
        let next = self.view.new_session_id(day);
        let event = Event::SessionStart(SessionStart {
            ts: self.now,
            session: Some(next.clone()),
            v: SchemaVersion::current(),
        });
        append(self.state, tx, self.user_id, &event).await?;
        let input = projection_input(self.content, self.now);
        store(self.state, project_and_save(tx, self.user_id, &input, None)).await?;
        Ok(next)
    }
}

/// The `409` of a request for a session the rollover just closed.
#[must_use]
pub fn rolled_over() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::CONFLICT,
        SESSION_ROLLED_OVER,
        "A new day started, so a new session is open. Load the plan again.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TaskProgress;
    use cadus_core::event::SessionStart;

    /// 2026-10-05 09:00 UTC.
    const NOW: i64 = 1_791_190_800_000_000;
    const HOUR: i64 = 3_600_000_000;
    const DAY: i64 = 24 * HOUR;

    fn open(started_us: i64, last_us: i64, quiz_running: bool) -> OpenSession {
        OpenSession {
            started_us,
            last_us,
            quiz_running,
        }
    }

    #[test]
    fn a_session_from_an_earlier_day_rolls_over() {
        assert!(is_stale(&open(NOW - 20 * DAY, NOW - DAY, false), NOW, None));
    }

    #[test]
    fn a_session_of_today_stays() {
        assert!(!is_stale(
            &open(NOW - 2 * HOUR, NOW - 2 * HOUR, false),
            NOW,
            None
        ));
    }

    #[test]
    fn a_learner_studying_across_midnight_keeps_the_session_until_a_pause() {
        // Started 23:30 yesterday, last answer 10 minutes ago.
        let started = NOW - 9 * HOUR - HOUR / 2;
        assert!(!is_stale(&open(started, NOW - HOUR / 6, false), NOW, None));
        assert!(is_stale(&open(started, NOW - HOUR, false), NOW, None));
    }

    #[test]
    fn a_half_answered_quiz_holds_the_session_until_it_is_two_days_old() {
        assert!(!is_stale(&open(NOW - DAY, NOW - DAY, true), NOW, None));
        assert!(is_stale(
            &open(NOW - 2 * DAY, NOW - 2 * DAY, true),
            NOW,
            None
        ));
    }

    #[test]
    fn the_day_boundary_reads_the_configured_timezone() {
        // 2026-10-05 01:00 UTC is still 2026-10-04 in New York.
        let now = NOW - 8 * HOUR;
        let started = now - 3 * HOUR;
        assert!(is_stale(&open(started, started, false), now, None));
        assert!(!is_stale(
            &open(started, started, false),
            now,
            Some("America/New_York")
        ));
        // An unknown zone reads UTC.
        assert!(is_stale(
            &open(started, started, false),
            now,
            Some("Nowhere/City")
        ));
    }

    #[test]
    fn the_new_session_is_named_by_the_local_day() {
        // 2026-10-05 01:00 UTC is 2026-10-04 in New York.
        let now = NOW - 8 * HOUR;
        let view = SessionView::default();
        assert_eq!(view.new_session_id(session_day(now, None)), "s_2026-10-05a");
        assert_eq!(
            view.new_session_id(session_day(now, Some("America/New_York"))),
            "s_2026-10-04a"
        );
    }

    #[test]
    fn the_facts_come_from_the_window_and_the_running_quiz() {
        let events = vec![
            EventRow {
                seq: 4,
                event: Event::SessionStart(SessionStart {
                    ts: Timestamp::from_micros(NOW - DAY),
                    session: Some("s_a".to_owned()),
                    v: SchemaVersion::current(),
                }),
            },
            EventRow {
                seq: 5,
                event: Event::SessionEnd(SessionEnd {
                    ts: Timestamp::from_micros(NOW - HOUR),
                    session: Some("s_other".to_owned()),
                    v: SchemaVersion::current(),
                    xp_earned: 0.0,
                    minutes: 0.0,
                }),
            },
        ];
        let mut scratch = WebState {
            session: Some("s_a".to_owned()),
            ..WebState::default()
        };
        scratch.tasks.insert(
            "s_a-quiz".to_owned(),
            TaskProgress {
                task_id: "s_a-quiz".to_owned(),
                task_type: "quiz".to_owned(),
                total: 5,
                served: 2,
                answered: 2,
                done: false,
                current_kp: None,
                proof_kp: None,
            },
        );
        let facts = OpenSession::of(&events, &scratch, "s_a").expect("a window");
        assert_eq!(facts, open(NOW - DAY, NOW - HOUR, true));
        assert_eq!(OpenSession::of(&events, &scratch, "s_b"), None);
    }
}
