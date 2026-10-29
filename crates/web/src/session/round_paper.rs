//! The paper round of an exam set (D-VBE3): the task, the record and the wait.
//!
//! A paper round serves the form of one exam set topic in the fixed order of
//! the paper. It appends no event. The retake wait of a paper counts per exam
//! set: it reads the last result of that set and no other.

use sqlx::types::Json;
use sqlx::types::chrono::NaiveDate;

use cadus_core::event::Event;
use cadus_core::selector::{PaperItem, QUIZ_RETAKE_DELAY_DAYS, paper_task, utc_date};

use super::store::{Ready, Tx};
use crate::error::ApiError;
use crate::state::{RecordedReview, RecordedRound, WebState};
use cadus_store::state::Projection;

/// The 1-based number of the next round of this session.
pub(super) fn round_no_of(scratch: &WebState) -> usize {
    scratch.rounds.len() + 1
}

/// The record of the paper round of `set` over `items`.
///
/// The target time is the `expected_time_secs` of the set topic.
pub(super) fn paper_round(
    graph: &cadus_core::curriculum::Curriculum,
    session: &str,
    set: &str,
    items: &[PaperItem],
    round_no: usize,
    projection: &Projection,
) -> RecordedRound {
    let topic = graph.idx_of(set).and_then(|idx| graph.topic(idx));
    let course = graph.idx_of(set).map(|idx| graph.course_of(idx).to_owned());
    let task = paper_task(
        set,
        items,
        topic.map_or(0, |topic| topic.expected_time_secs),
        projection.view.quiz_high_score_streak,
        &format!("{session}-round-paper-{set}-{round_no}"),
    );
    RecordedRound {
        task: RecordedReview(task),
        length: i64::try_from(items.len()).unwrap_or(0),
        scope: "paper".to_owned(),
        course_name: course
            .as_deref()
            .and_then(|id| graph.course(id))
            .map(|course| course.name.clone()),
        course,
        set: Some(set.to_owned()),
    }
}

/// The date an exam set becomes open again after a result under the retake
/// line, or `None` when no such result waits (D-VBE3).
///
/// The paper task id holds the set name, so the query reads the results whose
/// `quiz_id` holds `-round-paper-<set>-`. Only the newest result counts.
pub(super) async fn paper_wait(
    req: &Ready,
    tx: &mut Tx,
    set: &str,
) -> Result<Option<NaiveDate>, ApiError> {
    let marker = format!("-round-paper-{set}-");
    let rows: Vec<(Json<Event>,)> = sqlx::query_as(
        "SELECT payload FROM events \
         WHERE user_id = $1 AND payload->>'type' = 'quiz_result' \
         AND strpos(payload->>'quiz_id', $2) > 0 \
         ORDER BY seq DESC LIMIT 1",
    )
    .bind(req.user_id)
    .bind(&marker)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| ApiError::internal("The paper history could not be read."))?;
    let Some((Json(Event::QuizResult(last)),)) = rows.into_iter().next() else {
        return Ok(None);
    };
    if last.inconclusive || last.score >= req.content.cfg.quiz.retake_below {
        return Ok(None);
    }
    let days = u64::try_from(QUIZ_RETAKE_DELAY_DAYS).unwrap_or(0);
    Ok(utc_date(last.ts.micros())
        .and_then(|day| (0..days).try_fold(day, |date, _| date.succ_opt())))
}
