//! LaTeX spellings that mean a plain construct, rewritten before any reader runs.
//!
//! Each rewrite changes the spelling of an answer and never its value:
//!
//! - `\dfrac` and `\tfrac` are `\frac`.
//! - `\infty` is the infinity glyph.
//! - `\{` and `\}` are the braces of a set.
//! - `\,` `\;` `\:` and `\!` are spacing, and leave no token.
//! - `\text{..}`, and a font command around letters (`\operatorname{sin}`,
//!   `\mathrm{d}`), are the words in the braces.
//! - `\begin{pmatrix} 1 & 2 \\ 3 & 4 \end{pmatrix}` (also `bmatrix` and `matrix`) is the
//!   bracketed rows `[[1, 2], [3, 4]]`.
//! - `\ne` and `\neq` are `!=`, and `\langle` and `\rangle` are `<` and `>`.
//! - `90^\circ` is `90°`, and `x^{\frac{1}{2}}` is `x^(1/2)`.
//! - `1{,}205` is the thousands group of 1205.
//! - An angle in degrees inside a trigonometric call, as in `\cos(60^\circ)` or
//!   `sin 30°`, is the angle times `pi/180`.
//!
//! Every other text is returned as it came in. The function is idempotent.

/// The trigonometric names whose argument can be an angle in degrees.
const TRIG: [&str; 6] = ["sin", "cos", "tan", "sec", "csc", "cot"];

/// The font commands that write a word in another face.
const WORD_COMMANDS: [&str; 7] = [
    "mathrm",
    "mathit",
    "mathbf",
    "mathbb",
    "textbf",
    "operatorname",
    "boldsymbol",
];

/// Rewrite the spellings of the module header.
#[must_use]
pub fn prepare(text: &str) -> String {
    if !text.contains(['\\', '°', '{']) {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let chars = fraction_exponent_pass(&degree_sign_pass(&latex_pass(&degree_pass(&matrix_pass(
        &chars,
    )))));
    thousands_pass(&chars).into_iter().collect()
}

/// The index of the `}` that closes the `{` at `open`.
fn closing_brace(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0_usize;
    for (at, c) in chars.iter().enumerate().skip(open) {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether `chars` holds `word` at `at`.
fn starts_with(chars: &[char], at: usize, word: &str) -> bool {
    word.chars()
        .enumerate()
        .all(|(offset, c)| chars.get(at + offset) == Some(&c))
}

/// The index after the spaces that start at `at`.
fn skip_spaces(chars: &[char], mut at: usize) -> usize {
    while chars.get(at).is_some_and(|c| c.is_whitespace()) {
        at += 1;
    }
    at
}

/// Rewrite one `\begin{pmatrix}..\end{pmatrix}` environment into bracketed rows.
///
/// Text in front of or behind the environment stays as it is. An environment
/// that does not close, or one of another name, stays as it is.
fn matrix_pass(chars: &[char]) -> Vec<char> {
    for name in ["pmatrix", "bmatrix", "matrix"] {
        let begin = format!("\\begin{{{name}}}");
        let end = format!("\\end{{{name}}}");
        let Some(start) = (0..chars.len()).find(|at| starts_with(chars, *at, &begin)) else {
            continue;
        };
        let body_start = start + begin.chars().count();
        let Some(stop) = (body_start..chars.len()).find(|at| starts_with(chars, *at, &end)) else {
            return chars.to_vec();
        };
        let body: String = chars[body_start..stop].iter().collect();
        let rows: Vec<String> = body
            .split("\\\\")
            .filter(|row| !row.trim().is_empty())
            .map(|row| {
                let entries: Vec<&str> = row.split('&').map(str::trim).collect();
                format!("[{}]", entries.join(", "))
            })
            .collect();
        let mut out: Vec<char> = chars[..start].to_vec();
        out.extend(format!("[{}]", rows.join(", ")).chars());
        out.extend_from_slice(&chars[stop + end.chars().count()..]);
        return out;
    }
    chars.to_vec()
}

/// Rewrite the LaTeX commands, one command at a time.
fn latex_pass(chars: &[char]) -> Vec<char> {
    let mut out = Vec::with_capacity(chars.len());
    let mut at = 0;
    while at < chars.len() {
        if chars[at] != '\\' {
            out.push(chars[at]);
            at += 1;
            continue;
        }
        let Some(&next) = chars.get(at + 1) else {
            out.push('\\');
            break;
        };
        if !next.is_ascii_alphabetic() {
            match next {
                '{' | '}' => out.push(next),
                ',' | ';' | ':' => out.push(' '),
                '!' => {}
                _ => {
                    out.push('\\');
                    out.push(next);
                }
            }
            at += 2;
            continue;
        }
        let end = (at + 1..chars.len())
            .find(|i| !chars[*i].is_ascii_alphabetic())
            .unwrap_or(chars.len());
        let name: String = chars[at + 1..end].iter().collect();
        at = end;
        match name.as_str() {
            "dfrac" | "tfrac" => out.extend("\\frac".chars()),
            "infty" => out.push('∞'),
            "ne" | "neq" => out.extend("!=".chars()),
            "langle" => out.push('<'),
            "rangle" => out.push('>'),
            _ => {
                if let Some(body) = braced_word(chars, &name, at) {
                    out.extend(body.0);
                    at = body.1;
                } else {
                    out.push('\\');
                    out.extend(name.chars());
                }
            }
        }
    }
    out
}

/// The words of `\text{..}`, or of a font command around letters, with the index
/// after the closing brace.
fn braced_word(chars: &[char], name: &str, at: usize) -> Option<(Vec<char>, usize)> {
    let is_text = matches!(name, "text" | "textrm" | "mbox");
    if !is_text && !WORD_COMMANDS.contains(&name) {
        return None;
    }
    if chars.get(at) != Some(&'{') {
        return None;
    }
    let close = closing_brace(chars, at)?;
    let body = &chars[at + 1..close];
    let letters = body.iter().all(char::is_ascii_alphabetic) && !body.is_empty();
    let plain = !body.iter().any(|c| matches!(c, '{' | '}' | '\\'));
    (letters || (is_text && plain)).then(|| (body.to_vec(), close + 1))
}

/// Rewrite `d{,}ddd` into the digits of one number.
fn thousands_pass(chars: &[char]) -> Vec<char> {
    let mut out = Vec::with_capacity(chars.len());
    let mut at = 0;
    while at < chars.len() {
        let group = starts_with(chars, at, "{,}")
            && out.last().is_some_and(char::is_ascii_digit)
            && (1..=3).all(|i| chars.get(at + 2 + i).is_some_and(char::is_ascii_digit))
            && !chars.get(at + 6).is_some_and(char::is_ascii_digit);
        if group {
            at += 3;
        } else {
            out.push(chars[at]);
            at += 1;
        }
    }
    out
}

/// Rewrite `sin(30°)`, `\cos(60^\circ)`, and `tan 45^{\circ}` into a call with the
/// angle times `pi/180`.
fn degree_pass(chars: &[char]) -> Vec<char> {
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut at = 0;
    while at < chars.len() {
        if let Some((call, next)) = degree_call(chars, at, out.last().copied()) {
            out.extend(call.chars());
            at = next;
        } else {
            out.push(chars[at]);
            at += 1;
        }
    }
    out
}

/// The rewritten call that starts at `at`, with the index after it.
fn degree_call(chars: &[char], at: usize, before: Option<char>) -> Option<(String, usize)> {
    if before.is_some_and(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let mut at = at;
    if chars.get(at) == Some(&'\\') {
        at += 1;
    }
    let name = TRIG.iter().find(|name| {
        starts_with(chars, at, name)
            && !chars
                .get(at + name.len())
                .is_some_and(char::is_ascii_alphabetic)
    })?;
    at = skip_spaces(chars, at + name.len());
    let mut open = false;
    if starts_with(chars, at, "\\left(") {
        open = true;
        at += 6;
    } else if chars.get(at) == Some(&'(') {
        open = true;
        at += 1;
    }
    at = skip_spaces(chars, at);
    let mut number = String::new();
    if chars.get(at) == Some(&'-') {
        number.push('-');
        at = skip_spaces(chars, at + 1);
    }
    let digits_start = number.len();
    let mut dots = 0;
    while let Some(&c) = chars.get(at).filter(|c| c.is_ascii_digit() || **c == '.') {
        dots += usize::from(c == '.');
        number.push(c);
        at += 1;
    }
    if number.len() == digits_start || dots > 1 || number.ends_with('.') {
        return None;
    }
    at = skip_spaces(chars, at);
    at += ["°", "^\\circ", "^{\\circ}"]
        .iter()
        .find(|mark| starts_with(chars, at, mark))
        .map(|mark| mark.chars().count())?;
    if open {
        at = skip_spaces(chars, at);
        if starts_with(chars, at, "\\right)") {
            at += 7;
        } else if chars.get(at) == Some(&')') {
            at += 1;
        } else {
            return None;
        }
    }
    Some((format!("{name}(({number})*pi/180)"), at))
}

/// Rewrite `^\circ` and `^{\circ}` behind a digit into the degree sign.
fn degree_sign_pass(chars: &[char]) -> Vec<char> {
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut at = 0;
    while at < chars.len() {
        let mark = ["^\\circ", "^{\\circ}"]
            .iter()
            .find(|mark| starts_with(chars, at, mark));
        match mark {
            Some(mark) if out.last().is_some_and(char::is_ascii_digit) => {
                out.push('°');
                at += mark.chars().count();
            }
            _ => {
                out.push(chars[at]);
                at += 1;
            }
        }
    }
    out
}

/// Rewrite `^{\frac{a}{b}}` into `^((a)/(b))`.
fn fraction_exponent_pass(chars: &[char]) -> Vec<char> {
    let head = "^{\\frac{";
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut at = 0;
    while at < chars.len() {
        if starts_with(chars, at, head)
            && let Some(top_end) = closing_brace(chars, at + 7)
            && chars.get(top_end + 1) == Some(&'{')
            && let Some(bottom_end) = closing_brace(chars, top_end + 1)
            && chars.get(bottom_end + 1) == Some(&'}')
        {
            let top = &chars[at + 8..top_end];
            let bottom = &chars[top_end + 2..bottom_end];
            let plain = |part: &[char]| {
                !part.is_empty() && part.iter().all(|c| c.is_ascii_alphanumeric() || *c == '-')
            };
            let (open, close) = if plain(top) && plain(bottom) {
                ("", "")
            } else {
                ("(", ")")
            };
            out.extend("^(".chars());
            out.extend(open.chars());
            out.extend_from_slice(top);
            out.extend(close.chars());
            out.push('/');
            out.extend(open.chars());
            out.extend_from_slice(bottom);
            out.extend(close.chars());
            out.push(')');
            at = bottom_end + 2;
        } else {
            out.push(chars[at]);
            at += 1;
        }
    }
    out
}
