//! Diagnostic run boundaries and correction of frozen placement evidence.
use cadus_core::config::Config;
use cadus_core::diagnostic::{DiagState, placement};
use cadus_core::event::{AttemptOutcome, Event, Slug};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction, types::Uuid};
use crate::StoreError;

/// Start one diagnostic run after closing the preceding abandoned boundary.
pub async fn start(tx: &mut Transaction<'_, Postgres>, user: Uuid, state: &Value, config: &Value, digest: &str) -> Result<Uuid, StoreError> {
    let seq: i64 = sqlx::query_scalar("SELECT coalesce(max(seq),0) FROM events WHERE user_id=$1")
        .bind(user).fetch_one(&mut **tx).await?;
    sqlx::query("UPDATE problem_report_diagnostics SET end_seq=$2 WHERE user_id=$1 AND end_seq IS NULL")
        .bind(user).bind(seq).execute(&mut **tx).await?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO problem_report_diagnostics(user_id,run_id,start_seq,state,config,curriculum_digest) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(user).bind(id).bind(seq).bind(state).bind(config).bind(digest).execute(&mut **tx).await?;
    Ok(id)
}

/// Preserve the complete running universe after each appended diagnostic answer.
pub async fn save(tx: &mut Transaction<'_, Postgres>, user: Uuid, state: &Value) -> Result<(), StoreError> {
    sqlx::query("UPDATE problem_report_diagnostics SET state=$2 WHERE user_id=$1 AND end_seq IS NULL")
        .bind(user).bind(state).execute(&mut **tx).await?;
    Ok(())
}

/// Bind the completed run to the just-appended placement event.
pub async fn close(tx: &mut Transaction<'_, Postgres>, user: Uuid, state: &Value) -> Result<(), StoreError> {
    sqlx::query("UPDATE problem_report_diagnostics SET state=$2,end_seq=(SELECT max(seq) FROM events WHERE user_id=$1),completed=true WHERE user_id=$1 AND end_seq IS NULL")
        .bind(user).bind(state).execute(&mut **tx).await?;
    Ok(())
}

/// Find the exact run containing an answer; pre-tracking history has no match.
pub async fn for_answer(tx: &mut Transaction<'_, Postgres>, user: Uuid, seq: i64) -> Result<Option<Value>, StoreError> {
    Ok(sqlx::query_scalar("SELECT to_jsonb(r) FROM problem_report_diagnostics r WHERE user_id=$1 AND start_seq<$2 AND (end_seq IS NULL OR end_seq >= $2) ORDER BY start_seq DESC LIMIT 1")
        .bind(user).bind(seq).fetch_optional(&mut **tx).await?)
}

/// Reprice one diagnostic answer and its completed placement in the same report transaction.
pub(super) async fn complete_regrade(tx: &mut Transaction<'_, Postgres>, user: Uuid, packet: &Value, correction: &mut Event) -> Result<Value, StoreError> {
    let seq = packet["event_seq"].as_i64().ok_or_else(|| invalid("missing diagnostic sequence"))?;
    let run = for_answer(tx, user, seq).await?.ok_or_else(|| invalid("diagnostic run predates repair tracking"))?;
    let policy = &packet["task_policy"];
    if run["run_id"] != policy["diagnostic_run_id"] || run["curriculum_digest"] != policy["diagnostic_curriculum_digest"] {
        return Err(invalid("diagnostic run identity changed"));
    }
    let run_id = run["run_id"].as_str().and_then(|value| Uuid::parse_str(value).ok()).ok_or_else(|| invalid("invalid diagnostic run id"))?;
    let mut rows = crate::state::load_raw_events_after(tx, user, 0).await?;
    super::task_outcomes::overlay(tx, user, &mut rows).await?;
    let Some(Event::DiagnosticAnswer(original)) = rows.iter().find(|row| row.seq == seq).map(|row| &row.event) else {
        return Err(invalid("diagnostic answer is unavailable"));
    };
    let mut fixed = original.clone();
    let mut state: DiagState = serde_json::from_value(run["state"].clone()).map_err(|error| invalid(&error.to_string()))?;
    if !fixed.correct {
        apply_delta(&mut state, &policy["diagnostic_delta"])?;
    }
    fixed.correct = true;
    fixed.outcome = Some(AttemptOutcome::Correct);
    fixed.weight = serde_json::from_value(policy["diagnostic_weight"].clone()).map_err(|error| invalid(&error.to_string()))?;
    super::task_outcomes::replace_event(correction, seq, Event::DiagnosticAnswer(fixed))?;
    let config: Config = serde_json::from_value(run["config"].clone()).map_err(|error| invalid(&error.to_string()))?;
    let calculated = placement(&state, &config);
    if run["completed"] == true {
        let close_seq = run["end_seq"].as_i64().ok_or_else(|| invalid("missing diagnostic close"))?;
        let Some(Event::DiagnosticPlaced(original)) = rows.iter().find(|row| row.seq == close_seq).map(|row| &row.event) else {
            return Err(invalid("diagnostic placement event is unavailable"));
        };
        let mut fixed = original.clone();
        fixed.balances = calculated.event_balances();
        fixed.conditional = calculated.conditional.iter().map(Slug::new).collect::<Result<Vec<_>, _>>()
            .map_err(|error| invalid(&error.to_string()))?;
        super::task_outcomes::replace_event(correction, close_seq, Event::DiagnosticPlaced(fixed))?;
    } else if run["end_seq"].is_null() {
        // The tenant advisory lock serializes run starts and report publication.
        // An abandoned or completed run never overwrites the current diagnostic.
        crate::state::save_diag_state(tx, user, &json!(state)).await?;
    }
    sqlx::query("UPDATE problem_report_diagnostics SET state=$3 WHERE user_id=$1 AND run_id=$2")
        .bind(user).bind(run_id).bind(json!(state)).execute(&mut **tx).await?;
    Ok(json!({"status":"recalculated","placement_completed":run["completed"],"balances":calculated.event_balances(),"xp":0}))
}

fn apply_delta(state: &mut DiagState, delta: &Value) -> Result<(), StoreError> {
    let delta = delta.as_object().ok_or_else(|| invalid("missing diagnostic delta"))?;
    for (topic, change) in delta {
        let amount = change.as_f64().filter(|value| value.is_finite()).ok_or_else(|| invalid("invalid diagnostic delta"))?;
        let balance = state.balances.get_mut(topic).ok_or_else(|| invalid("diagnostic delta exceeds frozen universe"))?;
        *balance += amount;
        if !balance.is_finite() { return Err(invalid("diagnostic balance overflow")); }
    }
    Ok(())
}

fn invalid(message: &str) -> StoreError { StoreError::Document(message.into()) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placement_repair_retains_full_universe_and_cumulative_corrections() {
        let mut state: DiagState = serde_json::from_value(json!({"balances":{"a":-1.0,"b":0.0},"answered":["a"]})).unwrap();
        apply_delta(&mut state, &json!({"a":2.0,"b":1.0})).unwrap();
        let first = placement(&state, &Config::default());
        assert_eq!(first.placed["a"], 1.0); assert_eq!(first.placed["b"], 1.0);
        apply_delta(&mut state, &json!({"a":1.0})).unwrap();
        assert_eq!(placement(&state, &Config::default()).placed["a"], 2.0);
        assert!(apply_delta(&mut state, &json!({"outside":1.0})).is_err());
    }

    #[tokio::test]
    async fn closed_diagnostic_repair_replays_placement_and_keeps_original_events() {
        crate::test_support::TestDb::with(|db| async move {
            let user = db.seed_user("report-diagnostic@example.test").await;
            let other = db.seed_user("report-diagnostic-other@example.test").await;
            let mut tx = crate::begin_tenant(&db.app, user).await.unwrap();
            crate::state::lock_web_state(&mut tx, user).await.unwrap();
            let initial = json!({"balances":{"q":0.0},"answered":[]});
            let state = json!({"balances":{"q":-1.0},"answered":["q"]});
            let run = start(&mut tx,user,&initial,&json!(Config::default()),"curriculum").await.unwrap();
            let answer = Event::from_json(&json!({"type":"diagnostic_answer","ts":"2026-01-01T12:00:00Z",
                "topic":"q","correct":false,"secs":30,"weight":1.0}).to_string()).unwrap();
            let placed = Event::from_json(&json!({"type":"diagnostic_placed","ts":"2026-01-01T12:01:00Z",
                "balances":{},"conditional":[]}).to_string()).unwrap();
            crate::state::append_event(&mut tx,user,&answer,None).await.unwrap();
            save(&mut tx,user,&state).await.unwrap();
            crate::state::append_event(&mut tx,user,&placed,None).await.unwrap();
            close(&mut tx,user,&state).await.unwrap();
            let packet = json!({"event_seq":1,"task_policy":{"diagnostic_run_id":run,
                "diagnostic_curriculum_digest":"curriculum","diagnostic_delta":{"q":2.0},"diagnostic_weight":1.0}});
            let mut correction = Event::from_json(&json!({"type":"regraded","ts":"2026-01-02T12:00:00Z",
                "task_id":"diagnostic","topic":"q","reason":"verified test report"}).to_string()).unwrap();
            complete_regrade(&mut tx,user,&packet,&mut correction).await.unwrap();
            crate::state::append_event(&mut tx,user,&correction,None).await.unwrap();
            // A repeated repair reads its effective answer and applies no second delta.
            complete_regrade(&mut tx,user,&packet,&mut correction).await.unwrap();
            let effective = crate::state::load_events(&mut tx,user).await.unwrap();
            let Event::DiagnosticAnswer(answer) = &effective[0].event else { panic!() };
            assert!(answer.correct);
            let Event::DiagnosticPlaced(placed) = &effective[1].event else { panic!() };
            assert_eq!(placed.balances["q"],1.0);
            let raw: Value = sqlx::query_scalar("SELECT payload FROM events WHERE user_id=$1 AND seq=1")
                .bind(user).fetch_one(&mut *tx).await.unwrap();
            assert_eq!(raw["correct"], false);
            tx.commit().await.unwrap();
            let mut tx = crate::begin_tenant(&db.app,other).await.unwrap();
            assert!(for_answer(&mut tx,user,1).await.unwrap().is_none());
            tx.rollback().await.unwrap();
        }).await;
    }
}
