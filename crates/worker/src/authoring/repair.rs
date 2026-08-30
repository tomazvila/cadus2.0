//! The LaTeX escape repair of the authoring boundary (spec section 5, trap T1).
//!
//! # The trap
//!
//! A model that writes its own JSON with ONE backslash emits `"$\times$"`. That
//! is valid JSON, and `\t` is a valid JSON escape, so `serde_json` accepts it
//! and hands back `$<TAB>imes$`. The LaTeX command is gone before any gate sees
//! it. The same trap eats the first letter of `\boxed`, `\frac`, `\neq`,
//! `\rightarrow` and `\vec`.
//!
//! Nothing downstream catches it. The template gate reads braces and
//! placeholders and skips every other character; the teach gate and the hint
//! gate accept any string that is not empty after a trim. KaTeX then paints the
//! raw source red, and the append-only `events` table keeps the mangled
//! statement forever. A TEMPLATE is worse than one problem: the structure
//! mangles every instance it ever renders, and C6 binds the approval to those
//! bytes (M6 review finding F3).
//!
//! # The repair
//!
//! [`repair_latex_escapes`] is the port of 1.0 `repair_latex_escapes`
//! (`prompts.py:1021-1048`), and it is deliberately conservative:
//!
//! - Backspace, form feed, vertical tab and carriage return never belong in
//!   tutor text, so a run of ASCII letters after one of them always becomes a
//!   LaTeX command.
//! - TAB and LF are legal whitespace. The repair touches them inside a `$…$` or
//!   `$$…$$` span only, and only when the result names a real KaTeX command.
//!   Both guards carry weight: a prose line that starts with "to" is common, and
//!   display math on its own line puts a real LF in front of a letter.
//! - Text with no control character comes back unchanged.
//!
//! # Where 2.0 leaves 1.0
//!
//! 1.0 DROPS the control characters that survive the repair
//! (`prompts.py:1046`). 2.0 refuses the body instead: the authoring pipeline is
//! offline and it retries, so a refusal costs one more attempt and buys a
//! document nobody has to read twice. [`repair_arguments`] therefore answers a
//! [`Rejection`] whose message is [`CONTROL_CHARACTER`], and the retry block
//! hands that literal sentence to the next attempt.
//!
//! LF stays legal after the repair. A teach page writes a paragraph, and a
//! newline inside one is text and not corruption. Every other C0 control
//! character, TAB included, refuses the body.
//!
//! # The two fields the repair never touches
//!
//! [`NO_REPAIR_FIELDS`] names them. `answer_expr` is SymPy source that the
//! server evaluates and never renders, and `expected` is the answer a worked
//! sample states. A rewrite of either changes what the document COMPUTES, so
//! the repair leaves both alone and the control-character check still reads
//! them: a tab in an expression refuses the body (1.0 `prompts.py:1190-1192`,
//! "`answer_expr` is deliberately left alone").

use cadus_core::template::Rejection;
use serde_json::Value;

/// The `Rejection::code` of a body that keeps a control character.
pub const CONTROL_CODE: &str = "latex-control";

/// The literal message a body with a surviving control character earns.
///
/// The sentence is what the next attempt reads, so it names the cause and the
/// repair, exactly as every other authoring rejection does.
pub const CONTROL_CHARACTER: &str = "the text carries a control character that is not a LaTeX \
command — a JSON string eats the first letter of \\times when the backslash is written once, so \
write EVERY backslash of a LaTeX command twice";

/// The tool-argument fields the repair never rewrites.
///
/// `answer_expr` is the expression the server evaluates, and `expected` is the
/// answer of a worked sample. Both are computed over, never rendered, and a
/// repair of either changes the document's arithmetic.
pub const NO_REPAIR_FIELDS: [&str; 2] = ["answer_expr", "expected"];

/// The four control characters that never belong in tutor text.
///
/// A backspace, form feed, vertical tab or carriage return in front of a letter
/// is always the mangled form of a LaTeX command, so the repair restores those
/// four everywhere (1.0 `_ALWAYS_MANGLED`).
const ALWAYS_MANGLED: [char; 4] = ['\u{8}', '\u{c}', '\u{b}', '\r'];

/// The two control characters that are real whitespace as well.
///
/// The repair restores them inside a math span only, and only when the letters
/// after them spell a command of [`ambiguous_commands`] (1.0
/// `_AMBIGUOUS_COMMANDS`).
const AMBIGUOUS: [char; 2] = ['\t', '\n'];

/// The KaTeX commands that start with `t`, for the TAB case (1.0
/// `_T_COMMANDS`).
const T_COMMANDS: [&str; 18] = [
    "tan",
    "tanh",
    "tau",
    "tbinom",
    "text",
    "textbf",
    "textit",
    "textrm",
    "texttt",
    "tfrac",
    "therefore",
    "theta",
    "tilde",
    "times",
    "to",
    "top",
    "triangle",
    "triangleq",
];

/// The KaTeX commands that start with `n`, for the LF case (1.0
/// `_N_COMMANDS`).
const N_COMMANDS: [&str; 17] = [
    "nabla",
    "ne",
    "neq",
    "newline",
    "ngeq",
    "nleftarrow",
    "nleq",
    "nmid",
    "nolimits",
    "nonumber",
    "not",
    "notin",
    "nparallel",
    "nrightarrow",
    "nsubseteq",
    "nsupseteq",
    "nu",
];

/// The ASCII letter the model meant to put after a backslash (1.0
/// `_ESCAPE_LETTER`).
const fn escape_letter(control: char) -> Option<char> {
    match control {
        '\u{8}' => Some('b'),
        '\u{c}' => Some('f'),
        '\u{b}' => Some('v'),
        '\r' => Some('r'),
        '\t' => Some('t'),
        '\n' => Some('n'),
        _ => None,
    }
}

/// The command list one ambiguous control character may spell.
const fn ambiguous_commands(control: char) -> &'static [&'static str] {
    match control {
        '\t' => &T_COMMANDS,
        '\n' => &N_COMMANDS,
        _ => &[],
    }
}

/// Whether one character is a C0 control character or DEL.
#[must_use]
pub const fn is_control(ch: char) -> bool {
    (ch as u32) < 0x20 || ch == '\u{7f}'
}

/// The first control character a repaired text still carries, if it holds one.
///
/// LF is legal text and never refuses a body. Every other C0 control character
/// and DEL does, TAB included: after the repair a tab is either a mangled
/// command the repair could not name or corruption of the event payload.
#[must_use]
pub fn stray_control(text: &str) -> Option<char> {
    text.chars().find(|ch| is_control(*ch) && *ch != '\n')
}

/// Turn `<control><letters>` back into `\<letter><letters>`.
///
/// `controls` names the control characters to repair. With `commands` set, the
/// restored command must appear in [`ambiguous_commands`] — the guard that keeps
/// a real TAB or LF in front of an ordinary word (1.0 `_restore`).
fn restore(text: &[char], controls: &[char], commands: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        let ch = text[index];
        let Some(letter) = escape_letter(ch).filter(|_| controls.contains(&ch)) else {
            out.push(ch);
            index += 1;
            continue;
        };
        let mut end = index + 1;
        while end < text.len() && text[end].is_ascii_alphabetic() {
            end += 1;
        }
        let word: String = text[index + 1..end].iter().collect();
        let named = format!("{letter}{word}");
        if word.is_empty() || (commands && !ambiguous_commands(ch).contains(&named.as_str())) {
            out.push(ch);
            index += 1;
            continue;
        }
        out.push('\\');
        out.push_str(&named);
        index = end;
    }
    out
}

/// The end of the `$…$` or `$$…$$` span that starts at `start`.
///
/// The answer is the index after the closing delimiter. `None` means the text
/// holds no closing delimiter, so the `$` at `start` is an ordinary character. A
/// `$$` span wins over a `$` span, exactly as 1.0's alternation puts `\$\$`
/// first.
fn span_end(text: &[char], start: usize) -> Option<usize> {
    if text.get(start + 1) == Some(&'$') {
        let mut index = start + 2;
        while index + 1 < text.len() {
            if text[index] == '$' && text[index + 1] == '$' {
                return Some(index + 2);
            }
            index += 1;
        }
        return None;
    }
    let mut index = start + 1;
    while index < text.len() {
        if text[index] == '$' {
            return Some(index + 1);
        }
        index += 1;
    }
    None
}

/// Restore TAB and LF inside every math span of `text`, and nowhere else.
fn restore_in_math(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(chars.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index] != '$' {
            out.push(chars[index]);
            index += 1;
            continue;
        }
        match span_end(&chars, index) {
            Some(end) => {
                out.push_str(&restore(&chars[index..end], &AMBIGUOUS, true));
                index = end;
            }
            None => {
                out.push('$');
                index += 1;
            }
        }
    }
    out
}

/// Undo the JSON control escapes that eat the first letter of a LaTeX command.
///
/// The port of 1.0 `repair_latex_escapes` (`prompts.py:1021-1048`), less the
/// final drop: 2.0 refuses a body that still carries a control character
/// ([`repair_arguments`]).
///
/// Text with no control character comes back unchanged.
#[must_use]
pub fn repair_latex_escapes(text: &str) -> String {
    if !text.chars().any(is_control) {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let repaired = restore(&chars, &ALWAYS_MANGLED, false);
    if repaired.contains('\t') || repaired.contains('\n') {
        restore_in_math(&repaired)
    } else {
        repaired
    }
}

/// Repair every text field of one decoded tool call, and refuse what is left.
///
/// The walk covers every string of the arguments at every depth — the
/// statement, the solution sketch, every hint, the concept, every worked step,
/// and every distractor answer and note — except the fields of
/// [`NO_REPAIR_FIELDS`]. It runs BEFORE any gate, because a gate reads structure
/// and no gate reads a control character (M6 review finding F3).
///
/// # Errors
///
/// Returns a [`Rejection`] with code [`CONTROL_CODE`] and the literal message
/// [`CONTROL_CHARACTER`] when a string or an object key still carries a control
/// character that is not LF.
pub fn repair_arguments(arguments: &Value) -> Result<Value, Rejection> {
    let mut repaired = arguments.clone();
    walk(&mut repaired, true);
    check(&repaired)?;
    Ok(repaired)
}

/// Repair every string of `value`. `repair` is false inside a field of
/// [`NO_REPAIR_FIELDS`].
fn walk(value: &mut Value, repair: bool) {
    match value {
        Value::String(text) => {
            if repair {
                let fixed = repair_latex_escapes(text);
                if &fixed != text {
                    *text = fixed;
                }
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                walk(item, repair);
            }
        }
        Value::Object(fields) => {
            for (key, item) in fields.iter_mut() {
                walk(item, repair && !NO_REPAIR_FIELDS.contains(&key.as_str()));
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// Refuse the first string or key of `value` that keeps a control character.
fn check(value: &Value) -> Result<(), Rejection> {
    match value {
        Value::String(text) => refuse(text),
        Value::Array(items) => items.iter().try_for_each(check),
        Value::Object(fields) => fields.iter().try_for_each(|(key, item)| {
            refuse(key)?;
            check(item)
        }),
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
    }
}

/// The rejection one text earns when it keeps a control character.
fn refuse(text: &str) -> Result<(), Rejection> {
    match stray_control(text) {
        None => Ok(()),
        Some(ch) => {
            tracing::warn!(
                code_point = format!("U+{:04X}", ch as u32),
                "authoring: the tool arguments keep a control character after the LaTeX repair"
            );
            Err(Rejection {
                code: CONTROL_CODE,
                message: CONTROL_CHARACTER.to_owned(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    // Every `\t`, `\n` and `\r` in the literals below is the control character a
    // JSON decoder hands back for an under-escaped `\times`, `\neq` and
    // `\rightarrow`. That is the input the repair exists for.
    use super::{
        CONTROL_CHARACTER, CONTROL_CODE, repair_arguments, repair_latex_escapes, stray_control,
    };
    use serde_json::json;

    /// Spec trap T1, as the JSON decoder hands it back.
    #[test]
    fn the_under_escaped_command_comes_back() {
        let decoded: String =
            serde_json::from_str(r#""Compute ${a} \times 3$.""#).expect("the JSON reads");
        assert!(decoded.contains('\t'), "the TAB is the bug being repaired");
        assert!(!decoded.contains("\\times"));

        assert_eq!(repair_latex_escapes(&decoded), "Compute ${a} \\times 3$.");
    }

    /// The four always-mangled controls are restored anywhere, math or prose.
    #[test]
    fn the_always_mangled_controls_are_restored_outside_math() {
        assert_eq!(
            repair_latex_escapes("\u{c}rac{{1}}{{2}}"),
            "\\frac{{1}}{{2}}"
        );
        assert_eq!(repair_latex_escapes("\u{8}oxed{{7}}"), "\\boxed{{7}}");
        assert_eq!(repair_latex_escapes("\u{b}ec x"), "\\vec x");
        assert_eq!(repair_latex_escapes("\rightarrow"), "\\rightarrow");
    }

    /// A real TAB or LF outside math stays, and no prose word becomes a command.
    #[test]
    fn a_real_newline_and_a_prose_word_are_left_alone() {
        assert_eq!(repair_latex_escapes("one\ntwo"), "one\ntwo");
        assert_eq!(repair_latex_escapes("$a\tnot b$"), "$a\tnot b$");
        assert_eq!(repair_latex_escapes("no control here"), "no control here");
    }

    /// Inside a math span, TAB and LF are restored only for a real command.
    #[test]
    fn a_math_span_restores_a_named_command_only() {
        assert_eq!(repair_latex_escapes("$$x \times y$$"), "$$x \\times y$$");
        assert_eq!(repair_latex_escapes("$a \neq b$"), "$a \\neq b$");
        assert_eq!(repair_latex_escapes("$a \tzzz$"), "$a \tzzz$");
    }

    /// LF survives the check; every other control character refuses the body.
    #[test]
    fn the_check_keeps_a_newline_and_refuses_a_tab() {
        assert_eq!(stray_control("one\ntwo"), None);
        assert_eq!(stray_control("one\ttwo"), Some('\t'));
        assert_eq!(stray_control("one\u{1}two"), Some('\u{1}'));
    }

    /// The two computed fields are never rewritten, and the check still reads
    /// them.
    #[test]
    fn the_expression_fields_are_not_repaired_and_still_refuse_a_tab() {
        let rejection = repair_arguments(&json!({"answer_expr": "a\times"}))
            .expect_err("a tab in the expression refuses the body");
        assert_eq!(rejection.code, CONTROL_CODE);
        assert_eq!(rejection.message, CONTROL_CHARACTER);

        let kept = repair_arguments(&json!({"answer_expr": "a**2", "expected": "144"}))
            .expect("no control character");
        assert_eq!(kept["answer_expr"], json!("a**2"));
        assert_eq!(kept["expected"], json!("144"));
    }

    /// The walk reaches every text field at every depth.
    #[test]
    fn the_walk_repairs_hints_steps_and_distractor_notes() {
        let repaired = repair_arguments(&json!({
            "statement": "Compute ${a} \times 3$.",
            "hints": ["Think of $2 \times 3$."],
            "worked_example": {"steps": ["$6 \times 6 = 36$."]},
            "distractors": [{"answer": "a", "note": "It reads $a \times b$ backwards."}],
        }))
        .expect("the repair leaves no control character");

        assert_eq!(repaired["statement"], json!("Compute ${a} \\times 3$."));
        assert_eq!(repaired["hints"][0], json!("Think of $2 \\times 3$."));
        assert_eq!(
            repaired["worked_example"]["steps"][0],
            json!("$6 \\times 6 = 36$.")
        );
        assert_eq!(
            repaired["distractors"][0]["note"],
            json!("It reads $a \\times b$ backwards.")
        );
    }
}
