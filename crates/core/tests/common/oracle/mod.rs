//! The helpers of the `answer_oracle` tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

pub use super::fixtures::*;
pub use super::fuzz::*;
pub use cadus_core::answer::{Ast, Atom, Canon, Outcome, canonical_form, check, normalize, parse};
pub use cadus_core::curriculum::AnswerKind;
pub use num_traits::{One as _, Zero as _};
pub use std::collections::{BTreeMap, BTreeSet};
pub use std::fmt::Write as _;
pub use std::path::{Path, PathBuf};

mod part1;
mod part2;
mod part3;
mod part4;
mod part5;
mod part6;
mod part7;
mod part8;
mod part9;

pub use part1::*;
pub use part2::*;
pub use part3::*;
pub use part4::*;
pub use part5::*;
pub use part6::*;
pub use part7::*;
pub use part8::*;
pub use part9::*;
