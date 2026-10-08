//! Rule R4 on the tree: a key that is a product of factors that do not commute
//! (a matrix product, a group word, a product of cycles, a matrix size) needs
//! the `ordered_word` contract. The plain exact comparison sorts the factors of
//! a product, so it grades `ba` correct for the key `ab`.

use super::super::kp_view::{Item, KpView};
use super::finding;
use crate::output::Finding;
use cadus_core::curriculum::AnswerKind;

/// The words of a problem that name a setting where the product does not commute.
const SETTING_WORDS: [&str; 9] = [
    "group",
    "dihedral",
    "generator",
    "matrix",
    "matrices",
    "permutation",
    "commut",
    "composition",
    "product of the",
];

/// The key without its exponents: `r^2 s`, `a^{-1} b` and `a^(-1)b` give `r s`.
fn without_exponents(key: &str) -> String {
    let mut out = String::new();
    let mut chars = key.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '^' {
            out.push(ch);
            continue;
        }
        match chars.peek() {
            Some('{' | '(') => {
                let close = if chars.next() == Some('{') { '}' } else { ')' };
                for inner in chars.by_ref() {
                    if inner == close {
                        break;
                    }
                }
            }
            _ => {
                chars.next_if_eq(&'-');
                while chars.next_if(char::is_ascii_digit).is_some() {}
            }
        }
        out.push(' ');
    }
    out
}

/// The count of letter factors if the key is only letters, spaces, product
/// signs and parentheses (so `ab`, `A^-1 B`, `(AB)^-1`); `None` otherwise.
fn letter_factors(key: &str) -> Option<(usize, usize)> {
    let bare = without_exponents(key.trim().trim_matches('$'));
    let allowed = |ch: char| ch.is_ascii_alphabetic() || ch.is_whitespace() || "*()·⋅".contains(ch);
    if bare.is_empty() || !bare.chars().all(allowed) {
        return None;
    }
    let letters = bare.chars().filter(char::is_ascii_alphabetic).count();
    let capitals = bare.chars().filter(char::is_ascii_uppercase).count();
    let runs = bare
        .split(|ch: char| !ch.is_ascii_alphabetic())
        .filter(|run| run.len() > 1)
        .any(|run| {
            matches!(
                run,
                "sin" | "cos" | "tan" | "sec" | "csc" | "cot" | "ln" | "log" | "exp" | "sqrt"
            )
        });
    (!runs).then_some((letters, capitals))
}

/// Whether a comma follows a closing parenthesis, as in a list of points or
/// pairs such as `(0, 0), (1, 1)` or a nested pair `((7, 0), 5)`.
fn is_list_of_tuples(key: &str) -> bool {
    key.split(')')
        .skip(1)
        .any(|rest| rest.trim_start().starts_with(','))
}

/// Whether the key is a product of cycles such as `(1 2)(3 4)`. A list of
/// points or pairs is a set of answers, not a product.
fn is_cycle_product(key: &str) -> bool {
    let key = key.trim();
    !is_list_of_tuples(key)
        && key.matches('(').count() >= 2
        && key.starts_with('(')
        && key
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch.is_whitespace() || "(),".contains(ch))
}

/// Whether the key is a matrix size such as `4 x 3`.
fn is_size(key: &str) -> bool {
    let key = key.replace('×', "x");
    let parts: Vec<&str> = key.split('x').map(str::trim).collect();
    parts.len() == 2
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
}

/// Whether the plain exact comparison would grade a key of this shape.
pub fn needs_ordered_word(key: &str, problem: &str) -> bool {
    if is_cycle_product(key) || is_size(key) {
        return true;
    }
    let Some((letters, capitals)) = letter_factors(key) else {
        return false;
    };
    if letters < 2 {
        return false;
    }
    let problem = problem.to_lowercase();
    // A lowercase key that answers "how many" is a count; counts commute.
    if capitals < 2 && problem.contains("how many") {
        return false;
    }
    capitals >= 2 || SETTING_WORDS.iter().any(|word| problem.contains(word))
}

fn is_plain_exact(view: &KpView, item: &Item) -> bool {
    match item.kind() {
        Some(kind) => kind == "exact",
        None => view.answer_kind == AnswerKind::Expression,
    }
}

/// The R4 findings of the exemplars of the KP. An item the exact comparison
/// cannot read (`AB`) is a finding too: the key needs a contract that reads it.
pub fn check(view: &KpView) -> Vec<Finding> {
    view.items
        .iter()
        .filter(|item| is_plain_exact(view, item))
        .filter(|item| needs_ordered_word(&item.exemplar.answer, &item.exemplar.problem))
        .map(|item| {
            let detail = format!(
                "R4: the key `{}` is a product of factors that do not commute, and the plain \
exact comparison ignores their order; use the `ordered_word` contract",
                item.exemplar.answer
            );
            finding(view, "contract-rule", "R4", Some(item), detail)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::testkit::{invariants, item, view};
    use super::*;

    #[test]
    fn products_of_capital_letters_need_an_order() {
        for key in ["AB", "A^{-1}B", "B^-1 A^-1", "(AB)^-1", "A*C"] {
            assert!(needs_ordered_word(key, "Find the product."), "{key}");
        }
    }

    #[test]
    fn group_words_need_an_order_when_the_problem_names_a_group() {
        assert!(needs_ordered_word(
            "r^2 s",
            "In the dihedral group, simplify."
        ));
        assert!(!needs_ordered_word("xy", "Simplify the product."));
    }

    #[test]
    fn cycles_and_sizes_need_an_order() {
        assert!(needs_ordered_word("(1 2)(3 4)", "Compose."));
        assert!(needs_ordered_word("4 x 3", "State the size."));
        assert!(!needs_ordered_word("(1 2 3)", "Write it."));
    }

    #[test]
    fn lists_of_points_and_pairs_are_not_cycle_products() {
        for key in ["(0, 0), (1, 1)", "(1,2),(2,4)", "((7, 0), 5)"] {
            assert!(!needs_ordered_word(key, "List the points."), "{key}");
        }
    }

    #[test]
    fn a_count_asked_with_how_many_commutes() {
        assert!(!needs_ordered_word(
            "m*n",
            "Two groups have m and n elements. How many functions are there?"
        ));
        assert!(needs_ordered_word(
            "AB",
            "How many rows does the matrix AB have?"
        ));
    }

    #[test]
    fn plain_algebra_is_left_alone() {
        for key in ["x", "2x + 1", "sin x", "A", "x^2 y", "ln(2)"] {
            assert!(!needs_ordered_word(key, "Simplify."), "{key}");
        }
    }

    #[test]
    fn r4_reads_exact_items_only() {
        let word = |contract| item("Multiply the matrices.", "AB", contract, None);
        let found = check(&view(vec![word(json!({"kind": "exact"}))]));
        assert_eq!(invariants(&found), ["R4"]);
        assert_eq!(
            check(&view(vec![word(json!({"kind": "ordered_word"}))])),
            []
        );
    }
}
