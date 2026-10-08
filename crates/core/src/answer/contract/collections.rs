//! The natural spellings of lists, sets, coordinates and vectors.
//!
//! A learner writes a list as `{5, 7, 13}`, `x = 2 or x = 3` or `orders 2, 3
//! and 7`, a point as `7 and 3`, `a=6 and b=-2` or `the origin`, and a set
//! without its braces. Each of these is the same answer in another spelling.
//! This reading runs only after the strict reading fails to grade the answer
//! correct. It rewrites the learner text into the spelling the contract reads
//! and grades the rewrite again, so it never turns a correct verdict into a
//! wrong one, and it never accepts a different value: every rewrite keeps the
//! members and drops only words, names and brackets.

use std::cell::Cell;

use super::list;
use super::{AnswerContract, Canon, Undecidable, canonical_form};
use crate::answer::Outcome;

thread_local! {
    static DEPTH: Cell<u8> = const { Cell::new(0) };
}

/// The collection shape a contract reads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// An unordered or ordered list.
    List,
    /// An `exact` key that is a bare list.
    Bare,
    /// An `exact` key that is one value, against a learner value in braces.
    Single,
    /// A set with braces.
    Set,
    /// A point or a vector of this arity.
    Coordinates(usize),
    /// A matrix of this many rows and columns.
    Matrix(usize, usize),
}

/// The verdict of the rewritten learner text, when the rewrite reads and decides.
pub(super) fn rescue(text: &str, learner: &str, contract: &AnswerContract) -> Option<Outcome> {
    if DEPTH.with(Cell::get) > 0 {
        return None;
    }
    let rewritten = rewrite(text, learner, contract)?;
    DEPTH.with(|depth| depth.set(depth.get() + 1));
    let outcome = super::check_contract(text, &rewritten, contract.clone());
    DEPTH.with(|depth| depth.set(depth.get() - 1));
    matches!(outcome, Outcome::Decided(_)).then_some(outcome)
}

fn kind_of(text: &str, contract: &AnswerContract) -> Option<Kind> {
    match contract {
        AnswerContract::List { .. } => Some(Kind::List),
        AnswerContract::Set => Some(Kind::Set),
        AnswerContract::Coordinates { arity } => Some(Kind::Coordinates(usize::from(*arity))),
        AnswerContract::Matrix { rows, cols } => {
            Some(Kind::Matrix(usize::from(*rows), usize::from(*cols)))
        }
        AnswerContract::Exact => {
            let key = text.trim();
            let bare = !key.starts_with(['(', '[']) && !key.contains(" or ");
            let members = list::values_grouped(unbraced(key)).ok()?;
            Some(if bare && members.len() >= 2 {
                Kind::Bare
            } else {
                Kind::Single
            })
        }
        _ => None,
    }
}

/// The learner text in the spelling its contract reads, or `None` when the
/// text needs no rewrite.
fn rewrite(text: &str, learner: &str, contract: &AnswerContract) -> Option<String> {
    let kind = kind_of(text, contract)?;
    let source = learner.trim().trim_end_matches('.').trim();
    let mut s = strip_lead(source).to_owned();
    if kind == Kind::Single {
        let inner = unbraced(&s);
        let one = inner.len() != s.len() && split_top(inner).is_some_and(|m| m.len() == 1);
        return one
            .then(|| inner.to_owned())
            .and_then(|inner| changed(inner, learner));
    }
    // `1,205` for a point of two parts is the point `(1, 205)`, not the number 1205.
    if let Kind::Coordinates(arity) = kind
        && let Some(parts) = split_top(&s)
        && parts.len() == arity
        && !s.starts_with(['(', '[', '{', '<', '⟨'])
        && parts.iter().all(|part| {
            let body = part.strip_prefix('-').unwrap_or(part);
            !body.is_empty() && body.chars().all(|c| c.is_ascii_digit())
        })
    {
        return changed(format!("({})", parts.join(", ")), learner);
    }
    if let Some(joined) = thousands_members(text, &s, kind) {
        return changed(joined, learner);
    }
    if kind == Kind::Set && is_empty_set_word(&s) {
        return changed("{}".to_owned(), learner);
    }
    if let Kind::Coordinates(arity) = kind
        && is_origin_word(&s)
    {
        let zeros = vec!["0"; arity].join(", ");
        return changed(format!("({zeros})"), learner);
    }
    if kind == Kind::Set
        && let Some(inner) = set_builder(&s)
    {
        s = inner;
    }
    if matches!(kind, Kind::List | Kind::Bare)
        && let Some(bounds) = chain_bounds(&s)
    {
        return changed(bounds, learner);
    }
    if let Kind::Coordinates(arity) = kind
        && let Some(point) = combination(&s, arity)
    {
        return changed(point, learner);
    }
    if let Some(after) = equation_tail(&s) {
        s = after.to_owned();
    }
    if matches!(kind, Kind::Coordinates(_) | Kind::Matrix(..)) {
        let members = vector_members(&s).or_else(|| {
            let bracketed = s.starts_with(['(', '[', '{', '<', '⟨']);
            if bracketed {
                None
            } else {
                loose_members(&s, true)
            }
        })?;
        return changed(vector_text(kind, &members, text)?, learner);
    }
    // A tuple or an interval is not a set: its brackets keep their meaning.
    if kind == Kind::Set && s.starts_with(['(', '[']) && wraps(&s) {
        return None;
    }
    let inner = if s.starts_with('{') && s.ends_with('}') && wraps(&s) {
        s[1..s.len() - 1].trim().to_owned()
    } else {
        s.clone()
    };
    if inner.is_empty() {
        return changed("{}".to_owned(), learner).filter(|_| kind == Kind::Set);
    }
    let by_space = spaced_numbers(&inner);
    let members = match by_space {
        Some(members) => members,
        None => loose_members(&inner, false)?,
    };
    let members = clean_members(text, members);
    let members: Vec<String> = if kind == Kind::Bare {
        members
    } else {
        members.into_iter().flat_map(|m| expand_signs(&m)).collect()
    };
    let joined = members.join(", ");
    let result = if kind == Kind::Set {
        format!("{{{joined}}}")
    } else {
        joined
    };
    changed(result, learner)
}

/// A comma list whose members hold a thousands group (`{1,205, 3}`, `(1,205, 3)`)
/// as the list with that group joined (`{1205, 3}`). The groups join only when
/// the learner has more members than the key and the join gives the count of the
/// key, so `(1,205)` for the key `(1,205)` keeps its two members.
fn thousands_members(key: &str, learner: &str, kind: Kind) -> Option<String> {
    let unwrap = |text: &str| -> (String, String, String) {
        let text = text.trim();
        for (open, close) in [('{', '}'), ('(', ')'), ('[', ']')] {
            if text.starts_with(open) && text.ends_with(close) && wraps(text) {
                return (
                    open.to_string(),
                    text[1..text.len() - 1].to_owned(),
                    close.to_string(),
                );
            }
        }
        (String::new(), text.to_owned(), String::new())
    };
    if !matches!(
        kind,
        Kind::Set | Kind::List | Kind::Bare | Kind::Coordinates(_)
    ) {
        return None;
    }
    let (_, key_inner, _) = unwrap(key);
    let (open, inner, close) = unwrap(learner);
    let inner = inner.replace(" and ", ", ");
    let members = split_top(&inner)?;
    let key_count = split_top(&key_inner)?.len();
    if members.len() <= key_count {
        return None;
    }
    let whole = |text: &str| {
        let body = text.strip_prefix('-').unwrap_or(text);
        !body.is_empty() && body.chars().all(|c| c.is_ascii_digit())
    };
    let mut merged: Vec<String> = Vec::new();
    for member in members {
        match merged.last_mut() {
            Some(last)
                if member.len() == 3
                    && whole(&member)
                    && whole(last)
                    && !last.trim_start_matches('-').is_empty() =>
            {
                last.push_str(&member);
            }
            _ => merged.push(member),
        }
    }
    let (open, close) = if open.is_empty() && kind == Kind::Set {
        ("{".to_owned(), "}".to_owned())
    } else {
        (open, close)
    };
    (merged.len() == key_count).then(|| format!("{open}{}{close}", merged.join(", ")))
}

/// The verdict of a list of words (`HH, HT, TH, TT`, `I, III`) that the answer
/// grammar cannot read as numbers: the words compare by spelling, ignoring case.
pub(super) fn word_list(text: &str, learner: &str, contract: &AnswerContract) -> Option<Outcome> {
    let ordered = match contract {
        AnswerContract::List { ordered, member } if matches!(**member, AnswerContract::Exact) => {
            *ordered
        }
        AnswerContract::Exact => false,
        _ => return None,
    };
    let words = |text: &str| -> Option<Vec<String>> {
        let members = loose_members(unbraced(text.trim().trim_end_matches('.')), false)?;
        members
            .iter()
            .map(|m| {
                let m = m.trim();
                (!m.is_empty() && m.len() <= 8 && m.chars().all(|c| c.is_ascii_alphabetic()))
                    .then(|| m.to_lowercase())
            })
            .collect()
    };
    let key = words(text)?;
    if key.len() < 2 || key.iter().all(|word| word.len() < 2) {
        return None;
    }
    let key_refs: Vec<&str> = key.iter().map(String::as_str).collect();
    let stripped = strip_lead(learner.trim()).to_lowercase();
    let given = loose_members(unbraced(stripped.trim().trim_end_matches('.')), false).map(|m| {
        m.into_iter()
            .map(|member| noun_value(&member, &key_refs).unwrap_or(member))
            .collect::<Vec<_>>()
    });
    let Some(given) = given else {
        return Some(verdict(false));
    };
    let mut want = key.clone();
    let mut got: Vec<String> = given.iter().map(|word| word.trim().to_owned()).collect();
    if !ordered {
        want.sort();
        got.sort();
    }
    Some(verdict(want == got))
}

fn verdict(correct: bool) -> Outcome {
    Outcome::Decided(crate::answer::Verdict {
        correct,
        notation: false,
    })
}

fn changed(result: String, learner: &str) -> Option<String> {
    (result != learner.trim()).then_some(result)
}

/// The key of a `set` contract: a bare comma list gets its braces.
pub(super) fn set_key(expected: &str) -> Result<Canon, Undecidable> {
    let text = expected.trim().trim_end_matches('.').trim();
    let braced = text.starts_with(['{', '$']) || text.starts_with("\\{");
    let wrapped_text = matches!(text.chars().next(), Some('(' | '[')) && wraps(text);
    let source = if braced || wrapped_text {
        text.to_owned()
    } else {
        format!("{{{text}}}")
    };
    match canonical_form(&source)? {
        value @ Canon::Set(_) => Ok(value),
        _ => Err(Undecidable::new(
            "the authored answer does not match its contract shape",
        )),
    }
}

// ---------------------------------------------------------------------------
// Reading pieces

const LEADS: [&str; 14] = [
    "the coordinates are",
    "the ordered pair",
    "the answers are",
    "the solutions are",
    "the numbers are",
    "the values are",
    "ordered pair",
    "coordinates",
    "the point",
    "the pair",
    "the set",
    "between",
    "point",
    "pair",
];

/// The text without a leading phrase that only names what follows.
fn strip_lead(text: &str) -> &str {
    let lower = text.to_lowercase();
    for lead in LEADS {
        if let Some(rest) = lower.strip_prefix(lead)
            && (rest.starts_with([' ', ':']) || rest.is_empty())
            && lower.len() == text.len()
        {
            let rest = text[lead.len()..].trim_start_matches([':', ' ']);
            if !rest.is_empty() {
                return rest;
            }
        }
    }
    text
}

fn is_empty_set_word(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    matches!(
        lower.as_str(),
        "empty set"
            | "the empty set"
            | "an empty set"
            | "∅"
            | "\\emptyset"
            | "\\varnothing"
            | "{ }"
            | "{}"
            | "emptyset"
            | "{∅}"
    ) && lower != "{}"
}

fn is_origin_word(text: &str) -> bool {
    matches!(text.trim().to_lowercase().as_str(), "the origin" | "origin")
}

/// Whether one bracket pair encloses the whole of `text`.
fn wraps(text: &str) -> bool {
    let mut depth = 0_usize;
    let last = text.chars().count().saturating_sub(1);
    for (at, ch) in text.chars().enumerate() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return at == last;
                }
            }
            _ => {}
        }
    }
    false
}

fn unbraced(text: &str) -> &str {
    let trimmed = text.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') && wraps(trimmed) {
        trimmed[1..trimmed.len() - 1].trim()
    } else {
        trimmed
    }
}

/// `{x : x in {1, 2, 3}}` as `{1, 2, 3}`.
fn set_builder(text: &str) -> Option<String> {
    let inner = text.trim().strip_prefix('{')?.strip_suffix('}')?;
    let at = inner.find([':', '|'])?;
    let (name, rest) = (inner[..at].trim(), inner[at + 1..].trim());
    if name.is_empty() || !name.chars().all(char::is_alphabetic) {
        return None;
    }
    let rest = rest
        .strip_prefix(name)?
        .trim_start()
        .strip_prefix("in ")
        .or_else(|| rest.strip_prefix(name)?.trim_start().strip_prefix('∈'))?
        .trim();
    (rest.starts_with('{') && rest.ends_with('}') && wraps(rest)).then(|| rest.to_owned())
}

/// `A ∩ B = {1, 2}` as `{1, 2}`: the text after a last `=` when the text
/// before it is a name or a set expression and the text after it is a brace
/// group.
fn equation_tail(text: &str) -> Option<&str> {
    let at = text.rfind('=')?;
    let (before, after) = (&text[..at], text[at + 1..].trim());
    let comparison = before.ends_with(['<', '>', '!']);
    let braced = after.starts_with('{') && after.ends_with('}') && wraps(after);
    (!comparison && braced && !before.contains(['{', ',', '(']) && !before.trim().is_empty())
        .then_some(after)
}

/// A sum of scaled points, `2(1, 2) + 1(3, 0)`, as the one point it adds up to.
/// Only a text with a `+` or `-` between two or more bracketed points reads.
fn combination(text: &str, arity: usize) -> Option<String> {
    let mut terms: Vec<(bool, String, Vec<String>)> = Vec::new();
    let mut depth = 0_usize;
    let mut start = 0;
    let mut negative = false;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut cut = |end: usize, negative: bool, from: usize| -> Option<()> {
        let term = text[from..end].trim();
        let open = term.find('(')?;
        let scalar = term[..open].trim().trim_end_matches('*').trim();
        let scalar = if scalar.is_empty() { "1" } else { scalar };
        if !term.ends_with(')') || !wraps(&term[open..]) {
            return None;
        }
        let members = split_top(&term[open + 1..term.len() - 1])?;
        if members.len() != arity {
            return None;
        }
        terms.push((negative, scalar.to_owned(), members));
        Some(())
    };
    for &(at, ch) in &chars {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.checked_sub(1)?,
            '+' | '-' if depth == 0 && at > start => {
                cut(at, negative, start)?;
                negative = ch == '-';
                start = at + 1;
            }
            _ => {}
        }
    }
    cut(text.len(), negative, start)?;
    if terms.len() < 2 {
        return None;
    }
    let parts: Vec<String> = (0..arity)
        .map(|j| {
            let sum: Vec<String> = terms
                .iter()
                .map(|(neg, scalar, members)| {
                    format!(
                        "{}({scalar})*({})",
                        if *neg { "-" } else { "+" },
                        members[j]
                    )
                })
                .collect();
            format!("({})", sum.join(" ").trim_start_matches('+').trim())
        })
        .collect();
    Some(format!("({})", parts.join(", ")))
}

/// `a < b < c` as `a, c` for a middle that is one name: the two bounds of a
/// "between" answer.
fn chain_bounds(text: &str) -> Option<String> {
    let mut parts = Vec::new();
    let mut ops = Vec::new();
    let rest = text.trim();
    let mut depth = 0_i32;
    let mut start = 0;
    let bytes: Vec<(usize, char)> = rest.char_indices().collect();
    let mut at = 0;
    while at < bytes.len() {
        let (i, c) = bytes[at];
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '<' | '>' if depth == 0 => {
                let eq = rest[i + 1..].starts_with('=');
                parts.push(rest[start..i].trim());
                ops.push((c, eq));
                start = i + 1 + usize::from(eq);
                if eq {
                    at += 1;
                }
            }
            '≤' | '≥' if depth == 0 => {
                parts.push(rest[start..i].trim());
                ops.push((if c == '≤' { '<' } else { '>' }, true));
                start = i + c.len_utf8();
            }
            _ => {}
        }
        at += 1;
    }
    parts.push(rest[start..].trim());
    if parts.len() != 3 || ops.len() != 2 || ops[0].0 != ops[1].0 {
        return None;
    }
    let middle = parts[1];
    if middle.is_empty()
        || !middle
            .chars()
            .all(|ch| ch.is_alphabetic() || matches!(ch, '_' | '\''))
        || parts[0].is_empty()
        || parts[2].is_empty()
        || parts[0].contains(',')
        || parts[2].contains(',')
    {
        return None;
    }
    if ops[0].0 == '<' {
        Some(format!("{}, {}", parts[0], parts[2]))
    } else {
        Some(format!("{}, {}", parts[2], parts[0]))
    }
}

/// The numbers of `1 2 3`: whole numbers and fractions split at spaces.
fn spaced_numbers(text: &str) -> Option<Vec<String>> {
    if text.contains([',', '(', '{', '[']) {
        return None;
    }
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let number = |token: &str| {
        let digits = token.strip_prefix('-').unwrap_or(token);
        !digits.is_empty()
            && digits.chars().any(|c| c.is_ascii_digit())
            && digits
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '.' | '/'))
    };
    (tokens.len() > 1 && tokens.iter().all(|token| number(token)))
        .then(|| tokens.into_iter().map(str::to_owned).collect())
}

/// The members of a text cut at top-level commas, `and`, `or` and `;`, and
/// with `to` as a separator when `ranged` is set.
fn loose_members(text: &str, ranged: bool) -> Option<Vec<String>> {
    let normalized = text
        .replace(", and ", " and ")
        .replace(", or ", ", ")
        .replace(" or ", " and ")
        .replace(';', ",");
    let normalized = if ranged {
        normalized.replace(" to ", " and ")
    } else {
        normalized
    };
    let members = list::values_grouped(&normalized).ok()?;
    Some(members.into_iter().map(str::to_owned).collect())
}

fn is_function_word(word: &str) -> bool {
    matches!(
        word,
        "sqrt"
            | "sin"
            | "cos"
            | "tan"
            | "cot"
            | "sec"
            | "csc"
            | "ln"
            | "log"
            | "exp"
            | "abs"
            | "arcsin"
            | "arccos"
            | "arctan"
            | "pi"
            | "inf"
            | "infinity"
    )
}

/// Whether the key holds a word of its own, so a leading word of the learner
/// text may belong to the answer.
fn key_has_words(key: &str) -> bool {
    key.split(|c: char| !c.is_ascii_lowercase())
        .any(|word| word.len() >= 2 && !is_function_word(word))
}

/// Each member without a leading name (`a=6`), label (`domain 4`) or count noun
/// (`orders 2`).
fn clean_members(key: &str, members: Vec<String>) -> Vec<String> {
    let words = key_has_words(key);
    // Names fall away only when every member has its own (`a=6 and b=-2`).
    let named = !key.contains('=') && members.iter().all(|member| name_value(member).is_some());
    let members_of_key: Vec<&str> = list::values_grouped(unbraced(key)).unwrap_or_default();
    members
        .into_iter()
        .map(|member| {
            let mut member = member.trim().to_owned();
            if named && let Some(value) = name_value(&member) {
                member = value;
            }
            if !words && let Some(value) = noun_value(&member, &members_of_key) {
                member = value;
            }
            member
        })
        .collect()
}

/// `a=6` as `6`: a name, one `=`, and a value.
fn name_value(member: &str) -> Option<String> {
    let at = member.find('=')?;
    let (name, value) = (member[..at].trim(), member[at + 1..].trim());
    let plain = name.chars().next().is_some_and(char::is_alphabetic)
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '\''))
        && name.chars().count() <= 12;
    (plain && !value.is_empty() && !value.contains('=') && !name.ends_with(['<', '>', '!']))
        .then(|| value.to_owned())
}

/// `orders 2` as `2`: one lowercase word in front of a value.
fn noun_value(member: &str, key_members: &[&str]) -> Option<String> {
    let (word, value) = member.split_once(' ')?;
    let value = value.trim();
    let is_word =
        word.len() >= 2 && word.chars().all(|c| c.is_ascii_lowercase()) && !is_function_word(word);
    let starts_value = value
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '(' | '.'));
    (is_word && (starts_value || key_members.contains(&value))).then(|| value.to_owned())
}

/// Each `±` of a member read as its two signs, in every combination.
fn expand_signs(member: &str) -> Vec<String> {
    let spelled = member.replace("\\pm", "±").replace("+/-", "±");
    let count = spelled.matches('±').count();
    if count == 0 || count > 3 {
        return vec![member.to_owned()];
    }
    let mut out = vec![String::new()];
    let mut previous: Option<char> = None;
    for ch in spelled.chars() {
        if ch == '±' {
            let unary =
                previous.is_none_or(|p| matches!(p, '(' | ',' | '=' | '[' | '*' | '/' | '^'));
            let (plus, minus) = if unary { ("", "-") } else { ("+", "-") };
            out = out
                .into_iter()
                .flat_map(|prefix| [format!("{prefix}{plus}"), format!("{prefix}{minus}")])
                .collect();
        } else {
            for prefix in &mut out {
                prefix.push(ch);
            }
        }
        if !ch.is_whitespace() {
            previous = Some(ch);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Points, vectors and matrices

/// The members of a vector in brackets: `(a, b)`, `<a, b>`, `⟨a, b⟩`, `[a, b]`,
/// a column `[[a], [b]]` or a row `[[a, b]]`.
fn vector_members(text: &str) -> Option<Vec<String>> {
    let trimmed = strip_unit_tail(text.trim());
    let (open, close) = match trimmed.chars().next()? {
        '(' => ('(', ')'),
        '[' => ('[', ']'),
        '<' => ('<', '>'),
        '⟨' => ('⟨', '⟩'),
        _ => return None,
    };
    if !trimmed.ends_with(close) || (open == '(' || open == '[') && !wraps(trimmed) {
        return None;
    }
    let inner = trimmed[open.len_utf8()..trimmed.len() - close.len_utf8()].trim();
    // A flat `[a, b]` is an interval, not a point.
    if open == '[' && !inner.starts_with('[') {
        return None;
    }
    if open == '[' && inner.starts_with('[') {
        let rows = split_top(inner)?;
        let cells: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                let row = row.trim();
                if !(row.starts_with('[') && row.ends_with(']') && wraps(row)) {
                    return None;
                }
                split_top(row[1..row.len() - 1].trim())
            })
            .collect::<Option<_>>()?;
        return Some(cells.into_iter().flatten().collect());
    }
    split_top(inner)
}

/// The pieces of a text cut at top-level commas.
fn split_top(text: &str) -> Option<Vec<String>> {
    let mut pieces = Vec::new();
    let mut depth = 0_usize;
    let mut start = 0;
    for (at, ch) in text.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.checked_sub(1)?,
            ',' if depth == 0 => {
                pieces.push(text[start..at].trim().to_owned());
                start = at + 1;
            }
            _ => {}
        }
    }
    pieces.push(text[start..].trim().to_owned());
    (depth == 0 && pieces.iter().all(|piece| !piece.is_empty())).then_some(pieces)
}

/// `(9, 12, 6) grams` without its unit word, when the word is in the unit table.
fn strip_unit_tail(text: &str) -> &str {
    let Some(at) = text.rfind([')', ']', '>']) else {
        return text;
    };
    let tail = text[at + 1..].trim();
    if !tail.is_empty()
        && tail.chars().all(char::is_alphabetic)
        && crate::answer::unit::lookup(tail).is_some()
    {
        &text[..=at]
    } else {
        text
    }
}

/// The learner text of a point or a matrix, from its members.
fn vector_text(kind: Kind, members: &[String], key: &str) -> Option<String> {
    let members = clean_members(key, members.to_vec());
    match kind {
        Kind::Coordinates(arity) if members.len() == arity => {
            Some(format!("({})", members.join(", ")))
        }
        Kind::Matrix(rows, cols) if members.len() == rows * cols && (rows == 1 || cols == 1) => {
            let cells: Vec<String> = if cols == 1 {
                members.iter().map(|m| format!("[{m}]")).collect()
            } else {
                vec![format!("[{}]", members.join(", "))]
            };
            Some(format!("[{}]", cells.join(", ")))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list_contract() -> AnswerContract {
        AnswerContract::List {
            ordered: false,
            member: Box::new(AnswerContract::Exact),
        }
    }

    #[test]
    fn a_rewrite_is_a_fixed_point() {
        let contract = list_contract();
        for learner in [
            "{5, 7, 13}",
            "orders 2, 3 and 7",
            "x = 2 or x = 3",
            "a < b < c",
        ] {
            let once = rewrite("2, 3", learner, &contract);
            if let Some(once) = once {
                assert_eq!(rewrite("2, 3", &once, &contract), None, "{learner}");
            }
        }
    }

    #[test]
    fn signs_expand_in_every_combination() {
        assert_eq!(expand_signs("(±5, ±3)").len(), 4);
        assert_eq!(
            expand_signs("±1 ± i"),
            ["1 + i", "1 - i", "-1 + i", "-1 - i"]
        );
    }
}
