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
fn i6(view: &KpView) -> Vec<Finding> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    view.items
        .iter()
        .filter_map(|item| {
            let count = seen.entry(skeleton(&item.exemplar.problem)).or_insert(0);
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

    #[test]
    fn i6_permits_two_exemplars_of_one_skeleton() {
        let numbered = |n: usize| {
            let problem = format!("Find {n} + {n}.");
            item(
                &problem,
                &(2 * n).to_string(),
                json!({"kind": "exact"}),
                None,
            )
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
