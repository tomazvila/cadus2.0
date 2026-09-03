//! The paths the curriculum tests read: the checked-in tree and the fixture
//! trees under `crates/core/tests/fixtures/`, and the arenas built from them.

use std::path::{Path, PathBuf};

use cadus_core::curriculum::{Curriculum, load_curriculum};

/// The curriculum tree of the repository (C5).
#[must_use]
pub fn curriculum_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum")
}

/// One fixture tree under `crates/core/tests/fixtures/`.
#[must_use]
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// One lint fixture tree under `crates/core/tests/fixtures/lint/`.
#[must_use]
pub fn lint_fixture(name: &str) -> PathBuf {
    fixture("lint").join(name)
}

/// The arena of the checked-in tree, which has 0 parse findings.
#[must_use]
pub fn tree() -> Curriculum {
    let (curriculum, findings) = loaded(&curriculum_root(), "the tree loads");
    assert!(findings.is_empty(), "the clean tree has 0 parse findings");
    curriculum
}

/// The arena of one fixture tree.
#[must_use]
pub fn arena(name: &str) -> Curriculum {
    loaded(&fixture(name), "the fixture loads").0
}

/// The arena and the parse findings of one tree that loads.
fn loaded(root: &Path, what: &str) -> (Curriculum, Vec<cadus_core::curriculum::Finding>) {
    load_curriculum(root).unwrap_or_else(|error| panic!("{what}: {error}"))
}
