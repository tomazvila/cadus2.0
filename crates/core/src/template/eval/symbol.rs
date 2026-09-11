//! Explicit symbolic operands drawn from a closed, visible text-choice domain.
use super::EvalError;
use crate::answer::ast::Ast;
use crate::template::domain::{Bindings, Value};

/// Emit one conventional variable; raw expressions and reserved constants fail closed.
pub(super) fn variable(args: &[Ast], bindings: &Bindings) -> Result<Ast, EvalError> {
    let [Ast::Var(parameter)] = args else {
        return Err(shape());
    };
    let Some(Value::Text(name)) = bindings.get(parameter) else {
        return Err(shape());
    };
    if !matches!(
        name.as_str(),
        "u" | "v" | "w" | "x" | "y" | "z" | "k" | "n" | "r" | "t" | "theta"
    ) {
        return Err(shape());
    }
    Ok(Ast::Var(name.clone()))
}

fn shape() -> EvalError {
    EvalError::NotNumber {
        func: "symbol requires a text-choice parameter naming u,v,w,x,y,z,k,n,r,t,theta",
    }
}
