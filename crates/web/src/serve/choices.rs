//! The `choices` key of a serve payload and of a diagnostic probe.
//!
//! A top-level Label contract is a closed list of options. The payload shows
//! the display text of each option, so the learner selects one and does not
//! type it. Hard Rule 1 holds: the payload shows the options, never the key.
//! The order comes from `problem_id` only, so it gives no data about the key.

use cadus_core::answer::AnswerContract;
use sha2::{Digest, Sha256};

/// The display texts of a top-level Label contract in a shuffled order; `None` for each other contract.
///
/// The display text of an option is its first alias. One `problem_id` gives
/// one order, because the serve route is idempotent.
pub(crate) fn label_choices(
    contract: Option<&AnswerContract>,
    problem_id: &str,
) -> Option<Vec<String>> {
    let Some(AnswerContract::Label { options }) = contract else {
        return None;
    };
    let mut choices: Vec<String> = options
        .iter()
        .filter_map(|aliases| aliases.first().cloned())
        .collect();
    shuffle(&mut choices, seed_of(problem_id));
    Some(choices)
}

/// `options` in an order fixed by `seed` alone.
///
/// A step check's author often writes the correct option first; the shuffle
/// keeps the position from naming it. One seed gives one order, so a reload
/// shows the same page.
pub(crate) fn shuffled(options: &[String], seed: &str) -> Vec<String> {
    let mut items = options.to_vec();
    shuffle(&mut items, seed_of(seed));
    items
}

/// The first 8 bytes of SHA-256 of `problem_id`, as a big-endian integer.
fn seed_of(problem_id: &str) -> u64 {
    let digest = Sha256::digest(problem_id.as_bytes());
    digest
        .iter()
        .take(8)
        .fold(0, |seed, byte| (seed << 8) | u64::from(*byte))
}

/// One step of the splitmix64 generator.
fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut mixed = *state;
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    mixed ^ (mixed >> 31)
}

/// A Fisher-Yates shuffle of `items` from the generator that starts at `seed`.
///
/// A Label contract has 32 options or fewer, so the bias of the modulo is
/// below 2^-58 and the casts keep each value.
fn shuffle(items: &mut [String], seed: u64) {
    let mut state = seed;
    for i in (1..items.len()).rev() {
        let j = (next(&mut state) % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadus_core::answer::AnswerPart;

    /// A Label contract with one alias for each text.
    fn label(texts: &[&str]) -> AnswerContract {
        AnswerContract::Label {
            options: texts.iter().map(|text| vec![(*text).to_string()]).collect(),
        }
    }

    fn sorted(mut texts: Vec<String>) -> Vec<String> {
        texts.sort();
        texts
    }

    #[test]
    fn the_choices_are_a_permutation_and_one_id_gives_one_order() {
        let contract = label(&["Step 1", "Step 2", "Step 3", "Step 4"]);
        let first = label_choices(Some(&contract), "abc").unwrap();
        assert_eq!(
            sorted(first.clone()),
            ["Step 1", "Step 2", "Step 3", "Step 4"]
        );
        assert_eq!(label_choices(Some(&contract), "abc").unwrap(), first);
    }

    #[test]
    fn fifty_ids_give_two_or_more_orders() {
        let contract = label(&["Step 1", "Step 2", "Step 3", "Step 4"]);
        let orders: std::collections::BTreeSet<Vec<String>> = (0..50)
            .map(|n| label_choices(Some(&contract), &format!("ab{n}")).unwrap())
            .collect();
        assert!(orders.len() >= 2, "{orders:?}");
    }

    #[test]
    fn a_choice_is_the_first_alias_only() {
        let contract = AnswerContract::Label {
            options: [
                &["DNE", "does not exist"][..],
                &["infinity", "inf", "oo"][..],
                &["-infinity", "-inf", "-oo"][..],
                &["0"][..],
            ]
            .iter()
            .map(|aliases| aliases.iter().map(|alias| (*alias).to_string()).collect())
            .collect(),
        };
        let choices = label_choices(Some(&contract), "p1").unwrap();
        assert_eq!(sorted(choices), ["-infinity", "0", "DNE", "infinity"]);
    }

    #[test]
    fn each_other_contract_gives_no_choices() {
        let multipart = AnswerContract::Multipart {
            parts: vec![AnswerPart {
                name: "step".to_string(),
                contract: label(&["Step 1", "Step 2"]),
            }],
        };
        assert_eq!(label_choices(Some(&AnswerContract::Exact), "p1"), None);
        assert_eq!(label_choices(Some(&multipart), "p1"), None);
        assert_eq!(label_choices(Some(&AnswerContract::None), "p1"), None);
        assert_eq!(label_choices(None, "p1"), None);
    }

    #[test]
    fn a_label_of_one_option_and_of_32_options_keeps_each_text() {
        let one = label(&["only"]);
        assert_eq!(label_choices(Some(&one), "p1").unwrap(), ["only"]);

        let texts: Vec<String> = (0..32).map(|n| format!("option {n:02}")).collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let choices = label_choices(Some(&label(&refs)), "p1").unwrap();
        assert_ne!(choices, texts, "32 options stayed in the authored order");
        assert_eq!(sorted(choices), texts);
    }

    #[test]
    fn the_seed_is_the_first_8_bytes_of_the_sha_256_digest() {
        // SHA-256 of `abc` starts with `ba7816bf 8f01cfea`.
        assert_eq!(seed_of("abc"), 0xBA78_16BF_8F01_CFEA);
    }

    /// Each position holds the key option in 20% to 30% of 1,000 serves.
    #[test]
    fn the_order_gives_no_data_about_the_key() {
        let contract = label(&["Step 1", "Step 2", "Step 3", "Step 4"]);
        let mut counts = [0_u32; 4];
        for n in 0..1_000 {
            let choices = label_choices(Some(&contract), &format!("p{n}")).unwrap();
            let position = choices.iter().position(|text| text == "Step 3").unwrap();
            counts[position] += 1;
        }
        for count in counts {
            assert!((200..=300).contains(&count), "{counts:?}");
        }
    }
}
