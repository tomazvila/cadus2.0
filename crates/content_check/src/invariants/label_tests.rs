//! The tests of the label rules.

use serde_json::{Value, json};

use super::super::testkit::{exact, invariants, item, view};
use super::*;

const WORDS: [&str; 4] = ["ellipse", "parabola", "hyperbola", "circle"];

fn label_of(n: usize, key: &str, options: &[&str]) -> Item {
    let options: Vec<Value> = options.iter().map(|text| json!([text])).collect();
    let contract = json!({"kind": "label", "options": options});
    item(
        &format!("Name the class of curve {n}."),
        key,
        contract,
        None,
    )
}

fn multipart(n: usize, key: &str, parts: Value) -> Item {
    let contract = json!({"kind": "multipart", "parts": parts});
    item(
        &format!("Decide case {n} and give the limit."),
        key,
        contract,
        None,
    )
}

fn verdict_part(options: Value) -> Value {
    json!([{"name": "verdict", "contract": {"kind": "label", "options": options}},
        {"name": "L", "contract": {"kind": "exact"}}])
}

fn details(found: &[Finding]) -> Vec<&str> {
    found
        .iter()
        .map(|finding| finding.detail.as_str())
        .collect()
}

#[test]
fn a_clean_label_item_has_no_finding() {
    let one = view(vec![label_of(1, "circle", &WORDS)]);
    assert_eq!(check(&one), []);
    assert_eq!(one.status(0).0, "keep");
}

#[test]
fn i8_needs_four_options_at_the_top_level() {
    let two = view(vec![label_of(1, "circle", &["circle", "ellipse"])]);
    let found = check(&two);
    assert_eq!(invariants(&found), ["I8"]);
    assert_eq!(found[0].code, "label-quality");
    assert_eq!(
        two.status(0),
        (
            "must_replace",
            Some("I8: label with 2 options (minimum 4)".to_owned())
        )
    );
}

#[test]
fn i8_reads_the_label_part_of_a_multipart() {
    let key = "verdict = converges; L = 1/2";
    let good = multipart(1, key, verdict_part(json!([["converges"], ["diverges"]])));
    assert_eq!(check(&view(vec![good])), []);
    let one = multipart(1, key, verdict_part(json!([["converges"]])));
    let found = check(&view(vec![one]));
    assert_eq!(
        details(&found),
        ["I8: label part `verdict` with 1 options (minimum 2)"]
    );
    let long = multipart(
        1,
        key,
        verdict_part(json!([["converges"], ["does not converge"]])),
    );
    let found = check(&view(vec![long]));
    assert!(found[0].detail.contains("more than 1 word"), "{found:?}");
    assert_eq!(option_count_breach(&exact(1)), None);
}

#[test]
fn d27_refuses_a_multipart_of_label_parts_only() {
    let options = json!([["yes"], ["no"]]);
    let parts = json!([{"name": "a", "contract": {"kind": "label", "options": options}},
        {"name": "b", "contract": {"kind": "label", "options": options}}]);
    let found = check(&view(vec![multipart(1, "a = yes; b = no", parts)]));
    assert_eq!(invariants(&found), ["D27"]);
    assert_eq!(
        (found[0].code.as_str(), found[0].ck.as_str()),
        ("contract-rule", "CK4")
    );
}

#[test]
fn d27_permits_a_label_part_with_r14_and_r16_only() {
    let key = "verdict = converges; L = 1/2";
    let mut one = multipart(1, key, verdict_part(json!([["converges"], ["diverges"]])));
    for rule in ["R14", "R16"] {
        one.rule = Some(rule.to_owned());
        assert_eq!(check(&view(vec![one.clone()])), []);
    }
    one.rule = Some("R6".to_owned());
    let found = check(&view(vec![one]));
    assert_eq!(invariants(&found), ["D27"]);
    assert!(found[0].detail.ends_with("not R6"), "{}", found[0].detail);
}

#[test]
fn forbidden_option_texts_are_findings() {
    let top = label_of(
        1,
        "circle",
        &["circle", "ellipse", "square", "None of the above"],
    );
    let found = check(&view(vec![top]));
    assert_eq!(invariants(&found), ["I10"]);
    assert!(found[0].detail.contains("None of the above"), "{found:?}");
    let part = multipart(
        1,
        "verdict = yes; L = 1",
        verdict_part(json!([["yes"], ["no"], ["all of the above"]])),
    );
    let found = check(&view(vec![part]));
    assert_eq!(invariants(&found), ["I8", "I10"]);
}

#[test]
fn options_have_one_type_and_a_length_ratio_of_three_or_less() {
    let mixed = label_of(1, "12", &["12", "-3/4", "2.5", "many"]);
    let found = check(&view(vec![mixed]));
    assert_eq!(
        details(&found),
        ["L4: the options are not all numeric or all text"]
    );
    let numeric = label_of(1, "12", &["12", "-3/4", "2.5", "40"]);
    assert_eq!(check(&view(vec![numeric])), []);
    let ratio = label_of(1, "ab", &["ab", "abcdefg", "abc", "abcd"]);
    let found = check(&view(vec![ratio]));
    assert_eq!(
        details(&found),
        ["L4: option lengths 2 to 7: the ratio is more than 3"]
    );
    // The R16 option texts have no type rule and no length rule.
    let r16 = label_of(1, "DNE", &["DNE", "infinity", "-infinity", "0", "12"]);
    assert_eq!(check(&view(vec![r16])), []);
    // With R16 option texts only, no length is there to compare.
    let only_r16 = label_of(1, "DNE", &["DNE", "infinity", "-infinity", "Infinity "]);
    assert_eq!(check(&view(vec![only_r16])), []);
}

#[test]
fn the_key_is_an_alias_of_exactly_one_option() {
    let absent = label_of(1, "square", &WORDS);
    assert_eq!(invariants(&check(&view(vec![absent]))), ["X1"]);
    let twice = label_of(1, "circle", &["circle", "Circle", "ellipse", "parabola"]);
    assert_eq!(correct_option(&twice), None);
    let spaced = label_of(1, "  CIRCLE ", &WORDS);
    assert_eq!(correct_option(&spaced), Some(3));
}

#[test]
fn the_problem_text_does_not_show_an_option() {
    let mut shown = label_of(1, "circle", &WORDS);
    shown.exemplar.problem = "Is the curve an Ellipse or not?".to_owned();
    let found = check(&view(vec![shown]));
    assert_eq!(
        details(&found),
        ["X12: the problem text shows the option `ellipse`"]
    );
    // Short options and `Step <n>` options are exempt.
    let mut steps = label_of(1, "Step 3", &["Step 1", "Step 2", "Step 3", "Step 4"]);
    steps.exemplar.problem =
        "Step 1: a. Step 2: b. Step 3: c. Step 4: d. Which is wrong?".to_owned();
    assert_eq!(check(&view(vec![steps])), []);
    let mut short = label_of(1, "max", &["max", "min", "sad", "non"]);
    short.exemplar.problem = "Is it a max or a min?".to_owned();
    assert_eq!(check(&view(vec![short])), []);
    let mut not_a_step = label_of(1, "Step x", &["Step x", "Step y", "Step z", "Step w"]);
    not_a_step.exemplar.problem = "Is Step y wrong?".to_owned();
    assert_eq!(invariants(&check(&view(vec![not_a_step]))), ["X12"]);
}

#[test]
fn i9_counts_a_label_as_one_and_a_label_part_as_a_half() {
    let labels =
        |count: usize| -> Vec<Item> { (0..count).map(|n| label_of(n, WORDS[n], &WORDS)).collect() };
    assert_eq!(check(&view(labels(2))), []);
    let found = check(&view(labels(3)));
    assert_eq!(
        details(&found),
        ["I9: label count 3 (limit 2; a label part of a multipart counts 0.5)"]
    );
    let mut proof = view(labels(3));
    proof.proof_kp = true;
    assert_eq!(check(&proof), []);
    proof.items = labels(4);
    assert_eq!(invariants(&check(&proof)), ["I9"]);
    let mut half = labels(2);
    let key = "verdict = converges; L = 1/2";
    half.push(multipart(
        9,
        key,
        verdict_part(json!([["converges"], ["diverges"]])),
    ));
    let found = check(&view(half));
    assert!(
        found[0].detail.starts_with("I9: label count 2.5"),
        "{found:?}"
    );
}

#[test]
fn i11_refuses_one_correct_option_two_times() {
    let items = vec![label_of(1, "circle", &WORDS), label_of(2, "Circle", &WORDS)];
    let found = check(&view(items));
    assert_eq!(invariants(&found), ["I11"]);
    assert_eq!(
        found[0].detail,
        "I11: the correct option `Circle` occurs twice"
    );
}
