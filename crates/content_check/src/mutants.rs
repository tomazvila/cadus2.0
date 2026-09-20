//! `mutants`: near-miss variants of one key. Each variant must grade WRONG.
//!
//! The mutants come from the contract JSON, before the typed parse, because
//! the rule of a contract kind is a text rule (`content-check-cli.md`,
//! `rust-api.md` 2.4). The verdicts come from `check_contract`.

use cadus_core::answer::check_contract;
use serde_json::{Value, json};

use crate::cli::{self, Request};
use crate::grade::{fields, parse_contract};
use crate::mutate::mutate_plus_one;
use crate::output::Reply;

/// One wrong variant of a key and the name of the rule that made it.
#[derive(Debug, PartialEq, Eq)]
struct Mutant {
    rule: String,
    learner: String,
}

impl Mutant {
    fn new(rule: &str, learner: String) -> Self {
        Self {
            rule: rule.to_owned(),
            learner,
        }
    }
}

/// Run `mutants` with the arguments that follow the subcommand.
pub fn run(args: &[String]) -> Reply {
    cli::run_each(args, &["contract", "expected", "batch"], mutants_doc)
}

fn mutants_doc(request: &Request) -> Result<Value, String> {
    let key = request.expected.as_str();
    let plan = plan(&request.contract, key);
    let contract = parse_contract(&request.contract)?;
    let (key_verdict, key_reason, _) = fields(check_contract(key, key, contract.clone()));
    let mut all_wrong = true;
    let mutants: Vec<Value> = plan
        .iter()
        .map(|mutant| {
            let outcome = check_contract(key, &mutant.learner, contract.clone());
            let (verdict, _, _) = fields(outcome);
            all_wrong &= verdict == "wrong";
            json!({"rule": mutant.rule, "learner": mutant.learner, "verdict": verdict})
        })
        .collect();
    let pass = key_verdict == "correct" && !mutants.is_empty() && all_wrong;
    Ok(
        json!({"schema": "cadus.mutants.v1", "key_verdict": key_verdict,
        "key_reason": key_reason, "mutants": mutants, "pass": pass}),
    )
}

/// The mutants of one key, by the `kind` of the contract JSON.
///
/// A kind with no rule of its own uses `plus-one`. The kind `none` has no
/// deterministic grade, thus it has no mutant.
fn plan(contract: &Value, key: &str) -> Vec<Mutant> {
    match contract["kind"].as_str() {
        Some("none") => Vec::new(),
        Some("function") => function_mutant(contract, key),
        Some("label") => label_others(contract, key),
        Some("set" | "list") => member_removed(key),
        Some("multipart") => part_mutants(contract, key),
        _ => plus_one(key),
    }
}

/// `plus-one`: the rule of `check_keys::mutate::mutate_plus_one`.
fn plus_one(key: &str) -> Vec<Mutant> {
    mutate_plus_one(key)
        .map(|learner| Mutant::new("plus-one", learner))
        .into_iter()
        .collect()
}

/// `function-2x` for a contract JSON with `"kind": "function"`.
fn function_mutant(contract: &Value, key: &str) -> Vec<Mutant> {
    let up_to_constant = contract["up_to_constant"] == true;
    contract["vars"][0]
        .as_str()
        .map(|var| Mutant::new("function-2x", function_2x(key, var, up_to_constant)))
        .into_iter()
        .collect()
}

/// The text rule of `rust-api.md` 2.4: `2*(E) + v`.
///
/// `E` is the key with a leading `name =` label removed and, with
/// `up_to_constant`, one trailing `+ C`, `+ c`, `+ K` or `+ k` term removed.
fn function_2x(key: &str, var: &str, up_to_constant: bool) -> String {
    let body = key.split_once('=').map_or(key, |(_, body)| body).trim();
    let body = if up_to_constant {
        without_constant(body)
    } else {
        body
    };
    format!("2*({body}) + {var}")
}

fn without_constant(body: &str) -> &str {
    body.strip_suffix(['C', 'c', 'K', 'k'])
        .and_then(|head| head.trim_end().strip_suffix('+'))
        .map_or(body, str::trim_end)
}

/// The text that the core compares for a label: one space between words, lower case.
fn choice_key(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// `label-other`: the first alias of each option that is not the key.
fn label_others(contract: &Value, key: &str) -> Vec<Mutant> {
    let key = choice_key(key);
    let is_key = |alias: &Value| alias.as_str().is_some_and(|alias| choice_key(alias) == key);
    contract["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .filter(|aliases| !aliases.iter().any(is_key))
        .filter_map(|aliases| aliases.first().and_then(Value::as_str))
        .map(|alias| Mutant::new("label-other", alias.to_owned()))
        .collect()
}

/// `member-removed`: the collection without its first member.
///
/// A collection of one member uses `plus-one`.
fn member_removed(key: &str) -> Vec<Mutant> {
    let (open, inner, close) = unwrap_collection(key.trim());
    let members = split_members(inner);
    if members.len() < 2 {
        return plus_one(key);
    }
    let learner = format!("{open}{}{close}", members[1..].join(", "));
    vec![Mutant::new("member-removed", learner)]
}

/// Split `{..}` or `[..]` into the open mark, the inner text and the close mark.
///
/// Text such as `[1, 2], [3, 4]` has no outer pair, thus it stays whole.
fn unwrap_collection(text: &str) -> (&str, &str, &str) {
    [("{", "}"), ("[", "]")]
        .into_iter()
        .find_map(|(open, close)| {
            let inner = text.strip_prefix(open)?.strip_suffix(close)?;
            balanced(inner).then_some((open, inner, close))
        })
        .unwrap_or(("", text, ""))
}

/// True if no close mark comes before its open mark.
fn balanced(text: &str) -> bool {
    let mut depth = 0_i32;
    text.chars().all(|ch| {
        depth += match ch {
            '(' | '[' | '{' => 1,
            ')' | ']' | '}' => -1,
            _ => 0,
        };
        depth >= 0
    })
}

/// Split at each comma that is outside all brackets.
fn split_members(inner: &str) -> Vec<&str> {
    let mut members = Vec::new();
    let mut depth = 0_i32;
    let mut start = 0;
    for (at, ch) in inner.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                members.push(inner[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
    }
    members.push(inner[start..].trim());
    members
}

/// One mutant for each part of a `multipart` key (`name = value; name = value`).
///
/// The part takes the first mutant of the rule of its kind. The other parts
/// stay equal to the key. A part with no mutant gives no entry.
fn part_mutants(contract: &Value, key: &str) -> Vec<Mutant> {
    let fields: Vec<(&str, &str)> = key
        .split(';')
        .filter_map(|field| field.split_once('='))
        .map(|(name, value)| (name.trim(), value.trim()))
        .collect();
    contract["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| {
            let name = part["name"].as_str()?;
            let (_, value) = fields.iter().find(|(field, _)| *field == name)?;
            let first = plan(&part["contract"], value).into_iter().next()?;
            let learner = fields
                .iter()
                .map(|(field, value)| {
                    let value = if *field == name {
                        &first.learner
                    } else {
                        *value
                    };
                    format!("{field} = {value}")
                })
                .collect::<Vec<_>>()
                .join("; ");
            Some(Mutant::new(&format!("part:{name}:{}", first.rule), learner))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn learners(mutants: &[Mutant]) -> Vec<&str> {
        mutants.iter().map(|m| m.learner.as_str()).collect()
    }

    fn rules(mutants: &[Mutant]) -> Vec<&str> {
        mutants.iter().map(|m| m.rule.as_str()).collect()
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
        assert_eq!(plan(&json!({"kind": "exact"}), "x"), []);
        assert_eq!(plan(&json!("no object"), "7").len(), 1);
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
        for key in ["pi", "-x", "a; b", too_long, at_limit] {
            assert_eq!(plan(&exact, key), [], "{key}");
        }
    }

    #[test]
    fn none_kind_has_no_mutant() {
        assert_eq!(
            plan(&json!({"kind": "none"}), "See the solution, step 3."),
            []
        );
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
        assert_eq!(plan(&json!({"kind": "function"}), "t^2"), []);
    }

    #[test]
    fn label_other_gives_the_first_alias_of_each_other_option() {
        let contract = json!({"kind": "label",
            "options": [["Yes", "y"], ["No", "n"], ["Not  Defined", "undefined"], [], "bad"]});
        let mutants = plan(&contract, " not defined ");
        assert_eq!(rules(&mutants), ["label-other", "label-other"]);
        assert_eq!(learners(&mutants), ["Yes", "No"]);
        assert_eq!(learners(&plan(&contract, "N")), ["Yes", "Not  Defined"]);
        assert_eq!(plan(&json!({"kind": "label"}), "Yes"), []);
        assert_eq!(
            plan(&json!({"kind": "label", "options": [[1], ["a"]]}), "a"),
            []
        );
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
        assert_eq!(plan(&json!({"kind": "multipart"}), "a = 1"), []);
    }
}
