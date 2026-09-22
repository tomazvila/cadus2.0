//! I5 (equal problem hash), I6 (digit skeleton) and I7 (different keys).

use std::collections::{BTreeMap, BTreeSet};

use super::super::kp_view::KpView;
use super::finding;
use crate::output::Finding;

/// The digit skeleton of `hash.md`: each run of ASCII digits becomes one `#`.
pub fn skeleton(problem: &str) -> String {
    let mut out = String::with_capacity(problem.len());
    let mut in_run = false;
    for ch in problem.chars() {
        let digit = ch.is_ascii_digit();
        if !digit {
            out.push(ch);
        } else if !in_run {
            out.push('#');
        }
        in_run = digit;
    }
    out
}

fn i5(view: &KpView) -> Vec<Finding> {
    (0..view.items.len())
        .filter_map(|index| {
            let detail = view.duplicate_of(index)?;
            let item = &view.items[index];
            Some(finding(
                view,
                "duplicate",
                "I5",
                Some(item),
                detail.to_owned(),
            ))
        })
        .collect()
}

/// I6: the third exemplar of one skeleton, and each later one, is a finding.
/// Only a new item can be a finding (lane 33h, FLOW/reports/wave-top-codes.md):
/// the base exemplars of the packet are owner-authored, and the row must keep
/// them (CK2), so I6 on a kept base exemplar can never pass. The base
/// exemplars stay grandfathered, as I12 to I14 are (the hash baseline). The
/// budget of one skeleton is 2 minus its kept base exemplars (at the minimum
/// 0), so a new item that repeats the shape of the packet still fails.
fn i6(view: &KpView) -> Vec<Finding> {
    let mut budget: BTreeMap<String, usize> = BTreeMap::new();
    for item in view.items.iter().filter(|item| !item.is_new) {
        let count = budget.entry(skeleton(&item.exemplar.problem)).or_insert(0);
        *count = 2.min(*count + 1);
    }
    view.items
        .iter()
        .filter(|item| item.is_new)
        .filter_map(|item| {
            let count = budget.entry(skeleton(&item.exemplar.problem)).or_insert(0);
            *count += 1;
            (*count > 2).then(|| {
                let detail = format!("I6: exemplar {count} with one digit skeleton (limit 2)");
                finding(view, "duplicate", "I6", Some(item), detail)
            })
        })
        .collect()
}

/// I7 reads the first 6 verdict exemplars. A KP with fewer than 4 verdict
/// exemplars has the I2 finding, thus I7 does not run there.
fn i7(view: &KpView) -> Option<Finding> {
    let keys: Vec<&str> = view
        .items
        .iter()
        .filter(|item| item.verdict)
        .take(6)
        .map(|item| item.exemplar.answer.trim())
        .collect();
    let different = keys.iter().collect::<BTreeSet<_>>().len();
    (keys.len() >= 4 && different < 4).then(|| {
        let detail = format!(
            "I7: {different} different keys among the first {} verdict exemplars (minimum 4)",
            keys.len()
        );
        finding(view, "duplicate", "I7", None, detail)
    })
}

/// The findings of I5, I6 and I7.
pub fn check(view: &KpView) -> Vec<Finding> {
    let mut findings = i5(view);
    findings.extend(i6(view));
    findings.extend(i7(view));
    findings
}

#[cfg(test)]
mod tests {
    use cadus_core::learner::problem_text_hash;
    use serde_json::json;

    use super::super::testkit::{exact, invariants, item, view};
    use super::*;

    #[test]
    fn skeleton_replaces_each_digit_run() {
        assert_eq!(
            skeleton("Find 12 + 3.5x at x = 40"),
            "Find # + #.#x at x = #"
        );
        assert_eq!(skeleton("no digit"), "no digit");
    }

    #[test]
    fn i5_finds_an_equal_hash_in_the_kp_the_diagnostic_and_the_teach_page() {
        let mut items: Vec<_> = (0..6).map(exact).collect();
        assert_eq!(check(&view(items.clone())), []);
        items.push(exact(0));
        let found = check(&view(items.clone()));
        assert_eq!(invariants(&found), ["I5"]);
        assert_eq!(found[0].hash.as_deref(), Some(items[0].hash.as_str()));
        assert_eq!(view(items.clone()).status(6).0, "must_replace");
        assert_eq!(view(items.clone()).status(0).0, "keep");
        items.pop();
        let mut other = view(items);
        other.diagnostic_hash = Some(other.items[1].hash.clone());
        other.set_teach_problem(Some(&other.items[2].exemplar.problem.clone()));
        assert_eq!(
            other.teach_hash,
            Some(problem_text_hash("Find the value of item two."))
        );
        let found = check(&other);
        assert_eq!(invariants(&found), ["I5", "I5"]);
        assert!(found[0].detail.contains("diagnostic"));
        assert!(found[1].detail.contains("teach page"));
    }

    // Lane 33h: the row view holds the kept base exemplars of the packet. I6
    // must not fire on them (the wave defect of FLOW/reports/wave-top-codes.md:
    // 4 kept base exemplars of one skeleton made every re-author of the KP
    // fail again). The finding of one skeleton goes to a new item only.
    #[test]
    fn i6_spares_the_kept_base_exemplars_and_fires_on_a_new_item_of_one_skeleton() {
        let numbered = |n: usize| {
            let problem = format!("Find {n} + {n}.");
            item(
                &problem,
                &(2 * n).to_string(),
                json!({"kind": "exact"}),
                None,
            )
        };
        // The packet keeps 4 base exemplars of one digit skeleton; the row
        // keeps them and adds new items of other skeletons. No finding.
        let mut items: Vec<_> = (0..4).map(exact).collect();
        items.extend((1..4).map(numbered));
        assert_eq!(check(&view(items.clone())), []);
        // A new item that repeats the skeleton of the kept base exemplars is
        // the third exemplar of one skeleton: it is a finding.
        let mut repeated = numbered(4);
        repeated.is_new = true;
        items.push(repeated);
        let found = check(&view(items));
        assert_eq!(invariants(&found), ["I6"]);
        assert!(found[0].hash.is_some());
    }

    #[test]
    fn i6_permits_two_exemplars_of_one_skeleton() {
        let numbered = |n: usize| {
            let problem = format!("Find {n} + {n}.");
            let mut one = item(
                &problem,
                &(2 * n).to_string(),
                json!({"kind": "exact"}),
                None,
            );
            // The row items of the author: I6 reads them (lane 33h).
            one.is_new = true;
            one
        };
        let mut items: Vec<_> = (0..4).map(exact).collect();
        items.extend((1..3).map(numbered));
        assert_eq!(check(&view(items.clone())), []);
        items.push(numbered(3));
        assert_eq!(invariants(&check(&view(items))), ["I6"]);
    }

    #[test]
    fn i7_needs_four_different_keys_among_the_first_six() {
        let same = |n: usize| {
            let mut one = exact(n);
            one.exemplar.answer = "7".to_owned();
            one
        };
        let mut items: Vec<_> = (0..3).map(exact).collect();
        items.extend((3..6).map(same));
        assert_eq!(check(&view(items.clone())), []);
        items[2] = same(2);
        assert_eq!(invariants(&check(&view(items))), ["I7"]);
        // Fewer than 4 verdict exemplars: I2 reports the KP, I7 does not run.
        assert_eq!(check(&view((0..3).map(same).collect())), []);
    }
}
