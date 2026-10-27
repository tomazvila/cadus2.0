//! The Today card of the dashboard (D-DP1): the local date, the study-day
//! number, the set of the day, one read, and the pace of the week.
//!
//! The functions here are pure. They read no row and write no row. The caller
//! loads the frozen row and saves it. The card shows no statement, no answer
//! and no solution (Hard Rule 1).

use cadus_core::curriculum::Curriculum;
use cadus_core::event::{TaskType, Timestamp};
use cadus_core::numeric::local_day;
use cadus_core::selector::{SessionPlan, Task};
use cadus_store::state::SessionView;
use serde_json::{Value, json};
use sqlx::types::chrono::NaiveDate;

use crate::state::WebState;

/// The local calendar day of `now` in `tz`, with UTC for an unknown zone.
pub(crate) fn local_date(now: Timestamp, tz: Option<&str>) -> NaiveDate {
    local_day(now.micros(), tz)
        .or_else(|_| local_day(now.micros(), None))
        .unwrap_or_default()
}

/// The `{id, name}` view of one topic. `None` when the id is not in the graph.
fn topic_view(graph: &Curriculum, id: &str) -> Option<Value> {
    let idx = graph.idx_of(id)?;
    let found = graph.topic(idx)?;
    Some(json!({"id": id, "name": found.name}))
}

/// The topic view of one task, or `null`.
fn task_topic(graph: &Curriculum, task: &Task) -> Value {
    task.topic
        .as_deref()
        .and_then(|id| topic_view(graph, id))
        .unwrap_or(Value::Null)
}

/// The question count of a task. A review lists none (D-F11).
fn n_problems(task: &Task) -> Option<i64> {
    if task.task_type == TaskType::Review {
        None
    } else {
        task.n_problems
    }
}

/// The topic to read: the first lesson topic, else the first review topic, else `null`.
fn read_of(graph: &Curriculum, plan: &SessionPlan) -> Value {
    pick_read(&plan.tasks, |id| topic_view(graph, id))
}

/// [`read_of`] with the topic lookup passed in.
fn pick_read(tasks: &[Task], topic_of: impl Fn(&str) -> Option<Value>) -> Value {
    for (kind, source) in [(TaskType::Lesson, "lesson"), (TaskType::Review, "review")] {
        let found = tasks
            .iter()
            .filter(|task| task.task_type == kind)
            .find_map(|task| {
                let topic = task.topic.as_deref().and_then(&topic_of)?;
                Some(json!({"topic": topic, "kp": task.start_at_kp, "source": source}))
            });
        if let Some(read) = found {
            return read;
        }
    }
    Value::Null
}

/// The document that freezes the set of one local day: `{day, set, read}`.
///
/// Each entry of `set` keeps the task type, the topic, the question count and
/// the time budget. The progress of an entry is never frozen.
pub(crate) fn frozen_json(graph: &Curriculum, plan: &SessionPlan, day: NaiveDate) -> Value {
    let set: Vec<Value> = plan
        .tasks
        .iter()
        .map(|task| {
            json!({
                "task_type": task.task_type.as_str(),
                "topic": task_topic(graph, task),
                "n_problems": n_problems(task),
                "time_budget_secs": task.time_budget_secs,
            })
        })
        .collect();
    json!({"day": day.to_string(), "set": set, "read": read_of(graph, plan)})
}

/// The progress of one frozen entry, as `(task_id, answered, done)`.
///
/// A live task with the same type and topic gives the scratch progress. A task
/// the learner closed is not in the live plan any more, so a closed task id of
/// the day with the same type and topic gives `done`.
fn frozen_progress(
    view: &SessionView,
    plan: &SessionPlan,
    scratch: &WebState,
    day: NaiveDate,
    entry: &Value,
) -> (String, i64, bool) {
    let kind = entry["task_type"].as_str().unwrap_or_default();
    let topic = entry["topic"]["id"].as_str();
    let live = plan
        .tasks
        .iter()
        .find(|task| task.task_type.as_str() == kind && task.topic.as_deref() == topic);
    if let Some(task) = live {
        let (answered, done) = scratch.plan_progress(&task.task_id);
        return (task.task_id.clone(), answered, done);
    }
    let prefix = format!("s_{day}");
    let suffix = match topic {
        Some(id) => format!("-{kind}-{id}"),
        None => format!("-{kind}"),
    };
    let closed = view
        .closed_task_ids
        .iter()
        .find(|id| id.starts_with(&prefix) && id.ends_with(&suffix));
    (closed.cloned().unwrap_or_default(), 0, closed.is_some())
}

/// The local days that one UTC date overlaps in `tz`: the local day of its
/// first instant and the local day of its last instant.
///
/// The view keeps no time of day, so the two days are the only honest answer.
/// West of UTC, an evening attempt carries the next UTC date. Both days then
/// count it, and the card keeps the right local day.
fn local_days_of(utc_day: NaiveDate, tz: Option<&str>) -> [NaiveDate; 2] {
    const DAY_US: i64 = 86_400_000_000;
    let start = utc_day
        .and_hms_opt(0, 0, 0)
        .map_or(0, |at| at.and_utc().timestamp_micros());
    [
        local_date(Timestamp::from_micros(start), tz),
        local_date(Timestamp::from_micros(start + DAY_US - 1), tz),
    ]
}

/// `(day_number, study_days, topics_passed)` for the local day `date`.
///
/// One day rule serves all three: a study date counts when one of its local
/// days is `date` (for the day number) or lies in the 7 days that end on `date`.
fn day_counts(view: &SessionView, date: NaiveDate, tz: Option<&str>) -> (usize, usize, usize) {
    let in_week = |day: NaiveDate| (0..7).contains(&(date - day).num_days());
    let mut today = false;
    let mut study_days = 0;
    for utc_day in &view.active_study_days {
        let days = local_days_of(*utc_day, tz);
        today |= days.contains(&date);
        study_days += usize::from(days.iter().any(|day| in_week(*day)));
    }
    let topics_passed = view
        .learned_at
        .values()
        .filter(|us| in_week(local_date(Timestamp::from_micros(**us), tz)))
        .count();
    (
        view.active_study_days.len() + usize::from(!today),
        study_days,
        topics_passed,
    )
}

/// The Today card (D-DP1).
///
/// `frozen` is the row of `session_plans`. When its `day` equals the local day
/// of `now`, the set and the read come from the row. Otherwise they come from
/// the live `plan`.
///
/// `day_number` counts the study days of the session view plus one on a day
/// with no attempt. The view holds UTC dates, and the card date is the local
/// day. A UTC date counts for a local day when the UTC date overlaps that local
/// day in the learner's zone (see [`local_days_of`]). The fold stays as it is.
pub(crate) fn today_json(
    graph: &Curriculum,
    view: &SessionView,
    plan: &SessionPlan,
    scratch: &WebState,
    now: Timestamp,
    tz: Option<&str>,
    frozen: Option<&Value>,
) -> Value {
    let date = local_date(now, tz);
    let row = frozen.filter(|row| row["day"].as_str() == Some(date.to_string().as_str()));

    let (set, read): (Vec<Value>, Value) = match row {
        Some(row) => {
            let entries = row["set"].as_array().map_or(&[][..], Vec::as_slice);
            let set = entries
                .iter()
                .map(|entry| {
                    let (task_id, answered, done) =
                        frozen_progress(view, plan, scratch, date, entry);
                    json!({
                        "task_id": task_id,
                        "task_type": entry["task_type"],
                        "topic": entry["topic"],
                        "n_problems": entry["n_problems"],
                        "time_budget_secs": entry["time_budget_secs"],
                        "progress": {"answered": answered, "done": done},
                    })
                })
                .collect();
            (set, row["read"].clone())
        }
        None => {
            let set = plan
                .tasks
                .iter()
                .map(|task| {
                    let (answered, done) = scratch.plan_progress(&task.task_id);
                    json!({
                        "task_id": task.task_id,
                        "task_type": task.task_type.as_str(),
                        "topic": task_topic(graph, task),
                        "n_problems": n_problems(task),
                        "time_budget_secs": task.time_budget_secs,
                        "progress": {"answered": answered, "done": done},
                    })
                })
                .collect();
            (set, read_of(graph, plan))
        }
    };

    let (day_number, study_days, topics_passed) = day_counts(view, date, tz);

    json!({
        "date": date.to_string(),
        "day_number": day_number,
        "set": set,
        "read": read,
        "week": {"study_days": study_days, "topics_passed": topics_passed},
        "frozen": row.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(kind: TaskType, topic: Option<&str>) -> Task {
        Task {
            task_type: kind,
            topic: topic.map(str::to_string),
            ..Task::default()
        }
    }

    fn lookup(id: &str) -> Option<Value> {
        Some(json!({"id": id, "name": id}))
    }

    #[test]
    fn an_empty_plan_has_no_read() {
        assert_eq!(pick_read(&[], lookup), Value::Null);
    }

    #[test]
    fn the_first_lesson_wins_over_an_earlier_review() {
        let tasks = [
            task(TaskType::Review, Some("a")),
            task(TaskType::Lesson, Some("b")),
            task(TaskType::Lesson, Some("c")),
        ];
        let read = pick_read(&tasks, lookup);
        assert_eq!(read["source"], "lesson");
        assert_eq!(read["topic"]["id"], "b");
    }

    #[test]
    fn a_plan_of_reviews_reads_the_first_review() {
        let tasks = [
            task(TaskType::Quiz, None),
            task(TaskType::Review, Some("a")),
        ];
        let read = pick_read(&tasks, lookup);
        assert_eq!(read["source"], "review");
        assert_eq!(read["topic"]["id"], "a");
    }

    #[test]
    fn a_late_evening_attempt_west_of_utc_counts_for_its_local_day() {
        // 23:30 on 2026-10-20 in Los Angeles (UTC-7) is 06:30 UTC on 2026-10-21.
        let tz = Some("America/Los_Angeles");
        let at = Timestamp::parse("2026-10-21T06:30:00Z").map_or(0, |t| t.micros());
        assert_eq!(
            local_date(Timestamp::from_micros(at), tz).to_string(),
            "2026-10-20"
        );
        let mut view = SessionView::default();
        view.active_study_days
            .insert(NaiveDate::from_ymd_opt(2026, 10, 21).unwrap_or_default());
        view.learned_at.insert("t".to_string(), at);
        let date = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap_or_default();
        // The attempt is today: the day number does not rise, the week line has it.
        assert_eq!(day_counts(&view, date, tz), (1, 1, 1));
        // With no attempt, the day number rises by one.
        let later = NaiveDate::from_ymd_opt(2026, 11, 20).unwrap_or_default();
        assert_eq!(day_counts(&view, later, tz), (2, 0, 0));
    }
}
