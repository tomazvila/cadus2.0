//! The checker rule: five deterministic mistakes named from two canonical forms.
//!
//! After a decided miss with no authored distractor match, [`checker_diagnosis`]
//! compares the canonical form of the key with the canonical form of the answer.
//! If the two forms differ in one of five exact ways, it returns one vocabulary
//! tag and one sentence. No model runs, no row is read, and no row is written.
//! The verdict, the tier and the fold do not change.
//!
//! The sentences name the cause and no value, because `checker_text` already
//! shows both values to the learner. If no rule fits exactly, the function
//! returns `None` and the worker job runs.

use cadus_core::answer::rounding::rounds_to;
use cadus_core::answer::{Canon, Rounding, canonical_form};
use cadus_core::template::Preauthored;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};

/// The tag of a sign flip.
const SIGN: &str = "sign-error";
/// The tag of a slip in one number.
const SLIP: &str = "arithmetic-slip";
/// The tag of a reversed direction or a wrong order.
const MISREAD: &str = "misread-problem";

/// The most decimal places the cut-short rule reads.
const MAX_PLACES: usize = 30;

/// The words for the positions 1 to 9.
const ORDINALS: [&str; 9] = [
    "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth",
];

/// Name the mistake in `answer` against `expected`, or return `None`.
///
/// `vocabulary` is the section 5.3 tag list. A tag outside it is dropped and the
/// sentence still stands, as in the authored lookup.
#[must_use]
pub fn checker_diagnosis(
    expected: &str,
    answer: &str,
    vocabulary: &[String],
) -> Option<Preauthored> {
    let key = canonical_form(expected).ok()?;
    let given = canonical_form(answer).ok()?;
    if key == given {
        return None;
    }
    let (tag, prose) = match (&key, &given) {
        (Canon::Rational(k), Canon::Rational(a)) => number_rule(k, a, expected, answer)?,
        (
            Canon::Interval {
                var: Some(kv),
                lo: klo,
                lo_closed: klc,
                hi: khi,
                hi_closed: khc,
            },
            Canon::Interval {
                var: Some(av),
                lo: alo,
                lo_closed: alc,
                hi: ahi,
                hi_closed: ahc,
            },
        ) if kv == av => reversed((klo, *klc, khi, *khc), (alo, *alc, ahi, *ahc))?,
        (Canon::Tuple(k), Canon::Tuple(a)) | (Canon::List(k), Canon::List(a)) => members(k, a)?,
        _ => return None,
    };
    let error_tags = vocabulary
        .iter()
        .filter(|known| known.as_str() == tag)
        .cloned()
        .collect();
    Some(Preauthored {
        error_tags,
        prose: Some(prose),
    })
}

/// The three number rules, in order.
fn number_rule(
    key: &BigRational,
    given: &BigRational,
    key_text: &str,
    given_text: &str,
) -> Option<(&'static str, String)> {
    if !key.is_zero() && *given == -key.clone() {
        return Some((
            SIGN,
            "Your number has the opposite sign. Its size is right.".to_string(),
        ));
    }
    let diff = key - given;
    if diff.abs().is_one() {
        let word = if diff.is_positive() { "less" } else { "more" };
        return Some((
            SLIP,
            format!("Your answer is 1 {word} than the right answer. Do the last step again."),
        ));
    }
    if cut_short(key, given, key_text, given_text) {
        return Some((
            SLIP,
            "You cut the decimal short. Round the last digit. Do not drop it.".to_string(),
        ));
    }
    None
}

/// The count of digits after the point of a plain decimal text, or `None`.
fn places(text: &str) -> Option<usize> {
    let (_, tail) = text.trim().split_once('.')?;
    (!tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit())).then_some(tail.len())
}

/// True if `given` is `key` with the digits after place `d` dropped, not rounded.
fn cut_short(key: &BigRational, given: &BigRational, key_text: &str, given_text: &str) -> bool {
    if key.is_integer() || given.is_integer() {
        return false;
    }
    let Some(d) = places(given_text).filter(|d| *d <= MAX_PLACES) else {
        return false;
    };
    let longer = if key_text.contains('/') {
        true
    } else {
        places(key_text).is_some_and(|k| k > d)
    };
    if !longer {
        return false;
    }
    // Rule D6-dec grades a decimal that is the exact rounding of the key as correct.
    if rounds_to(&Canon::Rational(key.clone()), given, d as u32) == Rounding::Same {
        return false;
    }
    let ten = BigRational::from_integer(10.into());
    let scale = (0..d).fold(BigRational::one(), |acc, _| acc * &ten);
    let cut = (key * &scale).trunc() / &scale;
    *given == cut && given != key
}

/// The four parts of one half line: low end, low closed, high end, high closed.
type Bounds<'a> = (&'a Option<Box<Canon>>, bool, &'a Option<Box<Canon>>, bool);

/// The reversed inequality rule: one half line points up, the other points down.
fn reversed(key: Bounds<'_>, given: Bounds<'_>) -> Option<(&'static str, String)> {
    let (klo, klc, khi, khc) = key;
    let (alo, alc, ahi, ahc) = given;
    let mirrored = match (klo, khi, alo, ahi) {
        (None, Some(k), Some(a), None) => k == a && khc == alc,
        (Some(k), None, None, Some(a)) => k == a && klc == ahc,
        _ => false,
    };
    mirrored.then(|| {
        (
            MISREAD,
            "Your inequality points the wrong way. Read the sign again.".to_string(),
        )
    })
}

/// The two member rules for a tuple or a list.
fn members(key: &[Canon], given: &[Canon]) -> Option<(&'static str, String)> {
    if key.len() != given.len() || key.len() < 2 {
        return None;
    }
    let mut left: Vec<&Canon> = key.iter().collect();
    let mut right: Vec<&Canon> = given.iter().collect();
    left.sort();
    right.sort();
    if left == right {
        return Some((
            MISREAD,
            "All your values are right, but they are in the wrong order.".to_string(),
        ));
    }
    let differ: Vec<usize> = (0..key.len()).filter(|i| key[*i] != given[*i]).collect();
    let [index] = differ[..] else {
        return None;
    };
    let place = ORDINALS.get(index).map_or_else(
        || format!("value number {}", index + 1),
        |w| format!("{w} value"),
    );
    let sentence = format!("Only the {place} is wrong. The other values are right.");
    Some((SLIP, sentence))
}

#[cfg(test)]
#[path = "checker_tests.rs"]
mod tests;
