//! Rule R3 on the tree (pack decision D31): a key with a function name and a
//! variable is a formula. The plain exact comparison gives a decided "wrong"
//! for an equal learner form (`tan^-1`, `-ln|cos x|`, `+ K`), so such a key
//! needs the `function` contract. `exact` is legal only for a polynomial key.

use super::super::kp_view::{Item, KpView};
use super::finding;
use crate::output::Finding;
use cadus_core::curriculum::AnswerKind;

/// The function names of the answer grammar, and the forms that the shipped
/// keys use (`arctan`, `log`). `sqrt` is not in the list: a radical is a part
/// of the exact grammar (rule R1, key `2sqrt(3)`), and the exact comparison
/// has a canonical form for it.
const FUNCTIONS: [&str; 21] = [
    "sin", "cos", "tan", "sec", "csc", "cot", "asin", "acos", "atan", "sinh", "cosh", "tanh",
    "exp", "ln", "abs", "arcsin", "arccos", "arctan", "log", "arcsec", "arccot",
];
/// The names that are constants, not variables.
const CONSTANTS: [&str; 2] = ["pi", "e"];

/// The maximal runs of ASCII letters of a key. The name before a `=` is the
/// label of the answer (`x = ln(7)`), so it is not in the list. The word `or`
/// joins two answers.
fn words(key: &str) -> Vec<&str> {
    let letters = |text| str::split(text, |ch: char| !ch.is_ascii_alphabetic());
    let mut segments: Vec<&str> = key.split('=').collect();
    let last = segments.pop().unwrap_or("");
    let labeled = segments.into_iter().flat_map(|segment| {
        let mut run: Vec<&str> = letters(segment).collect();
        run.truncate(run.iter().rposition(|word| !word.is_empty()).unwrap_or(0));
        run
    });
    labeled
        .chain(letters(last))
        .filter(|word| !word.is_empty() && *word != "or")
        .collect()
}

/// The function name at the end of a letter run (`xsin` ends with `sin`).
/// The longest name wins, so `arctan` is not `x` + `tan`.
fn function_at_end(word: &str) -> Option<&'static str> {
    FUNCTIONS
        .iter()
        .copied()
        .filter(|name| word.ends_with(name))
        .max_by_key(|name| name.len())
}

/// True if the key has a function name and also a variable.
pub fn is_formula_key(key: &str) -> bool {
    let mut has_function = false;
    let mut has_variable = false;
    for word in words(key) {
        let function = function_at_end(word);
        let rest = &word[..word.len() - function.map_or(0, str::len)];
        has_function |= function.is_some();
        has_variable |= !rest.is_empty() && !CONSTANTS.contains(&rest);
    }
    has_function && has_variable
}

/// True if the plain exact comparison grades the item.
fn is_plain_exact(view: &KpView, item: &Item) -> bool {
    match item.kind() {
        Some(kind) => kind == "exact",
        None => view.answer_kind == AnswerKind::Expression,
    }
}

/// True if the item asks for a logarithm in a named form (expand or condense).
///
/// `function` grades by value, so it accepts `ln(x^3)` for "Expand ln(x^3)". The
/// plain exact comparison reads the logarithm by value too, but it keeps the
/// form of the sum, so `exact` is the contract for such an item.
fn is_rewrite_item(item: &Item) -> bool {
    let question = item.exemplar.problem.to_lowercase();
    let key = &item.exemplar.answer;
    let has_log = key.contains("ln") || key.contains("log");
    has_log
        && [
            "expand",
            "condense",
            "single logarithm",
            "as a sum",
            "as one log",
        ]
        .iter()
        .any(|word| question.contains(word))
}

/// The R3 findings of the verdict exemplars of the KP.
pub fn check(view: &KpView) -> Vec<Finding> {
    view.items
        .iter()
        .filter(|item| item.verdict && is_plain_exact(view, item))
        .filter(|item| is_formula_key(&item.exemplar.answer))
        .filter(|item| !is_rewrite_item(item))
        .map(|item| {
            let detail = format!(
                "R3: the key `{}` has a function name and a variable, and the plain exact \
comparison grades it; use the `function` contract (D31)",
                item.exemplar.answer
            );
            finding(view, "contract-rule", "R3", Some(item), detail)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::super::testkit::{invariants, item, view};
    use super::*;

    #[test]
    fn a_formula_key_has_a_function_name_and_a_variable() {
        for key in [
            "arctan(x) + C",
            "-ln|cos x|",
            "6x cos(3x^2)",
            "xsin(x)",
            "e^x sin(x)",
            "y = sin(x)",
        ] {
            assert!(is_formula_key(key), "{key}");
        }
        for key in [
            "x*sqrt(x)",
            "x = ln(10)/ln(2)",
            "x = ln(2) or x = -ln(2)",
            "2sqrt(3)",
            "ln(2)",
            "x^2 + 1",
            "5pi/6",
            "e^(2x)",
            "sin(pi/3)",
            "12",
        ] {
            assert!(!is_formula_key(key), "{key}");
        }
    }

    #[test]
    fn r3_allows_exact_on_an_expand_or_condense_log_item() {
        let rewrite =
            |question: &str, key: &str| item(question, key, json!({"kind": "exact"}), None);
        let found = check(&view(vec![rewrite(
            "Expand $\\ln(x^3 y)$.",
            "3 ln(x) + ln(y)",
        )]));
        assert!(found.is_empty());
        let found = check(&view(vec![rewrite("Differentiate.", "3 ln(x) + ln(y)")]));
        assert_eq!(invariants(&found), ["R3"]);
    }

    #[test]
    fn r3_reads_the_items_that_the_exact_comparison_grades() {
        let formula = |contract: Value| item("Find the derivative.", "2x cos(x^2)", contract, None);
        let found = check(&view(vec![formula(json!({"kind": "exact"}))]));
        assert_eq!(invariants(&found), ["R3"]);
        assert_eq!(
            (found[0].code.as_str(), found[0].ck.as_str()),
            ("contract-rule", "CK4")
        );
        // No contract: the topic kind decides.
        let mut bare = view(vec![formula(Value::Null)]);
        assert_eq!(check(&bare), []);
        bare.answer_kind = AnswerKind::Expression;
        assert_eq!(invariants(&check(&bare)), ["R3"]);
        // A different contract kind, and an item with no verdict.
        let list = json!({"kind": "set"});
        assert_eq!(check(&view(vec![formula(list)])), []);
        let mut unmarked = formula(json!({"kind": "exact"}));
        unmarked.verdict = false;
        assert_eq!(check(&view(vec![unmarked])), []);
    }
}
