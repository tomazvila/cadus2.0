//! Unit tests of `mutants.rs`: each mutant rule and the `function-2x` text rule.

use super::*;

fn learners(plan: &Plan) -> Vec<&str> {
    plan.slots.iter().map(|slot| slot.first.as_str()).collect()
}

fn rules(plan: &Plan) -> Vec<&str> {
    plan.slots.iter().map(|slot| slot.rule.as_str()).collect()
}

fn empty(plan: &Plan) -> bool {
    plan.slots.is_empty()
}

#[test]
fn plus_one_kinds_use_the_shared_mutator() {
    for kind in [
        "exact",
        "approx",
        "coordinates",
        "matrix",
        "inequality_union",
        "unit",
    ] {
        let mutants = plan(&json!({"kind": kind}), "(46, 54)");
        assert_eq!(rules(&mutants), ["plus-one"], "{kind}");
        assert_eq!(learners(&mutants), ["(47, 54)"], "{kind}");
    }
    let no_leaf = plan(&json!({"kind": "coordinates"}), "(x, y)");
    assert!(empty(&no_leaf));
    assert_eq!(no_leaf.cause, "no-numeric-leaf");
    assert_eq!(plan(&json!("no object"), "7").slots.len(), 1);
}

/// The shared mutator reaches the first numeric leaf of each tree shape, and
/// its text path serves a key that the grammar does not read.
#[test]
fn plus_one_reaches_each_shape_of_a_key() {
    let exact = json!({"kind": "exact"});
    for (key, want) in [
        ("2.5", None),
        ("1/2", None),
        ("2 1/2", None),
        ("-sqrt(3)", None),
        ("x^2", None),
        ("(x+1)^2", None),
        ("x^(1/2) + 4", None),
        ("x/3", None),
        ("sin(2x)", None),
        ("{x, 2}", None),
        ("[1, 2)", None),
        ("[x, 2, 3]", None),
        ("1 < x < 2", None),
        ("x < 3", None),
        ("5 m", None),
        ("x = 3", None),
        ("pi + 1", None),
        ("a = 3; b = x", Some("a = 4; b = x")),
        ("a = -9; b", Some("a = -10; b")),
    ] {
        let mutants = plan(&exact, key);
        assert_eq!(rules(&mutants), ["plus-one"], "{key}");
        assert_ne!(learners(&mutants), [key]);
        if let Some(want) = want {
            assert_eq!(learners(&mutants), [want]);
        }
    }
    let too_long = "a = 999999999999999999999999999999999999999999; b";
    let at_limit = "a = 170141183460469231731687303715884105727; b";
    let pair = json!({"kind": "coordinates", "arity": 2});
    for key in ["pi", "-x", "a; b", too_long, at_limit] {
        assert!(empty(&plan(&pair, key)), "{key}");
    }
}

#[test]
fn none_kind_has_no_mutant() {
    let none = plan(&json!({"kind": "none"}), "See the solution, step 3.");
    assert!(empty(&none));
    assert_eq!(none.cause, "no-rule");
}

#[test]
fn function_2x_plain_key() {
    assert_eq!(
        function_2x("x/sqrt(x^2+9)", "x", false),
        "2*(x/sqrt(x^2+9)) + x"
    );
}

#[test]
fn function_2x_removes_the_leading_label() {
    assert_eq!(function_2x("y = 3x^2 + 1", "x", false), "2*(3x^2 + 1) + x");
}

#[test]
fn function_2x_removes_one_trailing_constant_only_with_up_to_constant() {
    assert_eq!(function_2x("y = x^2/2 + C", "x", true), "2*(x^2/2) + x");
    for constant in ["+ c", "+K", "+  k"] {
        let key = format!("x^2/2 {constant}");
        assert_eq!(function_2x(&key, "x", true), "2*(x^2/2) + x", "{key}");
    }
    assert_eq!(function_2x("x^2/2 + C", "x", false), "2*(x^2/2 + C) + x");
    assert_eq!(function_2x("x + 2c", "x", true), "2*(x + 2c) + x");
    assert_eq!(function_2x("sin(t)", "t", true), "2*(sin(t)) + t");
}

#[test]
fn function_kind_reads_the_first_variable_and_the_constant_flag() {
    let contract = json!({"kind": "function", "vars": ["t", "x"], "up_to_constant": true});
    let mutants = plan(&contract, "s = t^2 + C");
    assert_eq!(rules(&mutants), ["function-2x"]);
    assert_eq!(learners(&mutants), ["2*(t^2) + t"]);
    let no_vars = plan(&json!({"kind": "function"}), "t^2");
    assert!(empty(&no_vars));
    assert_eq!(no_vars.cause, "no-variable");
}

#[test]
fn label_other_gives_the_first_alias_of_each_other_option() {
    let contract = json!({"kind": "label",
        "options": [["Yes", "y"], ["No", "n"], ["Not  Defined", "undefined"], [], "bad"]});
    let mutants = plan(&contract, " not defined ");
    assert_eq!(rules(&mutants), ["label-other", "label-other"]);
    assert_eq!(learners(&mutants), ["Yes", "No"]);
    assert_eq!(learners(&plan(&contract, "N")), ["Yes", "Not  Defined"]);
    let no_options = plan(&json!({"kind": "label"}), "Yes");
    assert!(empty(&no_options));
    assert_eq!(no_options.cause, "no-other-option");
    assert!(empty(&plan(
        &json!({"kind": "label", "options": [[1], ["a"]]}),
        "a"
    )));
}

#[test]
fn member_removed_drops_the_first_member() {
    let set = plan(&json!({"kind": "set"}), "{2, 3}");
    assert_eq!(rules(&set), ["member-removed"]);
    assert_eq!(learners(&set), ["{3}"]);
    let list = json!({"kind": "list", "ordered": true, "member": {"kind": "exact"}});
    assert_eq!(learners(&plan(&list, "[1, 2, 3]")), ["[2, 3]"]);
    assert_eq!(learners(&plan(&list, "1, 2")), ["2"]);
    assert_eq!(learners(&plan(&list, "(1, 2), (3, 4)")), ["(3, 4)"]);
    assert_eq!(learners(&plan(&list, "[1, 2], [3, 4]")), ["[3, 4]"]);
    assert_eq!(learners(&plan(&list, "{1, 2}, {3}")), ["{3}"]);
    assert_eq!(learners(&plan(&list, "{(1, 2), (3, 4)}")), ["{(3, 4)}"]);
    assert_eq!(learners(&plan(&list, "[1, 2), 5")), ["5"]);
}

#[test]
fn member_removed_uses_plus_one_for_one_member() {
    let mutants = plan(&json!({"kind": "set"}), "{4}");
    assert_eq!(rules(&mutants), ["plus-one"]);
    assert_eq!(learners(&mutants), ["{5}"]);
}

#[test]
fn member_removed_lists_each_member_as_a_candidate() {
    let set = plan(&json!({"kind": "set"}), "{2, 2, 3}");
    assert_eq!(set.slots[0].first, "{2, 3}");
    assert_eq!(set.slots[0].rest, ["{2, 3}", "{2, 2}"]);
}

#[test]
fn exact_key_with_no_numeric_leaf_gets_the_whole_mutant() {
    for (key, want) in [
        ("x", "(x) + 1"),
        ("pi", "(pi) + 1"),
        ("(x+y)/(x*y)", "((x+y)/(x*y)) + 1"),
    ] {
        let mutants = plan(&json!({"kind": "exact"}), key);
        assert_eq!(learners(&mutants), [want]);
        assert_eq!(rules(&mutants), ["plus-one"]);
        assert_eq!(mutants.slots[0].kind, Some("plus-one-whole"));
    }
    assert_eq!(plan(&json!({"kind": "exact"}), "x + 2").slots[0].kind, None);
}

#[test]
fn one_member_with_no_numeric_leaf_gets_the_whole_mutant_inside_the_marks() {
    let set = plan(&json!({"kind": "set"}), "{x}");
    assert_eq!(learners(&set), ["{(x) + 1}"]);
    assert_eq!(set.slots[0].kind, Some("plus-one-whole"));
    assert_eq!(
        learners(&plan(&json!({"kind": "list"}), "pi")),
        ["(pi) + 1"]
    );
    assert!(empty(&plan(&json!({"kind": "set"}), "{}")));
}

#[test]
fn approx_with_a_tolerance_steps_over_the_tolerance() {
    for (tolerance, key, want) in [
        ("0.01", "3.14", "3.14 + 2*(0.01) + 0.01"),
        ("1/100", " 2.5 ", "2.5 + 2*(1/100) + 0.1"),
        ("1", "5", "5 + 2*(1) + 1"),
        ("1", "7/2", "7/2 + 2*(1) + 1"),
        ("1", "5.", "5. + 2*(1) + 1"),
        ("1", "2.x", "2.x + 2*(1) + 1"),
    ] {
        let mutants = plan(&json!({"kind": "approx", "tolerance": tolerance}), key);
        assert_eq!(learners(&mutants), [want]);
        assert_eq!(rules(&mutants), ["plus-one"]);
    }
    let decimals = plan(&json!({"kind": "approx", "decimals": 2}), "3.14");
    assert_ne!(learners(&decimals), ["3.14 + 2*(0) + 0.01"]);
    assert_eq!(decimals.slots[0].rest, Vec::<String>::new());
}

#[test]
fn multipart_gives_one_mutant_for_each_part() {
    let contract = json!({"kind": "multipart", "parts": [
        {"name": "verdict", "contract": {"kind": "label",
            "options": [["converges"], ["diverges"], ["oscillates"]]}},
        {"name": "L", "contract": {"kind": "exact"}},
        {"name": "f", "contract": {"kind": "function", "vars": ["x"]}},
        {"name": "note", "contract": {"kind": "none"}},
        {"name": "absent", "contract": {"kind": "exact"}},
        {"contract": {"kind": "exact"}}]});
    let mutants = plan(
        &contract,
        "verdict = converges; L = 7; f = x^2; note = text 1; stray",
    );
    assert_eq!(
        rules(&mutants),
        [
            "part:verdict:label-other",
            "part:L:plus-one",
            "part:f:function-2x"
        ]
    );
    assert_eq!(
        learners(&mutants),
        [
            "verdict = diverges; L = 7; f = x^2; note = text 1",
            "verdict = converges; L = 8; f = x^2; note = text 1",
            "verdict = converges; L = 7; f = 2*(x^2) + x; note = text 1"
        ]
    );
    assert_eq!(mutants.no_mutant_parts, ["note", "absent", ""]);
    let no_parts = plan(&json!({"kind": "multipart"}), "a = 1");
    assert!(empty(&no_parts));
    assert_eq!(no_parts.cause, "no-mutant-part");
}

#[test]
fn whole_mutant_keeps_a_leading_name_label() {
    let exact = json!({"kind": "exact"});
    assert_eq!(
        learners(&plan(&exact, "x = (c - b)/a")),
        ["x = ((c - b)/a) + 1"]
    );
    assert_eq!(learners(&plan(&exact, "a <= b")), ["(a <= b) + 1"]);
    assert_eq!(learners(&plan(&exact, " = b")), ["( = b) + 1"]);
}

#[test]
fn member_removed_reads_the_latex_set_and_the_word_and() {
    let set = json!({"kind": "set"});
    assert_eq!(learners(&plan(&set, "$\\{A, B, C\\}$")), ["$\\{B, C\\}$"]);
    let list = json!({"kind": "list"});
    assert_eq!(learners(&plan(&list, "$i$ and $-i$")), ["$-i$"]);
    assert_eq!(learners(&plan(&list, "a, b and c")), ["b, c"]);
    assert_eq!(learners(&plan(&list, "(a and b), c")), ["c"]);
    assert_eq!(plan(&list, "x and and y").slots[0].rest.len(), 1);
}
