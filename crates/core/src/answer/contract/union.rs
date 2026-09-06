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

pub(super) fn read(text: &str) -> Result<Canon, Undecidable> {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
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
    let Some(var) = variable else {
        return Err(refused());
    };
    Ok(Canon::Assign {
        var,
        value: Box::new(Canon::List(
            merged
                .into_iter()
                .map(|range| Canon::Interval {
                    var: None,
                    lo: boxed(range.lo),
                    hi: boxed(range.hi),
                    lo_closed: range.lo_closed,
                    hi_closed: range.hi_closed,
                })
                .collect(),
        )),
    })
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
