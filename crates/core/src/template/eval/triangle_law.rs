//! Choose a starting law from exactly three named measurements of a triangle.
use super::{EvalError, text_binding};
use crate::answer::ast::Ast;
use crate::template::domain::Bindings;
use std::collections::BTreeSet;

pub(super) fn choose(args: &[Ast], bindings: &Bindings) -> Result<&'static str, EvalError> {
    let [given] = args else {
        return Err(EvalError::Arity {
            func: "trianglelaw".to_owned(),
            want: 1,
            given: args.len(),
        });
    };
    let given = text_binding(given, bindings).ok_or_else(|| domain("measurements must be text"))?;
    let tokens: Vec<_> = given.split(',').map(str::trim).collect();
    let unique: BTreeSet<_> = tokens.iter().copied().collect();
    if tokens.len() != 3
        || unique.len() != 3
        || unique
            .iter()
            .any(|x| !["a", "b", "c", "A", "B", "C"].contains(x))
    {
        return Err(domain(
            "exactly three distinct measurements a,b,c,A,B,C are required",
        ));
    }
    let sides: Vec<_> = unique
        .iter()
        .filter(|x| ["a", "b", "c"].contains(x))
        .copied()
        .collect();
    match sides.len() {
        3 => Ok("Law of Cosines"),
        1 => Ok("Law of Sines"),
        2 => {
            let missing = ["a", "b", "c"]
                .into_iter()
                .find(|x| !unique.contains(x))
                .ok_or_else(|| domain("missing third side"))?;
            if unique.contains(missing.to_ascii_uppercase().as_str()) {
                Ok("Law of Cosines")
            } else {
                // SSA identifies the sine rule; determining the number of
                // triangles requires numerical data and is not claimed here.
                Ok("Law of Sines")
            }
        }
        _ => Err(domain("angles alone do not determine side lengths")),
    }
}

fn domain(value: &str) -> EvalError {
    EvalError::Domain {
        func: "trianglelaw",
        value: value.to_owned(),
    }
}
