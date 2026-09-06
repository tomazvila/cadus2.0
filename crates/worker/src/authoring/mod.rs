//! The offline authoring pipeline of M6 (A2, R4, T3, T5, T6).
//!
//! Authoring is a batch job of `cadus-worker`. It never runs in a request
//! handler (R4, L6), and the request tier never depends on the model client.
//!
//! Spec: `docs/reference/authoring-and-spa-1.0-spec.md` section 2 in full, and
//! rows R1 to R3 of section 7.
//!
//! Unit R1 gives the pipeline its prompts ([`prompt`]). Unit R2 adds the batch
//! loop ([`job`]). Unit R3 adds the T3 accounting ([`cost`]): the ledger row of
//! every attempt, the bill on the stored row, and the alert above three
//! attempts. Unit R8 adds the operator entry point ([`cli`]): the `author`
//! subcommand, its plan, and its dry run. Unit FIX-M6-A2 adds the LaTeX escape
//! repair of the boundary ([`repair`]), which every gate runs behind.

pub mod budget;
pub mod cli;
pub mod cost;
pub mod job;
pub mod portable;
pub mod prompt;
pub mod repair;
