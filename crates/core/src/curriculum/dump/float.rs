//! The JSON text of the dump, the Python float `repr`, and the SHA-256 hash.

use serde_json::{Number, Value};
use sha2::{Digest, Sha256};

/// Append the canonical JSON text of one value.
///
/// The rules are the `json.dumps` arguments of the oracle: the separators are
/// `,` and `:`, and non-ASCII text stays unescaped. `serde_json` escapes a
/// string the same way `json.dumps(ensure_ascii=False)` does, so a string goes
/// through `serde_json`. A float does not: [`python_repr_f64`] writes it.
pub(super) fn render(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => render_number(number, out),
        // A `&str` always serializes, so the fallback is unreachable. An empty
        // string fails the parity test loudly rather than passes silently.
        Value::String(item) => out.push_str(&json_string(item.as_str())),
        Value::Array(items) => {
            out.push('[');
            for (position, item) in items.iter().enumerate() {
                if position > 0 {
                    out.push(',');
                }
                render(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (position, (key, item)) in map.iter().enumerate() {
                if position > 0 {
                    out.push(',');
                }
                out.push_str(&json_string(key.as_str()));
                out.push(':');
                render(item, out);
            }
            out.push('}');
        }
    }
}

/// Append the canonical JSON text of one number.
///
/// A count is an integer and takes the plain decimal text. A `difficulty` or a
/// `weight` is a float and takes the Python `repr` text (parity trap 16). A
/// float number always reads back as an `f64`, so the fallback never fires.
fn render_number(number: &Number, out: &mut String) {
    if number.is_f64() {
        out.push_str(&python_repr_f64(number.as_f64().unwrap_or(f64::NAN)));
        return;
    }
    out.push_str(&number.to_string());
}

/// One JSON string, escaped the way `json.dumps(ensure_ascii=False)` escapes it.
fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// The text Python `repr` writes for a float (parity trap 16).
///
/// The digits are the shortest run that round-trips, which is what Rust `{:e}`
/// writes and what Python picks. The two formatters differ on one point: an
/// exact tie between the two shortest candidates. Rust rounds such a tie away
/// from zero; CPython `repr` (David Gay, mode 0) rounds it to the even last
/// digit. [`even_last_digit_on_a_tie`] puts the CPython rule back.
///
/// The notation follows the Python rule: fixed notation while the exponent is
/// at or above -4 and below 16, and `d.ddde±XX` with a signed exponent of at
/// least two digits outside that range. An integral value in fixed notation
/// keeps a `.0` tail, so `1` reads `1.0`. Scientific notation keeps no `.0`
/// tail, so `1e-05` has one digit.
///
/// `NaN` and the infinities give `nan`, `inf`, and `-inf`, the Python `repr`
/// text. They have no JSON form, so [`float`] maps them to `null` before the
/// dump ever reaches this function.
pub fn python_repr_f64(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-inf".to_owned()
        } else {
            "inf".to_owned()
        };
    }

    // Rust writes `-d.ddde<exp>` with the shortest digits that round-trip, so
    // the text always splits at its `e` and the exponent always parses.
    let shortest = format!("{value:e}");
    let (mantissa, exponent_text) = shortest.split_once('e').unwrap_or((&shortest, "0"));
    let exponent = exponent_text.parse::<i32>().unwrap_or(0);
    let (sign, unsigned) = match mantissa.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", mantissa),
    };
    let mut digits: String = unsigned.chars().filter(|item| *item != '.').collect();
    even_last_digit_on_a_tie(value, &mut digits, exponent);

    if (-4..16).contains(&exponent) {
        return fixed_notation(sign, &digits, exponent);
    }
    let separator = if exponent < 0 { '-' } else { '+' };
    let magnitude = exponent.unsigned_abs();
    let head = digits.get(..1).unwrap_or("0");
    let tail = digits.get(1..).unwrap_or("");
    let point = if tail.is_empty() { "" } else { "." };
    format!("{sign}{head}{point}{tail}e{separator}{magnitude:02}")
}

/// Apply the CPython tie rule to the shortest digit run of `value` (finding #1).
///
/// `digits` holds the significant digits with no point and no sign, and
/// `exponent` is the decimal exponent of the first digit, so the run stands for
/// the integer `N` scaled by `10^k` with `k = exponent - (digits.len() - 1)`.
///
/// Rust and CPython both take the shortest run and, inside that length, the
/// candidate closest to `value`. They differ only when `value` sits exactly
/// halfway between two candidates: Rust takes the one away from zero, which is
/// `N`, and CPython takes the one with the even last digit. `N` and `N - 1`
/// have last digits of opposite parity, so the rule fires only while the last
/// digit of `N` is odd, and then the answer is `N - 1`.
///
/// A tie needs two tests, and both are necessary.
///
/// 1. The halfway point of `N - 1` and `N` is the decimal `W * 10^(k - 1)` with
///    `W = 10 * N - 5`, whose digit run is `N - 1` followed by a `5`. A last
///    digit of `N` that is odd never borrows, so that run has the length of
///    `digits` plus one. [`is_exact_decimal`] tests `value == W * 10^(k - 1)`
///    exactly, in integer arithmetic.
/// 2. `N - 1` scaled by `10^k` parses back to `value`. A value on a binade
///    boundary has a rounding interval twice as wide above as below, so the
///    lower candidate can miss the value even while the halfway point is exact.
///    `2^-24` is such a value: it is exactly `5.9604644775390625e-08`, yet
///    `5.960464477539062e-08` parses to the float below it, and CPython writes
///    `5.960464477539063e-08`.
fn even_last_digit_on_a_tie(value: f64, digits: &mut String, exponent: i32) {
    // The digit run of `{:e}` always holds a digit, so the fallback is even
    // and returns.
    let units = digits
        .chars()
        .next_back()
        .and_then(|last| last.to_digit(10))
        .unwrap_or(0);
    if units.is_multiple_of(2) {
        return;
    }
    let length = i32::try_from(digits.len()).unwrap_or(0);
    let k = exponent.saturating_sub(length.saturating_sub(1));
    let j = k.saturating_sub(1);

    // The midpoint digit run: the last digit down one, then a `5`. The run
    // holds at most 18 digits, so it always fits.
    let head = digits.get(..digits.len().saturating_sub(1)).unwrap_or("");
    let mut midpoint = String::with_capacity(digits.len() + 1);
    midpoint.push_str(head);
    midpoint.push(char::from_digit(units - 1, 10).unwrap_or('0'));
    midpoint.push('5');
    let w = midpoint.parse::<u128>().unwrap_or(0);
    if !is_exact_decimal(value, w, j) {
        return;
    }
    // Build the even neighbor. An odd last digit never borrows.
    let mut lower = String::with_capacity(digits.len());
    lower.push_str(head);
    lower.push(char::from_digit(units - 1, 10).unwrap_or('0'));

    // Test 2: the neighbor parses back to the same float. If it does not, keep
    // the digits of Rust.
    if format!("{lower}e{k}").parse::<f64>() != Ok(value.abs()) {
        return;
    }
    *digits = lower;
}

/// Test `|value| == w * 10^exponent` in exact arithmetic.
///
/// A finite `f64` is exactly `m * 2^p` with an integer `m`. Strip the factors of
/// two from `m` to get an odd `m_odd` and a corrected `e`, so
/// `|value| = m_odd * 2^e`. Because `m_odd` is odd, the equality can hold only
/// while `e` equals `exponent`, and it then reduces to a comparison of the odd
/// parts: `m_odd * 5^-exponent == w` below zero, and `m_odd == w * 5^exponent`
/// at or above zero. An overflow of the power of five means the two sides
/// cannot be equal, so the test gives `false`.
fn is_exact_decimal(value: f64, w: u128, exponent: i32) -> bool {
    let bits = value.abs().to_bits();
    let biased = (bits >> 52) & 0x7ff;
    let fraction = bits & 0x000f_ffff_ffff_ffff;
    let (mantissa, mut power) = if biased == 0 {
        (fraction, -1074i32)
    } else {
        // A normal number carries the hidden bit and a bias of 1023, less the
        // 52 fraction bits.
        (fraction | (1u64 << 52), (biased as i32) - 1075)
    };
    if mantissa == 0 {
        return false;
    }
    let shift = mantissa.trailing_zeros();
    let odd = u128::from(mantissa >> shift); // The shift is at most 52, so the sum always fits.
    power = power.saturating_add(i32::try_from(shift).unwrap_or(0));
    if power != exponent {
        return false;
    }
    let Some(scale) = checked_power_of_five(exponent.unsigned_abs()) else {
        return false;
    };
    if exponent < 0 {
        return odd.checked_mul(scale) == Some(w);
    }
    w.checked_mul(scale) == Some(odd)
}

/// `5^power` as a `u128`, or `None` when it does not fit.
fn checked_power_of_five(power: u32) -> Option<u128> {
    let mut total: u128 = 1;
    for _ in 0..power {
        total = total.checked_mul(5)?;
    }
    Some(total)
}

/// The fixed-notation form of `sign`, `digits`, and a decimal `exponent`.
///
/// `digits` holds the significant digits with no point and no sign. The point
/// goes after `exponent + 1` digits. Python pads with zeros on either side and
/// writes a `.0` tail when no digit falls after the point.
fn fixed_notation(sign: &str, digits: &str, exponent: i32) -> String {
    let point = exponent.saturating_add(1);
    if point <= 0 {
        let zeros = "0".repeat(usize::try_from(-point).unwrap_or(0));
        return format!("{sign}0.{zeros}{digits}");
    } // A positive `i32` always fits a `usize`, so the fallback never fires.
    let point = usize::try_from(point).unwrap_or(digits.len());
    if point >= digits.len() {
        let zeros = "0".repeat(point - digits.len());
        return format!("{sign}{digits}{zeros}.0");
    }
    let head = digits.get(..point).unwrap_or(digits);
    let tail = digits.get(point..).unwrap_or("");
    format!("{sign}{head}.{tail}")
}

/// The lowercase hex SHA-256 of a byte string.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        // Two lowercase hex digits per byte, high nibble first.
        out.push(hex_digit(byte >> 4));
        out.push(hex_digit(byte & 0x0f));
    }
    out
}

/// One lowercase hex digit of a nibble. A value above 15 cannot occur.
fn hex_digit(nibble: u8) -> char {
    const DIGITS: [char; 16] = [
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
    ];
    DIGITS.get(nibble as usize).copied().unwrap_or('0')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A zero has no odd part, and a power of five past `u128` makes both
    /// sides of the equality unequal.
    #[test]
    fn the_exact_decimal_test_refuses_a_zero_and_an_overflow() {
        assert!(!is_exact_decimal(0.0, 5, 0));
        assert!(!is_exact_decimal(2.0_f64.powi(60), 1, 60));
        assert!(!is_exact_decimal(2.0_f64.powi(-60), 1, -60));
        assert!(is_exact_decimal(0.5, 5, -1));
        assert!(is_exact_decimal(5.0, 5, 0));
        assert_eq!(checked_power_of_five(55).map(|n| n > 0), Some(true));
        assert_eq!(checked_power_of_five(56), None);
    }

    /// The number writer takes the Python text for a float and the plain text
    /// for an integer.
    #[test]
    fn a_number_renders_as_python_or_as_a_plain_integer() {
        let mut out = String::new();
        render_number(&Number::from(7), &mut out);
        render_number(
            &Number::from_f64(0.25).unwrap_or_else(|| unreachable!()),
            &mut out,
        );
        assert_eq!(out, "70.25");
    }
}
