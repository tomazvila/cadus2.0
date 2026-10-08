//! Persist the server-owned feedback chain and queued quiz skills.
use super::*;

/// Record feedback obligations and advance only on independent success.
pub(super) fn update_practice(
    scratch: &mut WebState,
    task_id: &str,
    served: &ServedProblem,
    recorded: &Attempt,
) {
    if (recorded.task_type != TaskType::Quiz || recorded.feedback_practice)
        && !recorded.outcome.is_ungraded()
        && (!recorded.correct || recorded.assisted)
    {
        let digest = problem_text_hash(&served.text);
        let mut digests = scratch
            .feedback_practice
            .get(task_id)
            .and_then(|value| value["digests"].as_array())
            .cloned()
            .unwrap_or_default();
        digests.push(json!(digest));
        let queue = scratch
            .feedback_practice
            .get(task_id)
            .map(|pending| pending["queue"].clone());
        scratch.feedback_practice.insert(task_id.to_owned(), json!({
            "queue": queue,
            "digest": digest, "digests": digests, "topic": served.serving_topic(), "record_topic": served.topic, "kp": served.kp,
        }));
    } else if recorded.correct && !recorded.assisted {
        finish_practice(scratch, task_id);
    }
}

/// Move to the next queued skill only after a fresh independent success.
fn finish_practice(scratch: &mut WebState, task_id: &str) {
    let Some(pending) = scratch.feedback_practice.remove(task_id) else {
        return;
    };
    let mut queue = pending["queue"].as_array().cloned().unwrap_or_default();
    if !queue.is_empty() {
        let mut next = queue.remove(0);
        next["queue"] = json!(queue);
        scratch.feedback_practice.insert(task_id.to_owned(), next);
    }
}

/// The learner line of a miss: what the key was and what was entered.
///
/// The reason a background check gives speaks of "the learner" and "the stored key", so the
/// panel never shows it. This line comes from the two answers alone. A property item has no
/// single key, so it gets no line. A blank or ungraded answer gets none either.
pub(super) fn checker_text(
    served: &ServedProblem,
    entered: &str,
    recorded: &Attempt,
) -> Option<String> {
    let entered = entered.trim();
    if recorded.correct || recorded.outcome.is_ungraded() || entered.is_empty() {
        return None;
    }
    let property = served
        .expected
        .answer_contract
        .as_ref()
        .and_then(cadus_core::answer::AnswerContract::property_description);
    if property.is_some() {
        return None;
    }
    Some(format!(
        "Expected {}. You entered {entered}.",
        key_with_decimal(&served.expected.answer)
    ))
}

/// A key as the learner reads it, with the decimal after a fraction that ends.
///
/// `6/5` gives `6/5 (1.2)`. A fraction that never ends, or any other key, stays as written.
fn key_with_decimal(key: &str) -> String {
    let key = key.trim();
    let plain = strip_frac(key);
    let Some((top, bottom)) = plain.split_once('/') else {
        return plain;
    };
    let (Ok(top), Ok(bottom)) = (top.trim().parse::<i64>(), bottom.trim().parse::<i64>()) else {
        return plain;
    };
    match terminating_decimal(top, bottom) {
        Some(decimal) => format!("{plain} ({decimal})"),
        None => plain,
    }
}

/// `\frac{a}{b}` and `\dfrac{a}{b}` as `a/b`; any other text unchanged.
fn strip_frac(key: &str) -> String {
    let body = key
        .strip_prefix("\\dfrac")
        .or_else(|| key.strip_prefix("\\frac"));
    if let Some(rest) = body
        && let Some(rest) = rest.strip_prefix('{')
        && let Some((top, rest)) = rest.split_once("}{")
        && let Some(bottom) = rest.strip_suffix('}')
    {
        return format!("{top}/{bottom}");
    }
    key.to_owned()
}

/// The decimal of `top / bottom` when it ends within six places and is not whole.
fn terminating_decimal(top: i64, bottom: i64) -> Option<String> {
    if bottom == 0 || top % bottom == 0 {
        return None;
    }
    let negative = (top < 0) != (bottom < 0);
    let (mut remainder, bottom) = (top.abs() % bottom.abs(), bottom.abs());
    let mut digits = String::new();
    for _ in 0..6 {
        if remainder == 0 {
            break;
        }
        remainder *= 10;
        digits.push(char::from(b'0' + u8::try_from(remainder / bottom).ok()?));
        remainder %= bottom;
    }
    if remainder != 0 {
        return None;
    }
    let whole = top.abs() / bottom;
    Some(format!(
        "{}{whole}.{digits}",
        if negative { "-" } else { "" }
    ))
}

#[cfg(test)]
mod tests {
    use super::{key_with_decimal, terminating_decimal};

    /// A fraction that ends gets its decimal; one that never ends does not.
    #[test]
    fn a_fraction_key_shows_its_decimal_when_it_ends() {
        assert_eq!(key_with_decimal("6/5"), "6/5 (1.2)");
        assert_eq!(key_with_decimal("\\frac{3}{8}"), "3/8 (0.375)");
        assert_eq!(key_with_decimal("1/3"), "1/3");
        assert_eq!(key_with_decimal("4/2"), "4/2");
        assert_eq!(key_with_decimal("12"), "12");
        assert_eq!(terminating_decimal(-7, 4).as_deref(), Some("-1.75"));
    }
}
