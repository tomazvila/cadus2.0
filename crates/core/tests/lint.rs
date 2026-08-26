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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cadus_core::curriculum::{Finding, lint_curriculum};

/// The tail that replaces a YAML library's exception text on both sides.
const YAML_TAIL: &str = "<yaml parser message>";

/// Every committed fixture tree: one per lint code of spec section 5, plus the
/// extra trees named in the comments below. The last three trees are multi-rule:
/// they fix the authored order against the id order and put nine codes in one
/// tree, so the byte comparison sees every `sorted()` site and the order of the
/// rule blocks. A missing directory fails the walk, so a deleted fixture cannot
/// pass unnoticed.
const FIXTURES: [&str; 22] = [
    "clean",                          // the 1.0 curriculum_mini copy: 0 findings
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
    "no_exemplar",                    // 10
    "no_kp",                          // 9
    "noncore_ancestor_of_core",       // 13, with the "(+n more)" tail
    "order_not_id_order",             // every `sorted()` site of the lint
    "schema",                         // 2
    "unreachable_from_floor",         // 16
    "weight_out_of_range",            // 3
    "yaml",                           // 1
];

/// The curriculum tree of the repository (C5).
fn curriculum_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum")
}

/// One fixture tree under `crates/core/tests/fixtures/lint/`.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/lint")
        .join(name)
}

/// The findings as canonical JSON: sorted keys, two-space indent, one trailing
/// newline. This is the format of `json.dump(..., sort_keys=True, indent=2)`
/// followed by `print()`, which is what the dumper writes.
fn canonical(findings: &[Finding]) -> String {
    let rows: Vec<BTreeMap<String, serde_json::Value>> = findings
        .iter()
        .map(|finding| {
            let value = serde_json::to_value(finding).expect("a finding serializes");
            let serde_json::Value::Object(map) = value else {
                panic!("a finding serializes to an object");
            };
            let mut row: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            for (key, value) in map {
                let value = if key == "message" && finding.code == "yaml" {
                    let head = finding
                        .message
                        .split_once(": ")
                        .map_or(finding.message.clone(), |(head, _)| head.to_owned());
                    serde_json::Value::String(format!("{head}: {YAML_TAIL}"))
                } else {
                    value
                };
                row.insert(key, value);
            }
            row
        })
        .collect();
    let mut text = serde_json::to_string_pretty(&rows).expect("the rows serialize");
    text.push('\n');
    text
}

/// The committed 1.0 output for one fixture.
fn expected(name: &str) -> String {
    let path = fixture(name).join("expected.json");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// The codes of a finding list, in order.
fn codes(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.code.as_str()).collect()
}

// --------------------------------------------------------------------------- //
// The oracle comparison
// --------------------------------------------------------------------------- //

#[test]
fn every_fixture_matches_the_committed_1_0_output_byte_for_byte() {
    for name in FIXTURES {
        let found = canonical(&lint_curriculum(&fixture(name)));
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
        codes(&lint_curriculum(&fixture("clean"))),
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
    let findings = lint_curriculum(&fixture("empty"));
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
    let missing = lint_curriculum(&fixture("missing_course_dir"));
    assert_eq!(codes(&missing), vec!["missing_course_dir"]);
    assert_eq!(
        missing[0].message,
        "no unit directory ghostcourse/ for course"
    );
    assert!(!missing[0].fatal);

    let empty = lint_curriculum(&fixture("empty_course"));
    assert_eq!(codes(&empty), vec!["empty_course"]);
    assert_eq!(empty[0].message, "course hollowcourse has no unit files");
    assert!(!empty[0].fatal);
}

#[test]
fn the_parse_stage_schema_codes_carry_the_dotted_location_and_the_file() {
    let schema = lint_curriculum(&fixture("schema"));
    assert_eq!(codes(&schema), vec!["schema"]);
    assert_eq!(
        schema[0].message,
        "topics.0.answer_kind: Input should be 'numeric', 'expression', 'multi-step' or 'proof'"
    );
    assert_eq!(schema[0].file.as_deref(), Some("c/00.yaml"));
    assert!(schema[0].fatal);

    let weight = lint_curriculum(&fixture("weight_out_of_range"));
    assert_eq!(codes(&weight), vec!["weight_out_of_range"]);
    assert_eq!(
        weight[0].message,
        "topics.1.prerequisites.0.weight: Input should be less than or equal to 1"
    );
    assert_eq!(weight[0].file.as_deref(), Some("c/00.yaml"));

    let yaml = lint_curriculum(&fixture("yaml"));
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
    let findings = lint_curriculum(&fixture("cycle"));
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
    let findings = lint_curriculum(&fixture("missing_ref"));
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
    let findings = lint_curriculum(&fixture("key_prereq_not_ancestor"));
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
    let findings = lint_curriculum(&fixture("noncore_ancestor_of_core"));
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
    let spans = lint_curriculum(&fixture("module_inconsistent"));
    assert_eq!(codes(&spans), vec!["module_inconsistent"]);
    assert_eq!(
        spans[0].message,
        "module 'Shared' spans multiple courses: ['c1', 'c2']"
    );
    assert_eq!(spans[0].topic, None);

    let blank = lint_curriculum(&fixture("module_inconsistent_empty_name"));
    assert_eq!(codes(&blank), vec!["module_inconsistent"]);
    assert_eq!(blank[0].message, "topic 'a' has an empty module name");
    assert_eq!(blank[0].topic.as_deref(), Some("a"));
}

#[test]
fn both_mastery_floor_forms_on_one_course_are_ambiguous() {
    let findings = lint_curriculum(&fixture("mastery_floor_ambiguous"));
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
    let findings = lint_curriculum(&fixture("unreachable_from_floor"));
    assert_eq!(codes(&findings), vec!["unreachable_from_floor"]);
    assert_eq!(
        findings[0].message,
        "topic 'b' is not reachable from course c2's floor/roots"
    );
    assert_eq!(findings[0].topic.as_deref(), Some("b"));
}

#[test]
fn the_cardinality_rules_name_the_topic_and_the_knowledge_point() {
    let no_kp = lint_curriculum(&fixture("no_kp"));
    assert_eq!(codes(&no_kp), vec!["no_kp"]);
    assert_eq!(no_kp[0].message, "topic 'a' has no knowledge_points");

    let no_exemplar = lint_curriculum(&fixture("no_exemplar"));
    assert_eq!(codes(&no_exemplar), vec!["no_exemplar"]);
    assert_eq!(no_exemplar[0].message, "KP a.kp1 has no exemplars");

    let no_diag = lint_curriculum(&fixture("missing_diagnostic_exemplar"));
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
    let findings = lint_curriculum(&fixture("duplicate_topic_id"));
    assert_eq!(codes(&findings), vec!["duplicate_topic_id"]);
    assert_eq!(findings[0].message, "topic id 'a' defined more than once");
    assert_eq!(findings[0].topic.as_deref(), Some("a"));
    assert!(findings[0].fatal);
}

#[test]
fn a_cycle_skips_the_reachability_rule() {
    // Spec section 5, rule 16: neither topic of a cycle is a root, so a naive
    // pass would call them all unreachable. The fixture declares no floor.
    let findings = lint_curriculum(&fixture("cycle"));
    assert_eq!(codes(&findings), vec!["cycle"]);
}

#[test]
fn a_missing_key_prerequisite_skips_the_ancestor_rule() {
    // Spec section 5, rule 12: already reported as `missing_ref`.
    let findings = lint_curriculum(&fixture("missing_ref"));
    assert!(
        !codes(&findings).contains(&"key_prereq_not_ancestor"),
        "codes were {:?}",
        codes(&findings)
    );
}

// --------------------------------------------------------------------------- //
// Order: the `sorted()` sites and the sequence of the rule blocks
// --------------------------------------------------------------------------- //

/// The messages of a finding list, in order.
fn messages(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|f| f.message.as_str()).collect()
}

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
    let findings = lint_curriculum(&fixture("order_not_id_order"));
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
fn nine_codes_in_one_tree_come_in_the_1_0_rule_block_order() {
    // Fixture `many_codes` trips nine rules at once, so the list below pins the
    // sequence of the rule blocks: referenced ids, then the per-topic
    // cardinality and key-prerequisite rules, then the core-ancestor invariant,
    // then the module names (the empty-name form first, in load order, then the
    // "spans multiple courses" form), then the mastery-floor form, then
    // reachability. Every value is the committed 1.0 output.
    let findings = lint_curriculum(&fixture("many_codes"));
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
    let findings = lint_curriculum(&fixture("cycle_duplicate_missing_ref"));
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
        let findings = lint_curriculum(&fixture(name));
        let mut distinct = codes(&findings);
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), 9, "fixture {name} holds {distinct:?}");
    }
}

// --------------------------------------------------------------------------- //
// The runner
// --------------------------------------------------------------------------- //

/// Run the `lint_curriculum` binary over one path.
fn run_runner(path: &Path) -> (i32, String, String) {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lint_curriculum"))
        .arg(path)
        .output()
        .expect("the runner starts");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn the_runner_prints_ok_and_exits_zero_on_a_clean_tree() {
    // The literal text of 1.0 `scripts/lint_curriculum.py` (spec section 5).
    let path = fixture("clean");
    let (code, stdout, stderr) = run_runner(&path);
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        format!(
            "OK: {} is a valid curriculum (0 findings).\n",
            path.display()
        )
    );
    assert_eq!(stderr, "");
}

#[test]
fn the_runner_prints_every_finding_and_exits_one() {
    // Verified against 1.0 `python scripts/lint_curriculum.py <fixture>` on the
    // same tree: the two runs print the same three lines.
    let path = fixture("missing_ref");
    let (code, stdout, stderr) = run_runner(&path);
    assert_eq!(code, 1);
    assert_eq!(stdout, "");
    assert_eq!(
        stderr,
        format!(
            "FAIL: 3 curriculum finding(s) in {}:\n\
             \x20 [missing_ref] (b) prerequisite 'ghost' of 'b' does not exist\n\
             \x20 [missing_ref] (b) encompassings_extra 'phantom' of 'b' does not exist\n\
             \x20 [missing_ref] (b) key_prerequisite 'nowhere' in b.kp1 does not exist\n",
            path.display()
        )
    );
}

#[test]
fn the_runner_omits_the_topic_when_a_finding_names_none() {
    // A `module_inconsistent` "spans multiple courses" finding carries no topic,
    // so 1.0 prints `  [code] message` with no parenthesis.
    let path = fixture("module_inconsistent");
    let (code, _, stderr) = run_runner(&path);
    assert_eq!(code, 1);
    assert_eq!(
        stderr,
        format!(
            "FAIL: 1 curriculum finding(s) in {}:\n\
             \x20 [module_inconsistent] module 'Shared' spans multiple courses: ['c1', 'c2']\n",
            path.display()
        )
    );
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
