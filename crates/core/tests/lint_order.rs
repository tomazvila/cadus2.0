//! U3 acceptance, part 2: the order of the lint output and the canonical form
//! (C5, spec section 5).
//!
//! The oracle is the 1.0 linter, as in `lint.rs`: every expected list below is
//! a literal from the committed `expected.json` beside the fixture tree.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_core::curriculum::{Finding, lint_curriculum};
use common::lint_view::{canonical, codes, messages};
use common::paths::lint_fixture;

// --------------------------------------------------------------------------- //
// Order: the `sorted()` sites and the sequence of the rule blocks
// --------------------------------------------------------------------------- //

#[test]
fn the_lint_walks_topic_ids_in_sorted_order_not_in_authored_order() {
    // Fixture `order_not_id_order` authors `z`, `m`, `y`, `a` in c1 and `x`, `b`
    // in c2, so the load order is the reverse of the id order at every
    // `sorted()` site of 1.0 `lint_curriculum`:
    //   * `for tid in sorted(topics)` orders the two noncore findings `m`, `z`;
    //   * `sorted(core_dependents)` names `'a'` as the example, not `'y'`;
    //   * `sorted(course_topics - reachable)` orders the two unreachable
    //     topics `b`, `x`.
    // Every value below is the committed 1.0 output.
    let findings = lint_curriculum(&lint_fixture("order_not_id_order"));
    assert_eq!(
        codes(&findings),
        vec![
            "key_prereq_not_ancestor",
            "noncore_ancestor_of_core",
            "noncore_ancestor_of_core",
            "unreachable_from_floor",
            "unreachable_from_floor",
        ]
    );
    assert_eq!(
        messages(&findings),
        vec![
            "key_prerequisite 'x' in b.kp1 is neither an ancestor nor an encompassings_extra \
             target",
            "non-core topic 'm' is a prerequisite (ancestor) of core topic 'a' (+3 more)",
            "non-core topic 'z' is a prerequisite (ancestor) of core topic 'a' (+1 more)",
            "topic 'b' is not reachable from course c2's floor/roots",
            "topic 'x' is not reachable from course c2's floor/roots",
        ]
    );
    // `sorted(core_dependents)` also fixes the context payload.
    assert_eq!(
        findings[1].context.as_deref(),
        Some(
            [
                "a".to_owned(),
                "b".to_owned(),
                "x".to_owned(),
                "y".to_owned()
            ]
            .as_slice()
        )
    );
    assert_eq!(
        findings[2].context.as_deref(),
        Some(["a".to_owned(), "y".to_owned()].as_slice())
    );
}

#[test]
fn two_modules_over_two_courses_come_in_sorted_module_order() {
    // Review finding 12. 1.0 walks `sorted(module_courses.items())`
    // (`cadus/graph.py:780`), which is the fourth `sorted()` site of the lint.
    // Fixture `module_spans_two_modules` authors `Zeta` before `Alpha` in both
    // courses, and both modules span both courses, so a port that walks the map
    // in load order or in reverse swaps the two findings. Every value below is
    // the committed 1.0 output.
    let findings = lint_curriculum(&lint_fixture("module_spans_two_modules"));
    assert_eq!(
        codes(&findings),
        vec!["module_inconsistent", "module_inconsistent"]
    );
    assert_eq!(
        messages(&findings),
        vec![
            "module 'Alpha' spans multiple courses: ['c1', 'c2']",
            "module 'Zeta' spans multiple courses: ['c1', 'c2']",
        ]
    );
}

#[test]
fn a_repeated_course_id_resolves_to_the_last_catalog_entry() {
    // Review finding 11. 1.0 builds `{c.id: c for c in catalog.courses}`
    // (`cadus/graph.py:824`), so a repeated course id keeps the LAST entry.
    // Fixture `course_id_repeated` declares `c1` at order 1 and again at order
    // 3, and c2 grounds its floor on `mastery_floor_course: c1`. The floor of c2
    // therefore unions every course at or below order 3, `mid` included, and
    // `b` is reachable. A port that keeps the first entry unions only order 1
    // and adds `[unreachable_from_floor] topic 'b' is not reachable from course
    // c2's floor/roots`. The two advisory findings below are the committed 1.0
    // output: `c1` holds no unit file, and the catalog names it twice.
    let findings = lint_curriculum(&lint_fixture("course_id_repeated"));
    assert_eq!(codes(&findings), vec!["empty_course", "empty_course"]);
    assert_eq!(
        messages(&findings),
        vec!["course c1 has no unit files", "course c1 has no unit files",]
    );
}

#[test]
fn nine_codes_in_one_tree_come_in_the_1_0_rule_block_order() {
    // Fixture `many_codes` trips nine rules at once, so the list below pins the
    // sequence of the rule blocks: referenced ids, then the per-topic
    // cardinality and key-prerequisite rules, then the core-ancestor invariant,
    // then the module names (the empty-name form first, in load order, then the
    // "spans multiple courses" form), then the mastery-floor form, then
    // reachability. Every value is the committed 1.0 output.
    let findings = lint_curriculum(&lint_fixture("many_codes"));
    assert_eq!(
        codes(&findings),
        vec![
            "missing_ref",
            "missing_ref",
            "missing_ref",
            "no_kp",
            "no_exemplar",
            "missing_diagnostic_exemplar",
            "key_prereq_not_ancestor",
            "noncore_ancestor_of_core",
            "module_inconsistent",
            "module_inconsistent",
            "module_inconsistent",
            "mastery_floor_ambiguous",
            "unreachable_from_floor",
            "unreachable_from_floor",
        ]
    );
    assert_eq!(
        messages(&findings),
        vec![
            "prerequisite 'ghost' of 'mid' does not exist",
            "encompassings_extra 'phantom' of 'mid' does not exist",
            "key_prerequisite 'nowhere' in mid.kp1 does not exist",
            "topic 'zcore' has no knowledge_points",
            "KP acore.kp1 has no exemplars",
            "topic 'acore' has no diagnostic_exemplar",
            "key_prerequisite 'zcore' in zun.kp1 is neither an ancestor nor an \
             encompassings_extra target",
            "non-core topic 'nbase' is a prerequisite (ancestor) of core topic 'acore' (+3 more)",
            "topic 'zun' has an empty module name",
            "topic 'aun' has an empty module name",
            "module 'Shared' spans multiple courses: ['c1', 'c2']",
            "course 'c2' sets both a mastery_floor list and mastery_floor_course 'c1'; \
             a course must use exactly one mastery-floor form",
            "topic 'aun' is not reachable from course c3's floor/roots",
            "topic 'zun' is not reachable from course c3's floor/roots",
        ]
    );
}

#[test]
fn a_cycle_and_a_duplicate_together_skip_reachability_in_a_nine_code_tree() {
    // Fixture `cycle_duplicate_missing_ref` trips both skip conditions of spec
    // section 5, rule 16 at once. `cyc1`, `cyc2` and `orphan` are all
    // ungrounded, so a port that drops the guard adds three
    // `unreachable_from_floor` findings. The other six codes pin the order of
    // the rule blocks around the two skipped rules. Every value is the
    // committed 1.0 output.
    let findings = lint_curriculum(&lint_fixture("cycle_duplicate_missing_ref"));
    assert_eq!(
        codes(&findings),
        vec![
            "duplicate_topic_id",
            "missing_ref",
            "cycle",
            "no_kp",
            "no_exemplar",
            "missing_diagnostic_exemplar",
            "key_prereq_not_ancestor",
            "noncore_ancestor_of_core",
            "module_inconsistent",
        ]
    );
    assert_eq!(
        messages(&findings),
        vec![
            "topic id 'zdup' defined more than once",
            "prerequisite 'ghost' of 'mref' does not exist",
            "prerequisite cycle: cyc1 -> cyc2 -> cyc1",
            "topic 'zkid' has no knowledge_points",
            "KP akid.kp1 has no exemplars",
            "topic 'akid' has no diagnostic_exemplar",
            "key_prerequisite 'mref' in kpx.kp1 is neither an ancestor nor an \
             encompassings_extra target",
            "non-core topic 'nbase' is a prerequisite (ancestor) of core topic 'akid' (+1 more)",
            "module 'Shared' spans multiple courses: ['c1', 'c2']",
        ]
    );
    assert_eq!(
        findings[2].context.as_deref(),
        Some(["cyc1".to_owned(), "cyc2".to_owned()].as_slice())
    );
}

#[test]
fn two_fixtures_trip_nine_distinct_codes_each() {
    // The guard of finding #25: with one code per fixture the byte comparison
    // never sees the order of the rule blocks. Both trees below hold nine
    // distinct codes, so a swapped pair of rule blocks changes their committed
    // output. A later edit that thins one of the trees fails here.
    for name in ["many_codes", "cycle_duplicate_missing_ref"] {
        let findings = lint_curriculum(&lint_fixture(name));
        let mut distinct = codes(&findings);
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), 9, "fixture {name} holds {distinct:?}");
    }
}

// --------------------------------------------------------------------------- //
// The canonical form itself
// --------------------------------------------------------------------------- //

#[test]
fn the_canonical_form_sorts_the_keys_and_drops_the_empty_options() {
    // The dumper writes `sort_keys=True`, so `code` precedes `context` precedes
    // `fatal` precedes `message` precedes `topic`. 1.0 `as_dict` drops `topic`,
    // `file` and an empty `context`.
    let finding = Finding::new("cycle", "prerequisite cycle: a -> b -> a")
        .with_context(vec!["a".to_owned(), "b".to_owned()]);
    let want = r#"[
  {
    "code": "cycle",
    "context": [
      "a",
      "b"
    ],
    "fatal": true,
    "message": "prerequisite cycle: a -> b -> a"
  }
]
"#;
    assert_eq!(canonical(&[finding]), want);
    assert_eq!(canonical(&[]), "[]\n");
}
