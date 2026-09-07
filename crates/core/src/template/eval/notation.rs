//! Convert bounded, parsed ray premises between inequality and interval notation.
use super::{Answer, EvalError, answer, contracted, text_binding};
use crate::{
    answer::{AnswerContract, Canon, ast::Ast},
    template::domain::Bindings,
};

pub(super) fn convert(
    args: &[Ast],
    bindings: &Bindings,
    contract: &AnswerContract,
) -> Result<Answer, EvalError> {
    let [source] = args else {
        return Err(refused());
    };
    let source = text_binding(source, bindings).ok_or_else(refused)?;
    if source.len() > 512 {
        return Err(refused());
    }
    let parsed = contract.validate_expected(&source).map_err(|_| refused())?;
    let (interval_input, ranges) = match parsed {
        Canon::Assign { var, value } if var == "x" => (false, *value),
        value @ Canon::List(_) => (true, value),
        _ => return Err(refused()),
    };
    let Canon::List(ranges) = ranges else {
        return Err(refused());
    };
    if ranges.is_empty() || ranges.len() > 2 {
        return Err(refused());
    }
    let parts = ranges
        .iter()
        .map(|range| part(range, interval_input))
        .collect::<Result<Vec<_>, _>>()?;
    contracted(
        parts.join(if interval_input { " or " } else { " ∪ " }),
        contract,
    )
}

fn part(range: &Canon, inequality: bool) -> Result<String, EvalError> {
    let Canon::Interval {
        lo,
        hi,
        lo_closed,
        hi_closed,
        ..
    } = range
    else {
        return Err(refused());
    };
    match (lo, hi) {
        (None, Some(bound)) => {
            let bound = rational(bound)?;
            Ok(if inequality {
                format!("x {} {bound}", if *hi_closed { "<=" } else { "<" })
            } else {
                format!("(-∞, {bound}{}", if *hi_closed { ']' } else { ')' })
            })
        }
        (Some(bound), None) => {
            let bound = rational(bound)?;
            Ok(if inequality {
                format!("x {} {bound}", if *lo_closed { ">=" } else { ">" })
            } else {
                format!("{}{bound}, ∞)", if *lo_closed { '[' } else { '(' })
            })
        }
        _ => Err(refused()),
    }
}

fn rational(value: &Canon) -> Result<String, EvalError> {
    if let Canon::Rational(value) = value {
        Ok(value.to_string())
    } else {
        Err(refused())
    }
}

fn refused() -> EvalError {
    EvalError::NotNumber {
        func: "convertnotation requires one or two proper rational rays in x",
    }
}

pub(super) fn ascending_chain(
    args: &[Ast],
    bindings: &Bindings,
    contract: Option<&AnswerContract>,
) -> Result<Answer, EvalError> {
    let Some(contract @ AnswerContract::AscendingChain) = contract else {
        return Err(EvalError::NotNumber {
            func: "ascendingchain requires ascending_chain contract",
        });
    };
    let [Ast::List(items)] = args else {
        return Err(EvalError::NotNumber {
            func: "ascendingchain requires a list",
        });
    };
    if !(2..=16).contains(&items.len()) {
        return Err(EvalError::NotNumber {
            func: "ascendingchain requires two to sixteen numbers",
        });
    }
    let mut values = items
        .iter()
        .map(|item| match answer(item, bindings)?.canon {
            Canon::Rational(number) => Ok(number),
            _ => Err(EvalError::NotNumber {
                func: "ascendingchain",
            }),
        })
        .collect::<Result<Vec<_>, _>>()?;
    values.sort();
    contracted(
        values
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" < "),
        contract,
    )
}
