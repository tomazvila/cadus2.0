//! Natural readings of a learner answer that the strict grammar misreads.
//!
//! Percent numbers can be written with or without the percent sign/word. The
//! authored percent literal is divided by 100 by the grammar, while learners
//! often write its displayed number. This module rewrites that number and
//! grades it with the same exact rule. Named units are always retained; measured
//! values use an explicit `Unit` contract.

use super::check::{Outcome, canonical_form};

/// The longest learner text this module rewrites.
const MAX_CHARS: usize = 64;

/// Unit and currency words the strict grammar does not hold. A single letter
/// is a unit only through this list or the grammar's own unit table, so `18 x`
/// keeps its strict reading.
const UNIT_WORDS: &[&str] = &[
    "mm",
    "cm",
    "m",
    "km",
    "g",
    "kg",
    "mg",
    "ml",
    "l",
    "s",
    "min",
    "h",
    "hr",
    "hrs",
    "ft",
    "feet",
    "foot",
    "in",
    "inch",
    "inches",
    "yd",
    "yard",
    "yards",
    "mi",
    "mile",
    "miles",
    "mph",
    "lb",
    "lbs",
    "oz",
    "metre",
    "metres",
    "meter",
    "meters",
    "centimetre",
    "centimetres",
    "centimeter",
    "centimeters",
    "millimetre",
    "millimetres",
    "kilometre",
    "kilometres",
    "kilometer",
    "kilometers",
    "gram",
    "grams",
    "kilogram",
    "kilograms",
    "litre",
    "litres",
    "liter",
    "liters",
    "millilitre",
    "millilitres",
    "second",
    "seconds",
    "minute",
    "minutes",
    "hour",
    "hours",
    "day",
    "days",
    "euro",
    "euros",
    "dollar",
    "dollars",
    "cent",
    "cents",
];

/// Currency marks a learner writes in front of or behind an amount.
const CURRENCY: [char; 4] = ['€', '$', '£', '°'];

/// Grade a natural reading of `learner` when the strict outcome was not
/// "correct". `grade` is the strict rule of the caller, applied to the
/// rewritten pair. `None` keeps the strict outcome.
pub(crate) fn rescue(
    expected: &str,
    learner: &str,
    strict: Outcome,
    grade: impl Fn(&str, &str) -> Outcome,
) -> Option<Outcome> {
    if matches!(strict, Outcome::Decided(verdict) if verdict.correct)
        || learner.chars().count() > MAX_CHARS
    {
        return None;
    }
    if let Some(number) = percent_number(expected) {
        // A percent key never takes the unit reading: `0.65 ft` or `65 ft` is
        // not a percent, and the rewrite below would grade it as `0.65` or `65`.
        let bare = strip_percent_word(learner);
        if strip_unit(bare).is_some() {
            return None;
        }
        let outcome = grade(number, bare);
        return matches!(outcome, Outcome::Decided(_)).then_some(outcome);
    }
    None
}

/// The number of a percent literal (`65%` gives `65`), or `None`.
fn percent_number(expected: &str) -> Option<&str> {
    let number = expected.trim().strip_suffix('%')?.trim_end();
    let digits = number.strip_prefix('-').unwrap_or(number);
    let plain = !digits.is_empty()
        && digits.chars().any(|ch| ch.is_ascii_digit())
        && digits
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, '.' | '/'));
    plain.then_some(number)
}

/// The learner text without a trailing `percent` or `per cent` word.
fn strip_percent_word(learner: &str) -> &str {
    let text = learner.trim().trim_end_matches('.').trim_end();
    for word in [" per cent", " percent", "percent"] {
        if let Some(head) = text
            .len()
            .checked_sub(word.len())
            .filter(|at| text.is_char_boundary(*at))
            .filter(|at| text[*at..].eq_ignore_ascii_case(word))
            .map(|at| text[..at].trim_end())
        {
            return head;
        }
    }
    text
}

/// The learner text without one currency mark and one trailing unit, or
/// `None` when the text carries neither.
fn strip_unit(learner: &str) -> Option<String> {
    let mut text = learner.trim().trim_end_matches('.').trim_end();
    let mut changed = false;
    // `$...$` is a math delimiter pair, not a dollar amount; the grammar owns it.
    if text.len() > 1 && text.starts_with('$') && text.ends_with('$') {
        return None;
    }
    if let Some(rest) = text.strip_prefix(CURRENCY) {
        text = rest.trim_start();
        changed = true;
    }
    if let Some(rest) = text.strip_suffix(CURRENCY) {
        text = rest.trim_end();
        changed = true;
    }
    if let Some((head, tail)) = text.rsplit_once(char::is_whitespace)
        && is_unit_word(tail)
    {
        text = head.trim_end();
        changed = true;
    }
    changed.then(|| text.to_owned())
}

/// True for a unit or currency word the strict grammar does not read as a
/// value: the word list above, or a word of two letters or more that names no
/// function, constant or variable of the grammar.
fn is_unit_word(word: &str) -> bool {
    let lower = word.to_lowercase();
    if UNIT_WORDS.contains(&lower.as_str())
        || matches!(word, "L" | "m^2" | "cm^2" | "m^3" | "cm^3" | "km/h" | "m/s")
    {
        return true;
    }
    word.chars().count() >= 2
        && word.chars().all(char::is_alphabetic)
        && !super::parse::FUNCTIONS.contains(&lower.as_str())
        && !matches!(
            lower.as_str(),
            "pi" | "e" | "inf" | "infinity" | "or" | "and"
        )
        && canonical_form(word).is_err()
}

/// The most words a counted noun phrase may carry ("12 red apples").
const MAX_COUNT_WORDS: usize = 3;

/// The plain words that may open a counted answer ("She gets 12 sections",
/// "there are 8 packs"). No word that negates or hedges is on the list.
const LEAD_WORDS: &[&str] = &[
    "she", "he", "they", "it", "we", "i", "you", "there", "are", "is", "gets", "get", "has",
    "have", "makes", "make", "the", "answer", "total", "a", "an",
];

/// The number of a counted answer to a "how many" question.
///
/// A learner answers "How many packs are there?" with `8 packs`. The strict
/// grammar refuses the noun, so this reading returns the number part when the
/// question asks "how many", the authored key is a bare integer, and the rest
/// of the answer is one to three plain words. A word is never swallowed when it
/// is a unit or currency of the unit table or of [`UNIT_WORDS`], a function or
/// constant of the grammar, `and`/`or`, or any word the grammar reads as a
/// value (`xy` is a product of variables). So `12 cm`, `12 x`, `12 xy` and
/// `12 or 13` keep their strict reading, and the caller grades the number under
/// the item's own contract.
#[must_use]
pub fn count_answer(problem: &str, key: &str, learner: &str) -> Option<String> {
    if learner.chars().count() > MAX_CHARS || !problem.to_lowercase().contains("how many") {
        return None;
    }
    match canonical_form(key) {
        Ok(super::Canon::Rational(value)) if value.is_integer() => {}
        _ => return None,
    }
    let text = learner.trim();
    let text = text.strip_suffix('.').unwrap_or(text).trim_end();
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let lead = tokens
        .iter()
        .take_while(|token| LEAD_WORDS.contains(&token.to_lowercase().as_str()))
        .count();
    let tokens = tokens.get(lead..)?.to_vec();
    let first_word = tokens.iter().position(|token| {
        token.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
            && token.chars().any(|c| c.is_ascii_alphabetic())
    })?;
    let (number, words) = tokens.split_at(first_word);
    if number.is_empty() || words.is_empty() || words.len() > MAX_COUNT_WORDS {
        return None;
    }
    if !words.iter().all(|word| countable_word(word)) {
        return None;
    }
    let number = number.join(" ");
    matches!(canonical_form(&number), Ok(super::Canon::Rational(_))).then_some(number)
}

/// A plain word of a counted noun phrase: letters (and inner hyphens) only, two
/// or more letters, no unit, no grammar word, and no value of the grammar.
fn countable_word(word: &str) -> bool {
    let lower = word.to_lowercase();
    word.chars().filter(char::is_ascii_alphabetic).count() >= 2
        && word.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
        && !word.starts_with('-')
        && !word.ends_with('-')
        && !UNIT_WORDS.contains(&lower.as_str())
        && super::unit::lookup(word).is_none()
        && super::unit::lookup(&lower).is_none()
        && !super::parse::FUNCTIONS.contains(&lower.as_str())
        && !matches!(
            lower.as_str(),
            "pi" | "e"
                | "inf"
                | "infinity"
                | "or"
                | "and"
                | "plus"
                | "minus"
                | "over"
                | "per"
                | "squared"
                | "cubed"
                | "percent"
                | "cent"
                | "cents"
        )
        && canonical_form(word).is_err()
}

/// The unit phrases a quantity answer may spell out, as the unit table writes
/// them. Longer phrases come first, so "square metres" wins over "metres".
const UNIT_PHRASES: &[(&str, &str)] = &[
    ("square centimetres", "cm^2"),
    ("square centimeters", "cm^2"),
    ("square centimetre", "cm^2"),
    ("square centimeter", "cm^2"),
    ("cubic centimetres", "cm^3"),
    ("cubic centimeters", "cm^3"),
    ("cubic centimetre", "cm^3"),
    ("cubic centimeter", "cm^3"),
    ("square metres", "m^2"),
    ("square meters", "m^2"),
    ("square metre", "m^2"),
    ("square meter", "m^2"),
    ("cubic metres", "m^3"),
    ("cubic meters", "m^3"),
    ("cubic metre", "m^3"),
    ("cubic meter", "m^3"),
    ("sq cm", "cm^2"),
    ("sq m", "m^2"),
    ("secs", "s"),
    ("sec", "s"),
    ("hrs", "h"),
    ("hr", "h"),
    ("mins", "min"),
];

/// Words that only describe a measured length ("10 m long").
const DESCRIPTORS: &[&str] = &[
    "long", "wide", "high", "tall", "deep", "away", "thick", "across",
];

/// A quantity answer with its spelled-out unit phrase in table spelling and a
/// trailing descriptor removed: `25 square metres` reads `25 m^2`, `4 sec`
/// reads `4 s`, `10 m long` reads `10 m`. The rewrite touches only the words
/// after the last number, so a variable inside the value is never rewritten.
#[must_use]
pub fn unit_phrases(text: &str) -> String {
    let mut words: Vec<String> = text.split_whitespace().map(str::to_owned).collect();
    let is_unit_end = |word: &str| {
        super::unit::lookup(word).is_some()
            || UNIT_PHRASES.iter().any(|(spoken, _)| {
                spoken
                    .rsplit(' ')
                    .next()
                    .is_some_and(|end| end.eq_ignore_ascii_case(word))
            })
    };
    if words.len() >= 3
        && words
            .last()
            .is_some_and(|word| DESCRIPTORS.contains(&word.as_str()))
        && is_unit_end(&words[words.len() - 2])
    {
        words.pop();
    }
    for size in [2_usize, 1] {
        if words.len() <= size {
            continue;
        }
        let phrase = words[words.len() - size..].join(" ");
        if let Some((_, table)) = UNIT_PHRASES
            .iter()
            .find(|(spoken, _)| phrase.eq_ignore_ascii_case(spoken))
        {
            words.truncate(words.len() - size);
            words.push((*table).to_owned());
            break;
        }
    }
    words.join(" ")
}

/// A number in scientific notation written with `x` or `X` as the times sign,
/// in e-notation, or with a superscript exponent, in the grammar's spelling:
/// `3.2x10^5`, `3.2e5`, `7.2×10⁻⁴` read `3.2*10^(5)`, `3.2*10^(5)`,
/// `7.2*10^(-4)`. Only a whole answer of that shape is read; anything else is
/// `None`.
#[must_use]
pub fn scientific(text: &str) -> Option<String> {
    let text = text.trim();
    if text.chars().count() > MAX_CHARS {
        return None;
    }
    let mantissa_end = text
        .char_indices()
        .find(|(at, c)| !(c.is_ascii_digit() || *c == '.' || (*at == 0 && matches!(c, '-' | '+'))))
        .map_or(text.len(), |(at, _)| at);
    let mantissa = &text[..mantissa_end];
    if !mantissa.chars().any(|c| c.is_ascii_digit()) || mantissa.matches('.').count() > 1 {
        return None;
    }
    let rest = text[mantissa_end..].trim_start();
    let exponent = if let Some(e) = rest.strip_prefix(['e', 'E']) {
        e.to_owned()
    } else {
        let after = rest
            .strip_prefix("\\times")
            .or_else(|| rest.strip_prefix(['x', 'X', '×', '*', '·']))?
            .trim_start()
            .strip_prefix("10")?
            .trim_start();
        if let Some(power) = after.strip_prefix('^') {
            let power = power.trim();
            power
                .strip_prefix('(')
                .and_then(|p| p.strip_suffix(')'))
                .or_else(|| power.strip_prefix('{').and_then(|p| p.strip_suffix('}')))
                .unwrap_or(power)
                .trim()
                .to_owned()
        } else {
            superscript(after)?
        }
    };
    let exponent = exponent.replace('−', "-");
    let digits = exponent.strip_prefix(['-', '+']).unwrap_or(&exponent);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) || digits.len() > 4 {
        return None;
    }
    Some(format!("{mantissa}*10^({exponent})"))
}

/// A superscript exponent such as `⁻⁴` or `⁵` in ASCII, or `None`.
fn superscript(text: &str) -> Option<String> {
    let mut out = String::new();
    for c in text.trim().chars() {
        out.push(match c {
            '⁻' => '-',
            '⁺' => '+',
            '⁰' => '0',
            '¹' => '1',
            '²' => '2',
            '³' => '3',
            '⁴' => '4',
            '⁵' => '5',
            '⁶' => '6',
            '⁷' => '7',
            '⁸' => '8',
            '⁹' => '9',
            _ => return None,
        });
    }
    (!out.is_empty()).then_some(out)
}

/// Names a learner may put before a measured or algebraic answer ("A = ...").
const QUANTITY_NAMES: &[&str] = &[
    "a",
    "p",
    "l",
    "v",
    "w",
    "h",
    "d",
    "area",
    "perimeter",
    "length",
    "width",
    "height",
    "volume",
    "side",
    "distance",
];

/// The answer to a measured question without its decoration, for grading
/// under the item's own contract.
///
/// The served question decides what may fall away. A leading `≈` goes when the
/// key is a number. A leading quantity name (`A =`, `Area =`, `P =`) goes when
/// the key names nothing. A trailing unit goes when the question itself names a
/// unit of the same kind (`(x+4)(x+5) m^2` for "area ... square metres",
/// `7.1 m` for "... metres"), the unit follows a space, and no letter of the
/// unit is a variable of the key (`5 m` stays whole for the key `5m`). `None`
/// when nothing falls away.
#[must_use]
pub fn measured_answer(problem: &str, key: &str, learner: &str) -> Option<String> {
    if learner.chars().count() > MAX_CHARS || key.contains('=') {
        return None;
    }
    let mut text = unit_phrases(learner);
    if matches!(canonical_form(key), Ok(super::Canon::Rational(_))) {
        if let Some(rest) = text.trim_start().strip_prefix('≈') {
            text = rest.trim().to_owned();
        } else if let Some(rest) = strip_hedge_word(&text) {
            text = rest.to_owned();
        }
    }
    text = squared_unit(&text);
    if let Some((name, value)) = text.split_once('=')
        && QUANTITY_NAMES.contains(&name.trim().to_lowercase().as_str())
        && !value.contains('=')
    {
        text = value.trim().to_owned();
    }
    if let Some((value, unit)) = text.rsplit_once(' ')
        && let Some(found) = super::unit::lookup(unit)
        && !value.trim().is_empty()
        && problem_names_unit(problem, found)
        && !key_uses_letters(key, unit)
    {
        text = value.trim().to_owned();
    }
    (text != learner.trim()).then_some(text)
}

/// The words that mark an estimate in front of a number ("about 7.1").
const HEDGE_WORDS: &[&str] = &["about", "approximately", "around", "roughly", "nearly"];

/// The text after one leading hedge word, or `None` when the text has none.
fn strip_hedge_word(text: &str) -> Option<&str> {
    let (word, rest) = text.trim_start().split_once(' ')?;
    HEDGE_WORDS
        .contains(&word.to_lowercase().as_str())
        .then(|| rest.trim())
}

/// The text with a superscript digit on its last word written as a power
/// (`m²` reads `m^2`).
fn squared_unit(text: &str) -> String {
    let Some((head, last)) = text.rsplit_once(' ') else {
        return text.to_owned();
    };
    for (mark, power) in [('²', "^2"), ('³', "^3")] {
        if let Some(unit) = last.strip_suffix(mark)
            && !unit.is_empty()
            && unit.chars().all(|c| c.is_ascii_alphabetic())
        {
            return format!("{head} {unit}{power}");
        }
    }
    text.to_owned()
}

/// Whether the question names `unit` itself ("metres" and "m" are one unit; "cm"
/// is another), so `7.1 cm` never passes for `7.1` in a question in metres.
fn problem_names_unit(problem: &str, unit: &super::unit::Unit) -> bool {
    let plain = problem.replace(['$', '{', '}'], "").replace('\\', " ");
    let spelled = unit_phrases_everywhere(&plain);
    spelled
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | '.' | ';' | ':' | '?' | '(' | ')'))
        .filter(|word| !word.is_empty())
        .any(|word| {
            super::unit::lookup(word).is_some_and(|named| {
                named.quantity == unit.quantity
                    && named.factor() == unit.factor()
                    && named.offset() == unit.offset()
            })
        })
}

/// Every spelled-out unit phrase of a text in table spelling.
fn unit_phrases_everywhere(text: &str) -> String {
    let mut out = text.replace("^ 2", "^2").replace("^ 3", "^3");
    for (spoken, table) in UNIT_PHRASES {
        let mut lower = out.to_lowercase();
        while let Some(at) = lower.find(spoken) {
            out.replace_range(at..at + spoken.len(), table);
            lower = out.to_lowercase();
        }
    }
    out
}

/// Whether any letter of `unit` is a variable of the key.
fn key_uses_letters(key: &str, unit: &str) -> bool {
    let letters: Vec<char> = unit.chars().filter(char::is_ascii_alphabetic).collect();
    let Ok(ast) = super::parse(&super::normalize(key).source) else {
        return true;
    };
    let mut names = Vec::new();
    variables(&ast, &mut names);
    names
        .iter()
        .any(|name| name.chars().any(|c| letters.contains(&c)))
}

/// The variable names of an expression.
fn variables(node: &super::Ast, out: &mut Vec<String>) {
    use super::Ast;
    match node {
        Ast::Var(name) => out.push(name.clone()),
        Ast::Neg(inner) | Ast::Pow(inner, _) | Ast::Sqrt(inner) => variables(inner, out),
        Ast::RationalPow { base, .. } => variables(base, out),
        Ast::Add(items)
        | Ast::Mul(items)
        | Ast::Tuple(items)
        | Ast::Set(items)
        | Ast::List(items) => {
            items.iter().for_each(|item| variables(item, out));
        }
        Ast::Div(numerator, denominator) => {
            variables(numerator, out);
            variables(denominator, out);
        }
        Ast::Func(_, args) => args.iter().for_each(|arg| variables(arg, out)),
        Ast::Assign { value, .. } => variables(value, out),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::answer::{AnswerContract, check, check_contract};
    use crate::curriculum::AnswerKind;

    fn exact(expected: &str, learner: &str) -> Option<bool> {
        match check_contract(expected, learner, AnswerContract::Exact) {
            Outcome::Decided(verdict) => Some(verdict.correct),
            Outcome::Undecidable(_) => None,
        }
    }

    fn plain(expected: &str, learner: &str) -> Option<bool> {
        match check(expected, learner, AnswerKind::Numeric) {
            Outcome::Decided(verdict) => Some(verdict.correct),
            Outcome::Undecidable(_) => None,
        }
    }

    #[test]
    fn a_percent_key_takes_its_bare_number() {
        for learner in [
            "65",
            "65%",
            "65 %",
            "65 percent",
            "65 per cent",
            "65 Percent.",
            "0.65",
            "13/20",
        ] {
            assert_eq!(exact("65%", learner), Some(true), "{learner}");
            assert_eq!(plain("65%", learner), Some(true), "{learner}");
        }
        assert_eq!(exact("12.5%", "12.5"), Some(true));
        for learner in ["66", "6.5", "650", "0.66", "66 percent"] {
            assert_eq!(exact("65%", learner), Some(false), "{learner}");
        }
    }

    #[test]
    fn a_percent_key_never_takes_a_unit() {
        for learner in ["0.65 ft", "0.65 kg", "65 ft", "65 kg", "€65", "0.65 m"] {
            assert_ne!(exact("65%", learner), Some(true), "{learner}");
            assert_ne!(plain("65%", learner), Some(true), "{learner}");
        }
        for learner in ["65", "65 percent", "65%"] {
            assert_eq!(exact("65%", learner), Some(true), "{learner}");
        }
    }

    #[test]
    fn a_named_unit_is_not_checked_against_the_question() {
        assert!(!matches!(
            check_contract("8", "8 cm", AnswerContract::Exact),
            Outcome::Decided(verdict) if verdict.correct
        ));
    }

    #[test]
    fn a_bare_number_key_rejects_an_uncontracted_named_unit() {
        for (key, learner) in [
            ("8", "8 ft"),
            ("4/9", "4/9 m"),
            ("18", "€18"),
            ("18", "€ 18"),
            ("18", "18€"),
            ("18", "$18"),
            ("18", "18 $"),
            ("18", "18 euros"),
            ("165", "165 km"),
            ("2.5", "2.5 litres"),
            ("30", "30°"),
            ("21", "21 beads"),
        ] {
            assert_ne!(exact(key, learner), Some(true), "{key} vs {learner}");
            assert_ne!(plain(key, learner), Some(true), "{key} vs {learner}");
        }
    }

    #[test]
    fn a_unit_never_turns_a_wrong_number_right() {
        for (key, learner) in [
            ("8", "9 ft"),
            ("18", "€19"),
            ("165", "16.5 km"),
            ("4/9", "4/7 m"),
            ("21", "21 x"),
            ("21", "21 pi"),
            ("21", "21 sqrt"),
            ("6", "2 x 4 cm"),
            ("21", "21 theta"),
            ("21", "21 ln"),
            ("9/2", "$9/4$."),
        ] {
            assert_ne!(exact(key, learner), Some(true), "{key} vs {learner}");
        }
    }

    #[test]
    fn an_expression_key_is_not_rewritten() {
        assert_eq!(strip_unit("2 x"), None);
        let strict = Outcome::Decided(crate::answer::Verdict {
            correct: false,
            notation: false,
        });
        assert_eq!(rescue("x + 1", "x + 1 cm", strict, |_, _| strict), None);
    }

    #[test]
    fn a_unit_contract_stays_strict() {
        let unit = AnswerContract::Unit {
            quantity: crate::answer::Quantity::Length,
            unit: "cm".to_owned(),
            allow_omitted: false,
            form: None,
        };
        assert!(!matches!(
            check_contract("18 cm", "18", unit),
            Outcome::Decided(verdict) if verdict.correct
        ));

        let required_form = AnswerContract::RequiredForm {
            form: crate::answer::NumericForm::Integer,
        };
        assert!(!matches!(
            check_contract("18", "18 cm", required_form),
            Outcome::Decided(verdict) if verdict.correct
        ));
    }
}
