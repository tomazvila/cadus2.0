//! The `ordered_word` contract: a product whose factors do not commute.
//!
//! A group word (`r^2 s`), a matrix product (`A^-1 B`), a permutation product
//! (`(1 2)(3 4)`) and a matrix size (`4 x 3`) all depend on the order of their
//! factors. The `exact` contract sorts the factors of a product, so it accepts
//! `ba` for `ab`. This contract reads the factors in the written order.
//!
//! A word is a run of factors. A factor is a letter, or a group in parentheses,
//! with an optional exponent (`^2`, `^-1`, `^(-1)`, `^{-1}`, `⁻¹`). A letter
//! run such as `ab` is two factors, unless the run is a Greek name (`sigma`).
//! The product sign can be `*`, `·`, `⋅`, `∘`, a space, or nothing. The reader
//! applies only the laws of every group: the inverse of a product reverses it
//! (`(ab)^-1` is `b^-1 a^-1`), and a letter next to its own inverse cancels.
//! A relation of one named group (`r^4 = 1`) is not applied.

use super::{Canon, Undecidable};

/// The longest word (in letters, after the exponents are written out).
const MAX_LETTERS: usize = 512;
/// The largest size of an exponent.
const MAX_EXPONENT: i64 = 64;
/// The largest size of a symbol in a permutation.
const MAX_POINT: usize = 99;

const UNREADABLE_WORD: &str =
    "write the product as letters with exponents, in order, such as a^2 b^-1";

/// One letter of a word: the generator and its sign (+1 or -1).
type Letter = (String, i32);

/// Read the authored word.
pub(super) fn expected(text: &str) -> Result<Canon, Undecidable> {
    read(text).map(Canon::Label)
}

/// The canonical text of a word, a size, or a permutation.
///
/// Two answers have the same text exactly when they have the same factors in
/// the same order, after the exponents and the inverses of groups are written
/// out and a letter next to its own inverse is cancelled.
pub(super) fn read(text: &str) -> Result<String, Undecidable> {
    let source = clean(text);
    if source.is_empty() {
        return Err(Undecidable::new(UNREADABLE_WORD));
    }
    if matches!(source.as_str(), "e" | "id" | "1" | "identity" | "()") {
        return Ok("word:".to_owned());
    }
    if source.chars().all(|ch| ch.is_ascii_digit()) {
        return Ok(format!("number:{source}"));
    }
    if let Some(size) = size(&source) {
        return Ok(size);
    }
    if is_cycles(&source) {
        return permutation(&source);
    }
    let chars: Vec<char> = source.chars().collect();
    let mut at = 0;
    let letters = product(&chars, &mut at, 0)?;
    if at != chars.len() {
        return Err(Undecidable::new(UNREADABLE_WORD));
    }
    Ok(format!("word:{}", spell(&reduce(letters))))
}

/// A few candidate wrong answers to the authored word, for the key self-check:
/// the reverse of the word, a rotation, and the word without its first letter.
#[must_use]
pub fn mutants(text: &str) -> Vec<String> {
    let source = clean(text);
    let chars: Vec<char> = source.chars().collect();
    let mut at = 0;
    let Ok(letters) = product(&chars, &mut at, 0) else {
        return Vec::new();
    };
    let letters = reduce(letters);
    if at != chars.len() || letters.len() < 2 {
        return Vec::new();
    }
    let show = |letters: &[Letter]| {
        letters
            .iter()
            .map(|(name, sign)| {
                if *sign > 0 {
                    name.clone()
                } else {
                    format!("{name}^-1")
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let reversed: Vec<Letter> = letters.iter().rev().cloned().collect();
    let mut rotated = letters.clone();
    rotated.rotate_left(1);
    vec![show(&reversed), show(&rotated), show(&letters[1..])]
}

/// Simplify the spelling of the text: one kind of minus, one product sign.
fn clean(text: &str) -> String {
    let mut out = String::new();
    for ch in text.trim().trim_matches('$').chars() {
        match ch {
            '\u{2212}' | '\u{2013}' => out.push('-'),
            '\u{22C5}' | '\u{00B7}' | '\u{2218}' | '*' | '\u{2062}' => out.push(' '),
            '\u{207B}' => out.push_str("^-"),
            '\u{00B9}' => out.push_str(if out.ends_with("^-") { "1" } else { "^1" }),
            '\u{00B2}' => out.push_str(if out.ends_with("^-") { "2" } else { "^2" }),
            '\u{00B3}' => out.push_str(if out.ends_with("^-") { "3" } else { "^3" }),
            '\u{2074}'..='\u{2079}' => {
                let digit = char::from_u32(u32::from(ch) - 0x2074 + u32::from('4')).unwrap_or('4');
                if !out.ends_with("^-") {
                    out.push('^');
                }
                out.push(digit);
            }
            other => out.push(other),
        }
    }
    let out = out
        .replace("\\cdot", " ")
        .replace("\\circ", " ")
        .replace("\\times", " ")
        .replace("\\left", "")
        .replace("\\right", "")
        .replace("\\(", "")
        .replace("\\)", "")
        .replace("^{", "^(")
        .replace(" inverse", "^-1")
        .replace('}', ")")
        .replace('{', "");
    let out = strip_label(out.trim());
    out.trim().trim_end_matches('.').trim().to_owned()
}

/// Remove a leading `name =` label.
fn strip_label(text: &str) -> &str {
    match text.split_once('=') {
        Some((name, rest))
            if !rest.trim().is_empty()
                && !name.trim().is_empty()
                && name
                    .trim()
                    .chars()
                    .all(|ch| ch.is_alphanumeric() || ch == '_') =>
        {
            rest.trim()
        }
        _ => text,
    }
}

/// A matrix size such as `4 x 3`, `4x3`, `4 by 3` or `4 × 3`.
fn size(source: &str) -> Option<String> {
    let spaced = source.replace('\u{00D7}', "x").replace(" by ", "x");
    let squeezed: String = spaced.chars().filter(|ch| !ch.is_whitespace()).collect();
    let parts: Vec<&str> = squeezed.split('x').collect();
    let numbers = parts.len() >= 2
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()));
    numbers.then(|| {
        let values: Vec<String> = parts
            .iter()
            .map(|part| part.trim_start_matches('0').to_owned())
            .collect();
        format!("size:{}", values.join("x"))
    })
}

/// Read a product of factors until a closing parenthesis or the end.
fn product(chars: &[char], at: &mut usize, depth: u32) -> Result<Vec<Letter>, Undecidable> {
    let bad = || Undecidable::new(UNREADABLE_WORD);
    let mut letters = Vec::new();
    loop {
        while chars.get(*at).is_some_and(|ch| ch.is_whitespace()) {
            *at += 1;
        }
        match chars.get(*at) {
            None => break,
            Some(')') if depth > 0 => break,
            Some(')') => return Err(bad()),
            Some('(') => {
                *at += 1;
                let inner = product(chars, at, depth + 1)?;
                if chars.get(*at) != Some(&')') || depth > 8 {
                    return Err(bad());
                }
                *at += 1;
                let power = exponent(chars, at)?;
                letters.extend(power_of(&inner, power)?);
            }
            Some(ch) if ch.is_alphabetic() => {
                let start = *at;
                while chars.get(*at).is_some_and(|ch| ch.is_alphabetic()) {
                    *at += 1;
                }
                let run: String = chars[start..*at].iter().collect();
                let names = split_run(&run);
                let count = names.len();
                for (index, name) in names.into_iter().enumerate() {
                    let name = subscripted(name, chars, at, index + 1 == count)?;
                    let power = if index + 1 == count {
                        exponent(chars, at)?
                    } else {
                        1
                    };
                    letters.extend(power_of(&[(name, 1)], power)?);
                }
            }
            Some(_) => return Err(bad()),
        }
        if letters.len() > MAX_LETTERS {
            return Err(Undecidable::new("the product is longer than the input cap"));
        }
    }
    Ok(letters)
}

/// The names of a letter run: a Greek name stays whole, every other letter is
/// one factor.
fn split_run(run: &str) -> Vec<String> {
    const GREEK: [&str; 12] = [
        "alpha", "beta", "gamma", "delta", "theta", "lambda", "rho", "sigma", "tau", "phi", "psi",
        "omega",
    ];
    if GREEK.contains(&run) {
        vec![run.to_owned()]
    } else {
        run.chars().map(String::from).collect()
    }
}

/// A subscript `_1` or `_i` after the last letter of a run joins its name.
fn subscripted(
    name: String,
    chars: &[char],
    at: &mut usize,
    last: bool,
) -> Result<String, Undecidable> {
    if !last || chars.get(*at) != Some(&'_') {
        return Ok(name);
    }
    *at += 1;
    let start = *at;
    while chars.get(*at).is_some_and(|ch| ch.is_alphanumeric()) {
        *at += 1;
    }
    if start == *at {
        return Err(Undecidable::new(UNREADABLE_WORD));
    }
    let tag: String = chars[start..*at].iter().collect();
    Ok(format!("{name}_{tag}"))
}

/// An optional exponent after a factor; 1 if there is none.
fn exponent(chars: &[char], at: &mut usize) -> Result<i64, Undecidable> {
    let bad = || Undecidable::new("write an exponent as ^2 or ^-1, with whole numbers only");
    if chars.get(*at).is_some_and(|ch| ch.is_ascii_digit()) {
        return Err(bad());
    }
    if chars.get(*at) != Some(&'^') {
        return Ok(1);
    }
    *at += 1;
    let wrapped = chars.get(*at) == Some(&'(');
    if wrapped {
        *at += 1;
    }
    let negative = chars.get(*at) == Some(&'-');
    if negative {
        *at += 1;
    }
    let start = *at;
    while chars.get(*at).is_some_and(|ch| ch.is_ascii_digit()) {
        *at += 1;
    }
    if start == *at || (wrapped && chars.get(*at) != Some(&')')) {
        return Err(bad());
    }
    if wrapped {
        *at += 1;
    }
    let digits: String = chars[start..*at - usize::from(wrapped)].iter().collect();
    let value: i64 = digits.parse().map_err(|_| bad())?;
    if value > MAX_EXPONENT {
        return Err(bad());
    }
    Ok(if negative { -value } else { value })
}

/// The product of `power` copies of a word; a negative power inverts it.
fn power_of(word: &[Letter], power: i64) -> Result<Vec<Letter>, Undecidable> {
    let count = usize::try_from(power.unsigned_abs()).unwrap_or(usize::MAX);
    if word.len().saturating_mul(count) > MAX_LETTERS {
        return Err(Undecidable::new("the product is longer than the input cap"));
    }
    let unit: Vec<Letter> = if power < 0 {
        word.iter()
            .rev()
            .map(|(name, sign)| (name.clone(), -sign))
            .collect()
    } else {
        word.to_vec()
    };
    Ok((0..count).flat_map(|_| unit.clone()).collect())
}

/// Cancel each letter that stands next to its own inverse.
fn reduce(letters: Vec<Letter>) -> Vec<Letter> {
    let mut stack: Vec<Letter> = Vec::new();
    for letter in letters {
        match stack.last() {
            Some((name, sign)) if *name == letter.0 && *sign == -letter.1 => {
                stack.pop();
            }
            _ => stack.push(letter),
        }
    }
    stack
}

fn spell(letters: &[Letter]) -> String {
    letters
        .iter()
        .map(|(name, sign)| format!("{name}{}", if *sign > 0 { "" } else { "'" }))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether the text is a product of cycles such as `(1 2 3)(4 5)`.
fn is_cycles(source: &str) -> bool {
    source.starts_with('(')
        && source.chars().any(|ch| ch.is_ascii_digit())
        && !source
            .chars()
            .any(|ch| ch.is_alphabetic() || matches!(ch, '_' | '='))
}

/// The permutation of a product of cycles, composed from right to left.
fn permutation(source: &str) -> Result<String, Undecidable> {
    let bad =
        || Undecidable::new("write a permutation as cycles of whole numbers, such as (1 2 3)(4 5)");
    let chars: Vec<char> = source.chars().collect();
    let mut at = 0;
    let mut cycles: Vec<(Vec<usize>, i64)> = Vec::new();
    loop {
        while chars.get(at).is_some_and(|ch| ch.is_whitespace()) {
            at += 1;
        }
        if at == chars.len() {
            break;
        }
        if chars[at] != '(' {
            return Err(bad());
        }
        at += 1;
        let start = at;
        while chars.get(at).is_some_and(|ch| *ch != ')') {
            at += 1;
        }
        if at == chars.len() {
            return Err(bad());
        }
        let inner: String = chars[start..at].iter().collect();
        at += 1;
        let power = exponent(&chars, &mut at)?;
        cycles.push((cycle_points(&inner).ok_or_else(bad)?, power));
    }
    let top = cycles
        .iter()
        .flat_map(|(points, _)| points.iter().copied())
        .max()
        .unwrap_or(0);
    let mut image: Vec<usize> = (0..=top).collect();
    // The product acts from right to left: the last cycle moves a point first.
    for (points, power) in cycles.iter().rev() {
        let mut step: Vec<usize> = (0..=top).collect();
        for (index, point) in points.iter().enumerate() {
            step[*point] = points[(index + 1) % points.len()];
        }
        for _ in 0..power.unsigned_abs() {
            image = if *power < 0 {
                let mut inverse: Vec<usize> = (0..=top).collect();
                for (point, target) in step.iter().enumerate() {
                    inverse[*target] = point;
                }
                image.iter().map(|value| inverse[*value]).collect()
            } else {
                image.iter().map(|value| step[*value]).collect()
            };
        }
    }
    let moved: Vec<String> = (1..=top)
        .filter(|point| image[*point] != *point)
        .map(|point| format!("{point}>{}", image[point]))
        .collect();
    Ok(format!("perm:{}", moved.join(",")))
}

/// The points of one cycle: whole numbers apart by a space or a comma, or a run
/// of single digits (`123`). A point appears once.
fn cycle_points(inner: &str) -> Option<Vec<usize>> {
    let fields: Vec<&str> = inner
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .filter(|field| !field.is_empty())
        .collect();
    let mut points = Vec::new();
    for field in &fields {
        if !field.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
        if fields.len() == 1 && field.len() > 1 {
            points.extend(
                field
                    .chars()
                    .map(|ch| ch.to_digit(10).unwrap_or(0) as usize),
            );
        } else {
            points.push(field.parse().ok()?);
        }
    }
    let mut seen = points.clone();
    seen.sort_unstable();
    seen.dedup();
    let fine =
        seen.len() == points.len() && points.iter().all(|point| (1..=MAX_POINT).contains(point));
    fine.then_some(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(left: &str, right: &str) -> bool {
        read(left).unwrap() == read(right).unwrap()
    }

    #[test]
    fn the_order_of_the_factors_decides() {
        assert!(!same("ab", "ba"));
        assert!(same("ab", "a*b"));
        assert!(same("ab", "a b"));
        assert!(same("r^2 s", "r^{2} s"));
        assert!(!same("r^2 s", "s r^2"));
    }

    #[test]
    fn exponents_and_inverses_are_written_out() {
        assert!(same("a^2", "aa"));
        assert!(same("(AB)^-1", "B^-1 A^-1"));
        assert!(same("a a^-1 b", "b"));
        assert!(same("A^{-1}B", "A⁻¹B"));
    }

    #[test]
    fn cycles_compose_from_the_right() {
        assert!(same("(1 2)(3 4)", "(3 4)(1 2)"));
        assert!(!same("(1 2)(2 3)", "(2 3)(1 2)"));
        assert!(same("(1 2 3)", "(123)"));
        assert!(same("(1 2 3)^-1", "(3 2 1)"));
    }

    #[test]
    fn sizes_keep_their_order() {
        assert!(same("4 x 3", "4 by 3"));
        assert!(!same("4 x 3", "3 x 4"));
    }
}
