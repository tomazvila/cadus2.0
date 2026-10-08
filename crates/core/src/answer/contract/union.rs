//! Exact finite unions of intervals over one unknown.
//!
//! An end is an exact number: a rational, or a combination of roots and
//! constants such as `1/sqrt(3)` and `pi/2`. Two ends compare by their exact
//! forms when those are equal, and by their decimal value otherwise.

use std::cmp::Ordering;

use num_traits::ToPrimitive;

use super::{Canon, Undecidable, canonical_form};

/// One end of a range: its exact form and its decimal value for ordering.
#[derive(Clone, Debug)]
struct Bound {
    value: Canon,
    approx: f64,
}

impl Bound {
    fn of(value: Canon) -> Result<Self, Undecidable> {
        let approx = approximate(&value)
            .filter(|approx| approx.is_finite())
            .ok_or_else(refused)?;
        Ok(Self { value, approx })
    }
}

/// The decimal value of a rational or a radical combination.
fn approximate(value: &Canon) -> Option<f64> {
    match value {
        Canon::Rational(number) => number.to_f64(),
        Canon::Radical(parts) => {
            let mut sum = 0.0_f64;
            for (basis, coefficient) in parts {
                sum += coefficient.to_f64()?
                    * basis.radicand.to_f64()?.sqrt()
                    * std::f64::consts::PI.powi(i32::try_from(basis.pi).ok()?)
                    * std::f64::consts::E.powi(i32::try_from(basis.e).ok()?);
            }
            Some(sum)
        }
        _ => None,
    }
}

impl PartialEq for Bound {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Bound {}

impl PartialOrd for Bound {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Bound {
    fn cmp(&self, other: &Self) -> Ordering {
        match (&self.value, &other.value) {
            (Canon::Rational(left), Canon::Rational(right)) => left.cmp(right),
            (left, right) if left == right => Ordering::Equal,
            _ => self.approx.total_cmp(&other.approx),
        }
    }
}

#[derive(Clone)]
struct Range {
    lo: Option<Bound>,
    hi: Option<Bound>,
    lo_closed: bool,
    hi_closed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Notation {
    Interval,
    Inequality,
}

pub(super) fn read(text: &str) -> Result<Canon, Undecidable> {
    read_with_notation(text).map(|(value, _)| value)
}

pub(super) fn read_with_notation(text: &str) -> Result<(Canon, Notation), Undecidable> {
    let source = notation_source(text);
    let normalized = source.split_whitespace().collect::<Vec<_>>().join(" ");
    let normalized = natural_text(&ascii_union(&normalized));
    if is_all_reals(&normalized) {
        let whole = Range {
            lo: None,
            hi: None,
            lo_closed: false,
            hi_closed: false,
        };
        return Ok((
            Canon::List(canonical_ranges(vec![whole])),
            Notation::Interval,
        ));
    }
    if let Some(excluded) = excluded_points(&normalized) {
        return excluded.map(|(var, ranges)| {
            let list = Canon::List(canonical_ranges(ranges));
            match var {
                Some(var) => (
                    Canon::Assign {
                        var,
                        value: Box::new(list),
                    },
                    Notation::Inequality,
                ),
                None => (list, Notation::Interval),
            }
        });
    }
    let single_interval = normalized.contains(',')
        && matches!(normalized.chars().next(), Some('(' | '['))
        && matches!(normalized.chars().last(), Some(')' | ']'));
    if normalized.contains('∪') || single_interval {
        return interval_notation(&normalized).map(|value| (value, Notation::Interval));
    }
    let branches: Vec<_> = normalized.split(" or ").collect();
    if branches.len() > 16 {
        return Err(refused());
    }
    let mut variable = None;
    let mut ranges = Vec::new();
    for branch in branches {
        // `x > -7 and x < 7` is the overlap of two half lines.
        let mut range: Option<Range> = None;
        for piece in branch.split(" and ") {
            let (var, next) = branch_range(piece)?;
            if variable.as_ref().is_some_and(|name| name != &var) {
                return Err(refused());
            }
            variable = Some(var);
            range = Some(match range {
                Some(previous) => overlap(&previous, &next),
                None => next,
            });
        }
        let Some(range) = range else {
            return Err(refused());
        };
        if nonempty(&range) {
            ranges.push(range);
        }
    }
    let Some(var) = variable else {
        return Err(refused());
    };
    Ok((
        Canon::Assign {
            var,
            value: Box::new(Canon::List(canonical_ranges(merge(ranges)))),
        },
        Notation::Inequality,
    ))
}

/// The variable of an exclusion, if the text names one, and the ranges left.
type Excluded = (Option<String>, Vec<Range>);

/// One inequality, as its variable and its range.
fn branch_range(text: &str) -> Result<(String, Range), Undecidable> {
    if let Canon::Assign { var, value } = canonical_form(text)? {
        // `x = 3` is the one point 3.
        let point = Bound::of(*value)?;
        let range = Range {
            lo: Some(point.clone()),
            hi: Some(point),
            lo_closed: true,
            hi_closed: true,
        };
        return Ok((var, range));
    }
    let Canon::Interval {
        var: Some(var),
        lo,
        hi,
        lo_closed,
        hi_closed,
    } = canonical_form(text)?
    else {
        return Err(refused());
    };
    let range = Range {
        lo: bound(lo)?,
        hi: bound(hi)?,
        lo_closed,
        hi_closed,
    };
    Ok((var, range))
}

/// The overlap of two ranges.
fn overlap(left: &Range, right: &Range) -> Range {
    let (lo, lo_closed) = match (&left.lo, &right.lo) {
        (None, _) => (right.lo.clone(), right.lo_closed),
        (_, None) => (left.lo.clone(), left.lo_closed),
        (Some(a), Some(b)) if a > b => (left.lo.clone(), left.lo_closed),
        (Some(a), Some(b)) if a < b => (right.lo.clone(), right.lo_closed),
        _ => (left.lo.clone(), left.lo_closed && right.lo_closed),
    };
    let (hi, hi_closed) = match (&left.hi, &right.hi) {
        (None, _) => (right.hi.clone(), right.hi_closed),
        (_, None) => (left.hi.clone(), left.hi_closed),
        (Some(a), Some(b)) if a < b => (left.hi.clone(), left.hi_closed),
        (Some(a), Some(b)) if a > b => (right.hi.clone(), right.hi_closed),
        _ => (left.hi.clone(), left.hi_closed && right.hi_closed),
    };
    Range {
        lo,
        hi,
        lo_closed,
        hi_closed,
    }
}

/// A `U`, or the word `union`, `or`, or `and`, between two intervals is the union sign.
fn ascii_union(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < chars.len() {
        let start = at;
        if chars[at].is_ascii_alphabetic() {
            while at < chars.len() && chars[at].is_ascii_alphabetic() {
                at += 1;
            }
            let word: String = chars[start..at].iter().collect();
            let joins =
                word == "U" || matches!(word.to_lowercase().as_str(), "union" | "or" | "and");
            let between = joins
                && chars[..start]
                    .iter()
                    .rev()
                    .find(|c| !c.is_whitespace())
                    .is_some_and(|c| matches!(c, ')' | ']' | '}'))
                && chars[at..]
                    .iter()
                    .find(|c| !c.is_whitespace())
                    .is_some_and(|c| matches!(c, '(' | '[' | '{'));
            out.push_str(if between { "∪" } else { &word });
        } else {
            out.push(chars[at]);
            at += 1;
        }
    }
    out
}

/// The spoken comparisons of a sentence in symbols: `y is greater than 0` is `y > 0`.
fn spoken_comparisons(text: &str) -> String {
    let mut out = text.to_owned();
    for (spoken, symbol) in [
        (" is greater than or equal to ", " >= "),
        (" is at least ", " >= "),
        (" is less than or equal to ", " <= "),
        (" is at most ", " <= "),
        (" is greater than ", " > "),
        (" is more than ", " > "),
        (" is less than ", " < "),
        (" is not equal to ", " != "),
        (" greater than or equal to ", " >= "),
        (" less than or equal to ", " <= "),
        (" greater than ", " > "),
        (" more than ", " > "),
        (" less than ", " < "),
        (" at least ", " >= "),
        (" at most ", " <= "),
    ] {
        out = out.replace(spoken, symbol);
    }
    out
}

/// The reading of a spoken or set-builder answer as symbols: `for s > 3`,
/// `s greater than 3`, `t between 2 and 4`, `{x : 0 < x <= 2}`, `x in (0, 2]`,
/// `120 cm <= x < 150 cm` and `|x| < 3`.
fn natural_text(text: &str) -> String {
    let mut text = text.trim().trim_end_matches('.').trim().to_owned();
    for lead in ["for ", "when ", "if ", "where "] {
        if let Some(rest) = text.strip_prefix(lead) {
            text = rest.trim().to_owned();
        }
    }
    if let Some(condition) = set_builder(&text) {
        text = condition;
    }
    if let Some(interval) = membership(&text) {
        return interval;
    }
    let text = between(&text);
    let text = spoken_comparisons(&text);
    let text = strip_units(&text);
    abs_branches(&text)
}

/// `{x : 0 < x <= 2}` and `{x | x > 3}` as their condition.
fn set_builder(text: &str) -> Option<String> {
    let inner = text.strip_prefix('{')?.strip_suffix('}')?;
    let at = inner.find([':', '|'])?;
    let name = inner[..at].trim();
    let condition = inner[at + 1..].trim();
    (!name.is_empty() && name.chars().all(char::is_alphabetic) && condition.contains(name))
        .then(|| condition.to_owned())
}

/// `x in [7, 8)` and `x ∈ (0, 2]` as the interval they name.
fn membership(text: &str) -> Option<String> {
    let (name, rest) = text.split_once(" in ").or_else(|| text.split_once(" ∈ "))?;
    let name = name.trim();
    let rest = rest.trim();
    (!name.is_empty()
        && name.chars().all(char::is_alphabetic)
        && rest.starts_with(['(', '['])
        && rest.ends_with([')', ']']))
    .then(|| rest.to_owned())
}

/// `t between 2 and 4` as `2 < t < 4`.
fn between(text: &str) -> String {
    let Some((name, rest)) = text.split_once(" between ") else {
        return text.to_owned();
    };
    let name = name.trim().trim_end_matches(" is").trim();
    match rest.split_once(" and ") {
        Some((low, high))
            if !name.is_empty()
                && name.chars().all(char::is_alphabetic)
                && !high.contains(" and ")
                && !high.contains(" or ") =>
        {
            format!("{} < {name} < {}", low.trim(), high.trim())
        }
        _ => text.to_owned(),
    }
}

/// Whether the text says every real number.
fn is_all_reals(text: &str) -> bool {
    let lower = text.trim().trim_end_matches('.').to_lowercase();
    let lower = lower
        .split_once(" is ")
        .or_else(|| lower.split_once(" can be "))
        .filter(|(name, _)| name.chars().all(char::is_alphabetic))
        .map_or(lower.as_str(), |(_, rest)| rest);
    matches!(
        lower,
        "all real numbers"
            | "all reals"
            | "every real number"
            | "any real number"
            | "all real values"
            | "real numbers"
            | "ℝ"
            | "\\mathbb{r}"
    )
}

/// The words of a unit that follow a number in a range, dropped: `120 cm <=
/// x < 150 cm` is `120 <= x < 150`. A one-letter unit stays, because the
/// letter may be the unknown.
fn strip_units(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    let mut out: Vec<&str> = Vec::with_capacity(words.len());
    for (at, word) in words.iter().enumerate() {
        let after_number = at > 0
            && words[at - 1]
                .chars()
                .last()
                .is_some_and(|c| c.is_ascii_digit());
        let unit = word.len() >= 2
            && word.chars().all(|c| c.is_ascii_alphabetic())
            && !matches!(*word, "or" | "and" | "in" | "pi" | "if" | "to")
            && crate::answer::unit::lookup(word).is_some();
        let alone = words
            .iter()
            .enumerate()
            .all(|(other, w)| other == at || w != word || after_number_at(&words, other));
        if after_number && unit && alone {
            continue;
        }
        out.push(word);
    }
    out.join(" ")
}

fn after_number_at(words: &[&str], at: usize) -> bool {
    at > 0
        && words[at - 1]
            .chars()
            .last()
            .is_some_and(|c| c.is_ascii_digit())
}

/// `|x| < 3` as `-3 < x < 3`, `|x| >= 3` as `x <= -3 or x >= 3`, and
/// `|x - 2| < 1` as `1 < x < 3`, for every `or` branch with no `and`.
fn abs_branches(text: &str) -> String {
    if !text.contains('|') {
        return text.to_owned();
    }
    text.split(" or ")
        .map(|branch| abs_branch(branch).unwrap_or_else(|| branch.to_owned()))
        .collect::<Vec<_>>()
        .join(" or ")
}

fn abs_branch(branch: &str) -> Option<String> {
    if branch.contains(" and ") || branch.matches('|').count() != 2 {
        return None;
    }
    let ops = ["<=", ">=", "<", ">"];
    let (at, op) = ops
        .iter()
        .filter_map(|op| branch.find(op).map(|at| (at, *op)))
        .min_by_key(|(at, op)| (*at, std::cmp::Reverse(op.len())))?;
    let (left, right) = (branch[..at].trim(), branch[at + op.len()..].trim());
    // `3 > |x|` has the bars on the right; flip it.
    let (bars, bound, op) = if left.starts_with('|') && left.ends_with('|') {
        (left, right, op)
    } else if right.starts_with('|') && right.ends_with('|') {
        let flipped = match op {
            "<" => ">",
            "<=" => ">=",
            ">" => "<",
            _ => "<=",
        };
        (right, left, flipped)
    } else {
        return None;
    };
    if bound.contains(['|', '<', '>']) {
        return None;
    }
    let inner = bars[1..bars.len() - 1].trim();
    let name_end = inner
        .find(|c: char| !c.is_alphabetic())
        .unwrap_or(inner.len());
    let name = &inner[..name_end];
    if name.is_empty() || bound.contains(name) {
        return None;
    }
    let tail = inner[name_end..].trim();
    let center = match tail {
        "" => "0".to_owned(),
        _ => {
            let (sign, number) = tail.split_at(1);
            match sign {
                "-" => format!("({})", number.trim()),
                "+" => format!("(-({}))", number.trim()),
                _ => return None,
            }
        }
    };
    let low = format!("{center} - ({bound})");
    let high = format!("{center} + ({bound})");
    let closed = op.ends_with('=');
    Some(match (op.starts_with('<'), closed) {
        (true, false) => format!("{low} < {name} < {high}"),
        (true, true) => format!("{low} <= {name} <= {high}"),
        (false, false) => format!("{name} < {low} or {name} > {high}"),
        (false, true) => format!("{name} <= {low} or {name} >= {high}"),
    })
}

/// A set of every number but a few points: `x != 0`, `x ≠ 2, 5`,
/// `x != 2 and x != 5`, `all real numbers except 3`. `None` when the text is
/// another notation. The variable is `None` for the sentence form.
fn excluded_points(text: &str) -> Option<Result<Excluded, Undecidable>> {
    let text = text.replace('≠', "!=");
    let lower = text.to_lowercase();
    let (var, list) = if let Some(rest) = [
        "all real numbers except ",
        "all reals except ",
        "every real number except ",
    ]
    .iter()
    .find_map(|lead| lower.strip_prefix(lead).map(|_| &text[lead.len()..]))
    {
        (None, rest.to_owned())
    } else if text.contains("!=") && !text.contains(['<', '>', '='])
        || text.matches("!=").count() > 0 && !text.contains(['<', '>'])
    {
        let mut var: Option<String> = None;
        let mut values = Vec::new();
        for piece in text.split(" and ") {
            let (name, rest) = piece.split_once("!=")?;
            let (name, rest) = swap_point_first(name.trim(), rest.trim());
            let name = &spelled_greek(name);
            let is_name = !name.is_empty() && name.chars().all(char::is_alphabetic);
            if is_name {
                if var.as_ref().is_some_and(|known| known != name) {
                    return Some(Err(refused()));
                }
                var = Some(name.to_owned());
            } else if !name.is_empty() {
                return Some(Err(refused()));
            }
            values.push(rest.trim().to_owned());
        }
        (var, values.join(","))
    } else {
        return None;
    };
    Some(points_complement(var, &list))
}

/// `0 != x` is `x != 0`: a number first and a lone name after the sign.
fn swap_point_first<'a>(first: &'a str, second: &'a str) -> (&'a str, &'a str) {
    let number = !first.is_empty() && first.chars().all(|c| c.is_ascii_digit() || c == '.');
    let lone = !second.is_empty() && second.chars().all(char::is_alphabetic);
    if number && lone {
        (second, first)
    } else {
        (first, second)
    }
}

/// A Greek glyph written as the name the reader gives it.
fn spelled_greek(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'θ' => "theta".to_owned(),
            'α' => "alpha".to_owned(),
            'β' => "beta".to_owned(),
            'λ' => "lamda".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

/// The complement of the listed points, as ranges.
fn points_complement(var: Option<String>, list: &str) -> Result<Excluded, Undecidable> {
    let mut points = Vec::new();
    for piece in list.replace(" and ", ",").split(',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        points.push(Bound::of(canonical_form(piece)?)?);
    }
    points.sort();
    points.dedup();
    if points.is_empty() || points.len() > 15 {
        return Err(refused());
    }
    let mut ranges = Vec::new();
    let mut lo: Option<Bound> = None;
    for point in points {
        ranges.push(Range {
            lo: lo.clone(),
            hi: Some(point.clone()),
            lo_closed: false,
            hi_closed: false,
        });
        lo = Some(point);
    }
    ranges.push(Range {
        lo,
        hi: None,
        lo_closed: false,
        hi_closed: false,
    });
    Ok((var, merge(ranges)))
}

/// Read only the LaTeX control words that are tokens of this contract's grammar.
///
/// The general answer lexer already treats `\left` and `\right` as structural
/// tokens instead of rewriting arbitrary substrings. Inequality unions have a
/// separate parser because infinity and union are not finite expression nodes,
/// so their structural controls are handled with the same exact-token rule here.
fn notation_source(text: &str) -> String {
    const CONTROLS: [(&str, &str); 9] = [
        (r"\infty", "∞"),
        (r"\right", ""),
        (r"\left", ""),
        (r"\cup", "∪"),
        (r"\lor", " or "),
        (r"\leq", "<="),
        (r"\geq", ">="),
        (r"\le", "<="),
        (r"\ge", ">="),
    ];
    let barred = bar_letters(text);
    let mut rest = barred.as_str();
    let mut out = String::with_capacity(text.len());
    while !rest.is_empty() {
        if let Some((token, replacement)) = CONTROLS.iter().find(|(token, _)| {
            rest.strip_prefix(token).is_some_and(|after| {
                after
                    .chars()
                    .next()
                    .is_none_or(|next| !next.is_ascii_alphabetic())
            })
        }) {
            out.push_str(replacement);
            rest = &rest[token.len()..];
            continue;
        }
        let Some(next) = rest.chars().next() else {
            break;
        };
        out.push(next);
        rest = &rest[next.len_utf8()..];
    }
    out
}

/// A letter with a bar above it (`x̄`) as the subscripted name `x_0`, which the
/// answer grammar reads as one unknown.
fn bar_letters(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\u{304}' | '\u{305}' | '\u{0332}') {
            out.push_str("_0");
        } else {
            out.push(c);
        }
    }
    out
}

fn interval_notation(text: &str) -> Result<Canon, Undecidable> {
    let branches: Vec<_> = text.split('∪').map(str::trim).collect();
    if branches.is_empty() || branches.len() > 16 {
        return Err(refused());
    }
    let mut ranges = Vec::with_capacity(branches.len());
    for branch in branches {
        let range = interval(branch)?;
        if !nonempty(&range) {
            return Err(refused());
        }
        ranges.push(range);
    }
    Ok(Canon::List(canonical_ranges(merge(ranges))))
}

fn parts(value: &Canon) -> (Option<&str>, &Canon) {
    match value {
        Canon::Assign { var, value } => (Some(var.as_str()), value),
        value => (None, value),
    }
}

/// Whether the text is a line without a few points (`x ≠ 4`) that this module reads.
pub(super) fn excludes(text: &str) -> bool {
    text.contains(['≠']) && read(text).is_ok() || text.contains("!=") && read(text).is_ok()
}

/// The spoken comparisons of a learner answer in symbols, for a reader that
/// takes `s > 3` and not `for s greater than 3`.
pub(super) fn spoken(text: &str) -> String {
    let text = text.trim().trim_end_matches('.').trim();
    let text = ["for ", "when ", "if ", "where "]
        .iter()
        .find_map(|lead| text.strip_prefix(lead))
        .unwrap_or(text);
    spoken_comparisons(text)
}

/// Whether the learner range is the key's upper half line with the lower end 0
/// added: key `t < 12`, learner `0 <= t < 12`. The key does not state a lower
/// bound, so the pair is not a wrong answer, and a grader without the question
/// text cannot say whether the item asks for the bound.
pub(super) fn adds_zero_bound(key: &Canon, learner: &Canon) -> bool {
    let ((key_var, key_ranges), (learner_var, learner_ranges)) = (parts(key), parts(learner));
    if let (Some(left), Some(right)) = (key_var, learner_var)
        && left != right
    {
        return false;
    }
    let (Canon::List(key_ranges), Canon::List(learner_ranges)) = (key_ranges, learner_ranges)
    else {
        return false;
    };
    let (
        [
            Canon::Interval {
                lo: None,
                hi: Some(key_hi),
                hi_closed: key_closed,
                ..
            },
        ],
        [
            Canon::Interval {
                lo: Some(lo),
                hi: Some(learner_hi),
                hi_closed: learner_closed,
                ..
            },
        ],
    ) = (key_ranges.as_slice(), learner_ranges.as_slice())
    else {
        return false;
    };
    matches!(**lo, Canon::Rational(ref zero) if num_traits::Zero::is_zero(zero))
        && key_hi == learner_hi
        && key_closed == learner_closed
        && matches!(**key_hi, Canon::Rational(ref top) if top.numer().sign() == num_bigint::Sign::Plus)
}

pub(super) fn equivalent(left: &Canon, right: &Canon) -> bool {
    let (left_var, left_ranges) = parts(left);
    let (right_var, right_ranges) = parts(right);
    if let (Some(left), Some(right)) = (left_var, right_var)
        && left != right
    {
        return false;
    }
    left_ranges == right_ranges
}

fn merge(mut ranges: Vec<Range>) -> Vec<Range> {
    ranges.sort_by(|left, right| {
        left.lo
            .cmp(&right.lo)
            .then_with(|| right.lo_closed.cmp(&left.lo_closed))
    });
    let mut merged: Vec<Range> = Vec::new();
    for range in ranges {
        if let Some(last) = merged.last_mut()
            && touches(last, &range)
        {
            extend(last, &range);
        } else {
            merged.push(range);
        }
    }
    merged
}

fn canonical_ranges(ranges: Vec<Range>) -> Vec<Canon> {
    ranges
        .into_iter()
        .map(|range| Canon::Interval {
            var: None,
            lo: boxed(range.lo),
            hi: boxed(range.hi),
            lo_closed: range.lo_closed,
            hi_closed: range.hi_closed,
        })
        .collect()
}

fn interval(text: &str) -> Result<Range, Undecidable> {
    let text = text.trim();
    if let Some(point) = text
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
    {
        // `{3}` is the one point 3.
        let point = Bound::of(canonical_form(point.trim())?)?;
        return Ok(Range {
            lo: Some(point.clone()),
            hi: Some(point),
            lo_closed: true,
            hi_closed: true,
        });
    }
    let lo_closed = text.starts_with('[');
    let hi_closed = text.ends_with(']');
    if !(lo_closed || text.starts_with('(')) || !(hi_closed || text.ends_with(')')) {
        return Err(refused());
    }
    let inner = &text[1..text.len() - 1];
    let (lo, hi) = inner.split_once(',').ok_or_else(refused)?;
    if hi.contains(',') {
        return Err(refused());
    }
    let lo = endpoint(lo, true)?;
    let hi = endpoint(hi, false)?;
    if (lo.is_none() && lo_closed) || (hi.is_none() && hi_closed) {
        return Err(refused());
    }
    Ok(Range {
        lo,
        hi,
        lo_closed,
        hi_closed,
    })
}

fn endpoint(text: &str, lower: bool) -> Result<Option<Bound>, Undecidable> {
    let text = text.trim();
    let infinite = if lower {
        matches!(text, "-∞" | "-infinity" | "-oo" | "-inf")
    } else {
        matches!(
            text,
            "∞" | "+∞" | "infinity" | "+infinity" | "oo" | "+oo" | "inf" | "+inf"
        )
    };
    if infinite {
        return Ok(None);
    }
    Bound::of(canonical_form(text)?).map(Some)
}

fn refused() -> Undecidable {
    Undecidable::new("an inequality union requires one unknown and at most 16 rational intervals")
}

fn bound(value: Option<Box<Canon>>) -> Result<Option<Bound>, Undecidable> {
    value.map(|value| Bound::of(*value)).transpose()
}

fn boxed(value: Option<Bound>) -> Option<Box<Canon>> {
    value.map(|value| Box::new(value.value))
}

fn nonempty(range: &Range) -> bool {
    match (&range.lo, &range.hi) {
        (Some(lo), Some(hi)) => lo < hi || lo == hi && range.lo_closed && range.hi_closed,
        _ => true,
    }
}

fn touches(left: &Range, right: &Range) -> bool {
    match (&left.hi, &right.lo) {
        (Some(hi), Some(lo)) => hi > lo || hi == lo && (left.hi_closed || right.lo_closed),
        _ => true,
    }
}

fn extend(left: &mut Range, right: &Range) {
    match (&left.hi, &right.hi) {
        (None, _) => {}
        (_, None) => {
            left.hi = None;
            left.hi_closed = false;
        }
        (Some(left_hi), Some(right_hi)) if right_hi > left_hi => {
            left.hi = right.hi.clone();
            left.hi_closed = right.hi_closed;
        }
        (Some(left_hi), Some(right_hi)) if right_hi == left_hi => left.hi_closed |= right.hi_closed,
        _ => {}
    }
}
