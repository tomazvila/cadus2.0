//! Restore LaTeX commands that a JSON decode turned into control characters.
//!
//! A model that writes `\frac` with one backslash inside a JSON string makes the decoder
//! read `\f` as a form feed. The prose then holds U+000C followed by `rac{3}{4}`. The same
//! happens to `\times` (tab), `\neq` (line feed), `\beta` (backspace) and `\right`
//! (carriage return). Inside a math segment a control character followed by letters is
//! never valid, so the command is put back. The browser has the same repair
//! (`web/src/lib/latex.ts`) for rows stored before this one existed.

/// The command endings that follow each control character, the longest first.
const ENDINGS: [(char, char, &[&str]); 5] = [
    ('\u{c}', 'f', &["rac", "orall"]),
    (
        '\t',
        't',
        &["imes", "heta", "ext", "frac", "riangle", "an", "au", "o"],
    ),
    (
        '\n',
        'n',
        &["otin", "abla", "eq", "ot", "eg", "u", "e", "i"],
    ),
    (
        '\u{8}',
        'b',
        &["inom", "eta", "ar", "igcup", "igcap", "egin"],
    ),
    (
        '\r',
        'r',
        &["ightarrow", "ight", "angle", "floor", "ceil", "ho"],
    ),
];

/// The text after repair, and the command of each repair in order.
#[derive(Debug, PartialEq, Eq)]
pub struct Repaired {
    /// The repaired text.
    pub text: String,
    /// One entry per repair, such as `\frac`.
    pub commands: Vec<String>,
}

/// Restore every damaged command inside `$…$` and `$$…$$`, and log each repair.
///
/// A repair needs the whole command name: the letters after the control character must
/// spell a known command and must not run on into more letters.
#[must_use]
pub fn repair_latex(text: &str) -> Repaired {
    let mut out = String::with_capacity(text.len());
    let mut commands = Vec::new();
    let mut in_math = false;
    let mut rest = text;
    while let Some(ch) = rest.chars().next() {
        let after = &rest[ch.len_utf8()..];
        if ch == '$' {
            in_math = !in_math;
        } else if in_math && let Some((letter, ending)) = known_ending(ch, after) {
            let command = format!("\\{letter}{ending}");
            tracing::warn!(command = %command, "diagnosis: a LaTeX command was repaired");
            out.push_str(&command);
            commands.push(command);
            rest = &after[ending.len()..];
            continue;
        }
        out.push(ch);
        rest = after;
    }
    Repaired {
        text: out,
        commands,
    }
}

/// The command letter and ending when `after` spells a known command after `ch`.
fn known_ending(ch: char, after: &str) -> Option<(char, &'static str)> {
    let (_, letter, endings) = ENDINGS.iter().find(|(control, _, _)| *control == ch)?;
    endings
        .iter()
        .find(|ending| {
            after.strip_prefix(**ending).is_some_and(|tail| {
                !tail
                    .chars()
                    .next()
                    .is_some_and(|next| next.is_ascii_alphabetic())
            })
        })
        .map(|ending| (*letter, *ending))
}

#[cfg(test)]
mod tests {
    use super::repair_latex;

    /// Each damaged command inside math comes back as the command.
    #[test]
    fn a_damaged_command_inside_math_is_restored() {
        let cases = [
            ("$\u{c}rac{3}{4}$", "$\\frac{3}{4}$"),
            ("$2 \times 3$", "$2 \\times 3$"),
            ("$\theta = 5$", "$\\theta = 5$"),
            ("$a \neq b$", "$a \\neq b$"),
            ("$\text{cm}$", "$\\text{cm}$"),
            ("$\\left( x \right)$", "$\\left( x \\right)$"),
            ("$\u{8}eta$", "$\\beta$"),
        ];
        for (damaged, fixed) in cases {
            assert_eq!(repair_latex(damaged).text, fixed, "{damaged:?}");
        }
    }

    /// Text outside math and endings that are not commands stay as they are.
    #[test]
    fn text_outside_math_is_left_alone() {
        let outside = "One\neq two\tand more";
        assert!(repair_latex(outside).commands.is_empty());
        assert_eq!(repair_latex(outside).text, outside);
        let unknown = "$x +\number$";
        assert_eq!(repair_latex(unknown).text, unknown);
    }

    /// Every repair is reported, in order.
    #[test]
    fn every_repair_is_reported() {
        let repaired = repair_latex("$\u{c}rac{1}{2} \times 2$");
        assert_eq!(repaired.commands, vec!["\\frac", "\\times"]);
    }
}
