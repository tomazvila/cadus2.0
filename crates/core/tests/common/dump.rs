//! Shared curriculum-dump fixtures and checked-in curriculum loading helpers.

#![allow(clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use cadus_core::curriculum::{Curriculum, load_curriculum};

use super::events::repo_root;

/// The curriculum tree of the repository (C5).
#[must_use]
pub fn curriculum_root() -> PathBuf {
    repo_root().join("curriculum")
}

/// One fixture tree under `crates/core/tests/fixtures/`.
#[must_use]
pub fn fixture_tree(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The arena of the checked-in tree.
#[must_use]
pub fn tree() -> Curriculum {
    let (curriculum, findings) = load_curriculum(&curriculum_root()).expect("the tree loads");
    assert!(findings.is_empty(), "the clean tree has 0 parse findings");
    curriculum
}

/// The arena of one fixture tree, which has no parse finding.
#[must_use]
pub fn fixture_arena(name: &str) -> Curriculum {
    let (curriculum, findings) = load_curriculum(&fixture_tree(name)).expect("the fixture loads");
    assert!(findings.is_empty(), "the fixture has 0 parse findings");
    curriculum
}
