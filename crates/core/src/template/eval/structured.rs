//! Bounded writers for structured closed-label answers.

use num_traits::ToPrimitive;

use crate::answer::ast::Ast;
use crate::answer::{AnswerContract, Canon};
use crate::template::{MAX_EXPONENT, domain::Bindings};

use super::{Answer, EvalError, answer, contracted, text_binding};

pub(super) fn label_answer(
    ast: &Ast,
    bindings: &Bindings,
    contract: &AnswerContract,
) -> Result<Answer, EvalError> {
    if let Some(text) = text_binding(ast, bindings) {
        return contracted(text, contract);
    }
    let Ast::Func(name, args) = ast else {
        return answer(ast, bindings);
    };
    let text = match (name.as_str(), args.as_slice()) {
        ("equalitylabel", [left, right]) => {
            if answer(left, bindings)?.canon == answer(right, bindings)?.canon {
                "yes"
            } else {
                "no"
            }
        }
        ("divisibilitylabel", [number, divisor]) => {
            let number = bounded_whole(number, bindings, "divisibilitylabel")?;
            let divisor = bounded_whole(divisor, bindings, "divisibilitylabel")?;
            if divisor == 0 {
                return Err(EvalError::Domain {
                    func: "divisibilitylabel",
                    value: divisor.to_string(),
                });
            }
            if number.is_multiple_of(divisor) {
                "yes"
            } else {
                "no"
            }
        }
        ("primeclass", [number]) => {
            let number = bounded_whole(number, bindings, "primeclass")?;
            if number < 2 {
                "neither"
            } else if is_prime(number) {
                "prime"
            } else {
                "composite"
            }
        }
        ("linearclass", [Ast::Tuple(left), Ast::Tuple(right)])
            if left.len() == 2 && right.len() == 2 =>
        {
            linear_class(&left[0], &left[1], &right[0], &right[1], bindings)?
        }
        _ => return answer(ast, bindings),
    };
    contracted(text.to_owned(), contract)
}

fn linear_class(
    left_coefficient: &Ast,
    left_constant: &Ast,
    right_coefficient: &Ast,
    right_constant: &Ast,
    bindings: &Bindings,
) -> Result<&'static str, EvalError> {
    let left_coefficient = bounded_integer(left_coefficient, bindings, "linearclass")?;
    let left_constant = bounded_integer(left_constant, bindings, "linearclass")?;
    let right_coefficient = bounded_integer(right_coefficient, bindings, "linearclass")?;
    let right_constant = bounded_integer(right_constant, bindings, "linearclass")?;
    Ok(if left_coefficient != right_coefficient {
        "one solution"
    } else if left_constant == right_constant {
        "all real numbers"
    } else {
        "no solution"
    })
}

const MAX_LABEL_INTEGER: u32 = 1_000_000;

fn bounded_integer(ast: &Ast, bindings: &Bindings, func: &'static str) -> Result<i64, EvalError> {
    let value = answer(ast, bindings)?;
    let Canon::Rational(value) = value.canon else {
        return Err(EvalError::NotWhole { func });
    };
    value
        .to_integer()
        .to_i64()
        .filter(|integer| {
            value.is_integer() && integer.unsigned_abs() <= u64::from(MAX_LABEL_INTEGER)
        })
        .ok_or_else(|| EvalError::Domain {
            func,
            value: value.to_string(),
        })
}

fn bounded_whole(ast: &Ast, bindings: &Bindings, func: &'static str) -> Result<u32, EvalError> {
    let value = answer(ast, bindings)?;
    let Canon::Rational(value) = value.canon else {
        return Err(EvalError::NotWhole { func });
    };
    if !value.is_integer() {
        return Err(EvalError::NotWhole { func });
    }
    value
        .to_integer()
        .to_u32()
        .filter(|value| *value <= MAX_LABEL_INTEGER)
        .ok_or_else(|| EvalError::Domain {
            func,
            value: value.to_string(),
        })
}

fn is_prime(number: u32) -> bool {
    if number < 2 {
        return false;
    }
    if number == 2 {
        return true;
    }
    if number.is_multiple_of(2) {
        return false;
    }
    let mut divisor = 3;
    while divisor <= number / divisor {
        if number.is_multiple_of(divisor) {
            return false;
        }
        divisor += 2;
    }
    true
}

pub(super) fn list_answer(
    ast: &Ast,
    bindings: &Bindings,
    contract: &AnswerContract,
) -> Result<Answer, EvalError> {
    let Ast::Func(name, args) = ast else {
        return answer(ast, bindings);
    };
    let items = match (name.as_str(), args.as_slice()) {
        ("factorlist", [number]) => factors(bounded_whole(number, bindings, "factorlist")?),
        ("firstmultiples", [number, count]) => {
            let number = bounded_whole(number, bindings, "firstmultiples")?;
            let count = bounded_count(count, bindings, "firstmultiples")?;
            (1..=count)
                .map(|item| number.checked_mul(item).ok_or(EvalError::TooWide))
                .collect::<Result<Vec<_>, _>>()?
        }
        ("primefactors", [number]) => {
            prime_factors(bounded_whole(number, bindings, "primefactors")?)?
        }
        ("repeatedfactors", [number, count]) => {
            let number = bounded_whole(number, bindings, "repeatedfactors")?;
            let count = bounded_count(count, bindings, "repeatedfactors")?;
            vec![number; count as usize]
        }
        _ => return answer(ast, bindings),
    };
    contracted(
        items
            .into_iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>()
            .join(", "),
        contract,
    )
}

fn bounded_count(ast: &Ast, bindings: &Bindings, func: &'static str) -> Result<u32, EvalError> {
    let count = bounded_whole(ast, bindings, func)?;
    if (1..=32).contains(&count) {
        Ok(count)
    } else {
        Err(EvalError::Domain {
            func,
            value: count.to_string(),
        })
    }
}

fn factors(number: u32) -> Vec<u32> {
    let mut low = Vec::new();
    let mut high = Vec::new();
    let mut divisor = 1;
    while divisor <= number / divisor {
        if number.is_multiple_of(divisor) {
            low.push(divisor);
            let partner = number / divisor;
            if partner != divisor {
                high.push(partner);
            }
        }
        divisor += 1;
    }
    high.reverse();
    low.extend(high);
    low
}

fn prime_factors(mut number: u32) -> Result<Vec<u32>, EvalError> {
    if number < 2 {
        return Err(EvalError::Domain {
            func: "primefactors",
            value: number.to_string(),
        });
    }
    let mut factors = Vec::new();
    let mut divisor = 2;
    while divisor <= number / divisor {
        while number.is_multiple_of(divisor) {
            factors.push(divisor);
            number /= divisor;
        }
        divisor += if divisor == 2 { 1 } else { 2 };
    }
    if number > 1 {
        factors.push(number);
    }
    Ok(factors)
}

/// Write `coefficient * base^exponent` without evaluating away the power.
pub(super) fn power_form(
    ast: &Ast,
    bindings: &Bindings,
    contract: Option<&AnswerContract>,
) -> Result<Answer, EvalError> {
    if contract != Some(&AnswerContract::Exact) {
        return Err(EvalError::NotNumber {
            func: "powerform requires exact contract",
        });
    }
    let Ast::Func(_, args) = ast else {
        unreachable!();
    };
    let [coefficient, Ast::List(parts)] = args.as_slice() else {
        return Err(EvalError::Arity {
            func: "powerform".to_owned(),
            want: 2,
            given: args.len(),
        });
    };
    let [base, exponent] = parts.as_slice() else {
        return Err(EvalError::NotNumber {
            func: "powerform requires [base, exponent]",
        });
    };
    let coefficient = numeric(coefficient, bindings)?;
    let base = numeric(base, bindings)?;
    let exponent = numeric(exponent, bindings)?;
    let Canon::Rational(number) = &exponent.canon else {
        unreachable!()
    };
    let power = number
        .to_integer()
        .to_i64()
        .filter(|power| number.is_integer() && power.unsigned_abs() <= MAX_EXPONENT as u64)
        .ok_or(EvalError::NotWhole {
            func: "powerform bounded exponent",
        })?;
    contracted(
        format!("({})*({})^({power})", coefficient.text, base.text),
        &AnswerContract::Exact,
    )
}

fn numeric(ast: &Ast, bindings: &Bindings) -> Result<Answer, EvalError> {
    let value = answer(ast, bindings)?;
    if !matches!(value.canon, Canon::Rational(_)) {
        return Err(EvalError::NotNumber { func: "powerform" });
    }
    Ok(value)
}
