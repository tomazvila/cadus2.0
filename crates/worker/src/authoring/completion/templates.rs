//! Exact template recipes for closed families that need coordinated operands.
use crate::authoring::prompt::AuthoringSpec;
use cadus_core::template::{Bindings, Scalar, answer, parse_answer_expr};
use serde_json::{Map, Value, json};

struct Recipe {
    statement: &'static str,
    answer_expr: &'static str,
    values: Vec<Scalar>,
}

fn integers(values: impl IntoIterator<Item = i64>) -> Vec<Scalar> {
    values.into_iter().map(Scalar::Int).collect()
}

fn decimals(values: &[&str]) -> Vec<Scalar> {
    values
        .iter()
        .map(|value| Scalar::Text((*value).to_owned()))
        .collect()
}

fn squares(limit: i64) -> Vec<Scalar> {
    (1..)
        .map(|value| value * value)
        .take_while(|value| *value <= limit)
        .map(Scalar::Int)
        .collect()
}

fn cubes(count: i64) -> Vec<Scalar> {
    (1..=count)
        .map(|value| Scalar::Int(value * value * value))
        .collect()
}

fn recipe(key: &str) -> Option<Recipe> {
    arithmetic_recipe(key).or_else(|| radical_recipe(key))
}

fn arithmetic_recipe(key: &str) -> Option<Recipe> {
    let item = match key {
        "mixed-numbers/kp1" => Recipe {
            statement: r"Compute ${a}\frac{{1}}{{2}} + 2\frac{{1}}{{4}}$.",
            answer_expr: "a + 1/2 + 2 + 1/4",
            values: integers(3..=15),
        },
        "mixed-numbers/kp2" => Recipe {
            statement: r"Compute ${a}\frac{{1}}{{3}} - 1\frac{{2}}{{3}}$.",
            answer_expr: "a + 1/3 - 1 - 2/3",
            values: integers((2..=14).filter(|value| *value != 3)),
        },
        "mixed-numbers/kp3" => Recipe {
            statement: r"Compute ${a}\frac{{1}}{{2}} \times \frac{{2}}{{3}}$.",
            answer_expr: "(a + 1/2) * 2/3",
            values: integers(2..=14),
        },
        "negative-fractions-decimals/kp3" => Recipe {
            statement: r"Compute $-{a}\frac{{1}}{{2}} + 2\frac{{1}}{{4}}$.",
            answer_expr: "-(a + 1/2) + 2 + 1/4",
            values: integers(2..=14),
        },
        "decimal-multiplication-powers-of-ten/kp1" => Recipe {
            statement: r"Compute ${a} \times 10$.",
            answer_expr: "a * 10",
            values: decimals(&[
                "0.10", "0.11", "0.12", "0.13", "0.14", "0.15", "0.16", "0.17", "0.18", "0.19",
                "0.20", "0.21",
            ]),
        },
        "decimal-multiplication-powers-of-ten/kp3" => Recipe {
            statement: r"Compute ${a} \times 1000$.",
            answer_expr: "a * 1000",
            values: decimals(&[
                "0.10", "0.11", "0.12", "0.13", "0.14", "0.15", "0.16", "0.17", "0.18", "0.19",
                "0.20", "0.21",
            ]),
        },
        "decimal-addition-subtraction/kp1" => Recipe {
            statement: "Compute ${a} + 1.3$.",
            answer_expr: "a + 1.3",
            values: decimals(&[
                "0.1", "0.2", "0.3", "0.4", "0.5", "0.6", "0.7", "0.8", "0.9", "1.0", "1.1", "1.2",
            ]),
        },
        "decimal-operations/kp1" => Recipe {
            statement: r"Compute ${a} \times 0.4$.",
            answer_expr: "a * 0.4",
            values: decimals(&[
                "0.1", "0.2", "0.3", "0.4", "0.5", "0.7", "0.8", "0.9", "1.0", "1.1", "1.2", "1.3",
            ]),
        },
        "signed-decimal-operations/kp1" => Recipe {
            statement: "Compute ${a} + 1.5$.",
            answer_expr: "a + 1.5",
            values: decimals(&[
                "-9.9", "-9.8", "-9.7", "-9.6", "-9.5", "-9.4", "-9.3", "-9.2", "-9.1", "-9.0",
                "-8.9", "-8.8",
            ]),
        },
        "signed-decimal-operations/kp2" => Recipe {
            statement: r"Compute ${a} \times 6$.",
            answer_expr: "a * 6",
            values: decimals(&[
                "-1.0", "-1.5", "-2.0", "-2.5", "-3.0", "-3.5", "-4.0", "-4.5", "-5.0", "-5.5",
                "-6.0", "-6.5",
            ]),
        },
        "adding-integers/kp3" => Recipe {
            statement: "Compute $-{a} + {a}$.",
            answer_expr: "-a + a",
            values: integers((1..=13).filter(|value| *value != 6)),
        },
        _ => return None,
    };
    Some(item)
}

fn radical_recipe(key: &str) -> Option<Recipe> {
    let item = match key {
        "perfect-square-roots/kp2" => Recipe {
            statement: r"Compute $\sqrt{{({a})}}$.",
            answer_expr: "sqrt(a)",
            values: squares(144),
        },
        "perfect-square-roots/kp3" => Recipe {
            statement: r"Compute $\sqrt{{({a})}}$.",
            answer_expr: "sqrt(a)",
            values: squares(400),
        },
        "square-roots/kp2" => Recipe {
            statement: r"Compute $\sqrt{{({a}/16)}}$.",
            answer_expr: "sqrt(a/16)",
            values: squares(144),
        },
        "square-roots/kp3" => Recipe {
            statement: r"Compute $\sqrt{{({a})}} \cdot \sqrt{{4}}$.",
            answer_expr: "sqrt(a) * sqrt(4)",
            values: squares(144),
        },
        "radical-operations/kp1" => Recipe {
            statement: r"Simplify $\sqrt{{3}} \cdot \sqrt{{({a})}}$.",
            answer_expr: "sqrt(3) * sqrt(a)",
            values: integers((1..=12).map(|value| 3 * value * value)),
        },
        "dividing-radicals/kp1" => Recipe {
            statement: r"Simplify $\sqrt{{({a})}} / \sqrt{{2}}$.",
            answer_expr: "sqrt(a) / sqrt(2)",
            values: integers((1..=12).map(|value| 2 * value * value)),
        },
        "radical-exponent-conversion/kp3" => Recipe {
            statement: "Compute $({a})^{{1/2}}$.",
            answer_expr: "a ** (1/2)",
            values: squares(144),
        },
        "rational-exponents/kp1" => Recipe {
            statement: "Compute $({a})^{{2/3}}$.",
            answer_expr: "a ** (2/3)",
            values: cubes(12),
        },
        "rational-exponents/kp2" => Recipe {
            statement: "Compute $({a})^{{-1/2}}$.",
            answer_expr: "a ** (-1/2)",
            values: squares(144),
        },
        "rational-expressions/kp3" => Recipe {
            statement: "Simplify $(({a}) - x)/(x - ({a}))$.",
            answer_expr: "-1",
            values: integers(1..=12),
        },
        _ => return None,
    };
    Some(item)
}

fn arguments(recipe: Recipe, rule: &str) -> Option<Value> {
    let ast = parse_answer_expr(recipe.answer_expr).ok()?;
    let mut samples = Vec::with_capacity(recipe.values.len());
    for scalar in &recipe.values {
        let mut bindings = Bindings::new();
        bindings.insert("a".to_owned(), scalar.value());
        let expected = answer(&ast, &bindings).ok()?.text;
        samples.push(json!({"params":{"a":scalar},"expected":expected}));
    }
    let mut params = Map::new();
    params.insert(
        "a".to_owned(),
        json!({"kind":"choice","values":recipe.values}),
    );
    Some(json!({
        "statement": recipe.statement,
        "params": params,
        "constraints": [],
        "answer_expr": recipe.answer_expr,
        "solution_sketch": rule,
        "hints": [rule],
        "distractors": [],
        "samples": samples,
    }))
}

/// Return an exact recipe for an audited closed family.
pub(super) fn special(spec: &AuthoringSpec) -> Option<Value> {
    let key = format!("{}/{}", spec.topic_id, spec.kp_id);
    arguments(recipe(&key)?, super::method::rule(spec))
}
