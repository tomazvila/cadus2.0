//! The tests of I12, I13, I14 and D28.

use serde_json::{Value, json};

use super::super::testkit::{invariants, item, view};
use super::*;

const SKETCH: &str =
    "Use the power rule on each term of the sum. Add the two results: the value is 12.";

fn new_item(answer: &str, contract: Value, sketch: Option<&str>) -> Item {
    let mut one = item("Find the value.", answer, contract, sketch);
    one.is_new = true;
    one
}

fn found_for(one: Item) -> Vec<Finding> {
    check(&view(vec![one]))
}

fn details(found: &[Finding]) -> Vec<&str> {
    found
        .iter()
        .map(|finding| finding.detail.as_str())
        .collect()
}

#[test]
fn the_rules_for_new_items_do_not_run_for_an_old_item() {
    let mut old = new_item("$12$", Value::Null, None);
    old.is_new = false;
    assert_eq!(found_for(old), []);
    assert_eq!(
        found_for(new_item("12", json!({"kind": "exact"}), Some(SKETCH))),
        []
    );
}

#[test]
fn i12_limits_the_answer_by_contract_kind() {
    let exact = json!({"kind": "exact"});
    let found = found_for(new_item("$12$", exact.clone(), Some(SKETCH)));
    assert_eq!(details(&found), ["I12: the answer has `$` or a backslash"]);
    assert_eq!(
        (found[0].code.as_str(), found[0].ck.as_str()),
        ("answer-format", "CK5")
    );
    let long = "12".repeat(21);
    let sketch = format!("{SKETCH} In full: {long}.");
    let found = found_for(new_item(&long, exact, Some(&sketch)));
    assert_eq!(
        details(&found),
        ["I12: the answer has 42 characters (limit 40)"]
    );
}

#[test]
fn i12_permits_a_label_key_of_eighty_characters_with_a_dollar() {
    let key = format!("${}$", "x".repeat(60));
    let contract = json!({"kind": "label", "options": [[key], ["b"], ["c"], ["d"]]});
    assert_eq!(
        found_for(new_item(&key, contract.clone(), Some(SKETCH))),
        []
    );
    let long = "x".repeat(81);
    let found = found_for(new_item(&long, contract, Some(SKETCH)));
    assert_eq!(
        details(&found),
        ["I12: the label key has 81 characters (limit 80)"]
    );
}

#[test]
fn i12_limits_a_multipart_key_and_each_part_value() {
    let parts = json!([{"name": "a", "contract": {"kind": "exact"}},
        {"name": "b", "contract": {"kind": "exact"}}]);
    let contract = json!({"kind": "multipart", "parts": parts});
    let sketch = format!("{SKETCH} Ones: {}.", "1".repeat(41));
    let good = format!("a = 12; b = {}", "1".repeat(40));
    assert_eq!(
        found_for(new_item(&good, contract.clone(), Some(&sketch))),
        []
    );
    let part = format!("a = 12; b = {}", "1".repeat(41));
    let found = found_for(new_item(&part, contract.clone(), Some(&sketch)));
    assert_eq!(
        details(&found),
        ["I12: the part `b` has 41 characters (limit 40)"]
    );
    let full = format!("a = {}; b = 12", "12".repeat(36));
    let sketch = format!("{SKETCH} In full: {}.", "12".repeat(36));
    let found = found_for(new_item(&full, contract, Some(&sketch)));
    assert_eq!(
        details(&found),
        ["I12: the answer has 84 characters (limit 80)"]
    );
}

#[test]
fn i13_needs_twelve_words_and_two_steps() {
    let exact = json!({"kind": "exact"});
    let found = found_for(new_item("12", exact.clone(), Some("Use the formula.")));
    assert_eq!(
        details(&found),
        ["I13: the sketch has 3 words (minimum 12)"]
    );
    assert_eq!(
        (found[0].code.as_str(), found[0].ck.as_str()),
        ("sketch", "CK9")
    );
    let found = found_for(new_item("12", exact.clone(), None));
    assert_eq!(
        details(&found),
        ["I13: the sketch has 0 words (minimum 12)"]
    );
    let one = "Use the power rule on each term of the sum and then add to get 12";
    let found = found_for(new_item("12", exact.clone(), Some(one)));
    assert_eq!(
        details(&found),
        ["I13: the sketch has fewer than 2 sentences or `;` steps"]
    );
    let steps = "use the power rule on each term of the sum; add the results to get 12";
    assert_eq!(found_for(new_item("12", exact, Some(steps))), []);
}

#[test]
fn sentence_count_reads_a_mark_before_a_space_or_the_end() {
    assert_eq!(sentence_count(""), 0);
    assert_eq!(sentence_count("The value is 3.5 here"), 1);
    assert_eq!(sentence_count("One. Two! Three?"), 3);
    assert_eq!(sentence_count("One. Two"), 2);
}

#[test]
fn i13_needs_the_digits_of_the_key_in_the_sketch() {
    let found = found_for(new_item("7/13", json!({"kind": "exact"}), Some(SKETCH)));
    assert_eq!(
        details(&found),
        ["I13: the sketch does not show the digits `7` of the key"]
    );
    // A label key and the P3 item have no digit rule.
    let contract = json!({"kind": "label", "options": [["77"], ["78"], ["79"], ["80"]]});
    assert_eq!(found_for(new_item("77", contract, Some(SKETCH))), []);
    assert_eq!(
        found_for(new_item("See 99.", json!({"kind": "none"}), Some(SKETCH))),
        []
    );
}

#[test]
fn i14_needs_an_explicit_contract_on_a_new_verdict_item() {
    let found = found_for(new_item("12", Value::Null, Some(SKETCH)));
    assert_eq!(invariants(&found), ["I14"]);
    assert_eq!(
        (found[0].code.as_str(), found[0].ck.as_str()),
        ("invariant:I14", "CK12")
    );
    // No contract and no verdict: I4 reports the item, I14 does not.
    assert_eq!(found_for(new_item("twelve", Value::Null, Some(SKETCH))), []);
}

#[test]
fn d28_refuses_e_notation_in_a_function_key() {
    let function = json!({"kind": "function", "vars": ["x"]});
    // `log` is base 10 in the grader, so a key may use it.
    for key in ["log(x)", "2log x", "x + log10(x)"] {
        assert_eq!(d28(&function, key), None, "{key}");
    }
    for key in ["1e-5 x", "2.5E3", "3e+2", "2e6x"] {
        let found = d28(&function, key);
        assert!(
            found.is_some_and(|text| text.contains("`e` notation")),
            "{key}"
        );
    }
    for key in [
        "ln(x)/ln(10)",
        "2e^(-x)",
        "e^(2x) - 1",
        "logistic",
        "x e",
        "3e",
    ] {
        assert_eq!(d28(&function, key), None, "{key}");
    }
    // The rule is for the `function` kind only.
    assert_eq!(d28(&json!({"kind": "exact"}), "log(2)"), None);
}

#[test]
fn d28_reads_each_function_part_and_each_item_of_the_tree() {
    let parts = json!([{"name": "fx", "contract": {"kind": "function", "vars": ["x"]}},
        {"name": "n", "contract": {"kind": "exact"}}]);
    let contract = json!({"kind": "multipart", "parts": parts});
    assert_eq!(d28(&contract, "fx = 2x; n = 1e-5"), None);
    let text = d28(&contract, "fx = 1e-5 x; n = 2");
    assert!(text.is_some_and(|text| text.starts_with("D28: the function part `fx`")));
    // An old item of the tree gets the finding too.
    let old = item(
        "Find f.",
        "1e-5 x",
        json!({"kind": "function", "vars": ["x"]}),
        None,
    );
    let found = found_for(old);
    assert_eq!(invariants(&found), ["D28"]);
    assert_eq!(
        (found[0].code.as_str(), found[0].ck.as_str()),
        ("answer-format", "CK5")
    );
}
