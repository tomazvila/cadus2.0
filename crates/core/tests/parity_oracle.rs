//! Regression checks for the curriculum-dump binary and canonical fixture output.
//! Expected hashes, lengths, and representations are pinned literals. These
//! checks run entirely in Rust and require no legacy interpreter or checkout.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::Path;
use std::process::Command;

use cadus_core::curriculum::{canonical_dump, curriculum_hash, sha256_hex};
use common::dump::{curriculum_root, fixture_arena};

/// The semantic curriculum hash of the checked-in tree (spec section 3).
// Regenerated for the expanded live tree; independently derived alongside parity.rs.
const TREE_HASH: &str = "4afb128b42fa599bc4ec40d612f914cc9667f75459ff99f8f4e3ee3c5ef7b5a6";

/// The length of the dump in bytes, without the trailing newline.
const DUMP_LEN: usize = 4_631_098;

#[test]
fn the_binary_writes_the_dump_and_the_hash() {
    let output = Command::new(env!("CARGO_BIN_EXE_dump_curriculum"))
        .arg(curriculum_root())
        .output()
        .expect("dump_curriculum runs");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout.len(), DUMP_LEN + 1, "the dump plus a newline");
    assert_eq!(output.stdout.last(), Some(&b'\n'));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("sha256={TREE_HASH}\n")
    );
    assert_eq!(sha256_hex(&output.stdout[..DUMP_LEN]), TREE_HASH);
}

#[test]
fn the_binary_exits_2_on_a_tree_with_no_courses_file() {
    let missing = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/no-courses-file");
    let output = Command::new(env!("CARGO_BIN_EXE_dump_curriculum"))
        .arg(missing)
        .output()
        .expect("dump_curriculum runs");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no courses.yaml under"));
}

#[test]
fn the_binary_exits_2_without_a_directory() {
    let output = Command::new(env!("CARGO_BIN_EXE_dump_curriculum"))
        .output()
        .expect("dump_curriculum runs");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "usage: dump_curriculum <dir>\n"
    );
}

/// Pin the float-format thresholds represented by the authored fixture.
#[test]
fn the_dump_preserves_the_float_fixture_representations() {
    let curriculum = fixture_arena("arena-float-repr");
    let text = canonical_dump(&curriculum);
    for literal in [
        "\"difficulty\":0.85",
        "\"difficulty\":1e-05",
        "\"difficulty\":1.234e-07",
        "\"difficulty\":0.0001",
        "\"difficulty\":1.0",
        "[\"b\",\"a\",1.5e-05]",
        "[\"c\",\"a\",6.300000000000001e-05]",
        "[\"d\",\"c\",9.999e-05]",
        "[\"e\",\"d\",0.0,false]",
    ] {
        assert!(text.contains(literal), "the dump must hold {literal}");
    }
}

/// Equal zero weights fall through to the key flag and otherwise retain order.
#[test]
fn the_dump_orders_the_two_signs_of_zero_stably() {
    let curriculum = fixture_arena("arena-negative-zero");
    let dump = canonical_dump(&curriculum);
    assert!(
        dump.contains(
            "\"prereq_edges\":[[\"b\",\"a\",0.0,false],[\"b\",\"a\",-0.0,true],\
[\"c\",\"a\",0.0,true],[\"c\",\"a\",-0.0,true]]"
        ),
        "the two signs of zero sort equal and the row falls through to the key flag"
    );
    assert_eq!(
        curriculum_hash(&curriculum),
        "1d364876fae80721cbcc790bf3f527e0b145a23d650c5fe640cce65f3000126d"
    );
}

/// Repeated encompassing edges retain their maximum authored weight.
#[test]
fn the_dump_keeps_the_maximum_repeated_edge_weight() {
    let curriculum = fixture_arena("arena-repeated-edge");
    let dump = canonical_dump(&curriculum);
    assert!(
        dump.contains("\"encompassing_edges\":[[\"b\",\"a\",0.7],[\"c\",\"a\",0.7]]"),
        "both encompassing rows keep the larger weight of the repeated edge"
    );
    assert_eq!(
        curriculum_hash(&curriculum),
        "d3dfd7bdd11ec965a1d6b2ee815d8a750a69f2180236231fd4d2762b1c3756bd"
    );
}
