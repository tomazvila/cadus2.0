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
//! # The bill of one pass (T3)
//!
//! Every attempt writes its `model_call_log` rows BEFORE the document reaches
//! the table, so no paid call is recorded as free. The stored row then carries
//! `authoring_attempts` — the count of model calls this pass spent — and
//! `authoring_cost_usd` — the sum of what those calls cost. A pass above
//! [`cost::ATTEMPT_ALERT`] raises the T3 operator alert and sets
//! [`Report::alert`]; the alert stops nothing. `crate::authoring::cost` holds
//! the rules.
//!
//! # The four gates the loop runs
//!
//! One loop serves every kind, and the kind picks the gate ([`verify_kind`]):
//! `template` goes to `cadus_core::template::gate_body`, `teach` to
//! `cadus_core::instruction::gate_teach`, `hint_ladder` to
//! `cadus_core::instruction::gate_hint_ladder` (unit R6), and `diagnosis` to
//! `cadus_core::template::gate_diagnosis_body` (unit R7). Every gate returns the
//! same [`Rejection`], so the retry block carries a literal message whatever the
//! kind is.
//!
//! # What this unit does not do
//!
//! Every kind has its gate now. Unit R6 added the teach gate and the hint ladder
//! gate (`cadus_core::instruction`); unit R7 added the diagnosis gate
//! (`cadus_core::template::distractor`). [`verify_kind`] answers a gate for all
//! four kinds, so no kind reaches the table unverified.

use cadus_core::instruction::{InstructionSpec, gate_hint_ladder, gate_teach};
use cadus_core::pool::kp_key;
use cadus_core::template::{
    GateSpec, Rejection, TEMPLATABLE_KINDS, TEMPLATE_VERSION, gate_body, gate_diagnosis_body,
    keep_known_tags, to_body, to_diagnosis_body, with_space_size,
};
use cadus_model_client::{Attempt, Client};
use cadus_store::Db;
use cadus_store::content::{Admin, NewDocument, insert_pending};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::WorkerError;
use crate::authoring::cost;
use crate::authoring::prompt::{self, AuthoringSpec, DIGEST_CHARS, Kind};
use crate::diagnosis::MODEL_ERROR_TAGS;
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
    /// One record per HTTP attempt, in order (T6).
    pub http_attempts: Vec<Attempt>,
    /// The money the stored row carries, as the exact text of
    /// `content_store.authoring_cost_usd` (T3).
    ///
    /// `None` in two cases: the pass stored no row, or no call of the pass
    /// reported a price.
    pub cost_usd: Option<String>,
    /// Whether the pass raised the T3 alert of
    /// [`cost::ATTEMPT_ALERT`](crate::authoring::cost::ATTEMPT_ALERT).
    pub alert: bool,
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
    /// The knowledge points that raised the T3 attempt alert.
    pub alerts: u32,
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
    write_body(&assemble_value(spec, arguments)?)
}

/// [`assemble`], before the body is written out.
fn assemble_value(spec: &AuthoringSpec, arguments: &Value) -> Result<Value, Rejection> {
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
    Ok(Value::Object(body))
}

/// Write one assembled body as the text the gate reads.
fn write_body(body: &Value) -> Result<String, Rejection> {
    serde_json::to_string(body).map_err(|err| Rejection {
        code: "tool-arguments",
        message: format!("the tool arguments do not write as JSON: {err}"),
    })
}

/// Assemble one body and drop the tags outside the vocabulary (spec section
/// 5.3, row R7).
///
/// The drop runs BEFORE the gate. A distractor note is a rendered field, so a
/// drop after the gate stores a document the gate refuses
/// (`cadus_core::template::keep_known_tags` gives the reason in full).
///
/// The template path calls this, because the template gate holds no vocabulary.
/// The diagnosis path does not: `cadus_core::template::gate_diagnosis_body`
/// takes the vocabulary and runs the same drop in the same place.
fn assemble_kept(kind: Kind, spec: &AuthoringSpec, arguments: &Value) -> Result<String, Rejection> {
    let mut body = assemble_value(spec, arguments)?;
    let dropped = keep_known_tags(&mut body, &authoring_vocabulary());
    report_dropped(spec, kind, &dropped);
    write_body(&body)
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
    // Spec section 5.3, and row R7: an error_tag outside the vocabulary is
    // dropped, on this document and on the diagnosis document alike, and the
    // drop runs before the gate reads the body.
    let body = assemble_kept(Kind::Template, spec, arguments)?;
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

/// The tool arguments of an instruction document, as the text its gate reads.
///
/// An instruction document carries no server-side field: the serve reader of L4
/// and L5 refuses an unknown field, so `v` or `topic_id` on a teach body would
/// serve a `500` instead of a page. The tool arguments are therefore the body,
/// and the gate is the only thing between them and the table.
fn instruction_body(arguments: &Value) -> Result<String, Rejection> {
    if arguments.is_object() {
        Ok(arguments.to_string())
    } else {
        Err(Rejection {
            code: "tool-arguments",
            message: NO_ARGUMENTS.to_owned(),
        })
    }
}

/// The refusal a document that the gate accepted but serde cannot write earns.
fn unwritable(err: &serde_json::Error) -> Rejection {
    Rejection {
        code: "tool-arguments",
        message: format!("the verified document does not write as JSON: {err}"),
    }
}

/// Gate one teach page, and write the body the row stores (L4, unit R6).
///
/// The written text comes from the GATED document and not from the arguments, so
/// a field the gate ignores never reaches the row and never reaches the digest.
///
/// # Errors
///
/// Returns the [`Rejection`] of `cadus_core::instruction::gate_teach`, and the
/// rejection a tool call with no JSON object earns.
pub fn verify_teach(spec: &AuthoringSpec, arguments: &Value) -> Result<String, Rejection> {
    let body = instruction_body(arguments)?;
    let gate_spec = InstructionSpec {
        exemplars: &spec.exemplars,
    };
    serde_json::to_string(&gate_teach(&body, &gate_spec)?).map_err(|err| unwritable(&err))
}

/// Gate one hint ladder, and write the body the row stores (L5, unit R6).
///
/// # Errors
///
/// Returns the [`Rejection`] of `cadus_core::instruction::gate_hint_ladder`, and
/// the rejection a tool call with no JSON object earns.
pub fn verify_hint_ladder(spec: &AuthoringSpec, arguments: &Value) -> Result<String, Rejection> {
    let body = instruction_body(arguments)?;
    let gate_spec = InstructionSpec {
        exemplars: &spec.exemplars,
    };
    serde_json::to_string(&gate_hint_ladder(&body, &gate_spec)?).map_err(|err| unwritable(&err))
}

/// The error tags an authored document keeps (spec section 5.3).
///
/// It is the vocabulary the prompt states to the model
/// ([`MODEL_ERROR_TAGS`]), so the gate keeps exactly what the instruction
/// invites. `blank-answer` stands outside it: the grade path stamps that tag on
/// a blank submission, and no distractor claims a blank answer.
/// `cadus_core::config::default_error_tags` holds every tag of this list, so the
/// grade path never drops a tag the gate kept.
#[must_use]
pub fn authoring_vocabulary() -> Vec<String> {
    MODEL_ERROR_TAGS
        .iter()
        .map(|tag| (*tag).to_owned())
        .collect()
}

/// Log the tags the vocabulary filter dropped, for the operator (T3).
///
/// A drop is silent to the model, because the schema already constrains
/// `error_tag` to the vocabulary and the prompt states the rule. It is not
/// silent to an operator: a model that keeps inventing tags shows up here.
fn report_dropped(spec: &AuthoringSpec, kind: Kind, dropped: &[String]) {
    if dropped.is_empty() {
        return;
    }
    tracing::warn!(
        kp = %kp_key(&spec.topic_id, &spec.kp_id),
        kind = kind.as_str(),
        dropped = ?dropped,
        "authoring: an error_tag outside the vocabulary is dropped at the gate"
    );
}

/// Assemble one distractor list and verify it (A4; spec section 6.2).
///
/// The returned text is what the row stores and what the digest covers. The
/// document is the FILTERED one: `cadus_core::template::gate_diagnosis_body`
/// drops every distractor whose tag is outside
/// [`authoring_vocabulary`], and it refuses the list when nothing is left.
///
/// # Errors
///
/// Returns the [`Rejection`] of [`assemble`] and every rejection of
/// `cadus_core::template::gate_diagnosis`. The message is the literal text the
/// next attempt reads.
pub fn verify_diagnosis(spec: &AuthoringSpec, arguments: &Value) -> Result<String, Rejection> {
    let body = assemble(spec, arguments)?;
    let gate_spec = GateSpec {
        answer_kind: spec.answer_kind,
        exemplars: &spec.exemplars,
    };
    let (doc, dropped) = gate_diagnosis_body(&body, &gate_spec, &authoring_vocabulary())?;
    report_dropped(spec, Kind::Diagnosis, &dropped);
    to_diagnosis_body(&doc).map_err(|err| Rejection {
        code: "tool-arguments",
        message: format!("the verified document does not write as JSON: {err}"),
    })
}

/// The gate of one kind (spec section 2.2, step 2).
///
/// # Errors
///
/// Returns the [`Rejection`] the kind's gate wrote. The message is the literal
/// text the next attempt reads.
pub fn verify_kind(
    kind: Kind,
    spec: &AuthoringSpec,
    arguments: &Value,
) -> Result<String, Rejection> {
    match kind {
        Kind::Template => verify(spec, arguments),
        Kind::Teach => verify_teach(spec, arguments),
        Kind::HintLadder => verify_hint_ladder(spec, arguments),
        Kind::Diagnosis => verify_diagnosis(spec, arguments),
    }
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

/// The money of one authoring pass, as the text `content_store` stores (T3).
///
/// The sum runs in Postgres and every term rounds to [`MONEY_SCALE`](crate::authoring::cost::MONEY_SCALE) first, so
/// the total matches the sum of the ledger rows of the same calls and no step
/// passes through a float (`crate::authoring::cost`).
///
/// The answer is `None` when the pass reported no price at all, and `None` when
/// the total is too large for `numeric(12,6)`. An overflow inside the INSERT
/// would raise and lose the document, so the guard runs here and the row keeps
/// its attempt count with a NULL bill.
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when the statement fails or the bound expires.
async fn total(db: &Db, spend: &[String]) -> Result<Option<String>, WorkerError> {
    let query = sqlx::query_scalar!(
        r#"
        SELECT (CASE WHEN abs(sum(round(c::numeric, 6))) < 1000000
                     THEN sum(round(c::numeric, 6)) END)::text AS "total?"
          FROM unnest($1::text[]) AS c
        "#,
        spend,
    )
    .fetch_one(db.pool());
    Ok(cadus_store::bounded(db, query).await?)
}

/// What the insert of one authored document did (T3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    /// `true` when the row is new, `false` when the table already held the
    /// digest.
    pub inserted: bool,
    /// The exact text of `authoring_cost_usd` on the new row.
    ///
    /// `None` in two cases: the row is not new, or no call of the pass reported
    /// a price.
    pub cost_usd: Option<String>,
}

/// Insert one verified document as `pending`, with its bill (C6, T3; spec
/// section 2.2, step 4).
///
/// The insert itself is [`cadus_store::content::insert_pending`], the one write
/// path of `content_store` (unit R4). The worker connects as `cadus_admin`, and
/// `cadus_app` holds SELECT only on the table (`docs/SCHEMA.md`, finding #14),
/// so the request tier can never write a row that already carries
/// `status = 'approved'`. [`Admin::new`] names that connection at the call site.
///
/// `attempts` is the count of model calls the pass spent, and `spend` is the
/// price of each of those calls that reported one
/// ([`cost::spend`](crate::authoring::cost::spend)). [`total`] sums the prices
/// in Postgres, so the money reaches `numeric(12,6)` with no float step
/// (`crate::authoring::cost`, section "The sum runs in Postgres").
///
/// [`Stored::inserted`] is `true` when the row is new. A digest the table
/// already holds is not an error and not a rewrite: the body is the same body,
/// and a human may have rejected it already, so `ON CONFLICT DO NOTHING` leaves
/// that verdict AND the earlier row's accounting alone. The second pass paid for
/// its own calls, and the ledger holds that spend.
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
    spend: &[String],
) -> Result<Stored, WorkerError> {
    let document: Value = serde_json::from_str(body)
        .map_err(|err| WorkerError::Config(format!("the verified body does not read: {err}")))?;
    let cost_usd = total(db, spend).await?;
    let digest = body_digest(body);
    let doc = NewDocument {
        digest: &digest,
        kp_id,
        kind: kind.as_str(),
        body: &document,
        authoring_attempts: attempts,
        cost_usd: cost_usd.as_deref(),
    };
    let inserted = insert_pending(Admin::new(db), &doc).await?;
    Ok(Stored {
        inserted,
        cost_usd: if inserted { cost_usd } else { None },
    })
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
        cost_usd: None,
        alert: false,
    };

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
    // template gate AND the diagnosis gate refuse every document of an
    // undecidable answer kind, with one message, so five calls of either kind
    // buy five copies of one refusal (T3, finding F19). A teach page and a hint
    // ladder carry no answer expression, so the rule is not theirs: a knowledge
    // point nothing can grade is still a knowledge point a page teaches and a
    // ladder supports.
    let mut reasons: Vec<String> = Vec::new();
    if matches!(kind, Kind::Template | Kind::Diagnosis)
        && !TEMPLATABLE_KINDS.contains(&spec.answer_kind)
    {
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
            Ok(arguments) => match verify_kind(kind, spec, &arguments) {
                Ok(body) => {
                    let digest = body_digest(&body);
                    // T3: the row carries the count of calls the pass spent and
                    // the sum of what those calls cost.
                    let spend = cost::spend(&http_attempts);
                    let stored = store_pending(db, &kp_id, kind, &body, attempt, &spend).await?;
                    let outcome = if stored.inserted {
                        Outcome::Stored
                    } else {
                        Outcome::Duplicate
                    };
                    tracing::info!(kp = %kp_id, kind = kind.as_str(), attempt, digest,
                                   inserted = stored.inserted,
                                   cost_usd = stored.cost_usd.as_deref().unwrap_or("unknown"),
                                   "authoring: the gate accepted the document");
                    let alert = cost::raise(&kp_id, kind, attempt, stored.cost_usd.as_deref());
                    return Ok(Report {
                        kp_id,
                        kind,
                        outcome,
                        attempts: attempt,
                        digest: Some(digest),
                        decline: None,
                        http_attempts,
                        cost_usd: stored.cost_usd,
                        alert,
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
    // T3: a decline stores no row, so the alert is the only place its spend is
    // named. A pass that used every attempt is above the bound by definition.
    let alert = cost::raise(&kp_id, kind, spent, None);
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
        cost_usd: None,
        alert,
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
        if report.alert {
            batch.alerts = batch.alerts.saturating_add(1);
        }
        match report.outcome {
            Outcome::Stored | Outcome::Duplicate => batch.stored += 1,
            Outcome::Skipped => batch.skipped += 1,
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
        alerts = batch.alerts,
        "authoring: the batch is complete"
    );
    Ok(batch)
}

#[cfg(test)]
mod tests {
    use super::{
        AUTHORING_ATTEMPTS, BANK_TARGET, DIGEST_PREFIX, NO_ARGUMENTS, SINGLE_DOCUMENT, assemble,
        bank_target, body_digest, verify_kind, verify_teach,
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
        let instruction = verify_teach(&spec(), &json!("emit_teach"))
            .expect_err("a string is not an arguments object");
        assert_eq!(instruction.code, "tool-arguments");
        assert_eq!(instruction.message, NO_ARGUMENTS);
    }

    /// The stored text of an instruction document is the GATED document and
    /// carries no server-side field: the serve reader refuses one (unit R6).
    #[test]
    fn a_gated_teach_page_stores_the_document_and_nothing_else() {
        let body = verify_kind(
            Kind::Teach,
            &spec(),
            &json!({
                "concept": "A square multiplies a number by itself.",
                "worked_example": {"problem": "Compute $6^2$.", "steps": ["$6 \\times 6 = 36$."]}
            }),
        )
        .expect("the gate accepts the page");

        assert_eq!(
            body,
            r#"{"concept":"A square multiplies a number by itself.","worked_example":{"problem":"Compute $6^2$.","steps":["$6 \\times 6 = 36$."]}}"#
        );
    }

    /// Every kind reaches a gate through [`verify_kind`], so no unverified body
    /// reaches the table (units R6 and R7).
    #[test]
    fn every_kind_reaches_a_gate() {
        let rejection = verify_kind(Kind::Diagnosis, &spec(), &json!({"distractors": []}))
            .expect_err("an empty distractor list is refused");
        assert_eq!(rejection.code, "distractor-missing");

        let rejection = verify_kind(Kind::Teach, &spec(), &json!({}))
            .expect_err("a teach page with no concept is refused");
        assert_eq!(rejection.code, "teach-concept");

        let rejection = verify_kind(Kind::HintLadder, &spec(), &json!({"hints": []}))
            .expect_err("an empty hint ladder is refused");
        assert_eq!(rejection.code, "hint-missing");
    }
}
