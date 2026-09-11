//! Bounded writers for structured closed-label answers.

use num_traits::ToPrimitive;

use crate::answer::ast::Ast;
use crate::answer::{AnswerContract, Canon};
use crate::template::domain::Bindings;

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
        ("divisibilitylabel", [number, divisor]) => {
            let number = bounded_whole(number, bindings, "divisibilitylabel")?;
            let divisor = bounded_whole(divisor, bindings, "divisibilitylabel")?;
            if divisor == 0 {
                return Err(EvalError::Domain {
                    func: "divisibilitylabel",
                    value: divisor.to_string(),
                });
            }
            if number % divisor == 0 { "yes" } else { "no" }
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
        _ => return answer(ast, bindings),
    };
    contracted(text.to_owned(), contract)
}

const MAX_LABEL_INTEGER: u32 = 1_000_000;

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
    if number % 2 == 0 {
        return false;
    }
    let mut divisor = 3;
    while divisor <= number / divisor {
        if number % divisor == 0 {
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
        if number % divisor == 0 {
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
        while number % divisor == 0 {
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
