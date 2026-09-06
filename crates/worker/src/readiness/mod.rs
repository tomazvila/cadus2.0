//! The readiness audit as an operator command (D-F5, unit f7).
//!
//! `cadus-worker readiness` answers one question: which knowledge points can
//! the service teach, practice and assess TODAY, and what does each missing one
//! need? The answer is a report per course, per topic and per knowledge point.
//!
//! # What the run exercises
//!
//! The report is not a read of the curriculum alone. Every knowledge point goes
//! through the THREE contracts a learner meets:
//!
//! | Contract | The code the run calls | What a failure means |
//! |---|---|---|
//! | serve | [`ExemplarSource::fill`](cadus_core::pool::ExemplarSource) — the A6 path of `crates/web/src/serve/draw.rs` | the knowledge point produces no problem at all |
//! | render | the statement and `problem_text_hash` of each instance, as the pool row stores them | the statement the pool holds is empty |
//! | grade | [`check`](cadus_core::answer::check) of each authored answer against ITSELF, with the topic's `answer_kind` | the grade route cannot mark a correct answer correct |
//!
//! A self-check that is not `Decided(correct)` is the sharpest signal in the
//! report: the learner types the authored answer and the service marks it
//! wrong. Audit finding (a) is that failure at the route.
//!
//! # What it costs
//!
//! One grouped read of `content_store` and pure CPU. It spends NO model token
//! (T1) and writes no row.

mod prereq_md;
mod render;
mod run;

pub use prereq_md::render_prereq_markdown;
pub use render::{render_json, render_markdown};
pub use run::{ContractCheck, ReadinessRun, run};
