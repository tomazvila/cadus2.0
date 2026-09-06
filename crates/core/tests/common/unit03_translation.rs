//! Independent sentence reconstruction for the Unit03 translation content.
#![allow(clippy::panic)]

/// Read only learner-visible prose and math spans, with no answer or bindings.
pub fn reconstruct(sentence: &str) -> (String, String) {
    let pieces: Vec<_> = sentence.split('$').collect();
    let values: Vec<_> = pieces.iter().skip(1).step_by(2).copied().collect();
    let shape = pieces
        .iter()
        .step_by(2)
        .copied()
        .collect::<Vec<_>>()
        .join("{}");
    let shape = shape.split(". Use *").next().unwrap_or(&shape);
    match (shape, values.as_slice()) {
        ("Write an equation: {} more than {} times a number {} is {}", [b, a, x, c]) => (
            format!("{a}*{x} + {b} = {c}"),
            format!("{a}*({x} + {b}) = {c}"),
        ),
        ("Write an equation: the quotient of a number {} and {} is {}", [x, a, c]) => {
            (format!("{x}/{a} = {c}"), format!("{a}/{x} = {c}"))
        }
        ("Write an equation: {} times the sum of a number {} and {} is {}", [a, x, b, c]) => (
            format!("{a}*({x} + {b}) = {c}"),
            format!("{a}*{x} + {b} = {c}"),
        ),
        (
            "Write an equation: {} less than the quotient of a number {} and {} is {}",
            [b, x, a, c],
        ) => (
            format!("{x}/{a} - {b} = {c}"),
            format!("({x} - {b})/{a} = {c}"),
        ),
        (
            "Write an equation: the difference of {} and {} times a number {} is {}",
            [b, a, x, c],
        ) => (
            format!("{b} - {a}*{x} = {c}"),
            format!("{a}*{x} - {b} = {c}"),
        ),
        (
            "Write an equation: the quotient of the difference of a number {} and {}, and {}, is {}",
            [x, b, a, c],
        ) => (
            format!("({x} - {b})/{a} = {c}"),
            format!("{x} - {b}/{a} = {c}"),
        ),
        _ => panic!("unsupported visible sentence: {sentence}"),
    }
}
