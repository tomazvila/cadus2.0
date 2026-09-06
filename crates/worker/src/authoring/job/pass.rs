//! One pass over one knowledge point and kind, and the batch over a list.

use cadus_core::instruction::ServedInstance;
use cadus_core::pool::kp_key;
use cadus_core::template::TEMPLATABLE_KINDS;
use cadus_model_client::{Attempt, Call};
use cadus_store::Db;

use super::{
    AuthoringJob, BatchReport, Decline, Outcome, Report, bank_target, served_instances,
    slots_taken, stale_slots, store_pending, verify_kind,
};
use crate::WorkerError;
use crate::authoring::cost;
use crate::authoring::prompt::{self, AuthoringSpec, Kind};
use crate::model_log::{self, CallRecord, PURPOSE_AUTHORING};

/// The reason a knowledge point no gate can accept declines with, or `None`
/// when the gate of `kind` reads no answer kind (T3, finding F19).
///
/// Contract-bearing multi-step templates use their deterministic item policy.
/// Other unsupported template and diagnosis kinds decline before spending a
/// model call. Teach pages and hint ladders can still support those topics.
fn undecidable(kind: Kind, spec: &AuthoringSpec) -> Option<String> {
    let gated = matches!(kind, Kind::Template | Kind::Diagnosis);
    let contracted_template = kind == Kind::Template
        && spec.answer_kind == cadus_core::curriculum::AnswerKind::MultiStep
        && spec.template_contract().is_some();
    if gated && !TEMPLATABLE_KINDS.contains(&spec.answer_kind) && !contracted_template {
        return Some(format!(
            "answer kind {} is not symbolically decidable",
            spec.answer_kind
        ));
    }
    None
}

/// Make one model call, and put its bill in the ledger BEFORE anything else
/// reads the reply (T6).
///
/// A ledger write that fails stops nothing and reaches the log with the record
/// it did not write, so no paid call is ever recorded as free.
async fn call_model(
    db: &Db,
    job: &AuthoringJob,
    kind: Kind,
    spec: &AuthoringSpec,
    feedback: Option<&str>,
    kp_id: &str,
) -> Call {
    let request = prompt::request(kind, spec, feedback);
    let call = job
        .client
        .call_guarded(
            &request,
            |body, max_tokens| {
                job.budget
                    .as_ref()
                    .map_or(Ok(()), |budget| budget.prepare(body, max_tokens))
            },
            |attempt| {
                if let Some(budget) = &job.budget {
                    budget.observe(attempt);
                }
            },
        )
        .await;
    let record = CallRecord {
        purpose: PURPOSE_AUTHORING,
        user_id: None,
        session_id: None,
    };
    if let Err(err) = model_log::write(db, &record, &call.attempts).await {
        tracing::error!(kp = %kp_id, error = %err, attempts = ?call.attempts,
                        "authoring: the model-call ledger did not write");
    }
    call
}

/// Store one verified body, and read what the table did with it (C6, T3).
///
/// A collision with a REFUSED row is not a store and not a duplicate. The
/// reviewer's verdict stands, the knowledge point still needs a document, and
/// the reason reaches the operator (finding F6). A collision with a row that
/// named an older prompt is a refresh: the row leaves the stale set and the next
/// pass makes no call (V3).
async fn store_verified(
    db: &Db,
    kp_id: String,
    kind: Kind,
    body: &str,
    attempt: u32,
    http_attempts: Vec<Attempt>,
    mut reasons: Vec<String>,
) -> Result<Report, WorkerError> {
    // T3: the row carries the count of calls the pass spent and the sum of
    // what those calls cost.
    let spend = cost::spend(&http_attempts);
    let stored = store_pending(db, &kp_id, kind, body, attempt, &spend).await?;
    let kind_name = kind.as_str();
    let refused = stored.refused();
    let outcome = if stored.inserted {
        Outcome::Stored
    } else if refused {
        Outcome::Rejected { same_body: true }
    } else if stored.refreshed {
        Outcome::Refreshed
    } else {
        Outcome::Duplicate
    };
    let decline = if refused {
        let reason = stored.same_body_reason();
        tracing::warn!(kp = %kp_id, kind = kind_name, attempt, digest = %stored.digest, reason,
                       "authoring: the pass reproduced a refused body");
        reasons.push(reason);
        Some(Decline {
            kp_id: kp_id.clone(),
            kind,
            attempts: attempt,
            reasons,
        })
    } else {
        let cost_usd = stored.cost_usd.as_deref().unwrap_or("unknown");
        tracing::info!(kp = %kp_id, kind = kind_name, attempt, digest = %stored.digest,
                       inserted = stored.inserted, cost_usd,
                       "authoring: the gate accepted the document");
        None
    };
    let alert = cost::raise(&kp_id, kind, attempt, stored.cost_usd.as_deref());
    Ok(Report {
        kp_id,
        kind,
        outcome,
        attempts: attempt,
        digest: Some(stored.digest),
        decline,
        http_attempts,
        cost_usd: stored.cost_usd,
        alert,
    })
}

/// The material the two instruction gates read: the instances the templates of
/// the knowledge point serve, `approved` AND `pending` (M6 review, findings F2,
/// F15 and F25; M6 review 2, finding V1).
///
/// The read runs once per pass, before the first call, so a five-attempt retry
/// costs one statement. The template gate and the diagnosis gate read the
/// document's own instances, so their list is empty.
async fn gate_material(
    db: &Db,
    kind: Kind,
    kp_id: &str,
) -> Result<Vec<ServedInstance>, WorkerError> {
    match kind {
        Kind::Teach | Kind::HintLadder => served_instances(db, kp_id).await,
        Kind::Template | Kind::Diagnosis => Ok(Vec::new()),
    }
}

/// Author one knowledge point and one kind (spec section 2.2).
///
/// The pass makes at most the attempt bound of the job in model calls and stores
/// at most one document, exactly as 1.0 authors one template for a short bank
/// and never a burst (`problem_templates.py:1298-1326`).
///
/// # Errors
///
/// Returns [`WorkerError::Store`] or [`WorkerError::Db`] when the database
/// refuses a statement. A model failure and a gate rejection are never errors of
/// this function: both end in an [`Outcome`], because the retry is the answer to
/// them.
pub async fn author_one(
    db: &Db,
    job: &AuthoringJob,
    kind: Kind,
    spec: &AuthoringSpec,
) -> Result<Report, WorkerError> {
    let kp_id = kp_key(&spec.topic_id, &spec.kp_id);
    let kind_name = kind.as_str();

    // Step 1: a full bank makes ZERO model calls. The check runs before every
    // other one, because it is the check that keeps T3 amortized: one knowledge
    // point is paid for once and then serves forever.
    let taken = slots_taken(db, &kp_id, kind).await?;
    // Spec section 2.2, "Prompt digest": a slot an EDITED prompt wrote does not
    // fill the bank. The pass re-authors it, and the old row keeps the approval
    // it has (M6 review finding F4).
    let stale = stale_slots(db, &kp_id, kind).await?;
    if taken.saturating_sub(stale) >= bank_target(kind) {
        tracing::debug!(kp = %kp_id, kind = kind_name, taken, stale,
                        "authoring: the bank is full; the pass makes no call");
        return Ok(Report::quiet(kp_id, kind, Outcome::Skipped));
    }
    if stale > 0 {
        tracing::info!(kp = %kp_id, kind = kind_name, taken, stale,
                       "authoring: a stored document names an older prompt; the pass re-authors");
    }

    // Step 2: a knowledge point the gate can never accept costs nothing.
    if let Some(reason) = undecidable(kind, spec) {
        tracing::warn!(kp = %kp_id, reason, "authoring: the knowledge point declines with no call");
        let decline = Decline {
            kp_id: kp_id.clone(),
            kind,
            attempts: 0,
            reasons: vec![reason],
        };
        return Ok(Report::declined(kp_id, kind, decline, Vec::new()));
    }

    // Step 3: the material of the gate, then the attempts.
    let instance_answers = gate_material(db, kind, &kp_id).await?;
    let mut feedback: Option<String> = None;
    let mut http_attempts: Vec<Attempt> = Vec::new();
    let mut reasons: Vec<String> = Vec::new();
    let mut spent = 0_u32;

    for attempt in 1..=job.attempts {
        let call = call_model(db, job, kind, spec, feedback.as_deref(), &kp_id).await;
        if call.attempts.is_empty() {
            reasons.push(
                call.result
                    .err()
                    .map_or_else(|| "no HTTP request".to_owned(), |e| e.to_string()),
            );
            break;
        }
        spent = attempt;
        http_attempts.extend(call.attempts);

        let refusal = match call.result {
            // A transport or reply failure is not authoring feedback: the model
            // never saw a document, so the next attempt repeats the message it
            // already had.
            Err(err) => err.to_string(),
            Ok(arguments) => match verify_kind(kind, spec, &arguments, &instance_answers) {
                Ok(body) => {
                    return store_verified(db, kp_id, kind, &body, attempt, http_attempts, reasons)
                        .await;
                }
                Err(rejection) => {
                    // The LITERAL message becomes the next attempt's feedback.
                    feedback = Some(rejection.message.clone());
                    format!("{}: {}", rejection.code, rejection.message)
                }
            },
        };
        tracing::warn!(kp = %kp_id, kind = kind_name, attempt, reason = refusal,
                       "authoring: the attempt was refused");
        reasons.push(refusal);
    }

    tracing::error!(kp = %kp_id, kind = kind_name, attempts = spent,
                    "authoring: the knowledge point declines; nothing is stored");
    // T3: a decline stores no row, so the alert is the only place its spend is
    // named. A pass that used every attempt is above the bound by definition.
    let alert = cost::raise(&kp_id, kind, spent, None);
    let decline = Decline {
        kp_id: kp_id.clone(),
        kind,
        attempts: spent,
        reasons,
    };
    Ok(Report {
        alert,
        ..Report::declined(kp_id, kind, decline, http_attempts)
    })
}

/// The specs of one batch, the pairs an older prompt wrote first.
///
/// Spec section 2.2, "Prompt digest": a prompt edit marks the affected rows for
/// re-authoring, and the batch loop takes those pairs before the rest. A pass an
/// operator stops halfway therefore spends its calls on the stale material and
/// not on a knowledge point that is already current (M6 review finding F4).
///
/// The order inside each half is the caller's order, so a batch with no stale
/// row runs exactly as it ran before.
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when a count fails or the bound expires.
pub(super) async fn stale_first<'a>(
    db: &Db,
    kind: Kind,
    specs: &'a [AuthoringSpec],
) -> Result<Vec<&'a AuthoringSpec>, WorkerError> {
    let mut stale: Vec<&AuthoringSpec> = Vec::new();
    let mut rest: Vec<&AuthoringSpec> = Vec::new();
    for spec in specs {
        let key = kp_key(&spec.topic_id, &spec.kp_id);
        if stale_slots(db, &key, kind).await? > 0 {
            stale.push(spec);
        } else {
            rest.push(spec);
        }
    }
    stale.extend(rest);
    Ok(stale)
}

/// Count one report of [`author_one`] in the batch.
pub(super) fn count(batch: &mut BatchReport, report: Report) {
    batch.calls = batch.calls.saturating_add(report.attempts);
    if report.alert {
        batch.alerts = batch.alerts.saturating_add(1);
    }
    match report.outcome {
        Outcome::Stored => batch.stored += 1,
        // V3: a refresh wrote no document either. It counts where a duplicate
        // counts, and the summary prints the count.
        Outcome::Duplicate | Outcome::Refreshed => batch.duplicate += 1,
        Outcome::Skipped => batch.skipped += 1,
        // C6: a re-author of a refused body wrote nothing, so it counts where
        // a decline counts (M6 review findings F1 and F6).
        Outcome::Rejected { .. } | Outcome::Declined => batch.declined += 1,
    }
    if let Some(decline) = report.decline {
        batch.declines.push(decline);
    }
}

/// Author one kind over a list of knowledge points (A2: a batch job; R4: never a
/// request handler).
///
/// One knowledge point that declines never stops the batch: the pass records the
/// decline and goes on to the next knowledge point.
///
/// # Errors
///
/// Returns the error of [`author_one`].
pub async fn run_batch(
    db: &Db,
    job: &AuthoringJob,
    kind: Kind,
    specs: &[AuthoringSpec],
) -> Result<BatchReport, WorkerError> {
    let mut batch = BatchReport::default();
    for spec in stale_first(db, kind, specs).await? {
        count(&mut batch, author_one(db, job, kind, spec).await?);
    }
    tracing::info!(
        stored = batch.stored,
        duplicate = batch.duplicate,
        skipped = batch.skipped,
        declined = batch.declined,
        calls = batch.calls,
        alerts = batch.alerts,
        "authoring: the batch is complete"
    );
    Ok(batch)
}

#[cfg(test)]
mod tests {
    use super::{BatchReport, Decline, Outcome, Report, count, undecidable};
    use crate::authoring::prompt::{AuthoringSpec, Kind};
    use cadus_core::curriculum::AnswerKind;

    /// A spec of this answer kind.
    fn spec(answer_kind: AnswerKind) -> AuthoringSpec {
        AuthoringSpec {
            kp_id: "squares".to_owned(),
            kp_name: "Perfect squares".to_owned(),
            topic_id: "perfect-squares".to_owned(),
            topic_name: "Perfect squares".to_owned(),
            answer_kind,
            difficulty_target: None,
            constraints: None,
            exemplars: Vec::new(),
        }
    }

    /// The zero-call guard reads the answer kind on the two gated kinds only
    /// (T3, finding F19).
    #[test]
    fn only_the_gated_kinds_decline_an_undecidable_answer_kind() {
        let proof = spec(AnswerKind::Proof);
        let reason = Some("answer kind proof is not symbolically decidable".to_owned());
        assert_eq!(undecidable(Kind::Template, &proof), reason);
        assert_eq!(undecidable(Kind::Diagnosis, &proof), reason);
        assert_eq!(undecidable(Kind::Teach, &proof), None);
        assert_eq!(undecidable(Kind::HintLadder, &proof), None);
        assert_eq!(
            undecidable(Kind::Template, &spec(AnswerKind::Numeric)),
            None
        );
    }

    /// Every outcome lands in one column of the batch, and a decline record
    /// reaches the list.
    #[test]
    fn the_batch_counts_every_outcome_in_its_own_column() {
        let mut batch = BatchReport::default();
        let decline = Decline {
            kp_id: "perfect-squares/squares".to_owned(),
            kind: Kind::Template,
            attempts: 5,
            reasons: Vec::new(),
        };
        let outcomes = [
            (Outcome::Stored, 1, true),
            (Outcome::Duplicate, 1, false),
            (Outcome::Refreshed, 1, false),
            (Outcome::Skipped, 0, false),
            (Outcome::Rejected { same_body: true }, 1, false),
            (Outcome::Declined, 5, true),
        ];
        for (outcome, attempts, alert) in outcomes {
            let mut report = Report::quiet(
                "perfect-squares/squares".to_owned(),
                Kind::Template,
                outcome,
            );
            report.attempts = attempts;
            report.alert = alert;
            if outcome == Outcome::Declined {
                report.decline = Some(decline.clone());
            }
            count(&mut batch, report);
        }
        assert_eq!(batch.stored, 1);
        assert_eq!(batch.duplicate, 2);
        assert_eq!(batch.skipped, 1);
        assert_eq!(batch.declined, 2);
        assert_eq!(batch.calls, 9);
        assert_eq!(batch.alerts, 2);
        assert_eq!(batch.declines, vec![decline]);
    }
}
