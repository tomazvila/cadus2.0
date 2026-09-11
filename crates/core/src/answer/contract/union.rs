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
        let Canon::Interval {
            var: Some(var),
            lo,
            hi,
            lo_closed,
            hi_closed,
        } = canonical_form(branch)?
        else {
            return Err(refused());
        };
        if variable.as_ref().is_some_and(|name| name != &var) {
            return Err(refused());
        }
        variable = Some(var);
        let range = Range {
            lo: rational(lo)?,
            hi: rational(hi)?,
            lo_closed,
            hi_closed,
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
        matches!(text, "-∞" | "-infinity" | "-oo")
    } else {
        matches!(text, "∞" | "+∞" | "infinity" | "+infinity" | "oo" | "+oo")
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
