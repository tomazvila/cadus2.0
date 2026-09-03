//! U2 acceptance, part 2: the fixture trees (D1, D2).
//!
//! Every expected value below is a literal: the orders, the closures, the
//! weights, the cycle lists and the error texts come from a run of the 1.0
//! loader over the same fixture, quoted in the unit report. No expected value
//! is derived by the code under test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_core::curriculum::{
    Curriculum, CurriculumError, EncNode, Finding, LoadError, TopicIdx, canonical_dump,
    load_curriculum, load_raw_curriculum,
};
use common::arena_view::{ids, idx, links};
use common::paths::{arena, fixture};

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

/// Findings #22 and #9. `arena-repeated-edge` declares b -> a twice with the
/// larger weight FIRST (0.7 then 0.2) and c -> a twice with the larger weight
/// LAST (0.2 then 0.7), so the pair separates "keep the maximum" from both
/// "keep the first weight" and "keep the last weight". `z -> a` carries the
/// weight 0. 1.0 on the fixture:
///
/// ```text
/// $ /home/deploy/dev/cadus/.venv/bin/python  (sys.path -> /home/deploy/dev/cadus)
/// topics ['a', 'b', 'c', 'z']
/// _enc {'a': {}, 'b': {'a': 0.7}, 'c': {'a': 0.7}, 'z': {}}
/// _enc_rev {'a': {'b': 0.7, 'c': 0.7, 'z': 0.0}, 'b': {}, 'c': {}, 'z': {}}
/// prereqs {'a': set(), 'b': {'a'}, 'c': {'a'}, 'z': {'a'}}
/// dependents {'a': ['b', 'c', 'z'], 'b': [], 'c': [], 'z': []}
/// W b a 0.7
/// W c a 0.7
/// ```
#[test]
fn a_repeated_edge_keeps_the_maximum_weight_in_both_encompassing_maps() {
    let curriculum = arena("arena-repeated-edge");
    let a = idx(&curriculum, "a");
    let b = idx(&curriculum, "b");
    let c = idx(&curriculum, "c");
    let z = idx(&curriculum, "z");

    assert_eq!(
        links(&curriculum, curriculum.enc_node(b), true),
        [("a", 0.7)],
        "the larger weight comes first, and the forward map keeps it"
    );
    assert_eq!(
        links(&curriculum, curriculum.enc_node(c), true),
        [("a", 0.7)],
        "the larger weight comes last, and the forward map keeps it"
    );
    assert_eq!(
        links(&curriculum, curriculum.enc_node(a), false),
        [("b", 0.7), ("c", 0.7), ("z", 0.0)],
        "the reverse map keeps the larger weight of each pair and the weight-0 edge"
    );
    assert_eq!(curriculum.enc_forward_count(), 2);
    assert_eq!(curriculum.enc_reverse_count(), 3);

    assert_eq!(
        ids(
            &curriculum,
            &curriculum.prerequisites(b).collect::<Vec<_>>()
        ),
        ["a"],
        "the repeated edge gives one prerequisite entry"
    );
    assert_eq!(
        ids(
            &curriculum,
            &curriculum.prerequisites(c).collect::<Vec<_>>()
        ),
        ["a"]
    );
    assert_eq!(
        ids(&curriculum, &curriculum.dependents(a).collect::<Vec<_>>()),
        ["b", "c", "z"]
    );
    assert_eq!(curriculum.prereq_edge_count(), 3);
    assert_eq!(curriculum.dependent_edge_count(), 3);
    assert_eq!(
        curriculum.encompassing_weight(b, a),
        0.7,
        "0.7 first, then 0.2: a rule of the last weight gives 0.2 here"
    );
    assert_eq!(
        curriculum.encompassing_weight(c, a),
        0.7,
        "0.2 first, then 0.7: a rule of the first weight gives 0.2 here"
    );
    assert_eq!(
        curriculum.encompassing_weight(z, a),
        0.0,
        "a weight-0 edge carries no credit"
    );
}

// --------------------------------------------------------------------------- //
// Fixtures: closures over a cycle (1.0 `_closure`)
// --------------------------------------------------------------------------- //

/// Finding #23. `cycle-3` authors the prerequisite loop a -> b -> c -> a. 1.0
/// `_closure` drops the start node even when the cycle leads back to it, so on
/// the fixture `g.ancestors('a') == {'b', 'c'}` and
/// `g.descendants('a') == {'b', 'c'}`, and the same holds for `b` and `c`.
#[test]
fn a_closure_over_a_cycle_leaves_out_the_start_node() {
    let curriculum = arena("cycle-3");
    let a = idx(&curriculum, "a");
    let b = idx(&curriculum, "b");
    let c = idx(&curriculum, "c");

    assert_eq!(ids(&curriculum, &curriculum.ancestors(a)), ["b", "c"]);
    assert_eq!(ids(&curriculum, &curriculum.descendants(a)), ["b", "c"]);
    assert_eq!(ids(&curriculum, &curriculum.ancestors(b)), ["a", "c"]);
    assert_eq!(ids(&curriculum, &curriculum.descendants(b)), ["a", "c"]);
    assert_eq!(ids(&curriculum, &curriculum.ancestors(c)), ["a", "b"]);
    assert_eq!(ids(&curriculum, &curriculum.descendants(c)), ["a", "b"]);
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

/// Findings #12 and #13. `arena-parse-fatal` writes `weight: 1.5`, so the parse
/// stage drops the whole unit file and reports one fatal finding. 1.0 on the
/// fixture raises
/// `CurriculumError: [weight_out_of_range] topics.1.prerequisites.0.weight:
/// Input should be less than or equal to 1`.
#[test]
fn a_fatal_parse_finding_stops_the_load() {
    let error = load_curriculum(&fixture("arena-parse-fatal"))
        .expect_err("a dropped unit file makes the arena misrepresent the tree");
    assert_eq!(
        error,
        LoadError::Curriculum(CurriculumError::FatalFindings {
            findings: vec![
                Finding::new(
                    "weight_out_of_range",
                    "topics.1.prerequisites.0.weight: Input should be less than or equal to 1",
                )
                .with_file("demo/01-basics.yaml"),
            ],
        })
    );
    // The 1.0 `CurriculumError` text, `[code] message` joined with `; `.
    assert_eq!(
        error.to_string(),
        "[weight_out_of_range] topics.1.prerequisites.0.weight: \
Input should be less than or equal to 1"
    );

    // The parse stage still hands the findings back for the lint of U3.
    let parsed = load_raw_curriculum(&fixture("arena-parse-fatal"));
    let (_raw, findings) = parsed.expect("the parse stage itself reports, it does not block");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, "weight_out_of_range");
    assert!(findings[0].fatal);
}

/// An advisory finding drops no content, so the load continues after it.
/// `arena-course-duplicate` declares the course `b` with no unit directory, and
/// 1.0 loads the tree with two `missing_course_dir` findings.
#[test]
fn an_advisory_parse_finding_does_not_stop_the_load() {
    let (curriculum, findings) =
        load_curriculum(&fixture("arena-course-duplicate")).expect("the tree loads");
    assert_eq!(curriculum.topic_count(), 4);
    assert_eq!(findings.len(), 2);
    for finding in &findings {
        assert_eq!(finding.code, "missing_course_dir");
        assert_eq!(finding.message, "no unit directory b/ for course");
        assert!(!finding.fatal);
    }
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

// --------------------------------------------------------------------------- //
// Fixtures: the dump of a defective tree and the unknown id
// --------------------------------------------------------------------------- //

/// The dump names the cycle and drops the dangling prerequisite from
/// `prereq_edges` (parity trap 7).
#[test]
fn the_dump_carries_the_cycle_and_drops_the_dangling_edge() {
    let cycle = canonical_dump(&arena("cycle-3"));
    assert!(
        cycle.contains(r#""cycle":["a","b","c"]"#),
        "the dump names the loop, it wrote {cycle}"
    );
    let dangling = canonical_dump(&arena("dangling-refs"));
    assert!(
        !dangling.contains(r#"["b","ghost""#),
        "a dangling prerequisite is no prereq_edges row, it wrote {dangling}"
    );
}

/// Every id query answers the empty value for an id no build knows.
#[test]
fn an_unknown_id_gives_the_empty_answer_from_every_query() {
    let curriculum = arena("cycle-3");
    assert_eq!(curriculum.encompassing_weight_by_id("a", "nowhere"), 0.0);
    assert_eq!(curriculum.encompassing_weight_by_id("nowhere", "a"), 0.0);
    assert_eq!(curriculum.reach_weights_by_id("nowhere"), Vec::new());
    assert_eq!(curriculum.upward_weights_by_id("nowhere"), Vec::new());
    assert_eq!(curriculum.neighborhood("nowhere"), Vec::<&str>::new());
    assert_eq!(curriculum.enc_node_id(EncNode::from_u32(99)), "");
    assert_eq!(curriculum.enc_node_by_id("nowhere"), None);
    assert_eq!(curriculum.id_of(TopicIdx::from_u32(99)), "");
    assert_eq!(curriculum.course_of(TopicIdx::from_u32(99)), "");
    assert_eq!(curriculum.mastery_floor("nowhere"), None);
}

/// The 1.0 `CurriculumError` text of an empty finding list.
#[test]
fn a_fatal_error_with_no_finding_reads_invalid_curriculum() {
    let error = CurriculumError::FatalFindings {
        findings: Vec::new(),
    };
    assert_eq!(error.to_string(), "invalid curriculum");
    let one = CurriculumError::FatalFindings {
        findings: vec![Finding::new("yaml", "c/00.yaml: broken")],
    };
    assert_eq!(one.to_string(), "[yaml] c/00.yaml: broken");
}

/// A tree with no `courses.yaml` does not start, and the error is the parse
/// error of 1.0 `CurriculumNotFound`.
#[test]
fn a_tree_without_a_catalog_is_a_parse_error() {
    let root = fixture("no-courses-file");
    let error = load_curriculum(&root).expect_err("no catalog");
    assert!(matches!(error, LoadError::Parse(_)), "it gave {error:?}");
    assert_eq!(
        error.to_string(),
        format!("no courses.yaml under {}", root.display())
    );
    let curriculum = arena("cycle-3");
    assert_eq!(
        curriculum.topic_of_enc_node(EncNode::from_u32(0)),
        Some(TopicIdx::from_u32(0))
    );

    let duplicate = load_curriculum(&fixture("duplicate-topic-id")).expect_err("no build");
    assert!(
        matches!(
            duplicate,
            LoadError::Curriculum(CurriculumError::DuplicateTopicId { .. })
        ),
        "it gave {duplicate:?}"
    );
}
