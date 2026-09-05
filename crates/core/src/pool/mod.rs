//! The serving pool: sources, anti-repeat, and the candidate rule (A5, A6, A7, D5).
//!
//! # What the pool is for
//!
//! A5 states the rule: "Fresh values per instantiation plus a served-instance
//! hash log satisfy Hard Rule 4. The server rejects an instantiation whose hash
//! was recently served and redraws." A7 states the shape: every problem reaches
//! the learner through the pool, and the pool is source-agnostic.
//!
//! This module holds the pure half of both. The store half — the batch insert,
//! the `FOR UPDATE SKIP LOCKED` pop, and the claim — is `cadus_store::pool`, and
//! the refill job is `cadus_worker` (D-O4).
//!
//! # The three pieces
//!
//! | Piece | What it owns |
//! |---|---|
//! | [`source`] | [`ProblemSource`], [`TemplateSource`] (A1), [`ExemplarSource`] (A6) |
//! | [`recheck`] | [`check_instance`], the per-instance rules every instance passes |
//! | [`ring`] | [`Ring`] (20 per topic), [`TaskMemory`] (12 per task), the candidate rule |
//! | this file | [`Source`], the wire tag of the pool row, and [`PoolCounters`] |
//!
//! # The serve path never draws
//!
//! ```text
//!   worker (D-O4, off the request path)      serve (D-O1, L1 < 150 ms)
//!   ─────────────────────────────────────    ────────────────────────────────
//!   ProblemSource::fill(kp, n, seed)         pop at most POP_CANDIDATES rows
//!         │  draws, renders, evaluates             │
//!         ▼                                        ▼
//!   Vec<Instance> ──▶ serving_pool           pool::serve(rows, ring, task, …)
//!                                                  │ skips ring hits
//!                                                  ▼
//!                                            one row, claimed and served
//! ```
//!
//! A pool miss must not generate (A6). The serve path instantiates an exemplar
//! in process, records the serve as [`Source::Exemplar`], and enqueues a refill.
//! There is no synchronous live-generation fallback anywhere.
//!
//! # No model call, no clock, no socket
//!
//! Nothing in this module calls a model (T1), reads a clock, or opens a socket
//! (R3). Every draw takes a recorded `u64` seed from its caller, so a reviewer
//! reproduces any served instance from the pool row.

pub mod ring;
pub mod row;
pub mod source;

/// The per-instance rules of the gate, re-exported for the fill path.
pub use crate::template::check_instance;
pub use ring::{
    Avoid, Candidate, Pick, RING_CAPACITY, Ring, TASK_MEMORY_CAPACITY, TaskMemory, Window, pick,
    serve,
};
pub use row::{
    KP_KEY_SEPARATOR, POOL_ROW_VERSION, PoolAnswer, PoolBodyError, PoolProblem, kp_key,
    split_kp_key,
};
pub use source::{
    Batch, ExemplarRefusal, ExemplarSource, FILL_ROUNDS, FillError, ProblemSource,
    REFUSAL_FLAG_PERCENT, Refused, TemplateSource,
};

use std::fmt;

use serde::{Deserialize, Serialize};

/// The count of pool rows one serve pops before it gives up on the ring.
///
/// The serve pops this many rows with `FOR UPDATE SKIP LOCKED`, skips the ring
/// hits, and serves the last row when every one of them is blocked (the M4 pool
/// decision, specification section 5.5). The bound keeps the pop inside its
/// share of the L1 budget: the redraw belongs to the worker (D-O4).
pub const POP_CANDIDATES: usize = 8;

/// The source that produced a pool row (A7).
///
/// The three values are the three values of the `serving_pool.source` check
/// constraint (`migrations/0005_content.sql`). The column records the source per
/// problem, so the pedagogical effect of each source is measurable per source
/// before it earns more budget (A7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// A1: an instance of an approved template.
    Template,
    /// A6: an authored exemplar of the knowledge point.
    Exemplar,
    /// A7: a future generator. No 2.0 code writes this value.
    Generator,
}

/// The wire values of [`Source`], in declaration order.
pub const SOURCES: [&str; 3] = ["template", "exemplar", "generator"];

impl Source {
    /// The wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Template => "template",
            Self::Exemplar => "exemplar",
            Self::Generator => "generator",
        }
    }

    /// Read one wire value back.
    ///
    /// The three accepted strings are the three values of the `serving_pool.source`
    /// check constraint. Every other string returns `None`, so a row a later
    /// migration writes never decodes into a value this build guesses.
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "template" => Some(Self::Template),
            "exemplar" => Some(Self::Exemplar),
            "generator" => Some(Self::Generator),
            _ => None,
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The serve counters of one process (A6 operator flag).
///
/// 1.0 keeps the same facts in one Prometheus counter with a `result` label
/// (`cadus_web/metrics.py:143-148`); `resample_exhausted` there is
/// [`PoolCounters::pool_exhausted`] here. The core holds the numbers and exports
/// nothing: the adapter that owns the metrics registry reads them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PoolCounters {
    /// The count of served instances.
    pub served: u64,
    /// The count of blocked candidates the rule walked past.
    pub blocked: u64,
    /// The count of serves where every candidate was blocked.
    ///
    /// A rising number says the pool is too shallow for the ring, or that the
    /// knowledge point has too few distinct instances. 1.0 counts the same event
    /// as `resample_exhausted` and serves the repeat anyway.
    pub pool_exhausted: u64,
    /// The count of pool rows a pop retired because they did not decode.
    ///
    /// A row this build refuses is claimed and skipped, and the pop continues
    /// with the rows behind it. A rising number says a version bump or a writer
    /// of another build left rows the reader does not know.
    pub pool_row_undecodable: u64,
}

impl PoolCounters {
    /// Build a counter set at zero.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            served: 0,
            blocked: 0,
            pool_exhausted: 0,
            pool_row_undecodable: 0,
        }
    }

    /// Count one serve.
    pub fn record(&mut self, pick: Pick) {
        let skipped = u64::try_from(pick.skipped).unwrap_or(u64::MAX);
        self.served = self.served.saturating_add(1);
        self.blocked = self.blocked.saturating_add(skipped);
        if pick.exhausted {
            self.pool_exhausted = self.pool_exhausted.saturating_add(1);
        }
    }

    /// Count the rows one pop retired because they did not decode.
    pub fn record_undecodable(&mut self, rows: usize) {
        let rows = u64::try_from(rows).unwrap_or(u64::MAX);
        self.pool_row_undecodable = self.pool_row_undecodable.saturating_add(rows);
    }
}
