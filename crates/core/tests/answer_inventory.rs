//! The answer-shape inventory of the Foundations course (unit f1-inventory, D-F1).
//!
//! The test loads the checked-in curriculum tree with the real loader, walks
//! every Foundations topic, knowledge point and exemplar, and runs the real
//! answer grammar (`cadus_core::answer::canonical_form`) on every authored
//! answer. Each answer gets one SHAPE and one GRAMMAR verdict.
//!
//! The counts below are the counts of the ground audit
//! `docs/reviews/FRAMEWORK-audit-2026-09-06.md`, finding (i): 809 knowledge
//! points and 1,695 exemplars.
//!
//! # The dump
//!
//! ```sh
//! CADUS_INVENTORY_DUMP=/path/inventory.jsonl cargo test -p cadus-core \
//!     --test answer_inventory -- --nocapture
//! ```
//!
//! The dump writes one JSON line per exemplar, in load order, with the fields
//! `course, unit, topic_id, answer_kind, kp_id, exemplar_index, answer, shape,
//! verdict, has_solution_sketch`. The env-gated pattern follows
//! `dump_the_undecidable_residue_when_asked` of `answer_oracle_2.rs`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use common::inventory::{
    COURSE, Row, distinct_decidable_counts, knowledge_points, rows, shape_counts, verdict_counts,
};
use common::paths::tree;
use common::shape::SHAPES;

/// The Foundations knowledge-point count of audit finding (i).
const KNOWLEDGE_POINTS: usize = 809;

/// The Foundations exemplar count of audit finding (i).
const EXEMPLARS: usize = 1_695;

/// The Foundations exemplars with no solution sketch, audit finding (i).
const WITHOUT_SKETCH: usize = 679;

/// The count of rows per shape. The table is the shape column of the report
/// `docs/reports/foundations-answer-inventory.md`.
const SHAPE_COUNTS: [(&str, usize); 16] = [
    ("coordinates", 52),
    ("decimal", 47),
    ("equation_or_inequality", 220),
    ("expression", 300),
    ("fraction", 109),
    ("integer", 588),
    ("interval", 6),
    ("mixed_number", 7),
    ("ordered_list", 33),
    ("other", 12),
    ("prose", 196),
    ("quotient_remainder", 12),
    ("radical", 56),
    ("rational_exponent", 3),
    ("set", 4),
    ("value_with_unit", 50),
];

/// The count of knowledge points per number of distinct decidable exemplars.
const DISTINCT_DECIDABLE: [(usize, usize); 4] = [(0, 146), (1, 58), (2, 548), (3, 57)];

#[test]
fn the_foundations_inventory_carries_the_audited_counts() {
    let curriculum = tree();
    let points = knowledge_points(&curriculum, COURSE);
    let inventory = rows(&curriculum, COURSE);
    assert_eq!(
        points.len(),
        KNOWLEDGE_POINTS,
        "Foundations knowledge points"
    );
    assert_eq!(inventory.len(), EXEMPLARS, "Foundations exemplars");
    let sketched = inventory
        .iter()
        .filter(|row| row.has_solution_sketch)
        .count();
    assert_eq!(
        inventory.len() - sketched,
        WITHOUT_SKETCH,
        "exemplars with no solution sketch"
    );
    assert!(
        inventory.iter().all(|row| row.course == COURSE),
        "every row belongs to the course"
    );
}

#[test]
fn every_answer_carries_one_shape_and_one_verdict() {
    let curriculum = tree();
    let inventory = rows(&curriculum, COURSE);
    let counts = shape_counts(&inventory);
    let table: BTreeMap<&str, usize> = SHAPE_COUNTS.into_iter().collect();
    for (shape, want) in table {
        let found = counts.get(shape).copied().unwrap_or(0);
        assert_eq!(found, want, "{shape}: row count");
    }
    for shape in counts.keys() {
        assert!(
            SHAPE_COUNTS.iter().any(|(known, _)| known == shape),
            "{shape} is a shape the count table does not list"
        );
    }
    for shape in SHAPES {
        assert!(
            SHAPE_COUNTS
                .iter()
                .any(|(known, _)| *known == shape.as_str()),
            "{} is a shape the count table forgets",
            shape.as_str()
        );
    }
    let total: usize = counts.values().sum();
    assert_eq!(total, EXEMPLARS, "every exemplar carries one shape");
    print_report(&inventory);
}

#[test]
fn every_knowledge_point_has_three_distinct_decidable_exemplars_or_fewer() {
    let curriculum = tree();
    let counts = distinct_decidable_counts(&curriculum, COURSE);
    let want: BTreeMap<usize, usize> = DISTINCT_DECIDABLE.into_iter().collect();
    assert_eq!(
        counts, want,
        "distinct decidable exemplars per knowledge point"
    );
    let total: usize = counts.values().sum();
    assert_eq!(total, KNOWLEDGE_POINTS, "every knowledge point is counted");
}

#[test]
fn dump_the_foundations_inventory_when_asked() {
    let Ok(path) = std::env::var("CADUS_INVENTORY_DUMP") else {
        return;
    };
    let curriculum = tree();
    let inventory = rows(&curriculum, COURSE);
    let mut out = String::new();
    for row in &inventory {
        let _ = writeln!(out, "{}", row.json());
    }
    std::fs::write(&path, out).unwrap_or_else(|e| panic!("write {path}: {e}"));
    println!("wrote {} exemplars to {path}", inventory.len());
}

/// Print the shape table and the verdict table.
fn print_report(inventory: &[Row]) {
    let decided = inventory.iter().filter(|row| row.decided()).count();
    println!("decided {decided} of {}", inventory.len());
    for (shape, count) in shape_counts(inventory) {
        println!("shape {shape}: {count}");
    }
    for (verdict, count) in verdict_counts(inventory) {
        println!("verdict {verdict}: {count}");
    }
}
