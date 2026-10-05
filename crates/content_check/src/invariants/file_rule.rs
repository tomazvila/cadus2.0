//! I10, the file rule: in a unit file with 10 or more label items, the correct
//! option is the one longest option in 40% or fewer of the label items.
//!
//! The rule guards against a length cue among invented distractors, so it reads
//! label items with three or more options. A two-way verdict (`yes`/`no`,
//! `prime`/`composite`, `no solution`/`all real numbers`) has the options the
//! mathematics gives it, and the longer word is the key about half the time.

use super::super::kp_view::{Item, KpView};
use super::{finding, label};
use crate::output::Finding;

/// The smallest count of label items for which the rule applies.
const MIN_LABEL_ITEMS: usize = 10;

/// The fewest options a label item needs to count for the rule.
const MIN_OPTIONS: usize = 3;

/// True if the correct option text is longer than each other option text.
fn correct_is_longest(item: &Item) -> bool {
    let lengths: Vec<usize> = item.contract["options"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|option| option[0].as_str().map_or(0, |text| text.chars().count()))
        .collect();
    label::correct_option(item).is_some_and(|correct| {
        lengths
            .iter()
            .enumerate()
            .all(|(index, length)| index == correct || *length < lengths[correct])
    })
}

/// The number of options of a label item.
fn option_count(item: &Item) -> usize {
    item.contract["options"].as_array().map_or(0, Vec::len)
}

/// The I10 finding of one unit file. `views` are the KPs of the file in file
/// order. The finding names the first KP of the file, because the rule has no
/// KP of its own.
pub fn check(views: &[&KpView]) -> Option<Finding> {
    let labels: Vec<&Item> = views
        .iter()
        .flat_map(|view| &view.items)
        .filter(|item| label::is_label(item) && option_count(item) >= MIN_OPTIONS)
        .collect();
    let longest = labels
        .iter()
        .filter(|item| correct_is_longest(item))
        .count();
    let first = views.first()?;
    (labels.len() >= MIN_LABEL_ITEMS && longest * 10 > labels.len() * 4).then(|| {
        let detail = format!(
            "{}: the correct option is the longest option in {longest} of {} label items \
(limit 40%)",
            first.file,
            labels.len()
        );
        finding(first, "invariant:I10", "I10", None, detail)
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::testkit::{item, view};
    use super::*;

    fn label(n: usize, key: &str) -> Item {
        let contract =
            json!({"kind": "label", "options": [["long option"], ["aa"], ["bb"], ["cc"]]});
        item(&format!("Select case {n}."), key, contract, None)
    }

    #[test]
    fn i10_skips_two_way_verdicts() {
        let contract = json!({"kind": "label", "options": [["yes"], ["no"]]});
        let verdicts = view(
            (0..12)
                .map(|n| item(&format!("Is case {n} true?"), "yes", contract.clone(), None))
                .collect(),
        );
        assert_eq!(check(&[&verdicts]), None);
    }

    #[test]
    fn i10_reads_files_with_ten_or_more_label_items() {
        let nine = view((0..9).map(|n| label(n, "long option")).collect());
        assert_eq!(check(&[&nine]), None);
        assert_eq!(check(&[]), None);
        let one_more = view(vec![label(9, "long option")]);
        let found = check(&[&nine, &one_more]);
        assert!(
            found
                .as_ref()
                .is_some_and(|f| f.code == "invariant:I10" && f.kp == nine.kp)
        );
        assert!(found.is_some_and(|f| f.detail.contains("10 of 10")));
    }

    #[test]
    fn i10_permits_forty_percent() {
        let mut items: Vec<_> = (0..4).map(|n| label(n, "long option")).collect();
        items.extend((4..10).map(|n| label(n, "aa")));
        assert_eq!(check(&[&view(items.clone())]), None);
        items[4] = label(4, "long option");
        assert!(check(&[&view(items)]).is_some());
    }
}
