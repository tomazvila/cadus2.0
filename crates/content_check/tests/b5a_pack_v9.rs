//! Lane B5a: the amendments of freeze pack v9 to `mutants` (D38, D39, D40),
//! with the cases of the review of the lane.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::json;

#[path = "b5a_support.rs"]
mod support;
use support::{mutant_learners, mutants};

/// Review case m12: a label part with ONE option has no mutant.
#[test]
fn multipart_part_with_no_mutant_fails_the_item_and_is_named() {
    let contract = r#"{"kind":"multipart","parts":[{"name":"a","contract":{"kind":"exact"}},{"name":"v","contract":{"kind":"label","options":[["converges","conv"]]}}]}"#;
    let doc = mutants(contract, "a = 3; v = converges").doc();
    assert_eq!(doc["key_verdict"], "correct");
    assert_eq!(mutant_learners(&doc), ["a = 4; v = converges"]);
    assert_eq!(doc["no_mutant_parts"], json!(["v"]));
    assert_eq!(doc["pass"], false);
    // A part with no numeric leaf and no whole-key rule.
    let contract = r#"{"kind":"multipart","parts":[{"name":"a","contract":{"kind":"exact"}},{"name":"p","contract":{"kind":"coordinates","arity":2}}]}"#;
    let doc = mutants(contract, "a = 3; p = (x, y)").doc();
    assert_eq!(doc["no_mutant_parts"], json!(["p"]));
    assert_eq!(doc["pass"], false);
    // No part has a mutant: the list is empty and has a cause.
    let contract = r#"{"kind":"multipart","parts":[{"name":"p","contract":{"kind":"coordinates","arity":2}}]}"#;
    let doc = mutants(contract, "p = (x, y)").doc();
    assert_eq!(doc["mutants"], json!([]));
    assert_eq!(doc["cause"], "no-mutant-part");
    assert_eq!(doc["pass"], false);
}

/// Review case m10: each part has a mutant; the part `b = x` takes the whole-key rule.
#[test]
fn multipart_part_with_no_numeric_leaf_gets_its_own_mutant() {
    let contract = r#"{"kind":"multipart","parts":[{"name":"a","contract":{"kind":"exact"}},{"name":"b","contract":{"kind":"exact"}}]}"#;
    let doc = mutants(contract, "a = 3; b = x").doc();
    assert_eq!(
        mutant_learners(&doc),
        ["a = 4; b = x", "a = 3; b = (x) + 1"]
    );
    assert_eq!(doc["mutants"][1]["rule"], "part:b:plus-one");
    assert_eq!(doc["mutants"][1]["mutant_kind"], "plus-one-whole");
    assert_eq!(doc.get("no_mutant_parts"), None);
    assert_eq!(doc["pass"], true);
}

#[test]
fn member_removed_tries_the_next_member_when_the_mutant_grades_correct() {
    for (key, want) in [
        ("{2, 2, 3}", "{2, 2}"),
        ("{1/2, 0.5, 3}", "{1/2, 0.5}"),
        ("{(1,2), (1, 2), 5}", "{(1,2), (1, 2)}"),
    ] {
        let doc = mutants(r#"{"kind":"set"}"#, key).doc();
        assert_eq!(mutant_learners(&doc), [want], "{key}");
        assert_eq!(doc.get("cause"), None);
        assert_eq!(doc["pass"], true, "{key}");
    }
    // Each candidate is equal to the key: no distinct mutant.
    let doc = mutants(r#"{"kind":"set"}"#, "{2, 2}").doc();
    assert_eq!(doc["mutants"][0]["verdict"], "correct");
    assert_eq!(doc["cause"], "no-distinct-mutant");
    assert_eq!(doc["pass"], false);
}

#[test]
fn set_part_of_a_multipart_key_tries_the_next_member() {
    let contract = r#"{"kind":"multipart","parts":[{"name":"a","contract":{"kind":"exact"}},{"name":"s","contract":{"kind":"set"}}]}"#;
    let doc = mutants(contract, "a = 1; s = {2, 2, 3}").doc();
    assert_eq!(
        mutant_learners(&doc),
        ["a = 2; s = {2, 2, 3}", "a = 1; s = {2, 2}"]
    );
    assert_eq!(doc["mutants"][1]["rule"], "part:s:member-removed");
    assert_eq!(doc["pass"], true);
}

#[test]
fn approx_with_a_tolerance_gets_a_mutant_outside_the_tolerance() {
    for (tolerance, key) in [
        ("0.01", "3.14"),
        ("1/100", "3.14"),
        ("0.1", "2.5"),
        ("1", "5"),
        ("1000", "5"),
    ] {
        let contract = json!({"kind": "approx", "tolerance": tolerance}).to_string();
        let doc = mutants(&contract, key).doc();
        assert_eq!(mutant_learners(&doc).len(), 1, "{doc}");
        assert_eq!(doc["pass"], true, "{doc}");
    }
    let doc = mutants(r#"{"kind":"approx","tolerance":"0.01"}"#, "3.14").doc();
    assert_eq!(mutant_learners(&doc), ["3.14 + 2*(0.01) + 0.01"]);
}

#[test]
fn exact_key_with_no_numeric_leaf_gets_the_whole_key_mutant() {
    for key in ["x", "pi", "(x+y)/(x*y)"] {
        let doc = mutants(r#"{"kind":"exact"}"#, key).doc();
        assert_eq!(mutant_learners(&doc), [format!("({key}) + 1")]);
        assert_eq!(doc["mutants"][0]["mutant_kind"], "plus-one-whole");
        assert_eq!(doc["pass"], true);
    }
    let doc = mutants(r#"{"kind":"set"}"#, "{x}").doc();
    assert_eq!(mutant_learners(&doc), ["{(x) + 1}"]);
    assert_eq!(doc["pass"], true);
    // A key with a numeric leaf has no `mutant_kind`.
    let doc = mutants(r#"{"kind":"exact"}"#, "x + 2").doc();
    assert_eq!(doc["mutants"][0].get("mutant_kind"), None);
}
