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

use std::path::{Path, PathBuf};

use cadus_core::curriculum::{
    Curriculum, CurriculumError, EncNode, KpIdx, TopicIdx, load_curriculum, load_raw_curriculum,
};

/// The curriculum tree of the repository (C5).
fn curriculum_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum")
}

/// One fixture tree under `crates/core/tests/fixtures/`.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The arena of the checked-in tree.
fn tree() -> Curriculum {
    let (curriculum, findings) = load_curriculum(&curriculum_root()).expect("the tree loads");
    assert!(findings.is_empty(), "the clean tree has 0 parse findings");
    curriculum
}

/// The arena of one fixture.
fn arena(name: &str) -> Curriculum {
    let (curriculum, _findings) = load_curriculum(&fixture(name)).expect("the fixture loads");
    curriculum
}

/// The index of a topic that must exist.
fn idx(curriculum: &Curriculum, id: &str) -> TopicIdx {
    curriculum
        .idx_of(id)
        .unwrap_or_else(|| panic!("topic {id} is missing"))
}

/// Topic indices as string ids.
fn ids<'a>(curriculum: &'a Curriculum, list: &[TopicIdx]) -> Vec<&'a str> {
    list.iter().map(|t| curriculum.id_of(*t)).collect()
}

/// Encompassing edges as `(target id, weight)` pairs, in stored order.
fn links(curriculum: &Curriculum, node: EncNode, forward: bool) -> Vec<(&str, f64)> {
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

// --------------------------------------------------------------------------- //
// Fixtures: order, closures, and max-over-paths
// --------------------------------------------------------------------------- //

#[test]
fn topo_order_breaks_every_tie_by_load_index() {
    // `enc-paths` authors a(root) b(a) c(a) d(b,c) e(root). A queue that took the
    // roots first would emit e second; the min-heap on load index emits it last.
    let curriculum = arena("enc-paths");
    assert_eq!(
        ids(&curriculum, curriculum.topo_order()),
        ["a", "b", "c", "d", "e"]
    );
}

#[test]
fn ancestors_and_descendants_come_back_ascending() {
    let curriculum = arena("enc-paths");
    let d = idx(&curriculum, "d");
    let a = idx(&curriculum, "a");
    assert_eq!(ids(&curriculum, &curriculum.ancestors(d)), ["a", "b", "c"]);
    assert_eq!(
        ids(&curriculum, &curriculum.descendants(a)),
        ["b", "c", "d"]
    );
    assert!(curriculum.ancestors(a).is_empty());
    assert!(
        curriculum.descendants(idx(&curriculum, "e")).is_empty(),
        "e stands alone"
    );
}

#[test]
fn encompassing_weight_takes_the_max_over_paths() {
    // 1.0 on the fixture: W from d = {d: 1.0, b: 0.5, c: 0.9, a: 0.45}.
    // Path d->b->a is 0.5 * 0.9; path d->c->a is 0.9 * 0.4. The larger wins.
    let curriculum = arena("enc-paths");
    let a = idx(&curriculum, "a");
    let b = idx(&curriculum, "b");
    let c = idx(&curriculum, "c");
    let d = idx(&curriculum, "d");
    assert_eq!(curriculum.encompassing_weight(d, a), 0.45);
    assert_eq!(curriculum.encompassing_weight(d, b), 0.5);
    assert_eq!(curriculum.encompassing_weight(d, c), 0.9);
    assert_eq!(curriculum.encompassing_weight(b, a), 0.9);
    assert_eq!(curriculum.encompassing_weight(a, b), 0.0, "no upward flow");
    assert_eq!(curriculum.encompassing_weight(a, a), 1.0);
}

// --------------------------------------------------------------------------- //
// Fixtures: cycles (parity trap 11)
// --------------------------------------------------------------------------- //

#[test]
fn find_cycle_returns_the_three_node_loop() {
    // 1.0 on the fixture: `cycle: ['a', 'b', 'c']`.
    let curriculum = arena("cycle-3");
    let cycle = curriculum.find_cycle().expect("the fixture holds a cycle");
    assert_eq!(ids(&curriculum, &cycle), ["a", "b", "c"]);
}

#[test]
fn find_cycle_starts_at_the_re_entered_node() {
    // `cycle-entered` authors outer -> a -> b -> c -> a. The search starts at
    // `outer`, which is not on the loop. 1.0 answers `['a', 'b', 'c']`, not
    // `['outer', 'a', 'b', 'c']` (parity trap 11).
    let curriculum = arena("cycle-entered");
    let cycle = curriculum.find_cycle().expect("the fixture holds a cycle");
    assert_eq!(ids(&curriculum, &cycle), ["a", "b", "c"]);
}

#[test]
fn a_cycle_leaves_the_topological_order_short() {
    let curriculum = arena("cycle-3");
    assert_eq!(curriculum.topic_count(), 3);
    assert!(
        curriculum.topo_order().is_empty(),
        "every topic of the loop keeps a prerequisite, so Kahn starts with nothing"
    );
}

// --------------------------------------------------------------------------- //
// Fixtures: dangling targets and weight-0 edges (parity traps 7 and 8)
// --------------------------------------------------------------------------- //

#[test]
fn a_dangling_prerequisite_drops_out_of_the_adjacency() {
    // 1.0 on the fixture:
    //   prereqs: {'alpha': [], 'beta': ['alpha']}
    //   enc: {'alpha': {}, 'beta': {'ghost': 0.5, 'alpha': 0.6, 'phantom': 0.3}}
    //   enc_rev: {'alpha': {'beta': 0.6}, 'beta': {},
    //             'ghost': {'beta': 0.5}, 'phantom': {'beta': 0.3}}
    let curriculum = arena("dangling-refs");
    assert_eq!(curriculum.topic_count(), 2);
    let alpha = idx(&curriculum, "alpha");
    let beta = idx(&curriculum, "beta");

    assert_eq!(
        ids(
            &curriculum,
            &curriculum.prerequisites(beta).collect::<Vec<_>>()
        ),
        ["alpha"],
        "the ghost prerequisite is dropped"
    );
    assert_eq!(
        ids(
            &curriculum,
            &curriculum.dependents(alpha).collect::<Vec<_>>()
        ),
        ["beta"]
    );
    assert_eq!(curriculum.prereq_edge_count(), 1);
    assert_eq!(curriculum.idx_of("ghost"), None, "ghost is not a topic");

    assert_eq!(
        links(&curriculum, curriculum.enc_node(beta), true),
        [("ghost", 0.5), ("alpha", 0.6), ("phantom", 0.3)],
        "the encompassing map keeps every dangling target"
    );
    assert_eq!(
        links(&curriculum, curriculum.enc_node(alpha), false),
        [("beta", 0.6)]
    );

    assert_eq!(
        curriculum.enc_node_count(),
        4,
        "two topics plus the ghost and phantom nodes"
    );
    for (id, weight) in [("ghost", 0.5), ("phantom", 0.3)] {
        let node = curriculum
            .enc_node_by_id(id)
            .unwrap_or_else(|| panic!("{id} is an encompassing node"));
        assert_eq!(
            curriculum.topic_of_enc_node(node),
            None,
            "{id} has no topic"
        );
        assert_eq!(curriculum.enc_node_id(node), id);
        assert_eq!(links(&curriculum, node, false), [("beta", weight)]);
        assert!(
            links(&curriculum, node, true).is_empty(),
            "a phantom node has no forward edge"
        );
    }
}

#[test]
fn a_weight_zero_edge_is_absent_forward_and_present_in_reverse() {
    // 1.0 on the fixture: enc = {'alpha': {}, 'beta': {}};
    // enc_rev = {'alpha': {'beta': 0.0}, 'beta': {}}. Parity trap 8.
    let curriculum = arena("zero-weight-edge");
    let alpha = idx(&curriculum, "alpha");
    let beta = idx(&curriculum, "beta");

    assert_eq!(curriculum.enc_forward_count(), 0);
    assert_eq!(curriculum.enc_reverse_count(), 1);
    assert!(links(&curriculum, curriculum.enc_node(beta), true).is_empty());
    assert_eq!(
        links(&curriculum, curriculum.enc_node(alpha), false),
        [("beta", 0.0)]
    );
    assert_eq!(
        curriculum.encompassing_weight(beta, alpha),
        0.0,
        "a weight-0 edge carries no credit"
    );
    assert_eq!(
        ids(
            &curriculum,
            &curriculum.prerequisites(beta).collect::<Vec<_>>()
        ),
        ["alpha"],
        "the prerequisite edge itself stays"
    );
}

// --------------------------------------------------------------------------- //
// Fixtures: the one build error (parity traps 13 and 14)
// --------------------------------------------------------------------------- //

#[test]
fn a_duplicate_topic_id_stops_the_build() {
    let (raw, findings) = load_raw_curriculum(&fixture("duplicate-topic-id"))
        .expect("the parse stage passes: the schema is valid");
    assert!(findings.is_empty(), "no parse-stage finding");

    let error = Curriculum::build(raw).expect_err("a topic map cannot hold both topics");
    assert_eq!(
        error,
        CurriculumError::DuplicateTopicId {
            id: "alpha".to_owned()
        }
    );
    // The 1.0 finding message for this fixture.
    assert_eq!(error.to_string(), "topic id 'alpha' defined more than once");
}

#[test]
fn a_graph_stage_defect_does_not_stop_the_build() {
    // Parity trap 13: 1.0 `Graph.load` tolerates cycles, dangling references,
    // and missing knowledge points. Only the parse stage can block a load.
    for name in [
        "cycle-3",
        "cycle-entered",
        "dangling-refs",
        "zero-weight-edge",
    ] {
        let loaded = load_curriculum(&fixture(name));
        assert!(loaded.is_ok(), "{name} must load");
    }
}
