//! Part 4 of the helpers of the `answer_oracle` tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    unused_imports
)]

use super::*;

/// Reorder the factors of a product (spec section 9.3, "commutative reorder").
///
/// `2*x` becomes `x*2`, and the expected verdict is True on both sides. The rule
/// applies only when the whole source is one multiplicative term: a rotation
/// across a `+` or a binary `-` builds a different value, and the family asks
/// about commutativity and not about arithmetic.
pub fn generate_product_reorder(row: &Row) -> Option<String> {
    if !is_one_term(&row.printed) {
        return None;
    }
    let factors = product_factors(&row.printed)?;
    if factors.len() < 2 {
        return None;
    }
    let mut rotated = factors;
    rotated.rotate_left(1);
    let joined: Vec<String> = rotated
        .iter()
        .map(|factor| {
            // A factor that starts with its own sign takes a bracket, because
            // `x*-2` is a spelling no learner types.
            if factor.starts_with('-') || factor.starts_with('+') {
                format!("({factor})")
            } else {
                factor.clone()
            }
        })
        .collect();
    changed_printed(row, joined.join("*"))
}

/// Write the answer with one algebraic step applied (spec section 9.3,
/// "algebraic refactor").
///
/// The expected verdict is True on both sides. Two rules run, in a fixed order:
///
/// 1. A difference of two squares takes its factored form, which is the pair the
///    spec names: `(x-1)*(x+1)` for `x**2-1`.
/// 2. A product whose last factor is a parenthesized sum multiplies out, which
///    is the same step in the other direction: the corpus authors the factored
///    form (`(x + 3)(x - 3)`) and the learner multiplies it out.
pub fn generate_algebraic_refactor(row: &Row) -> Option<String> {
    let candidate = factor_a_difference_of_squares(&row.printed)
        .or_else(|| multiply_out_last_group(&row.printed));
    changed_printed(row, candidate?)
}

/// Write `a**2 - b**2` as `(a - b)*(a + b)`.
pub fn factor_a_difference_of_squares(source: &str) -> Option<String> {
    let terms = signed_terms(source)?;
    if terms.len() != 2 {
        return None;
    }
    let (left_negative, left) = terms.first()?;
    let (right_negative, right) = terms.get(1)?;
    if *left_negative || !*right_negative {
        return None;
    }
    let a = square_root_text(left)?;
    let b = square_root_text(right)?;
    Some(format!("({a} - {b})*({a} + {b})"))
}

/// The square root of one term, when the term is a plain square.
///
/// The term is a whole number that is a perfect square, or `name**2`, or
/// `k*name**2` and `kname**2` with a perfect-square `k`.
pub fn square_root_text(term: &str) -> Option<String> {
    let term = term.trim();
    if let Some((negative, digits)) = integer_digits(term) {
        if negative {
            return None;
        }
        let value: i128 = digits.parse().ok()?;
        return Some(integer_square_root(value)?.to_string());
    }
    let digits: String = term.chars().take_while(char::is_ascii_digit).collect();
    let rest = term.get(digits.len()..)?.trim_start_matches('*');
    let name = rest.strip_suffix("**2")?;
    if name.is_empty()
        || !name.chars().all(|c| c.is_ascii_alphanumeric())
        || name.starts_with(|c: char| c.is_ascii_digit())
    {
        return None;
    }
    if digits.is_empty() {
        return Some(name.to_string());
    }
    let coefficient: i128 = digits.parse().ok()?;
    let root = integer_square_root(coefficient)?;
    Some(format!("{root}*{name}"))
}

/// The exact square root of a whole number, when the number is a square.
pub fn integer_square_root(value: i128) -> Option<i128> {
    if value < 0 {
        return None;
    }
    let mut root = 0_i128;
    while root.checked_mul(root)? < value {
        root += 1;
    }
    (root * root == value).then_some(root)
}

/// Multiply the last parenthesized sum of a product out over its other factor.
pub fn multiply_out_last_group(source: &str) -> Option<String> {
    let text = source.trim();
    let chars: Vec<char> = text.chars().collect();
    if chars.last() != Some(&')') {
        return None;
    }
    let start = matching_open(&chars)?;
    if start == 0 {
        return None;
    }
    // The group must be a factor and not a function argument. A function name
    // puts a letter in front of the bracket, and a `/` in front of it makes the
    // group a divisor.
    let before = chars.get(start - 1).copied()?;
    if !(before == ')' || before.is_ascii_digit() || before == '*') {
        return None;
    }
    let prefix_end = if before == '*' { start - 1 } else { start };
    let prefix: String = chars.get(..prefix_end)?.iter().collect();
    let prefix = prefix.trim();
    if prefix.is_empty() || !is_one_term(prefix) {
        return None;
    }
    let inner: String = chars.get(start + 1..chars.len() - 1)?.iter().collect();
    let terms = signed_terms(&inner)?;
    if terms.len() < 2 {
        return None;
    }
    let mut out = String::new();
    for (index, (negative, body)) in terms.iter().enumerate() {
        if index == 0 {
            if *negative {
                out.push('-');
            }
        } else if *negative {
            out.push_str(" - ");
        } else {
            out.push_str(" + ");
        }
        let _ = write!(out, "{prefix}*({body})");
    }
    Some(out)
}

/// The index of the bracket that the last character of `chars` closes.
pub fn matching_open(chars: &[char]) -> Option<usize> {
    let mut depth = 0_i32;
    for index in (0..chars.len()).rev() {
        match chars.get(index).copied()? {
            ')' => depth += 1,
            '(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

// ---------------------------------------------------------------------------
// The rational-rewrite family (M2 review 3, finding #14)
// ---------------------------------------------------------------------------
/// One line of `crates/core/tests/fixtures/answers/rational_rewrites_1_0.jsonl`.
///
/// The file holds one spelling per (answer, kind, rule). SymPy wrote every
/// spelling through `scripts/oracle/rewrite_1_0.py`, from the tree the 1.0 parser
/// itself reads. SymPy judges nothing: `check_1_0.py` still records the verdict of
/// every pair, and 1.0 grades a spelling like any other learner answer.
#[derive(Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct RewriteLine {
    /// The authored corpus answer, verbatim.
    pub answer: String,
    /// The authored answer kind.
    pub kind: String,
    /// The SymPy rule that wrote the spelling.
    pub rule: String,
    /// The spelling, as `str()` printed it.
    pub learner: String,
    /// Whether SymPy `cancel(expected - learner)` is zero.
    pub cancel_zero: bool,
    /// Whether SymPy `radsimp(expected - learner)` is zero.
    pub radsimp_zero: bool,
}

/// The rules of `scripts/oracle/rewrite_1_0.py`, in the order that file runs them.
pub const REWRITE_RULES: [&str; 6] = ["together", "apart", "cancel", "factor", "expand", "radsimp"];

/// Read the committed rewrite spellings.
pub fn committed_rewrites() -> &'static Vec<RewriteLine> {
    static CACHE: std::sync::OnceLock<Vec<RewriteLine>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| read_jsonl("rational_rewrites_1_0.jsonl"))
}

/// Index the committed rewrites by the key and the value `entry` reads off a row.
fn index_rewrites<K: Ord, V>(entry: impl Fn(&RewriteLine) -> (K, V)) -> BTreeMap<K, V> {
    committed_rewrites().iter().map(entry).collect()
}

/// The spelling of one rule, keyed by (answer, kind, rule).
pub fn rewrite_by_rule() -> &'static BTreeMap<(String, String, String), String> {
    static CACHE: std::sync::OnceLock<BTreeMap<(String, String, String), String>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        index_rewrites(|row| {
            (
                (row.answer.clone(), row.kind.clone(), row.rule.clone()),
                row.learner.clone(),
            )
        })
    })
}

/// The SymPy difference evidence of one pair, keyed by (expected, learner, kind).
///
/// The two flags are the only reason a pair may leave class 3 under the two
/// rewrite divergences. They are recorded facts about SymPy, not verdicts: a
/// verdict always comes from the 1.0 checker.
pub fn rewrite_evidence() -> &'static BTreeMap<PairKey, (bool, bool)> {
    static CACHE: std::sync::OnceLock<BTreeMap<PairKey, (bool, bool)>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        index_rewrites(|row| {
            (
                (row.answer.clone(), row.learner.clone(), row.kind.clone()),
                (row.cancel_zero, row.radsimp_zero),
            )
        })
    })
}

/// Whether the answer is one scalar value that holds a denominator or a radical.
///
/// The gate reads the TREE and never the source text: FIXM2g leaves `\frac`,
/// `√`, and `\sqrt` in the source, and a text test would miss every one of them.
///
/// A tuple, a set, a list, a range, an inequality, and a labeled value are all
/// refused, whatever they hold. SymPy reads `(1/2, 8)` as a plain Python tuple,
/// and `cancel` of a tuple returns the numerator, the denominator, and the terms
/// of a rational function, so the "spelling" would be a value no learner ever
/// writes and no rule of the family names.
pub fn holds_a_denominator_or_a_radical(ast: &Ast) -> bool {
    match ast {
        Ast::Fraction { .. } | Ast::Mixed { .. } | Ast::Div(_, _) | Ast::Sqrt(_) => true,
        Ast::Pow(base, exponent) => *exponent < 0 || holds_a_denominator_or_a_radical(base),
        // A rational exponent is a root (D-F3).
        Ast::RationalPow { .. } => true,
        Ast::Quantity { value, .. } => holds_a_denominator_or_a_radical(value),
        Ast::Integer(_) | Ast::Decimal { .. } | Ast::Var(_) | Ast::Const(_) => false,
        Ast::Neg(inner) => holds_a_denominator_or_a_radical(inner),
        Ast::Add(items) | Ast::Mul(items) | Ast::Func(_, items) => {
            items.iter().any(holds_a_denominator_or_a_radical)
        }
        Ast::Tuple(_)
        | Ast::Set(_)
        | Ast::List(_)
        | Ast::Interval { .. }
        | Ast::Chain { .. }
        | Ast::Ineq { .. }
        | Ast::Assign { .. } => false,
    }
}

/// Whether SymPy reads the answer as the value 2.0 reads.
///
/// The family asks about a REWRITE, so it needs a spelling of the SAME value.
/// SymPy reads the answer through the 1.0 rewrite, and two constructs of the V4
/// table make that reading another value:
///
/// 1. A mixed number. 1.0 has no mixed-number reading, so `3 1/2` is the product
///    `3*(1/2)`, and `together` of that product is `3/2`.
/// 2. A spaced `x` as the times sign. 1.0 reads the letter as a free symbol, so
///    `3 x 10^-2` is `3*x/100` and not the number 0.03.
///
/// Both are documented divergences with literal pairs in
/// `crates/core/tests/answer_divergence.rs`. A spelling built on top of one of
/// them measures the base divergence again, and never the rewrite.
pub fn the_two_checkers_read_the_answer_alike(row: &Row) -> bool {
    if holds_a_mixed_number(&row.ast) {
        return false;
    }
    !(names_a_bare_word(&one_zero_source(&row.answer), "x") && !holds_the_variable_x(&row.answer))
}

/// Whether the answer tree holds a mixed number at any depth.
pub fn holds_a_mixed_number(ast: &Ast) -> bool {
    match ast {
        Ast::Mixed { .. } => true,
        Ast::Neg(inner) | Ast::Sqrt(inner) | Ast::Pow(inner, _) => holds_a_mixed_number(inner),
        Ast::RationalPow { base: inner, .. } | Ast::Quantity { value: inner, .. } => {
            holds_a_mixed_number(inner)
        }
        Ast::Add(items)
        | Ast::Mul(items)
        | Ast::Func(_, items)
        | Ast::Tuple(items)
        | Ast::Set(items)
        | Ast::List(items) => items.iter().any(holds_a_mixed_number),
        Ast::Div(left, right) => holds_a_mixed_number(left) || holds_a_mixed_number(right),
        Ast::Interval { lo, hi, .. } | Ast::Chain { lo, hi, .. } => {
            holds_a_mixed_number(lo) || holds_a_mixed_number(hi)
        }
        Ast::Ineq { bound, .. } => holds_a_mixed_number(bound),
        Ast::Assign { value, .. } => holds_a_mixed_number(value),
        Ast::Integer(_) | Ast::Decimal { .. } | Ast::Fraction { .. } => false,
        Ast::Var(_) | Ast::Const(_) => false,
    }
}

/// The committed spelling of one rule, when the family applies to the row.
pub fn rewrite_spelling(row: &Row, rule: &str) -> Option<String> {
    if !holds_a_denominator_or_a_radical(&row.ast) {
        return None;
    }
    if !the_two_checkers_read_the_answer_alike(row) {
        return None;
    }
    let key = (
        row.answer.clone(),
        row.kind.as_str().to_string(),
        rule.to_string(),
    );
    let learner = rewrite_by_rule().get(&key)?.clone();
    let learner = changed(row, learner)?;
    changed_printed(row, learner)
}

/// Put the fractions of the answer over one common denominator.
pub fn generate_rewrite_together(row: &Row) -> Option<String> {
    rewrite_spelling(row, "together")
}

/// Split the answer into partial fractions.
pub fn generate_rewrite_apart(row: &Row) -> Option<String> {
    rewrite_spelling(row, "apart")
}

/// Cancel the common factors of the numerator and the denominator.
pub fn generate_rewrite_cancel(row: &Row) -> Option<String> {
    rewrite_spelling(row, "cancel")
}

/// Factor the answer.
pub fn generate_rewrite_factor(row: &Row) -> Option<String> {
    rewrite_spelling(row, "factor")
}

/// Multiply the answer out.
pub fn generate_rewrite_expand(row: &Row) -> Option<String> {
    rewrite_spelling(row, "expand")
}

/// Rationalize the radicals of the answer.
pub fn generate_rewrite_radsimp(row: &Row) -> Option<String> {
    rewrite_spelling(row, "radsimp")
}

pub fn generate_last_digit_bumped(row: &Row) -> Option<String> {
    bump_last_digit(&row.printed)
}

pub fn generate_sign_flipped(row: &Row) -> Option<String> {
    if row.printed.chars().all(|c| c == '0' || c == '-') {
        return None;
    }
    match row.printed.strip_prefix('-') {
        Some(rest) => Some(rest.to_string()),
        None => Some(format!("-{}", row.printed)),
    }
}

pub fn generate_digit_transposition(row: &Row) -> Option<String> {
    let chars: Vec<char> = row.printed.chars().collect();
    let mut position = None;
    for index in 0..chars.len().saturating_sub(1) {
        let left = chars.get(index).copied().unwrap_or(' ');
        let right = chars.get(index + 1).copied().unwrap_or(' ');
        if left.is_ascii_digit() && right.is_ascii_digit() && left != right {
            position = Some(index);
        }
    }
    let index = position?;
    let mut out = chars;
    out.swap(index, index + 1);
    Some(out.into_iter().collect())
}

pub fn generate_times_thousand(row: &Row) -> Option<String> {
    let (negative, digits) = integer_digits(&row.printed)?;
    if digits.len() > 12 || digits == "0" {
        return None;
    }
    let sign = if negative { "-" } else { "" };
    Some(format!("{sign}{digits}000"))
}

pub fn generate_over_thousand(row: &Row) -> Option<String> {
    let (negative, digits) = integer_digits(&row.printed)?;
    if digits.len() > 3 || digits == "0" {
        return None;
    }
    let sign = if negative { "-" } else { "" };
    Some(format!("{sign}0.{digits:0>3}"))
}

pub fn generate_coarse_decimal(row: &Row) -> Option<String> {
    let (numerator, denominator) = fraction_parts(&row.printed)?;
    let scaled = numerator.checked_mul(1_000)?;
    if scaled % denominator == 0 {
        return None;
    }
    let rounded =
        (scaled * 2 + denominator.signum() * numerator.signum().abs()) / (denominator * 2);
    let negative = rounded < 0;
    let digits = format!("{:0>4}", rounded.abs());
    let (whole, fraction) = digits.split_at(digits.len() - 3);
    let sign = if negative { "-" } else { "" };
    Some(format!("{sign}{whole}.{fraction}"))
}

pub fn generate_tuple_swapped(row: &Row) -> Option<String> {
    let mut items = bracket_items(&row.printed, '(', ')')?;
    if items.len() < 2 || items.first() == items.get(1) {
        return None;
    }
    items.swap(0, 1);
    Some(format!("({})", items.join(", ")))
}

pub fn generate_set_element_changed(row: &Row) -> Option<String> {
    let items = bracket_items(&row.printed, '{', '}')?;
    let first = items.first()?;
    let bumped = bump_last_digit(first)?;
    if items.contains(&bumped) {
        return None;
    }
    let mut out = items.clone();
    *out.first_mut()? = bumped;
    Some(format!("{{{}}}", out.join(", ")))
}

pub fn generate_wrong_radicand(row: &Row) -> Option<String> {
    bump_number_after(&row.printed, "sqrt(")
}

pub fn generate_wrong_exponent(row: &Row) -> Option<String> {
    bump_number_after(&row.printed, "**")
}
