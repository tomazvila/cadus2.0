//! U2 and U3 acceptance on trees the test writes itself: the shapes no
//! committed fixture carries (D1, spec section 5).
//!
//! Every message below is the literal 1.0 text of `cadus/graph.py:628-842`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_core::curriculum::{lint_curriculum, load_curriculum};
use common::scratch::ScratchTree;

/// A non-core topic with exactly one core dependent names it with no
/// "(+n more)" tail.
#[test]
fn a_noncore_ancestor_of_one_core_topic_has_no_more_tail() {
    let tree = ScratchTree::new("one-core-dependent");
    tree.courses(&["c"]).unit(
        "c/00.yaml",
        "c",
        &[
            ("a", "    core: false\n"),
            (
                "b",
                "    prerequisites:\n      - id: a\n        weight: 0.5\n",
            ),
        ],
    );
    let findings = lint_curriculum(tree.root());
    let core = findings
        .iter()
        .find(|f| f.code == "noncore_ancestor_of_core")
        .expect("the rule fires");
    assert_eq!(
        core.message,
        "non-core topic 'a' is a prerequisite (ancestor) of core topic 'b'"
    );
    assert_eq!(core.context, Some(vec!["b".to_owned()]));
}

/// Two references to one dangling id share one phantom node (parity trap 7).
#[test]
fn a_dangling_id_named_twice_is_one_phantom_node() {
    let tree = ScratchTree::new("phantom-twice");
    tree.courses(&["c"]).unit(
        "c/00.yaml",
        "c",
        &[
            (
                "a",
                "    prerequisites:\n      - id: ghost\n        weight: 0.5\n",
            ),
            (
                "b",
                "    encompassings_extra:\n      - id: ghost\n        weight: 0.25\n",
            ),
        ],
    );
    let (curriculum, _) = load_curriculum(tree.root()).expect("the tree loads");
    assert_eq!(curriculum.topic_count(), 2);
    assert_eq!(curriculum.enc_node_count(), 3, "one phantom for both edges");
    assert_eq!(
        curriculum.reach_weights_by_id("a"),
        vec![("a", 1.0), ("ghost", 0.5)]
    );
    assert_eq!(
        curriculum.reach_weights_by_id("b"),
        vec![("b", 1.0), ("ghost", 0.25)]
    );
    assert_eq!(
        curriculum.upward_weights_by_id("ghost"),
        vec![("a", 0.5), ("b", 0.25), ("ghost", 1.0)]
    );
}
