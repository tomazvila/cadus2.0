//! The readers both instruction gates share: the JSON object, the field list,
//! and one non-empty string field.

use crate::template::gate::{Rejection, py_str};

/// Read one JSON object, or refuse the body with the reader's own words.
pub(super) fn object(
    body: &str,
    what: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, Rejection> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|err| Rejection {
        code: "body",
        message: format!("the {what} body is not JSON: {err}"),
    })?;
    match value {
        serde_json::Value::Object(fields) => Ok(fields),
        _ => Err(Rejection {
            code: "body",
            message: format!("the {what} body is not a JSON object"),
        }),
    }
}

/// No field the reader does not know.
///
/// The serve reader carries `deny_unknown_fields`, so a stored body with a spare
/// field reads as a `500` at the route and teaches nobody. The gate says so
/// first, in words the next attempt acts on.
pub(super) fn only_known(
    fields: &serde_json::Map<String, serde_json::Value>,
    known: &[&str],
    what: &str,
    code: &'static str,
) -> Result<(), Rejection> {
    for name in fields.keys() {
        if !known.contains(&name.as_str()) {
            return Err(Rejection {
                code,
                message: format!(
                    "the {what} carries the unknown field {} — it holds {} and nothing else",
                    py_str(name),
                    py_names(known)
                ),
            });
        }
    }
    Ok(())
}

/// Write a field list the way the messages read it.
fn py_names(names: &[&str]) -> String {
    let written: Vec<String> = names.iter().map(|name| py_str(name)).collect();
    written.join(" and ")
}

/// One non-empty string field, or the refusal it earns.
pub(super) fn text(
    fields: &serde_json::Map<String, serde_json::Value>,
    name: &str,
    code: &'static str,
    reason: &str,
) -> Result<String, Rejection> {
    match fields.get(name).and_then(serde_json::Value::as_str) {
        Some(value) if !value.trim().is_empty() => Ok(value.to_owned()),
        _ => Err(Rejection {
            code,
            message: reason.to_owned(),
        }),
    }
}
