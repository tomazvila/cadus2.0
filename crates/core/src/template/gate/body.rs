//! The body read, for the rows the typed read owns.

use super::Rejection;
use super::text::py_str;

/// The 1.0 rejection a body that does not read as a document earns.
pub(super) fn body_rejection(body: &str, err: &serde_json::Error) -> Rejection {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return Rejection::new("body", format!("the template body is not JSON: {err}"));
    };
    if let Some(rejection) = params_rejection(&value) {
        return rejection;
    }
    if let Some(sketch) = value.get("solution_sketch")
        && !sketch.is_string()
        && !sketch.is_null()
    {
        return Rejection::new(
            "body",
            "solution_expr must be a string when present".to_string(),
        );
    }
    if let Some(samples) = value.get("samples").and_then(serde_json::Value::as_array) {
        for sample in samples {
            let Some(object) = sample.as_object() else {
                return Rejection::new("body", "a sample was not an object".to_string());
            };
            let params_ok = object
                .get("params")
                .is_some_and(serde_json::Value::is_object);
            let expected_ok = object
                .get("expected")
                .is_some_and(|value| value.is_string() || value.is_i64() || value.is_u64());
            if !params_ok || !expected_ok {
                return Rejection::new(
                    "body",
                    "a sample needs 'params' and a scalar 'expected'".to_string(),
                );
            }
        }
    }
    Rejection::new("body", format!("the template body does not read: {err}"))
}

/// The 1.0 rejection a malformed parameter domain earns.
fn params_rejection(value: &serde_json::Value) -> Option<Rejection> {
    let params = value.get("params")?.as_object()?;
    for domain in params.values() {
        let Some(object) = domain.as_object() else {
            return Some(Rejection::new(
                "body",
                "a parameter domain was not an object".to_string(),
            ));
        };
        match object.get("kind").and_then(serde_json::Value::as_str) {
            Some("int") => {
                let whole = |key: &str| {
                    object
                        .get(key)
                        .is_some_and(|end| end.is_i64() || end.is_u64())
                };
                if !whole("low") || !whole("high") {
                    return Some(Rejection::new(
                        "body",
                        "an int domain needs integer 'low' and 'high'".to_string(),
                    ));
                }
            }
            Some("choice") => {
                let Some(values) = object.get("values").and_then(serde_json::Value::as_array)
                else {
                    return Some(Rejection::new(
                        "body",
                        "a choice domain needs a non-empty 'values' list".to_string(),
                    ));
                };
                if values.is_empty() {
                    return Some(Rejection::new(
                        "body",
                        "a choice domain needs a non-empty 'values' list".to_string(),
                    ));
                }
                if !values
                    .iter()
                    .all(|entry| entry.is_string() || entry.is_i64() || entry.is_u64())
                {
                    return Some(Rejection::new(
                        "body",
                        "choice values must be strings or integers".to_string(),
                    ));
                }
            }
            Some("rational" | "decimal") => {}
            other => {
                let written = other.map_or_else(|| "None".to_string(), py_str);
                return Some(Rejection::new(
                    "body",
                    format!("unknown domain kind {written}"),
                ));
            }
        }
    }
    None
}
