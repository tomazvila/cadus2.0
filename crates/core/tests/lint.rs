//! U3 acceptance: the curriculum lint (C5, spec section 5).
//!
//! The oracle is the 1.0 linter. `scripts/oracle/make_lint_fixtures.py` writes
//! one minimal broken tree per lint code under `tests/fixtures/lint/`, and
//! `scripts/oracle/dump_lint_1_0.py` writes the `expected.json` beside each tree
//! from a run of 1.0 `cadus.graph.lint_curriculum`. Those files are committed:
//! they are the expected values, and no expected value here comes from the code
//! under test.
//!
//! ## The one normalized field
//!
//! A `yaml` finding carries the YAML library's own exception text, and that text
//! embeds the absolute path of the file. PyYAML and `serde_norway` never write
//! the same words, so both the dumper and [`canonical`] replace the text after
//! the `{rel}: ` prefix with the literal `<yaml parser message>`. Everything else
//! — the code, the prefix, the file, the fatal flag — is compared verbatim.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_core::curriculum::lint_curriculum;
use common::lint_view::{canonical, codes, expected};
use common::paths::{curriculum_root, lint_fixture};

/// Every committed fixture tree: one per lint code of spec section 5, plus the
/// extra trees named in the comments below. Five trees are order trees: they fix
/// the authored order against the id order, put nine codes in one tree, span two
/// modules over two courses, and repeat one course id, so the byte comparison
/// sees every `sorted()` site, the order of the rule blocks, and the
/// last-entry-wins rule of the catalog. A missing directory fails the walk, so a
/// deleted fixture cannot pass unnoticed.
const FIXTURES: [&str; 24] = [
    "clean",                          // the 1.0 curriculum_mini copy: 0 findings
    "course_id_repeated",             // the catalog keeps the LAST entry of an id
    "cycle",                          // 8
    "cycle_duplicate_missing_ref",    // nine codes, and both skip rules at once
    "duplicate_topic_id",             // 6
    "empty",                          // the graph-only code
    "empty_course",                   // 5
    "key_prereq_not_ancestor",        // 12
    "many_codes",                     // nine codes, one per rule block
    "mastery_floor_ambiguous",        // 15
    "missing_course_dir",             // 4
    "missing_diagnostic_exemplar",    // 11
    "missing_ref",                    // 7, all three messages
    "module_inconsistent",            // 14, the "spans multiple courses" message
    "module_inconsistent_empty_name", // 14, the "empty module name" message
    "module_spans_two_modules",       // 14, the `sorted(module_courses)` site
    "no_exemplar",                    // 10
    "no_kp",                          // 9
    "noncore_ancestor_of_core",       // 13, with the "(+n more)" tail
    "order_not_id_order",             // three `sorted()` sites of the lint
    "schema",                         // 2
    "unreachable_from_floor",         // 16
    "weight_out_of_range",            // 3
    "yaml",                           // 1
];

// --------------------------------------------------------------------------- //
// The oracle comparison
// --------------------------------------------------------------------------- //

#[test]
fn every_fixture_matches_the_committed_1_0_output_byte_for_byte() {
    for name in FIXTURES {
        let found = canonical(&lint_curriculum(&lint_fixture(name)));
        assert_eq!(
            found,
            expected(name),
            "fixture {name}: the Rust lint output differs from the committed 1.0 output"
        );
    }
}

#[test]
fn the_checked_in_curriculum_has_zero_findings() {
    // Spec section 3, the verbatim facts of a real load: `lint findings: 0`.
    let findings = lint_curriculum(&curriculum_root());
    assert_eq!(
        codes(&findings),
        Vec::<&str>::new(),
        "the checked-in tree must lint clean (C5)"
    );
}

#[test]
fn the_clean_fixture_has_zero_findings() {
    // The 1.0 `tests/fixtures/curriculum_mini`, copied verbatim.
    assert_eq!(
        codes(&lint_curriculum(&lint_fixture("clean"))),
        Vec::<&str>::new()
    );
    assert_eq!(expected("clean"), "[]\n");
}

// --------------------------------------------------------------------------- //
// Literal messages, flags and payloads
// --------------------------------------------------------------------------- //

#[test]
fn a_missing_courses_file_is_the_empty_code() {
    // Spec section 5: the graph-only code. 1.0 `lint_curriculum` raises
    // `CurriculumNotFound`; `Graph.load` reports this finding for the same tree.
    let findings = lint_curriculum(&lint_fixture("empty"));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, "empty");
    assert_eq!(findings[0].message, "no curriculum found");
    assert_eq!(findings[0].topic, None);
    assert_eq!(findings[0].file, None);
    assert!(findings[0].fatal);
}

#[test]
fn the_parse_stage_advisory_codes_are_not_fatal() {
    // Spec section 5, rules 4 and 5: neither drops content, so neither is fatal.
    let missing = lint_curriculum(&lint_fixture("missing_course_dir"));
    assert_eq!(codes(&missing), vec!["missing_course_dir"]);
    assert_eq!(
        missing[0].message,
        "no unit directory ghostcourse/ for course"
    );
    assert!(!missing[0].fatal);

    let empty = lint_curriculum(&lint_fixture("empty_course"));
    assert_eq!(codes(&empty), vec!["empty_course"]);
    assert_eq!(empty[0].message, "course hollowcourse has no unit files");
    assert!(!empty[0].fatal);
}

#[test]
fn the_parse_stage_schema_codes_carry_the_dotted_location_and_the_file() {
    let schema = lint_curriculum(&lint_fixture("schema"));
    assert_eq!(codes(&schema), vec!["schema"]);
    assert_eq!(
        schema[0].message,
        "topics.0.answer_kind: Input should be 'numeric', 'expression', 'multi-step' or 'proof'"
    );
    assert_eq!(schema[0].file.as_deref(), Some("c/00.yaml"));
    assert!(schema[0].fatal);

    let weight = lint_curriculum(&lint_fixture("weight_out_of_range"));
    assert_eq!(codes(&weight), vec!["weight_out_of_range"]);
    assert_eq!(
        weight[0].message,
        "topics.1.prerequisites.0.weight: Input should be less than or equal to 1"
    );
    assert_eq!(weight[0].file.as_deref(), Some("c/00.yaml"));

    let yaml = lint_curriculum(&lint_fixture("yaml"));
    assert_eq!(codes(&yaml), vec!["yaml"]);
    assert_eq!(yaml[0].file.as_deref(), Some("c/00.yaml"));
    assert!(
        yaml[0].message.starts_with("c/00.yaml: "),
        "message was {:?}",
        yaml[0].message
    );
}

#[test]
fn a_cycle_starts_at_the_re_entered_node_and_carries_its_nodes() {
    // Parity trap 11 and spec section 5, rule 8.
    let findings = lint_curriculum(&lint_fixture("cycle"));
    assert_eq!(codes(&findings), vec!["cycle"]);
    assert_eq!(findings[0].message, "prerequisite cycle: a -> b -> c -> a");
    assert_eq!(
        findings[0].context.as_deref(),
        Some(["a".to_owned(), "b".to_owned(), "c".to_owned()].as_slice())
    );
    assert_eq!(findings[0].topic, None);
}

#[test]
fn the_three_missing_ref_messages_come_in_the_1_0_order() {
    // Spec section 5, rule 7: prerequisites, then encompassings_extra, then the
    // knowledge points, per topic in load order.
    let findings = lint_curriculum(&lint_fixture("missing_ref"));
    let messages: Vec<&str> = findings.iter().map(|f| f.message.as_str()).collect();
    assert_eq!(
        messages,
        vec![
            "prerequisite 'ghost' of 'b' does not exist",
            "encompassings_extra 'phantom' of 'b' does not exist",
            "key_prerequisite 'nowhere' in b.kp1 does not exist",
        ]
    );
    for finding in &findings {
        assert_eq!(finding.code, "missing_ref");
        assert_eq!(finding.topic.as_deref(), Some("b"));
    }
}

#[test]
fn a_key_prerequisite_that_is_no_ancestor_is_reported_once() {
    let findings = lint_curriculum(&lint_fixture("key_prereq_not_ancestor"));
    assert_eq!(codes(&findings), vec!["key_prereq_not_ancestor"]);
    assert_eq!(
        findings[0].message,
        "key_prerequisite 'c' in b.kp1 is neither an ancestor nor an encompassings_extra target"
    );
    assert_eq!(findings[0].topic.as_deref(), Some("b"));
}

#[test]
fn a_noncore_ancestor_names_one_core_dependent_and_counts_the_rest() {
    // Spec section 5, rule 13: the example is the first sorted core dependent,
    // the tail counts the others, and the context lists them all.
    let findings = lint_curriculum(&lint_fixture("noncore_ancestor_of_core"));
    assert_eq!(codes(&findings), vec!["noncore_ancestor_of_core"]);
    assert_eq!(
        findings[0].message,
        "non-core topic 'base' is a prerequisite (ancestor) of core topic 'top' (+1 more)"
    );
    assert_eq!(findings[0].topic.as_deref(), Some("base"));
    assert_eq!(
        findings[0].context.as_deref(),
        Some(["top".to_owned(), "top2".to_owned()].as_slice())
    );
}

#[test]
fn the_two_module_messages_use_the_python_repr_forms() {
    // Spec section 5, rule 14. The course list is a Python list repr.
    let spans = lint_curriculum(&lint_fixture("module_inconsistent"));
    assert_eq!(codes(&spans), vec!["module_inconsistent"]);
    assert_eq!(
        spans[0].message,
        "module 'Shared' spans multiple courses: ['c1', 'c2']"
    );
    assert_eq!(spans[0].topic, None);

    let blank = lint_curriculum(&lint_fixture("module_inconsistent_empty_name"));
    assert_eq!(codes(&blank), vec!["module_inconsistent"]);
    assert_eq!(blank[0].message, "topic 'a' has an empty module name");
    assert_eq!(blank[0].topic.as_deref(), Some("a"));
}

#[test]
fn both_mastery_floor_forms_on_one_course_are_ambiguous() {
    let findings = lint_curriculum(&lint_fixture("mastery_floor_ambiguous"));
    assert_eq!(codes(&findings), vec!["mastery_floor_ambiguous"]);
    assert_eq!(
        findings[0].message,
        "course 'c2' sets both a mastery_floor list and mastery_floor_course 'c1'; \
         a course must use exactly one mastery-floor form"
    );
}

#[test]
fn an_ungrounded_course_topic_is_unreachable_from_its_floor() {
    // Spec section 5, rule 16. The course id is plain, the topic id is a repr.
    let findings = lint_curriculum(&lint_fixture("unreachable_from_floor"));
    assert_eq!(codes(&findings), vec!["unreachable_from_floor"]);
    assert_eq!(
        findings[0].message,
        "topic 'b' is not reachable from course c2's floor/roots"
    );
    assert_eq!(findings[0].topic.as_deref(), Some("b"));
}

#[test]
fn the_cardinality_rules_name_the_topic_and_the_knowledge_point() {
    let no_kp = lint_curriculum(&lint_fixture("no_kp"));
    assert_eq!(codes(&no_kp), vec!["no_kp"]);
    assert_eq!(no_kp[0].message, "topic 'a' has no knowledge_points");

    let no_exemplar = lint_curriculum(&lint_fixture("no_exemplar"));
    assert_eq!(codes(&no_exemplar), vec!["no_exemplar"]);
    assert_eq!(no_exemplar[0].message, "KP a.kp1 has no exemplars");

    let no_diag = lint_curriculum(&lint_fixture("missing_diagnostic_exemplar"));
    assert_eq!(codes(&no_diag), vec!["missing_diagnostic_exemplar"]);
    assert_eq!(no_diag[0].message, "topic 'a' has no diagnostic_exemplar");
}

// --------------------------------------------------------------------------- //
// The skip rules
// --------------------------------------------------------------------------- //

#[test]
fn a_duplicate_topic_id_keeps_the_first_definition_and_skips_reachability() {
    // Spec section 2 and section 5, rule 16. The fixture holds `a` twice in a
    // course whose floor is `a`; the tree yields the duplicate finding alone.
    let findings = lint_curriculum(&lint_fixture("duplicate_topic_id"));
    assert_eq!(codes(&findings), vec!["duplicate_topic_id"]);
    assert_eq!(findings[0].message, "topic id 'a' defined more than once");
    assert_eq!(findings[0].topic.as_deref(), Some("a"));
    assert!(findings[0].fatal);
}

#[test]
fn a_cycle_skips_the_reachability_rule() {
    // Spec section 5, rule 16: neither topic of a cycle is a root, so a naive
    // pass would call them all unreachable. The fixture declares no floor.
    let findings = lint_curriculum(&lint_fixture("cycle"));
    assert_eq!(codes(&findings), vec!["cycle"]);
}

#[test]
fn a_missing_key_prerequisite_skips_the_ancestor_rule() {
    // Spec section 5, rule 12: already reported as `missing_ref`.
    let findings = lint_curriculum(&lint_fixture("missing_ref"));
    assert!(
        !codes(&findings).contains(&"key_prereq_not_ancestor"),
        "codes were {:?}",
        codes(&findings)
    );
}
