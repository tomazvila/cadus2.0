//! Exact finite unions of rational intervals over one unknown.

use num_rational::BigRational;

use super::{Canon, Undecidable, canonical_form};

#[derive(Clone)]
struct Range {
    lo: Option<BigRational>,
    hi: Option<BigRational>,
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
    let normalized = spoken_comparisons(&ascii_union(&normalized));
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
        lo: rational(lo)?,
        hi: rational(hi)?,
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

/// An ASCII `U` between two intervals is the union sign.
fn ascii_union(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (at, c) in chars.iter().enumerate() {
        let between = *c == 'U'
            && chars[..at]
                .iter()
                .rev()
                .find(|c| !c.is_whitespace())
                .is_some_and(|c| matches!(c, ')' | ']'))
            && chars[at + 1..]
                .iter()
                .find(|c| !c.is_whitespace())
                .is_some_and(|c| matches!(c, '(' | '['));
        out.push(if between { '∪' } else { *c });
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
    ] {
        out = out.replace(spoken, symbol);
    }
    out
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
            let name = name.trim();
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

/// The complement of the listed points, as ranges.
fn points_complement(var: Option<String>, list: &str) -> Result<Excluded, Undecidable> {
    let mut points = Vec::new();
    for piece in list.replace(" and ", ",").split(',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        match canonical_form(piece)? {
            Canon::Rational(value) => points.push(value),
            _ => return Err(refused()),
        }
    }
    points.sort();
    points.dedup();
    if points.is_empty() || points.len() > 15 {
        return Err(refused());
    }
    let mut ranges = Vec::new();
    let mut lo: Option<BigRational> = None;
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
    let mut rest = text;
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

pub(super) fn equivalent(left: &Canon, right: &Canon) -> bool {
    fn parts(value: &Canon) -> (Option<&str>, &Canon) {
        match value {
            Canon::Assign { var, value } => (Some(var.as_str()), value),
            value => (None, value),
        }
    }
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

fn endpoint(text: &str, lower: bool) -> Result<Option<BigRational>, Undecidable> {
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
    match canonical_form(text)? {
        Canon::Rational(value) => Ok(Some(value)),
        _ => Err(refused()),
    }
}

fn refused() -> Undecidable {
    Undecidable::new("an inequality union requires one unknown and at most 16 rational intervals")
}

fn rational(value: Option<Box<Canon>>) -> Result<Option<BigRational>, Undecidable> {
    match value {
        None => Ok(None),
        Some(value) => match *value {
            Canon::Rational(number) => Ok(Some(number)),
            _ => Err(refused()),
        },
    }
}

fn boxed(value: Option<BigRational>) -> Option<Box<Canon>> {
    value.map(|value| Box::new(Canon::Rational(value)))
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
