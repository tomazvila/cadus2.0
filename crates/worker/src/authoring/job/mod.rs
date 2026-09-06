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
//!      │                        │              ├─ the digest is held -> DUPLICATE
//!      │                        │              └─ a reviewer refused it -> REJECTED
//!      │                        └─ Err -> the LITERAL message becomes the
//!      │                                  feedback of the next attempt
//!      └─ Err -> the transport reason becomes the reason of this attempt
//!      │
//!      ▼ (after AUTHORING_ATTEMPTS refusals)
//!   DECLINED           -- no `content_store` row, one decline record
//! ```
//!
//! # The key of one document
//!
//! [`document_digest`] is the one function that computes a `content_store`
//! digest, and the digest covers the knowledge point, the kind AND the body. Two
//! knowledge points that earn one teach page therefore store two rows. The
//! digest of the body alone gave them one primary key, and the second document
//! vanished under `ON CONFLICT DO NOTHING` (M6 review finding F1).
//!
//! A pass that collides with a digest reads the verdict of that row
//! ([`cadus_store::content::verdict`]). A `pending` or `approved` row is
//! [`Outcome::Duplicate`], which the batch counts beside `stored` and never
//! inside it. A `rejected` row is [`Outcome::Rejected`]: the pass reproduced a
//! body a human refused, the batch counts a decline, and the reviewer's reason
//! reaches the operator in the decline record (M6 review finding F6).
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
//! # The two doors before a gate
//!
//! [`verify_kind`] runs `crate::authoring::repair::repair_arguments` on the
//! decoded tool arguments FIRST, on every kind. No gate reads a control
//! character, so an under-escaped `"$\times$"` used to reach `content_store` as
//! `$<TAB>imes$` (spec section 5, trap T1; M6 review finding F3).
//!
//! [`store_pending`] then writes `content_store.prompt_digest`: the row names
//! the prompt that authored it, so a prompt edit marks the row for re-authoring
//! and never unapproves it ([`stale_slots`], [`stale_rows`]; spec section 2.2,
//! "Prompt digest"; M6 review finding F4).

mod parallel;
mod pass;
mod store;
mod verify;

use cadus_model_client::{Attempt, Client};
use sha2::{Digest, Sha256};

use crate::authoring::prompt::{DIGEST_CHARS, Kind};

pub use parallel::run_parallel;
pub use pass::{author_one, run_batch};
pub use store::{
    StaleRow, Stored, render_stale, served_instances, slots_taken, stale_rows, stale_slots,
    store_pending,
};
pub use verify::{
    NO_ARGUMENTS, assemble, authoring_vocabulary, verify, verify_diagnosis, verify_hint_ladder,
    verify_kind, verify_teach,
};

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

/// The `content_store.status` of a document a human refused (C6).
pub const STATUS_REJECTED: &str = "rejected";

/// The prefix of a `content_store.digest`, so the row names its hash function.
pub const DIGEST_PREFIX: &str = "sha256:";

/// The byte between the three parts of the digest material.
///
/// Postgres holds no NUL byte in a `text` value, so no `kp_id` and no kind
/// carries this byte and the three parts of [`document_digest`] cannot run
/// together into one ambiguous string.
pub const DIGEST_SEPARATOR: u8 = 0;

/// The decline reason a pass that reproduces a refused body earns (C6).
///
/// The reviewer's own reason follows it, after a colon and a space.
pub const SAME_BODY: &str = "a reviewer refused this exact body already";

/// What stands in the decline reason when the refused row carries no reason.
pub const NO_REVIEW_REASON: &str = "the row carries no reason";

/// What one pass did with one knowledge point and kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The bank is full. The pass made no model call.
    Skipped,
    /// The gate accepted a document and the row is `pending`.
    Stored,
    /// The gate accepted a document that the table already holds as `pending`
    /// or as `approved`. Nothing is stored, and the pass is NOT a store.
    Duplicate,
    /// The gate accepted a document the table already holds, and the held row
    /// named an OLDER prompt. Nothing is stored; the row now names the current
    /// prompt (M6 review finding V3).
    ///
    /// The body is the body the current prompt writes, so the row leaves the
    /// stale set and the next pass makes no call. Without the stamp the pass
    /// re-authored the same row on every run, forever. A batch counts this
    /// outcome where it counts a duplicate: the pass paid for its calls and
    /// wrote no document.
    Refreshed,
    /// The gate accepted a document that a reviewer already REFUSED (C6).
    ///
    /// `same_body` is `true` when the digest of the pass names the refused row.
    /// The digest covers the knowledge point, the kind and the body, so a
    /// digest that collides is the same body of the same knowledge point, and
    /// this pass reaches the outcome that way alone. The reviewer's reason
    /// reaches [`Report::decline`], and the pass counts as a decline: the
    /// knowledge point still needs a document (M6 review finding F6).
    Rejected {
        /// Whether the refused row carries the body of this pass, byte for
        /// byte.
        same_body: bool,
    },
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

impl Report {
    /// The report of a pass that made no call and stored nothing.
    fn quiet(kp_id: String, kind: Kind, outcome: Outcome) -> Self {
        Self {
            kp_id,
            kind,
            outcome,
            attempts: 0,
            digest: None,
            decline: None,
            http_attempts: Vec::new(),
            cost_usd: None,
            alert: false,
        }
    }

    /// The report of a pass that used every attempt and stored nothing.
    fn declined(kp_id: String, kind: Kind, decline: Decline, http_attempts: Vec<Attempt>) -> Self {
        Self {
            attempts: decline.attempts,
            decline: Some(decline),
            http_attempts,
            ..Self::quiet(kp_id, kind, Outcome::Declined)
        }
    }
}

/// What one pass of [`run_batch`] did over a list of knowledge points.
#[derive(Debug, Default)]
pub struct BatchReport {
    /// The knowledge points that hold a NEW `pending` document after the pass.
    pub stored: u32,
    /// The knowledge points whose document the table already held (C6).
    ///
    /// The pass paid for its calls and wrote no row, so the count stands beside
    /// `stored` and never inside it (M6 review finding F1).
    ///
    /// [`Outcome::Refreshed`] counts here as well: that pass wrote no document
    /// either, and it stamped the current prompt on the held row (M6 review
    /// finding V3).
    pub duplicate: u32,
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
    budget: Option<crate::authoring::budget::Budget>,
    endpoint_status: std::sync::atomic::AtomicU16,
}

impl AuthoringJob {
    /// The permanent HTTP rejection that stopped this shared job.
    #[must_use]
    pub fn endpoint_failure(&self) -> Option<u16> {
        let status = self
            .endpoint_status
            .load(std::sync::atomic::Ordering::SeqCst);
        (status != 0).then_some(status)
    }

    /// Share one reservation cap across every kind and concurrent request.
    #[must_use]
    pub fn with_budget(mut self, budget: crate::authoring::budget::Budget) -> Self {
        self.budget = Some(budget);
        self
    }

    /// Build the job around a client, with the [`AUTHORING_ATTEMPTS`] bound.
    #[must_use]
    pub const fn new(client: Client) -> Self {
        Self {
            client,
            attempts: AUTHORING_ATTEMPTS,
            budget: None,
            endpoint_status: std::sync::atomic::AtomicU16::new(0),
        }
    }

    /// Build the job with another attempt bound.
    ///
    /// A bound of 0 makes no call and declines at once, which is what the dry
    /// run of unit R8 wants.
    #[must_use]
    pub const fn with_attempts(client: Client, attempts: u32) -> Self {
        Self {
            client,
            attempts,
            budget: None,
            endpoint_status: std::sync::atomic::AtomicU16::new(0),
        }
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

/// The content address of one stored document (C6, spec section 8, trap 8).
///
/// The key of `content_store` is the knowledge point, the kind, and the body,
/// and this is the ONE function that computes it. Every writer and every reader
/// of a digest calls it.
///
/// A teach page and a hint ladder carry no knowledge-point field: the serve
/// reader of L4 and L5 refuses an unknown field, so [`instruction_body`] keeps
/// the model's fields alone. Two knowledge points can therefore earn one body,
/// and a digest of the body alone gave both documents one primary key. The
/// second insert then vanished under `ON CONFLICT (digest) DO NOTHING`, the pass
/// counted a store it did not make, and the losing knowledge point stayed at
/// zero slots forever (M6 review finding F1).
///
/// The material is `kp_id`, the kind, and the body text, with
/// [`DIGEST_SEPARATOR`] between the parts.
///
/// The digest covers the WHOLE body, `samples` and `space_size` included, so a
/// change to either asks for a new approval. The text it hashes is the output of
/// [`to_body`], which serde writes in field order with no whitespace, so a
/// recomputation reads the row through `from_body` and writes it through
/// `to_body` again. The column holds jsonb, and jsonb keeps neither key order
/// nor whitespace.
///
/// [`instruction_body`]: verify::instruction_body
/// [`to_body`]: cadus_core::template::to_body
#[must_use]
pub fn document_digest(kp_id: &str, kind: Kind, body: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kp_id.as_bytes());
    hasher.update([DIGEST_SEPARATOR]);
    hasher.update(kind.as_str().as_bytes());
    hasher.update([DIGEST_SEPARATOR]);
    hasher.update(body.as_bytes());
    let hash = hasher.finalize();
    let mut hex = String::with_capacity(DIGEST_CHARS);
    for byte in hash.iter().take(DIGEST_CHARS.div_ceil(2)) {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex.truncate(DIGEST_CHARS);
    format!("{DIGEST_PREFIX}{hex}")
}

#[cfg(test)]
mod tests {
    use super::{
        AUTHORING_ATTEMPTS, BANK_TARGET, DIGEST_PREFIX, Decline, Outcome, Report, SINGLE_DOCUMENT,
        bank_target, document_digest,
    };
    use crate::authoring::prompt::Kind;

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

    /// The digest is the prefix and 16 hex characters of the SHA-256 of the
    /// knowledge point, the kind, and the body.
    ///
    /// The material of the first literal is `perfect-squares/squares`, one NUL
    /// byte, `template`, one NUL byte, and the two bytes `{}`. The value comes
    /// from outside this tree:
    ///
    /// ```sh
    /// printf 'perfect-squares/squares\0template\0{}' | sha256sum
    /// # deb2349817ba6f6dcee6997c850abbf070a6b05b172e9f98b6fa86be54347c86
    /// ```
    #[test]
    fn the_digest_is_sixteen_hex_characters_of_sha256() {
        let kp = "perfect-squares/squares";
        assert_eq!(
            document_digest(kp, Kind::Template, "{}"),
            "sha256:deb2349817ba6f6d"
        );
        assert_eq!(
            document_digest(kp, Kind::Template, "{}").len(),
            DIGEST_PREFIX.len() + 16
        );
        assert_ne!(
            document_digest(kp, Kind::Template, "{}"),
            document_digest(kp, Kind::Template, "{ }")
        );
    }

    /// F1: the kind and the knowledge point are inside the digest, so one body
    /// under two knowledge points is two keys, and under two kinds is two keys.
    #[test]
    fn one_body_under_two_knowledge_points_is_two_digests() {
        let body = r#"{"concept":"one page"}"#;
        assert_eq!(
            document_digest("perfect-squares/squares", Kind::Teach, body),
            "sha256:81cf98710751b9f0"
        );
        assert_ne!(
            document_digest("perfect-squares/squares", Kind::Teach, body),
            document_digest("perfect-cubes/cubes", Kind::Teach, body)
        );
        assert_ne!(
            document_digest("perfect-squares/squares", Kind::Teach, body),
            document_digest("perfect-squares/squares", Kind::HintLadder, body)
        );
    }

    /// A declined report carries the decline record, its attempt count and the
    /// HTTP attempts, and nothing else.
    #[test]
    fn a_declined_report_carries_the_decline_and_its_attempts() {
        let decline = Decline {
            kp_id: "perfect-squares/squares".to_owned(),
            kind: Kind::Teach,
            attempts: 2,
            reasons: vec!["one".to_owned(), "two".to_owned()],
        };
        let report = Report::declined(
            "perfect-squares/squares".to_owned(),
            Kind::Teach,
            decline.clone(),
            Vec::new(),
        );
        assert_eq!(report.outcome, Outcome::Declined);
        assert_eq!(report.attempts, 2);
        assert_eq!(report.decline, Some(decline));
        assert_eq!(report.digest, None);
        assert_eq!(report.cost_usd, None);
        assert!(!report.alert);
    }
}
