//! The per-knowledge-point batch loop (A2, C6, T2, T3, T5, T6, R4).
//!
//! Spec: `docs/reference/authoring-and-spa-1.0-spec.md` section 2.2, and row R2
//! of section 7.
//!
//! # One pass over one knowledge point and one kind
//!
//! ```text
//!   slots_taken()      -- an approved or pending slot is occupied;
//!      │                  a full bank makes ZERO model calls
//!      ▼
//!   request(kind, spec, feedback)  -- attempt 1 carries no feedback
//!      │
//!      ▼
//!   Client::call()     -- the T5 body; one ledger row per HTTP attempt (T6)
//!      │
//!      ├─ Ok  -> assemble -> gate -> Ok  -> store `pending` -> STORED
//!      │                        └─ Err -> the LITERAL message becomes the
//!      │                                  feedback of the next attempt
//!      └─ Err -> the transport reason becomes the reason of this attempt
//!      │
//!      ▼ (after AUTHORING_ATTEMPTS refusals)
//!   DECLINED           -- no `content_store` row, one decline record
//! ```
//!
//! # Why the rejection message goes in verbatim
//!
//! 1.0 measures the rejection message as the biggest yield lever: three of four
//! knowledge points were refused on the first attempt, for reasons that read as
//! instructions, and a told retry converted most of them
//! (`problem_templates.py:1386-1393`). [`prompt::request`] therefore takes the
//! gate's own [`Rejection`] text and nothing else. A summary is a different
//! instruction.
//!
//! # Why five attempts
//!
//! 1.0 stops at two, because a doomed knowledge point paid two model calls per
//! SERVE forever (`problem_templates.py:1024-1029`). The 2.0 pipeline is offline
//! and pays per BATCH, so the spec raises the bound to five (section 2.2, step
//! 3). T3 alerts above three attempts; it does not stop the loop.
//!
//! # Why a decline writes no row
//!
//! 1.0 parks a decline marker in the cache slot, so a later SERVE does not pay
//! for the same doomed knowledge point again. 2.0 has no serve-time authoring at
//! all: A6 serves the authored exemplars of a knowledge point with no approved
//! template, and the operator dashboard flags it. A decline is therefore an
//! operator fact, not content: it reaches the caller in [`Report::decline`] and
//! the log, and it never occupies a `content_store` row that a reviewer would
//! have to approve or reject.
//!
//! # What this unit does not do
//!
//! Kinds `teach`, `hint_ladder` and `diagnosis` have no gate in
//! `cadus_core::template` yet, and a loop with no gate would spend tokens on
//! content nothing can verify. [`author_one`] answers [`Outcome::NoGate`] for
//! those three kinds and makes zero calls. Units R6 and R7 add the gates.

use cadus_core::pool::kp_key;
use cadus_core::template::{
    GateSpec, Rejection, TEMPLATABLE_KINDS, TEMPLATE_VERSION, gate_body, to_body, with_space_size,
};
use cadus_model_client::{Attempt, Client};
use cadus_store::Db;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::WorkerError;
use crate::authoring::prompt::{self, AuthoringSpec, DIGEST_CHARS, Kind};
use crate::model_log::{self, CallRecord, PURPOSE_AUTHORING};

/// The model calls one knowledge point and kind gets before it declines.
///
/// Spec section 2.2, step 3. 1.0's bound is 2 (`problem_templates.py:178`).
pub const AUTHORING_ATTEMPTS: u32 = 5;

/// The approved templates one knowledge point keeps (1.0 `BANK_TARGET`,
/// `problem_templates.py:156`).
///
/// A knowledge point that serves one template forever serves one problem shape
/// forever, which Hard Rule 4 refuses.
pub const BANK_TARGET: i64 = 3;

/// The approved documents one knowledge point keeps of every other kind.
///
/// A teach page and a hint ladder are one document per knowledge point (spec
/// section 2.2, "Bank target").
pub const SINGLE_DOCUMENT: i64 = 1;

/// The `content_store.status` of an authored document that waits for a human (C6).
pub const STATUS_PENDING: &str = "pending";

/// The `content_store.status` of a document a human approved (C6).
pub const STATUS_APPROVED: &str = "approved";

/// The prefix of a `content_store.digest`, so the row names its hash function.
pub const DIGEST_PREFIX: &str = "sha256:";

/// The refusal a forced tool call with no arguments object earns.
pub const NO_ARGUMENTS: &str = "the tool call carried no JSON object of arguments — emit every required field of the tool in \
one object";

/// What one pass did with one knowledge point and kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The bank is full. The pass made no model call.
    Skipped,
    /// The kind has no gate yet (units R6 and R7). The pass made no model call.
    NoGate,
    /// The gate accepted a document and the row is `pending`.
    Stored,
    /// The gate accepted a document whose digest the table already holds.
    Duplicate,
    /// Every attempt was refused. Nothing is stored.
    Declined,
}

/// One knowledge point and kind that used every attempt (spec section 2.2).
///
/// The record is what an operator reads: the reasons are the gate's own
/// messages, in the order the attempts earned them, so a doomed knowledge point
/// explains itself without a log dig.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decline {
    /// The serving key `"<topic_id>/<kp_id>"`.
    pub kp_id: String,
    /// The kind the pass tried to author.
    pub kind: Kind,
    /// The model calls the pass spent.
    pub attempts: u32,
    /// One reason per attempt, in order.
    pub reasons: Vec<String>,
}

/// What one pass of [`author_one`] did, with its bill (T6).
#[derive(Debug)]
pub struct Report {
    /// The serving key `"<topic_id>/<kp_id>"`.
    pub kp_id: String,
    /// The kind the pass authored.
    pub kind: Kind,
    /// What the pass did.
    pub outcome: Outcome,
    /// The model calls the pass spent. 0 for a skip.
    pub attempts: u32,
    /// The digest of the stored document, when the pass stored one.
    pub digest: Option<String>,
    /// The decline record, when every attempt was refused.
    pub decline: Option<Decline>,
    /// One record per HTTP attempt, in order (T6). Unit R3 sums the cost.
    pub http_attempts: Vec<Attempt>,
}

/// What one pass of [`run_batch`] did over a list of knowledge points.
#[derive(Debug, Default)]
pub struct BatchReport {
    /// The knowledge points that hold a `pending` document after the pass.
    pub stored: u32,
    /// The knowledge points the pass did not call for.
    pub skipped: u32,
    /// The knowledge points that used every attempt.
    pub declined: u32,
    /// The model calls the whole pass spent (T3).
    pub calls: u32,
    /// One record per declined knowledge point.
    pub declines: Vec<Decline>,
}

/// The model client and the attempt bound of the running job.
#[derive(Debug)]
pub struct AuthoringJob {
    client: Client,
    attempts: u32,
}

impl AuthoringJob {
    /// Build the job around a client, with the [`AUTHORING_ATTEMPTS`] bound.
    #[must_use]
    pub const fn new(client: Client) -> Self {
        Self {
            client,
            attempts: AUTHORING_ATTEMPTS,
        }
    }

    /// Build the job with another attempt bound.
    ///
    /// A bound of 0 makes no call and declines at once, which is what the dry
    /// run of unit R8 wants.
    #[must_use]
    pub const fn with_attempts(client: Client, attempts: u32) -> Self {
        Self { client, attempts }
    }

    /// The attempt bound of this job.
    #[must_use]
    pub const fn attempts(&self) -> u32 {
        self.attempts
    }

    /// The client this job calls.
    #[must_use]
    pub const fn client(&self) -> &Client {
        &self.client
    }
}

/// The approved documents one kind keeps per knowledge point.
#[must_use]
pub const fn bank_target(kind: Kind) -> i64 {
    match kind {
        Kind::Template => BANK_TARGET,
        Kind::Teach | Kind::HintLadder | Kind::Diagnosis => SINGLE_DOCUMENT,
    }
}

/// The content address of one stored body (C6, spec section 8, trap 8).
///
/// The digest covers the WHOLE body, `samples` and `space_size` included, so a
/// change to either asks for a new approval. The text it hashes is the output of
/// [`to_body`], which serde writes in field order with no whitespace, so a
/// recomputation reads the row through `from_body` and writes it through
/// `to_body` again. The column holds jsonb, and jsonb keeps neither key order
/// nor whitespace.
#[must_use]
pub fn body_digest(body: &str) -> String {
    let hash = Sha256::digest(body.as_bytes());
    let mut hex = String::with_capacity(DIGEST_CHARS);
    for byte in hash.iter().take(DIGEST_CHARS.div_ceil(2)) {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex.truncate(DIGEST_CHARS);
    format!("{DIGEST_PREFIX}{hex}")
}

/// Turn the arguments of one forced tool call into a template body.
///
/// The tool asks for the authored subset only. `v`, `topic_id` and `answer_kind`
/// are server-side: the knowledge point owns them, so a model that states them
/// wrong rewrites the row's own identity. This function drops all three, and
/// `space_size`, and writes the server's values instead
/// (`problem_templates.py:309`).
///
/// # Errors
///
/// Returns the [`Rejection`] the retry block carries when the tool call answered
/// no JSON object.
pub fn assemble(spec: &AuthoringSpec, arguments: &Value) -> Result<String, Rejection> {
    let Some(fields) = arguments.as_object() else {
        return Err(Rejection {
            code: "tool-arguments",
            message: NO_ARGUMENTS.to_owned(),
        });
    };
    let mut body = fields.clone();
    for server_side in ["v", "topic_id", "answer_kind", "space_size"] {
        body.remove(server_side);
    }
    body.insert("v".to_owned(), Value::from(TEMPLATE_VERSION));
    body.insert("topic_id".to_owned(), Value::from(spec.topic_id.clone()));
    body.insert(
        "answer_kind".to_owned(),
        Value::from(spec.answer_kind.as_str()),
    );
    serde_json::to_string(&Value::Object(body)).map_err(|err| Rejection {
        code: "tool-arguments",
        message: format!("the tool arguments do not write as JSON: {err}"),
    })
}

/// Assemble, gate, and fill in the satisfying count.
///
/// The returned text is what the row stores and what the digest covers.
///
/// # Errors
///
/// Returns the [`Rejection`] of [`assemble`] and every rejection of
/// `cadus_core::template::gate`. The message is the literal text of the check
/// that refused the document, and it is what the next attempt reads.
pub fn verify(spec: &AuthoringSpec, arguments: &Value) -> Result<String, Rejection> {
    let body = assemble(spec, arguments)?;
    let gate_spec = GateSpec {
        answer_kind: spec.answer_kind,
        exemplars: &spec.exemplars,
    };
    let (doc, verified) = gate_body(&body, &gate_spec)?;
    let filled = with_space_size(&doc, &verified);
    to_body(&filled).map_err(|err| Rejection {
        code: "tool-arguments",
        message: format!("the verified document does not write as JSON: {err}"),
    })
}

/// How many slots of one knowledge point and kind are occupied (C6).
///
/// An approved row serves; a pending row waits for a human. Both occupy a slot,
/// so a nightly pass never floods the review queue with a second copy of a
/// document nobody has read yet (1.0 `problem_templates.py:1352-1374`). A
/// `rejected` row occupies nothing: a human refused that body, and the knowledge
/// point still needs a document.
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when the statement fails or the bound expires.
pub async fn slots_taken(db: &Db, kp_id: &str, kind: Kind) -> Result<i64, WorkerError> {
    let query = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!"
          FROM content_store
         WHERE kp_id = $1 AND kind = $2 AND status IN ($3, $4)
        "#,
        kp_id,
        kind.as_str(),
        STATUS_APPROVED,
        STATUS_PENDING,
    )
    .fetch_one(db.pool());
    Ok(cadus_store::bounded(db, query).await?)
}

/// Insert one verified document as `pending` (C6, spec section 2.2, step 4).
///
/// The worker connects as `cadus_admin`. `cadus_app` holds SELECT only on the
/// table (`docs/SCHEMA.md`, finding #14), so the request tier can never write a
/// row that already carries `status = 'approved'`.
///
/// Returns `true` when the row is new. A digest the table already holds is not
/// an error and not a rewrite: the body is the same body, and a human may have
/// rejected it already, so `ON CONFLICT DO NOTHING` leaves that verdict alone.
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when the statement fails or the bound expires,
/// and [`WorkerError::Config`] when the verified body does not read as JSON.
pub async fn store_pending(
    db: &Db,
    kp_id: &str,
    kind: Kind,
    body: &str,
    attempts: u32,
) -> Result<bool, WorkerError> {
    let document: Value = serde_json::from_str(body)
        .map_err(|err| WorkerError::Config(format!("the verified body does not read: {err}")))?;
    let query = sqlx::query!(
        r#"
        INSERT INTO content_store (digest, kp_id, kind, body, status, authoring_attempts)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (digest) DO NOTHING
        "#,
        body_digest(body),
        kp_id,
        kind.as_str(),
        document,
        STATUS_PENDING,
        i32::try_from(attempts).unwrap_or(i32::MAX),
    )
    .execute(db.pool());
    Ok(cadus_store::bounded(db, query).await?.rows_affected() == 1)
}

/// Author one knowledge point and one kind (spec section 2.2).
///
/// The pass makes at most [`AuthoringJob::attempts`] model calls and stores at
/// most one document, exactly as 1.0 authors one template for a short bank and
/// never a burst (`problem_templates.py:1298-1326`).
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
    let quiet = |outcome, key: String| Report {
        kp_id: key,
        kind,
        outcome,
        attempts: 0,
        digest: None,
        decline: None,
        http_attempts: Vec::new(),
    };

    if kind != Kind::Template {
        tracing::info!(kp = %kp_id, kind = kind.as_str(),
                       "authoring: the kind has no gate yet; the pass makes no call");
        return Ok(quiet(Outcome::NoGate, kp_id));
    }

    // Step 1: a full bank makes ZERO model calls. The check runs before every
    // other one, because it is the check that keeps T3 amortized: one knowledge
    // point is paid for once and then serves forever.
    let taken = slots_taken(db, &kp_id, kind).await?;
    if taken >= bank_target(kind) {
        tracing::debug!(kp = %kp_id, kind = kind.as_str(), taken,
                        "authoring: the bank is full; the pass makes no call");
        return Ok(quiet(Outcome::Skipped, kp_id));
    }

    // Step 2: a knowledge point the gate can never accept costs nothing. The
    // gate refuses every document of an undecidable answer kind, so five calls
    // would buy five copies of one refusal (T3).
    let mut reasons: Vec<String> = Vec::new();
    if !TEMPLATABLE_KINDS.contains(&spec.answer_kind) {
        let reason = format!(
            "answer kind {} is not symbolically decidable",
            spec.answer_kind
        );
        tracing::warn!(kp = %kp_id, reason, "authoring: the knowledge point declines with no call");
        reasons.push(reason);
        let mut report = quiet(Outcome::Declined, kp_id.clone());
        report.decline = Some(Decline {
            kp_id,
            kind,
            attempts: 0,
            reasons,
        });
        return Ok(report);
    }

    let mut feedback: Option<String> = None;
    let mut http_attempts: Vec<Attempt> = Vec::new();
    let mut spent = 0_u32;

    for attempt in 1..=job.attempts {
        let request = prompt::request(kind, spec, feedback.as_deref());
        let call = job.client.call(&request).await;
        spent = attempt;

        // T6: the bill lands BEFORE the document does, so no paid call is ever
        // recorded as free. A ledger write that fails stops nothing and reaches
        // the log with the record it did not write.
        let record = CallRecord {
            purpose: PURPOSE_AUTHORING,
            user_id: None,
            session_id: None,
        };
        if let Err(err) = model_log::write(db, &record, &call.attempts).await {
            tracing::error!(kp = %kp_id, error = %err, attempts = ?call.attempts,
                            "authoring: the model-call ledger did not write");
        }
        http_attempts.extend(call.attempts);

        let refusal = match call.result {
            // A transport or reply failure is not authoring feedback: the model
            // never saw a document, so the next attempt repeats the message it
            // already had.
            Err(err) => err.to_string(),
            Ok(arguments) => match verify(spec, &arguments) {
                Ok(body) => {
                    let digest = body_digest(&body);
                    let inserted = store_pending(db, &kp_id, kind, &body, attempt).await?;
                    let outcome = if inserted {
                        Outcome::Stored
                    } else {
                        Outcome::Duplicate
                    };
                    tracing::info!(kp = %kp_id, kind = kind.as_str(), attempt, digest,
                                   inserted, "authoring: the gate accepted the document");
                    return Ok(Report {
                        kp_id,
                        kind,
                        outcome,
                        attempts: attempt,
                        digest: Some(digest),
                        decline: None,
                        http_attempts,
                    });
                }
                Err(rejection) => {
                    // The LITERAL message becomes the next attempt's feedback.
                    feedback = Some(rejection.message.clone());
                    format!("{}: {}", rejection.code, rejection.message)
                }
            },
        };
        tracing::warn!(kp = %kp_id, kind = kind.as_str(), attempt, reason = refusal,
                       "authoring: the attempt was refused");
        reasons.push(refusal);
    }

    tracing::error!(kp = %kp_id, kind = kind.as_str(), attempts = spent,
                    "authoring: the knowledge point declines; nothing is stored");
    Ok(Report {
        kp_id: kp_id.clone(),
        kind,
        outcome: Outcome::Declined,
        attempts: spent,
        digest: None,
        decline: Some(Decline {
            kp_id,
            kind,
            attempts: spent,
            reasons,
        }),
        http_attempts,
    })
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
    for spec in specs {
        let report = author_one(db, job, kind, spec).await?;
        batch.calls = batch.calls.saturating_add(report.attempts);
        match report.outcome {
            Outcome::Stored | Outcome::Duplicate => batch.stored += 1,
            Outcome::Skipped | Outcome::NoGate => batch.skipped += 1,
            Outcome::Declined => batch.declined += 1,
        }
        if let Some(decline) = report.decline {
            batch.declines.push(decline);
        }
    }
    tracing::info!(
        stored = batch.stored,
        skipped = batch.skipped,
        declined = batch.declined,
        calls = batch.calls,
        "authoring: the batch is complete"
    );
    Ok(batch)
}

#[cfg(test)]
mod tests {
    use super::{
        AUTHORING_ATTEMPTS, BANK_TARGET, DIGEST_PREFIX, NO_ARGUMENTS, SINGLE_DOCUMENT, assemble,
        bank_target, body_digest,
    };
    use crate::authoring::prompt::{AuthoringSpec, Kind};
    use cadus_core::curriculum::AnswerKind;
    use serde_json::json;

    /// The two bounds of spec section 2.2, as literals.
    #[test]
    fn the_bounds_are_five_attempts_and_a_bank_of_three() {
        assert_eq!(AUTHORING_ATTEMPTS, 5);
        assert_eq!(BANK_TARGET, 3);
        assert_eq!(SINGLE_DOCUMENT, 1);
        assert_eq!(bank_target(Kind::Template), 3);
        assert_eq!(bank_target(Kind::Teach), 1);
        assert_eq!(bank_target(Kind::HintLadder), 1);
        assert_eq!(bank_target(Kind::Diagnosis), 1);
    }

    /// The digest is the prefix and 16 hex characters of the SHA-256 of the body.
    ///
    /// The literal below is `sha256sum` of the two bytes `{}`:
    /// `44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a`.
    #[test]
    fn the_digest_is_sixteen_hex_characters_of_sha256() {
        assert_eq!(body_digest("{}"), "sha256:44136fa355b3678a");
        assert_eq!(body_digest("{}").len(), DIGEST_PREFIX.len() + 16);
        assert_ne!(body_digest("{}"), body_digest("{ }"));
    }

    /// A spec for the perfect-squares knowledge point.
    fn spec() -> AuthoringSpec {
        AuthoringSpec {
            kp_id: "squares".to_owned(),
            kp_name: "Perfect squares".to_owned(),
            topic_id: "perfect-squares".to_owned(),
            topic_name: "Perfect squares".to_owned(),
            answer_kind: AnswerKind::Numeric,
            difficulty_target: None,
            constraints: None,
            exemplars: Vec::new(),
        }
    }

    /// The three server-side fields are the server's, whatever the model sends.
    #[test]
    fn the_server_side_fields_overwrite_what_the_model_sends() {
        let body = assemble(
            &spec(),
            &json!({
                "v": 99,
                "topic_id": "another-topic",
                "answer_kind": "proof",
                "space_size": 1_000_000,
                "statement": "Compute ${a}^{{2}}$.",
                "answer_expr": "a**2"
            }),
        )
        .expect("the arguments are an object");
        let read: serde_json::Value = serde_json::from_str(&body).expect("the body reads");
        assert_eq!(read["v"], json!(1));
        assert_eq!(read["topic_id"], json!("perfect-squares"));
        assert_eq!(read["answer_kind"], json!("numeric"));
        assert_eq!(read.get("space_size"), None);
        assert_eq!(read["statement"], json!("Compute ${a}^{{2}}$."));
    }

    /// A tool call with no arguments object is a rejection, not a panic.
    #[test]
    fn arguments_that_are_not_an_object_are_a_rejection() {
        let rejection = assemble(&spec(), &json!("emit_template"))
            .expect_err("a string is not an arguments object");
        assert_eq!(rejection.code, "tool-arguments");
        assert_eq!(rejection.message, NO_ARGUMENTS);
    }
}
