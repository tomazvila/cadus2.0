//! Integrated tasks: one scenario, intermediate steps, and one final answer
//! (D-F10, Phase 4).
//!
//! # What an integrated task is
//!
//! A multi-step task of 1.0 serves one independent question per component
//! topic. That is a sequence of unrelated drills, and it does not show that the
//! learner can apply the components together. An integrated task states ONE
//! scenario, gives the quantities and the constraints, asks the learner to
//! choose a method, asks the intermediate steps that the scenario needs, and
//! ends in one final answer with an interpretation.
//!
//! # The pieces
//!
//! | Piece | What it owns |
//! |---|---|
//! | [`model`] | the authored shape: scenario, given, method, steps, final |
//! | [`validate`] | the rules an authored item passes before it is served |
//! | [`load`] | reading `curriculum/<course>/integrated/*.yaml` |
//! | [`view`] | the client-safe payload, with no answer in it |
//! | [`hint`] | one rung of a hint ladder at a time |
//! | [`grade`] | per-step and final grading with the authored contracts |
//!
//! # What this module never does
//!
//! It calls no model, opens no socket, and reads no clock. It never marks a
//! learner's prose right or wrong: a reasoning note rides along with the
//! submission, is stored, and is shown back, and no rule reads it. It approves
//! no content: an authored file becomes servable only through the human review
//! path, and validation here refuses a broken item, it does not accept one.
//!
//! # Secrecy
//!
//! [`view::view_of`] names every field it emits. The authored answer, the
//! alternate answers, the `correct` flag of a method option, and the final
//! interpretation have no field in the view, so Hard Rule 1 holds by shape and
//! not by care.

pub mod grade;
pub mod hint;
pub mod journey;
pub mod load;
pub mod model;
pub mod validate;
pub mod view;

pub use grade::{
    FINAL_FIELD_ID, FieldGrade, FieldResponse, IntegratedGrade, MethodGrade, Submission, grade,
};
pub use hint::{hint, hints_available};
pub use load::{INTEGRATED_DIR, IntegratedSet, parse_item};
pub use model::{Domain, Field, Final, Given, IntegratedItem, MethodChoice, MethodOption, Step};
pub use validate::{check_against, check_item};
pub use view::{IntegratedView, view_of};
