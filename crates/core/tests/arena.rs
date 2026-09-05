//! U2 acceptance: the curriculum arena and its graph algorithms (D1, D2).
//!
//! Every expected value below is a literal. The counts and the topological
//! order come from `docs/reference/curriculum-1.0-spec.md` sections 2 and 3. The
//! encompassing weights, the fixture adjacencies, and the cycle lists come from
//! a run of the 1.0 loader, quoted in the unit report. No expected value is
//! derived by the code under test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_core::curriculum::{KpIdx, TopicIdx};
use common::arena_view::{ids, idx, links};
use common::paths::{arena, tree};

// --------------------------------------------------------------------------- //
// The checked-in tree: counts (spec section 2 and section 3)
// --------------------------------------------------------------------------- //

#[test]
fn the_arena_holds_the_literal_edge_counts() {
    let curriculum = tree();
    assert_eq!(curriculum.topic_count(), 1090, "topics");
    assert_eq!(curriculum.prereq_edge_count(), 3281, "prerequisite edges");
    assert_eq!(curriculum.dependent_edge_count(), 3281, "dependent edges");
    assert_eq!(
        curriculum.enc_forward_count(),
        3200,
        "forward encompassing entries: 3282 declared edges less the 82 that stay at weight 0"
    );
    assert_eq!(
        curriculum.enc_reverse_count(),
        3282,
        "reverse encompassing entries: every declared edge, weight 0 included"
    );
    assert_eq!(
        curriculum.enc_node_count(),
        1090,
        "the tree has no dangling encompassing target, so no phantom node"
    );
}

#[test]
fn the_prerequisite_and_dependent_directions_hold_the_same_edges() {
    let curriculum = tree();
    let forward: usize = (0..1090)
        .map(|raw| curriculum.prerequisites(TopicIdx::from_u32(raw)).len())
        .sum();
    let backward: usize = (0..1090)
        .map(|raw| curriculum.dependents(TopicIdx::from_u32(raw)).len())
        .sum();
    assert_eq!(
        forward, 3281,
        "prerequisite edges walked one topic at a time"
    );
    assert_eq!(backward, 3281, "dependent edges walked one topic at a time");
}

// --------------------------------------------------------------------------- //
// The checked-in tree: topological order (spec section 3)
// --------------------------------------------------------------------------- //

#[test]
fn topo_order_matches_the_spec_literals() {
    let curriculum = tree();
    let order = curriculum.topo_order();
    assert_eq!(order.len(), 1090, "every topic is ordered");

    let names = ids(&curriculum, order);
    assert_eq!(
        &names[..5],
        [
            "single-digit-addition",
            "subtraction-facts",
            "multiplication-tables",
            "division-facts",
            "perfect-squares",
        ],
        "the first five of spec section 3"
    );
    assert_eq!(
        &names[1087..],
        [
            "cartesian-closed-categories",
            "presheaves-intro",
            "toposes-glimpse",
        ],
        "the last three of spec section 3"
    );
}

#[test]
fn find_cycle_on_the_checked_in_tree_is_none() {
    // Spec section 3: `cycle: None` on a real load.
    assert_eq!(tree().find_cycle(), None);
}

// --------------------------------------------------------------------------- //
// The checked-in tree: interning and per-topic data (D2)
// --------------------------------------------------------------------------- //

#[test]
fn interning_round_trips_and_carries_the_authored_course() {
    let curriculum = tree();

    let first = idx(&curriculum, "single-digit-addition");
    assert_eq!(first, TopicIdx::from_u32(0), "load index 0");
    assert_eq!(curriculum.id_of(first), "single-digit-addition");
    assert_eq!(curriculum.load_index(first), 0);
    assert_eq!(curriculum.course_of(first), "foundations");
    assert_eq!(curriculum.module_of(first), "Arithmetic");
    assert_eq!(curriculum.unit_of(first), "arithmetic-core");

    let last = idx(&curriculum, "toposes-glimpse");
    assert_eq!(curriculum.load_index(last), 1089);
    assert_eq!(curriculum.course_of(last), "category-theory");
    assert_eq!(curriculum.module_of(last), "Monads & Beyond");
    assert_eq!(curriculum.unit_of(last), "monads");

    assert_eq!(curriculum.idx_of("no-such-topic"), None);
    assert_eq!(curriculum.id_of(TopicIdx::from_u32(9999)), "");
}

#[test]
fn topics_in_course_holds_the_authored_counts() {
    let curriculum = tree();
    // The size of every course, from a 1.0 load.
    let expected = [
        ("foundations", 285),
        ("proofs", 92),
        ("geometry", 87),
        ("probability-statistics", 82),
        ("precalculus", 37),
        ("discrete-mathematics", 38),
        ("calculus-1", 79),
        ("calculus-2", 67),
        ("linear-algebra", 75),
        ("multivariable-calculus", 65),
        ("differential-equations", 59),
        ("abstract-algebra", 54),
        ("category-theory", 70),
    ];
    for (course, size) in expected {
        assert_eq!(
            curriculum.topics_in_course(course).len(),
            size,
            "topics in {course}"
        );
    }
    assert!(curriculum.topics_in_course("no-such-course").is_empty());
}

#[test]
fn a_knowledge_point_is_addressed_by_topic_and_position() {
    let curriculum = tree();
    let topic = idx(&curriculum, "quadratic-formula");
    let kp_ids: Vec<&str> = curriculum
        .knowledge_points(topic)
        .iter()
        .map(|kp| kp.id.as_str())
        .collect();
    assert_eq!(kp_ids, ["kp1", "kp2", "kp3"], "authored order");

    let second = curriculum
        .kp_idx_of(topic, "kp2")
        .expect("kp2 is a knowledge point of quadratic-formula");
    assert_eq!(second, KpIdx::from_u16(1));
    assert_eq!(second.index(), 1);
    let third = curriculum
        .kp_idx_of(topic, "kp3")
        .expect("kp3 is a knowledge point of quadratic-formula");
    assert_eq!(third.index(), 2);
    assert_eq!(
        curriculum
            .knowledge_point(topic, second)
            .expect("the pair addresses a knowledge point")
            .id
            .as_str(),
        "kp2"
    );
    assert_eq!(curriculum.kp_idx_of(topic, "kp9"), None);
}

// --------------------------------------------------------------------------- //
// The checked-in tree: closures
// --------------------------------------------------------------------------- //

#[test]
fn ancestors_and_descendants_have_the_oracle_sizes() {
    let curriculum = tree();
    let quadratic = idx(&curriculum, "quadratic-formula");
    let ancestors = curriculum.ancestors(quadratic);
    assert_eq!(ancestors.len(), 82, "ancestors of quadratic-formula");
    assert_eq!(
        &ids(&curriculum, &ancestors)[..5],
        [
            "single-digit-addition",
            "subtraction-facts",
            "multiplication-tables",
            "division-facts",
            "perfect-squares",
        ],
        "ancestors come back ascending by load index"
    );

    let first = idx(&curriculum, "single-digit-addition");
    assert_eq!(
        curriculum.descendants(first).len(),
        1076,
        "descendants of single-digit-addition"
    );
    assert!(
        curriculum.ancestors(first).is_empty(),
        "single-digit-addition is a root"
    );
}

// --------------------------------------------------------------------------- //
// The checked-in tree: encompassing maps (parity traps 7 and 8)
// --------------------------------------------------------------------------- //

#[test]
fn the_one_encompassings_extra_is_in_both_encompassing_maps() {
    // Spec section 6: the single `encompassings_extra` of the tree is
    // `foundations/07-polynomials-quadratics.yaml` -> difference-of-squares, 0.3.
    let curriculum = tree();
    let source = idx(&curriculum, "factoring-trinomials");
    let target = idx(&curriculum, "difference-of-squares");

    assert_eq!(
        links(&curriculum, curriculum.enc_node(source), true),
        [
            ("factoring-monic-trinomials", 0.8),
            ("factoring-by-grouping", 0.6),
            ("polynomial-multiplication", 0.5),
            ("factoring-gcf", 0.5),
            ("integer-multiplication-division", 0.4),
            ("difference-of-squares", 0.3),
        ],
        "the extra edge sits last, after the five prerequisite edges"
    );
    assert_eq!(
        links(&curriculum, curriculum.enc_node(target), false),
        [
            ("factoring-trinomials", 0.3),
            ("sum-difference-of-cubes", 0.5),
            ("quadratics-in-form", 0.5),
            ("choosing-factoring-strategy", 0.6),
            ("quadratic-equations-factoring", 0.5),
            ("rational-expressions", 0.4),
            ("factoring-limits", 0.5),
        ],
        "the reverse map records the extra edge too"
    );

    let prerequisites = ids(
        &curriculum,
        &curriculum.prerequisites(source).collect::<Vec<_>>(),
    );
    assert_eq!(
        prerequisites,
        [
            "integer-multiplication-division",
            "polynomial-multiplication",
            "factoring-gcf",
            "factoring-by-grouping",
            "factoring-monic-trinomials",
        ],
        "an encompassings_extra target is not a prerequisite"
    );
}

#[test]
fn encompassing_weight_matches_the_1_0_oracle() {
    // The three values come from `g.encompassing_weight(a, b)` on the 1.0 loader
    // over this repository's `curriculum/`. Exact equality, per parity trap 9.
    let curriculum = tree();
    let pairs = [
        (
            "quadratic-formula",
            "single-digit-addition",
            0.096_768_000_000_000_02_f64,
        ),
        (
            "adjoints-preserve-limits",
            "compound-inequalities",
            0.000_450_000_000_000_000_04_f64,
        ),
        (
            "chain-rule-one-parameter",
            "graphing-from-a-table",
            0.000_476_279_999_999_999_93_f64,
        ),
    ];
    for (source, target, expected) in pairs {
        let weight =
            curriculum.encompassing_weight(idx(&curriculum, source), idx(&curriculum, target));
        assert_eq!(
            weight.to_bits(),
            expected.to_bits(),
            "W({source} -> {target}) = {weight:?}, oracle {expected:?}"
        );
    }

    let quadratic = idx(&curriculum, "quadratic-formula");
    assert_eq!(
        curriculum.encompassing_weight(quadratic, quadratic),
        1.0,
        "W(a -> a)"
    );
    assert_eq!(
        curriculum.encompassing_weight(idx(&curriculum, "single-digit-addition"), quadratic),
        0.0,
        "credit does not flow upward"
    );
}

// --------------------------------------------------------------------------- //
// The checked-in tree: mastery floors (parity trap 12)
// --------------------------------------------------------------------------- //

#[test]
fn mastery_floor_sizes_are_the_spec_literals() {
    let curriculum = tree();
    let foundations = curriculum
        .mastery_floor("foundations")
        .expect("foundations is a course");
    assert_eq!(foundations.len(), 3, "foundations uses the list form");
    assert_eq!(
        ids(&curriculum, &foundations),
        [
            "single-digit-addition",
            "multiplication-tables",
            "place-value"
        ],
        "ascending by load index"
    );

    assert_eq!(
        curriculum
            .mastery_floor("proofs")
            .expect("proofs is a course")
            .len(),
        7,
        "proofs uses the list form"
    );
    assert_eq!(
        curriculum
            .mastery_floor("geometry")
            .expect("geometry is a course")
            .len(),
        285,
        "geometry references foundations, so its floor is every foundations topic"
    );
    assert_eq!(curriculum.mastery_floor("no-such-course"), None);
}

/// Finding #11. `arena-floor-order` declares base(order 1), mid(order 2) and
/// top(order 3, `mastery_floor_course: mid`). 1.0 on the fixture:
/// `mastery_floor('top') = ['b1', 'b2', 'm1']` and `mastery_floor('mid') = []`.
/// A floor that took the referenced course alone would give `['m1']`.
#[test]
fn a_mid_order_mastery_floor_course_unions_every_lower_course() {
    let curriculum = arena("arena-floor-order");
    let top = curriculum
        .mastery_floor("top")
        .expect("top is a course of the catalog");
    assert_eq!(top.len(), 3, "base has two topics and mid has one");
    assert_eq!(ids(&curriculum, &top), ["b1", "b2", "m1"]);
    assert!(
        curriculum
            .mastery_floor("mid")
            .expect("mid is a course of the catalog")
            .is_empty(),
        "mid names no floor of its own"
    );
}

/// Findings #7 and #9. `arena-course-duplicate` declares the course id `b`
/// twice. 1.0 on the fixture: `course_by_id['b']` is `B second` with order 5,
/// and `mastery_floor('c') = ['a1', 'a2', 'c1', 'c2']`. Keeping the first entry
/// would give order 2 and the floor `['a1', 'a2']`.
#[test]
fn a_repeated_course_id_resolves_to_the_last_catalog_entry() {
    let curriculum = arena("arena-course-duplicate");
    let course = curriculum
        .course("b")
        .expect("b is a course of the catalog");
    assert_eq!(course.name, "B second");
    assert_eq!(course.order, 5);
    assert_eq!(curriculum.courses().len(), 4, "the catalog keeps both rows");

    let floor = curriculum
        .mastery_floor("c")
        .expect("c is a course of the catalog");
    assert_eq!(ids(&curriculum, &floor), ["a1", "a2", "c1", "c2"]);
    assert!(
        curriculum
            .mastery_floor("a")
            .expect("a is a course of the catalog")
            .is_empty()
    );
}
