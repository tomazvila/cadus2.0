//! Readers of an arena, as the literal lists the arena tests compare.

use cadus_core::curriculum::{Curriculum, EncNode, TopicIdx};

/// The index of a topic that must exist.
#[must_use]
pub fn idx(curriculum: &Curriculum, id: &str) -> TopicIdx {
    curriculum
        .idx_of(id)
        .unwrap_or_else(|| panic!("topic {id} is missing"))
}

/// Topic indices as string ids.
#[must_use]
pub fn ids<'a>(curriculum: &'a Curriculum, list: &[TopicIdx]) -> Vec<&'a str> {
    list.iter().map(|t| curriculum.id_of(*t)).collect()
}

/// Encompassing edges as `(target id, weight)` pairs, in stored order.
#[must_use]
pub fn links(curriculum: &Curriculum, node: EncNode, forward: bool) -> Vec<(&str, f64)> {
    let edges: Vec<_> = if forward {
        curriculum.enc_forward(node).collect()
    } else {
        curriculum.enc_reverse(node).collect()
    };
    edges
        .into_iter()
        .map(|edge| (curriculum.enc_node_id(edge.target), edge.weight))
        .collect()
}
