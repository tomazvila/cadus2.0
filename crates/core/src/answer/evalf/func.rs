//! The 17 functions of the grammar as `f64` functions.

/// One function of one real argument.
type Real = fn(f64) -> f64;

/// Each function name of the grammar with its `f64` function.
///
/// `ln` and `log` are the natural logarithm. A value outside the domain is
/// NaN or infinite here, and the caller refuses it.
const TABLE: [(&str, Real); 17] = [
    ("sqrt", f64::sqrt),
    ("sin", f64::sin),
    ("cos", f64::cos),
    ("tan", f64::tan),
    ("sec", |x| 1.0 / x.cos()),
    ("csc", |x| 1.0 / x.sin()),
    ("cot", |x| x.cos() / x.sin()),
    ("asin", f64::asin),
    ("acos", f64::acos),
    ("atan", f64::atan),
    ("sinh", f64::sinh),
    ("cosh", f64::cosh),
    ("tanh", f64::tanh),
    ("exp", f64::exp),
    ("ln", f64::ln),
    ("log", f64::ln),
    ("abs", f64::abs),
];

/// Apply the named function to the value. A name outside the table is `None`.
pub(super) fn apply(name: &str, value: f64) -> Option<f64> {
    TABLE
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, function)| function(value))
}
