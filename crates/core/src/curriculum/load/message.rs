//! The message text of the schema walk and of the parser findings.
//!
//! 1.0 validates in pydantic lax mode, so it accepts values 2.0 refuses. The
//! functions here write the pydantic text only where pydantic also refuses the
//! value. Every other message is 2.0's own, and it names the fix (spec section
//! 7, "2.0 strictness").

use serde_norway::Value;

use super::super::finding::Finding;
use super::{I64_MAX_EXCLUSIVE_AS_F64, I64_MIN_AS_F64, OUT_OF_RANGE_INTEGER};

/// The text `serde_norway` writes for a repeated mapping key, up to the key.
const DUPLICATE_KEY_MARKER: &str = "duplicate entry with key \"";

/// The text `serde_norway` writes for an integer literal outside `i64`/`u64`.
const BIG_INTEGER_MARKERS: [&str; 2] = [" as u128, expected", " as i128, expected"];

/// The plain scalars YAML 1.1 reads as a boolean and YAML 1.2 reads as a
/// string, in lower case (`true` and `false` are booleans in both versions).
const YAML_1_1_BOOL_WORDS: [&str; 6] = ["y", "n", "yes", "no", "on", "off"];

/// The strings 1.0 pydantic reads as a boolean in lax mode, in lower case.
const LAX_BOOL_WORDS: [&str; 12] = [
    "true", "false", "t", "f", "1", "0", "yes", "no", "on", "off", "y", "n",
];

/// The text of a mapping key, for the dotted location of an extra key.
pub(super) fn key_name(key: &Value) -> String {
    match key {
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Null => "None".to_owned(),
        _ => String::new(),
    }
}
/// The integer behind a YAML scalar, or the message for a value that is not
/// one.
///
/// A float with no fractional part counts, the same as 1.0 pydantic in lax
/// mode (spec section 7, "2.0 strictness").
pub(super) fn integer_of(value: &Value) -> Result<i64, String> {
    match value {
        Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                return Ok(integer);
            }
            if number.as_u64().is_some() {
                // Above `i64::MAX`. Python integers have no upper bound.
                return Err(OUT_OF_RANGE_INTEGER.to_owned());
            }
            // A number that is neither an `i64` nor a `u64` is an `f64`.
            let float = number.as_f64().unwrap_or(f64::NAN);
            if !float.is_finite() {
                return Err("Input should be a finite number".to_owned());
            }
            if float.fract() != 0.0 {
                return Err(
                    "Input should be a valid integer, got a number with a fractional part"
                        .to_owned(),
                );
            }
            if !(I64_MIN_AS_F64..I64_MAX_EXCLUSIVE_AS_F64).contains(&float) {
                // A literal of 39 or more digits arrives here as an `f64`. It is
                // one more integer literal outside `i64`, so it reports the one
                // message of spec section 7.
                return Err(OUT_OF_RANGE_INTEGER.to_owned());
            }
            Ok(float as i64)
        }
        Value::Bool(flag) => Err(format!("boolean {flag} is not accepted; write an integer")),
        Value::String(text) => Err(integer_string_message(text)),
        _ => Err("Input should be a valid integer".to_owned()),
    }
}

/// The message for a string in an integer field.
fn integer_string_message(text: &str) -> String {
    match text.trim().parse::<i64>() {
        Ok(number) => format!("string '{text}' is not accepted; write {number}"),
        Err(_) => {
            if is_decimal_digits(text.trim()) {
                // A literal of about 309 or more digits overflows `f64` too,
                // and arrives as a string.
                OUT_OF_RANGE_INTEGER.to_owned()
            } else if text
                .trim()
                .parse::<f64>()
                .is_ok_and(|number| !number.is_finite())
            {
                // A decimal literal that overflows `f64`. 1.0 reads the
                // infinity and pydantic reports this text for it.
                "Input should be a finite number".to_owned()
            } else {
                "Input should be a valid integer, unable to parse string as an integer".to_owned()
            }
        }
    }
}

/// True for a decimal integer literal with an optional sign and at least one
/// digit. The pre-scan accepts the spelling; `str::parse` refuses the value only
/// because no `i64` holds it.
fn is_decimal_digits(text: &str) -> bool {
    let body = text.strip_prefix('-').unwrap_or(text);
    !body.is_empty() && body.bytes().all(|byte| byte.is_ascii_digit())
}

/// The message for a value that is not a boolean, or `None` when it is one.
pub(super) fn bool_message(value: &Value) -> Option<String> {
    match value {
        Value::Bool(_) => None,
        Value::String(text) => Some(if is_yaml_1_1_bool(text) {
            format!("YAML 1.1 boolean '{text}' is not accepted; write true or false")
        } else if is_lax_bool_text(text) {
            format!("string '{text}' is not accepted; write true or false")
        } else {
            "Input should be a valid boolean, unable to interpret input".to_owned()
        }),
        Value::Number(number) => Some(match number.as_f64() {
            // 1.0 pydantic reads 0 and 1 as booleans, in both the integer and
            // the float form.
            Some(float) if float == 0.0 || float == 1.0 => {
                format!("number {number} is not accepted; write true or false")
            }
            Some(float) if float.is_finite() && float.fract() == 0.0 => {
                "Input should be a valid boolean, unable to interpret input".to_owned()
            }
            _ => "Input should be a valid boolean".to_owned(),
        }),
        _ => Some("Input should be a valid boolean".to_owned()),
    }
}

/// The message for a string in a `difficulty` or `weight` field.
///
/// The numeric-literal pre-scan owns every unquoted spelling, so the text here
/// is a quoted scalar, or a plain scalar the type check reads as text.
pub(super) fn string_number_message(text: &str) -> String {
    if text.trim().parse::<f64>().is_ok() {
        format!("string '{text}' is not accepted; write the number unquoted")
    } else {
        "Input should be a valid number, unable to parse string as a number".to_owned()
    }
}

/// True for a plain scalar that YAML 1.1 reads as a boolean and YAML 1.2 reads
/// as a string.
fn is_yaml_1_1_bool(text: &str) -> bool {
    YAML_1_1_BOOL_WORDS.contains(&text.to_ascii_lowercase().as_str())
}

/// True for a string that 1.0 pydantic reads as a boolean in lax mode.
fn is_lax_bool_text(text: &str) -> bool {
    LAX_BOOL_WORDS.contains(&text.to_ascii_lowercase().as_str())
}

/// The finding for a document the YAML parser rejects.
///
/// Two of those rejections are YAML 1.1 forms that 1.0 accepts, and the parser
/// text names neither the form nor the fix, so the port writes its own message
/// with the `schema` code (spec section 7, "2.0 strictness"). Every other parser
/// error keeps the 1.0 `yaml` code and text.
pub(super) fn parse_finding(rel: &str, text: &str, error: &str) -> Finding {
    if let Some((_, tail)) = error.split_once(DUPLICATE_KEY_MARKER)
        && let Some((key, tail)) = tail.split_once('"')
    {
        let parser_line = tail
            .split_once("at line ")
            .and_then(|(_, tail)| tail.split(' ').next())
            .map(str::to_owned);
        // The parser writes a location for a duplicate inside a nested mapping
        // and none for one in the root mapping, so the root case reads the line
        // out of the text: the second line that opens the key at column 1.
        let line = match parser_line {
            Some(line) => line,
            None => {
                root_key_line(text, key).map_or_else(|| "?".to_owned(), |line| line.to_string())
            }
        };
        return Finding::new(
            "schema",
            format!("{rel}: duplicate mapping key '{key}' at line {line}"),
        )
        .with_file(rel);
    }
    for marker in BIG_INTEGER_MARKERS {
        if let Some((head, _)) = error.split_once(marker) {
            let loc = head
                .split_once(": invalid type")
                .map(|(loc, _)| loc)
                .filter(|loc| !loc.is_empty())
                .unwrap_or(rel);
            return Finding::new("schema", format!("{}: {OUT_OF_RANGE_INTEGER}", dotted(loc)))
                .with_file(rel);
        }
    }
    Finding::new("yaml", format!("{rel}: {error}")).with_file(rel)
}

/// The line of the second `<key>:` at column 1 of a document, counted from 1.
///
/// A root-level key starts its line, so the scan needs no parser state.
fn root_key_line(text: &str, key: &str) -> Option<usize> {
    let mut seen = false;
    for (index, line) in text.lines().enumerate() {
        let Some(rest) = line.strip_prefix(key) else {
            continue;
        };
        if !rest.starts_with(':') {
            continue;
        }
        if seen {
            return Some(index + 1);
        }
        seen = true;
    }
    None
}

/// The dotted form of the bracket path of a parser error: `topics[0].id` becomes
/// `topics.0.id`, which is the location shape of spec section 5, rule 2.
fn dotted(loc: &str) -> String {
    loc.replace('[', ".").replace(']', "")
}

/// `'a', 'b' or 'c'` — the alternatives list of a pydantic enum message.
pub(super) fn quoted_alternatives(allowed: &[&str]) -> String {
    let quoted: Vec<String> = allowed.iter().map(|item| format!("'{item}'")).collect();
    let joined = quoted.join(", ");
    joined.rsplit_once(", ").map_or_else(
        || joined.clone(),
        |(head, last)| format!("{head} or {last}"),
    )
}

#[cfg(test)]
mod tests {
    use super::quoted_alternatives;

    /// The list reads the way pydantic writes it, for every length.
    #[test]
    fn the_alternatives_list_joins_with_commas_and_one_or() {
        assert_eq!(quoted_alternatives(&[]), "");
        assert_eq!(quoted_alternatives(&["a"]), "'a'");
        assert_eq!(quoted_alternatives(&["a", "b"]), "'a' or 'b'");
        assert_eq!(quoted_alternatives(&["a", "b", "c"]), "'a', 'b' or 'c'");
    }
}
