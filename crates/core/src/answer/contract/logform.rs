//! The forms of a logarithmic answer: expanded (`3ln(x)`) and condensed (`ln(x^3)`).

use crate::answer::Ast;

/// Whether the name is a logarithm.
fn is_log(name: &str) -> bool {
    matches!(name, "ln" | "log")
}

/// An expanded logarithm: no logarithm holds a product, a quotient, a root, or a
/// power in its argument. `3ln(x)` and `ln(x) + ln(y)` pass, `ln(x^3)` and
/// `ln(x/y)` do not.
pub(super) fn expanded(tree: &Ast) -> bool {
    match tree {
        Ast::Func(name, arguments) => {
            let first_is_plain = arguments.first().is_none_or(plain_argument);
            (!is_log(name) || first_is_plain) && arguments.iter().all(expanded)
        }
        Ast::Neg(inner) | Ast::Pow(inner, _) | Ast::Sqrt(inner) => expanded(inner),
        Ast::Add(items) | Ast::Mul(items) => items.iter().all(expanded),
        Ast::Div(top, bottom) => expanded(top) && expanded(bottom),
        _ => true,
    }
}

/// An argument that has no product, quotient, root, or power at its top.
fn plain_argument(argument: &Ast) -> bool {
    match argument {
        Ast::Neg(inner) => plain_argument(inner),
        Ast::Mul(_) | Ast::Div(..) | Ast::Pow(..) | Ast::Sqrt(_) | Ast::RationalPow { .. } => false,
        other => expanded(other),
    }
}

/// A condensed logarithm: one logarithm call, with no coefficient and no other term.
pub(super) fn condensed(tree: &Ast) -> bool {
    matches!(tree, Ast::Func(name, _) if is_log(name))
}
