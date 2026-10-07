//! A list of whole numbers read from the spellings a learner types it in.
//!
//! The key `89, 698, 712, 1205` is four whole numbers. A learner writes them
//! with commas, a glued thousands group (`1,205`), semicolons, lines, a `<`
//! or `>` chain or plain spaces. [`rewrite`] turns each of those into the comma list
//! the key uses, and only when the learner's text has exactly the key's count of
//! numbers. A glued group and a separator never fit one correct answer together:
//! a group is read as one number only when the list then has the key's count, so
//! no answer gains a second reading. When two readings have the key's count,
//! the one equal to the key is read, and otherwise the text is left as it is.

/// The whole numbers of a comma list key, or `None` when the key is anything else.
fn key_numbers(expected: &str) -> Option<Vec<&str>> {
    let parts: Vec<&str> = expected.split(',').map(str::trim).collect();
    ((2..=32).contains(&parts.len()) && parts.iter().all(|part| whole(part))).then_some(parts)
}

fn whole(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// Whether the text can lead a glued thousands group: one to three digits.
fn group_head(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    (1..=3).contains(&digits.len()) && digits.bytes().all(|byte| byte.is_ascii_digit())
}

fn group_tail(text: &str) -> bool {
    text.len() == 3 && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// Every reading of the comma pieces of one token, as lists of numbers.
fn readings(pieces: &[&str]) -> Vec<Vec<String>> {
    if pieces.is_empty() {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    // The first piece alone is one number.
    if whole(pieces[0]) {
        for rest in readings(&pieces[1..]) {
            let mut all = vec![pieces[0].to_owned()];
            all.extend(rest);
            out.push(all);
        }
    }
    // The first piece starts a group of two or more pieces.
    if group_head(pieces[0]) {
        for end in 2..=pieces.len() {
            if pieces[1..end].iter().all(|piece| group_tail(piece)) {
                let joined = pieces[..end].concat();
                for rest in readings(&pieces[end..]) {
                    let mut all = vec![joined.clone()];
                    all.extend(rest);
                    out.push(all);
                }
            }
        }
    }
    out
}

/// The comma list the learner's text means, written the way the key is written.
///
/// `None` when the key is not a list of whole numbers, when the text has another
/// character, or when no reading (or no single reading) has the key's count.
#[must_use]
pub fn rewrite(expected: &str, learner: &str) -> Option<String> {
    let count = key_numbers(expected)?.len();
    let mut tokens: Vec<Vec<Vec<String>>> = Vec::new();
    let text = learner.trim().trim_end_matches('.');
    let spaced = text
        .replace([';', '<', '>', '\n', '\r'], " ")
        .replace(" and ", " ");
    let spaced = spaced.strip_prefix("and ").unwrap_or(&spaced).to_owned();
    for token in spaced.split_whitespace() {
        let token = token.trim_matches(',');
        if token.is_empty() {
            continue;
        }
        let pieces: Vec<&str> = token.split(',').collect();
        let options = readings(&pieces);
        if options.is_empty() {
            return None;
        }
        tokens.push(options);
    }
    if tokens.is_empty() {
        return None;
    }
    // Combine the readings of the tokens, keeping only lists of the key's count.
    let mut lists: Vec<Vec<String>> = vec![Vec::new()];
    for options in &tokens {
        let mut next = Vec::new();
        for list in &lists {
            for option in options {
                if list.len() + option.len() <= count {
                    let mut all = list.clone();
                    all.extend(option.iter().cloned());
                    next.push(all);
                }
            }
        }
        next.truncate(256);
        lists = next;
    }
    lists.retain(|list| list.len() == count);
    lists.sort();
    lists.dedup();
    let key = key_numbers(expected)?;
    let chosen = match lists.as_slice() {
        [only] => only,
        // Two readings fit: the one that is the key is the answer. Two readings
        // never both equal one key, so no answer has two correct readings.
        _ => lists
            .iter()
            .find(|list| list.iter().map(String::as_str).eq(key.iter().copied()))?,
    };
    Some(chosen.join(", "))
}
