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

/// One mutant place of a key: the rule and its candidates in the order of use.
///
/// The first candidate that does not grade `correct` is the mutant (D39).
#[derive(Debug, PartialEq, Eq)]
struct Slot {
    rule: String,
    first: String,
    rest: Vec<String>,
    /// The `mutant_kind` value of the document, if the rule has one (D40).
    kind: Option<&'static str>,
}

impl Slot {
    fn new(rule: &str, first: String) -> Self {
        Self {
            rule: rule.to_owned(),
            first,
            rest: Vec::new(),
            kind: None,
        }
    }

    fn candidates(&self) -> impl Iterator<Item = &String> {
        std::iter::once(&self.first).chain(&self.rest)
    }
}

/// The mutant places of one key.
#[derive(Debug, PartialEq, Eq)]
struct Plan {
    slots: Vec<Slot>,
    /// The `cause` value of the document if `slots` is empty (D40).
    cause: &'static str,
    /// The parts of a `multipart` key for which no mutant exists (D38).
    no_mutant_parts: Vec<String>,
}

impl Plan {
    fn of(slots: Vec<Slot>, cause: &'static str) -> Self {
        Self {
            slots,
            cause,
            no_mutant_parts: Vec::new(),
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
    let verdict_of = |learner: &str| fields(check_contract(key, learner, contract.clone())).0;
    let (key_verdict, key_reason, _) = fields(check_contract(key, key, contract.clone()));
    let mut all_wrong = true;
    let mut exhausted = false;
    let mutants: Vec<Value> = plan
        .slots
        .iter()
        .map(|slot| {
            let (learner, verdict) = slot
                .candidates()
                .map(|learner| (learner, verdict_of(learner)))
                .find(|(_, verdict)| *verdict != "correct")
                .unwrap_or_else(|| {
                    exhausted = true;
                    (&slot.first, "correct")
                });
            all_wrong &= verdict == "wrong";
            let mut entry = json!({"rule": slot.rule, "learner": learner, "verdict": verdict});
            if let Some(kind) = slot.kind {
                entry["mutant_kind"] = json!(kind);
            }
            entry
        })
        .collect();
    let pass = key_verdict == "correct"
        && !mutants.is_empty()
        && all_wrong
        && plan.no_mutant_parts.is_empty();
    let cause = if mutants.is_empty() {
        Some(plan.cause)
    } else {
        exhausted.then_some("no-distinct-mutant")
    };
    let mut doc = json!({"schema": "cadus.mutants.v1", "key_verdict": key_verdict,
        "key_reason": key_reason, "mutants": mutants, "pass": pass});
    if let Some(cause) = cause {
        doc["cause"] = json!(cause);
    }
    if !plan.no_mutant_parts.is_empty() {
        doc["no_mutant_parts"] = json!(plan.no_mutant_parts);
    }
    Ok(doc)
}

/// The mutant places of one key, by the `kind` of the contract JSON.
///
/// A kind with no rule of its own uses `plus-one`. The kind `none` has no
/// deterministic grade, thus it has no mutant.
fn plan(contract: &Value, key: &str) -> Plan {
    match contract["kind"].as_str() {
        Some("none") => Plan::of(Vec::new(), "no-rule"),
        Some("function" | "function_form") => function_mutant(contract, key),
        Some("label") => label_others(contract, key),
        Some("set" | "list") => member_removed(key),
        Some("multipart") => part_mutants(contract, key),
        Some("exact") => plus_one(key, true),
        Some("ordered_word") => word_mutants(key),
        Some("property") => property_near_miss(contract, key),
        Some("approx") if contract["tolerance"].is_string() => approx_tolerance(contract, key),
        _ => plus_one(key, false),
    }
}

/// `word-order`: other words that share the letters of the key, so the order of
/// the factors decides the verdict (the key reversed, rotated, shortened).
fn word_mutants(key: &str) -> Plan {
    let mut candidates = cadus_core::answer::contract::word_mutants(key).into_iter();
    let slot = candidates.next().map(|first| Slot {
        rest: candidates.collect(),
        ..Slot::new("word-order", first)
    });
    Plan::of(slot.into_iter().collect(), "no-word-mutant")
}

/// `property-near-miss` for a contract JSON with `"kind": "property"`.
///
/// The key is one example of many, so `+1` may have the property too. The
/// candidates are the boundary values of the predicate, then the integers
/// walking outward from the key; the first one that does not grade correct is
/// the mutant (D39), and none at all is `no-distinct-mutant`.
fn property_near_miss(contract: &Value, key: &str) -> Plan {
    let misses = parse_contract(contract)
        .ok()
        .and_then(|contract| contract.property_near_misses(key))
        .unwrap_or_default();
    let mut misses = misses.into_iter();
    let slot = misses.next().map(|first| Slot {
        rest: misses.collect(),
        ..Slot::new("property-near-miss", first)
    });
    Plan::of(slot.into_iter().collect(), "no-near-miss")
}

/// `plus-one`: the rule of `check_keys::mutate::mutate_plus_one`.
///
/// With `whole`, a key with no numeric leaf gets the mutant `(<key>) + 1` (D40).
fn plus_one(key: &str, whole: bool) -> Plan {
    let slot = mutate_plus_one(key)
        .map(|learner| Slot::new("plus-one", learner))
        .or_else(|| whole.then(|| whole_plus_one(key, "", "")));
    Plan::of(slot.into_iter().collect(), "no-numeric-leaf")
}

/// The mutant `(<key>) + 1` between the marks of its collection, if it has one.
///
/// A leading `name =` label stays in front: `x = (c - b)/a` gives
/// `x = ((c - b)/a) + 1`.
fn whole_plus_one(key: &str, open: &str, close: &str) -> Slot {
    let is_name = |text: &str| {
        let text = text.trim();
        !text.is_empty() && text.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
    };
    let learner = match key.split_once('=') {
        Some((name, body)) if is_name(name) => format!("{} = ({}) + 1", name.trim(), body.trim()),
        _ => format!("({key}) + 1"),
    };
    Slot {
        kind: Some("plus-one-whole"),
        ..Slot::new("plus-one", format!("{open}{learner}{close}"))
    }
}

/// `plus-one` for `approx` with a `tolerance`: the step is two times the
/// tolerance plus one step of the last digit of the key (D39).
///
/// The mutant is an expression, so that this file needs no rational arithmetic.
fn approx_tolerance(contract: &Value, key: &str) -> Plan {
    let tolerance = contract["tolerance"].as_str().unwrap_or("0");
    let key = key.trim();
    let step = match key.rsplit_once('.') {
        Some((_, digits)) if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) => {
            format!("0.{}1", "0".repeat(digits.len() - 1))
        }
        _ => "1".to_owned(),
    };
    let learner = format!("{key} + 2*({tolerance}) + {step}");
    Plan::of(vec![Slot::new("plus-one", learner)], "no-numeric-leaf")
}

/// `function-2x` for a contract JSON with `"kind": "function"`.
fn function_mutant(contract: &Value, key: &str) -> Plan {
    let up_to_constant = contract["up_to_constant"] == true;
    let slot = contract["vars"][0]
        .as_str()
        .map(|var| Slot::new("function-2x", function_2x(key, var, up_to_constant)));
    Plan::of(slot.into_iter().collect(), "no-variable")
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
fn label_others(contract: &Value, key: &str) -> Plan {
    let key = choice_key(key);
    let is_key = |alias: &Value| alias.as_str().is_some_and(|alias| choice_key(alias) == key);
    let slots = contract["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .filter(|aliases| !aliases.iter().any(is_key))
        .filter_map(|aliases| aliases.first().and_then(Value::as_str))
        .map(|alias| Slot::new("label-other", alias.to_owned()))
        .collect();
    Plan::of(slots, "no-other-option")
}

/// `member-removed`: the collection without one member.
///
/// The candidates remove the first member, then the second, and so on; a
/// candidate that grades `correct` (a repeated member) gives way to the next
/// one (D39). A collection of one member uses `plus-one`, or the mutant
/// `(<member>) + 1` if the member has no numeric leaf.
fn member_removed(key: &str) -> Plan {
    let (open, inner, close) = unwrap_collection(key.trim());
    let members = split_members(inner);
    let without = |skip: usize| {
        let kept: Vec<&str> = members
            .iter()
            .enumerate()
            .filter(|(at, _)| *at != skip)
            .map(|(_, member)| *member)
            .collect();
        format!("{open}{}{close}", kept.join(", "))
    };
    if members.len() < 2 {
        let mut plan = plus_one(key, false);
        if plan.slots.is_empty() && !inner.trim().is_empty() {
            plan.slots.push(whole_plus_one(inner.trim(), open, close));
        }
        return plan;
    }
    let slot = Slot {
        rest: (1..members.len()).map(without).collect(),
        ..Slot::new("member-removed", without(0))
    };
    Plan::of(vec![slot], "no-numeric-leaf")
}

/// Split `{..}` or `[..]` into the open mark, the inner text and the close mark.
///
/// Text such as `[1, 2], [3, 4]` has no outer pair, thus it stays whole. The
/// third pair is the LaTeX set of the shipped keys: `$\{A, B\}$`.
fn unwrap_collection(text: &str) -> (&str, &str, &str) {
    [("{", "}"), ("[", "]"), ("$\\{", "\\}$")]
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

/// Split at each comma and each ` and ` that is outside all brackets.
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
            ' ' if depth == 0 && at >= start && inner[at..].starts_with(" and ") => {
                members.push(inner[start..at].trim());
                start = at + 5;
            }
            _ => {}
        }
    }
    members.push(inner[start..].trim());
    members
}

/// One mutant place for each part of a `multipart` key (`name = value; name = value`).
///
/// The part takes the first place of the rule of its kind. The other parts
/// stay equal to the key. A part with no mutant goes to `no_mutant_parts`;
/// the item then has `pass: false` (D38).
fn part_mutants(contract: &Value, key: &str) -> Plan {
    let fields: Vec<(&str, &str)> = key
        .split(';')
        .filter_map(|field| field.split_once('='))
        .map(|(name, value)| (name.trim(), value.trim()))
        .collect();
    let whole_key = |name: &str, learner: &str| {
        fields
            .iter()
            .map(|(field, value)| {
                let value = if *field == name { learner } else { *value };
                format!("{field} = {value}")
            })
            .collect::<Vec<_>>()
            .join("; ")
    };
    let mut plan = Plan::of(Vec::new(), "no-mutant-part");
    for part in contract["parts"].as_array().into_iter().flatten() {
        let name = part["name"].as_str().unwrap_or("");
        let slot = fields
            .iter()
            .find(|(field, _)| *field == name)
            .and_then(|(_, value)| {
                self::plan(&part["contract"], value)
                    .slots
                    .into_iter()
                    .next()
            });
        match slot {
            Some(slot) => plan.slots.push(Slot {
                rule: format!("part:{name}:{}", slot.rule),
                first: whole_key(name, &slot.first),
                rest: slot
                    .rest
                    .iter()
                    .map(|learner| whole_key(name, learner))
                    .collect(),
                kind: slot.kind,
            }),
            None => plan.no_mutant_parts.push(name.to_owned()),
        }
    }
    plan
}

#[cfg(test)]
#[path = "mutants_tests.rs"]
mod tests;
