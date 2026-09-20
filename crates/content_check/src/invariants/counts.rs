//! I2 (the count of verdict exemplars of the KP) and I4 (the exemplars with
//! no verdict). The counts are for one KP. No course average exists here.

use super::super::kp_view::{HARD_FLOOR, Item, KpView};
use super::finding;
use crate::output::Finding;

/// The start of the problem text of the one legal item with no verdict (P3).
pub const P3_PREFIX: &str = "Write the full proof";

/// True if the item at `index` is the legal P3 item of the KP.
pub fn is_p3(view: &KpView, index: usize) -> bool {
    view.proof_kp
        && index + 1 == view.items.len()
        && view.items[index].exemplar.problem.starts_with(P3_PREFIX)
}

fn i2(view: &KpView) -> Option<Finding> {
    let v = view.v();
    let floor = view.floor;
    let detail = if v < HARD_FLOOR {
        format!("V = {v}: fewer than {HARD_FLOOR} verdict exemplars (hard floor)")
    } else if v < floor {
        format!("below-goal: V = {v}, the floor of the course is {floor}")
    } else {
        return None;
    };
    Some(finding(view, "invariant:I2", "I2", None, detail))
}

fn i4_detail(view: &KpView, item: &Item) -> String {
    let reason = item
        .exemplar
        .verdict_policy(view.answer_kind)
        .err()
        .map_or("-", |refusal| refusal.reason);
    let kind = item.kind().unwrap_or("no contract");
    format!(
        "the exemplar gives no verdict (contract: {kind}; reason: {reason}); this is legal only \
for the one P3 item of a proof KP: the last exemplar, problem starts with `{P3_PREFIX}`"
    )
}

/// The findings of I2 and I4.
pub fn check(view: &KpView) -> Vec<Finding> {
    let unmarked = view
        .items
        .iter()
        .enumerate()
        .filter(|(index, item)| !item.verdict && !is_p3(view, *index))
        .map(|(_, item)| {
            finding(
                view,
                "invariant:I4",
                "I4",
                Some(item),
                i4_detail(view, item),
            )
        });
    i2(view).into_iter().chain(unmarked).collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::testkit::{exact, invariants, item, view};
    use super::*;

    #[test]
    fn i2_has_the_hard_floor_and_the_goal() {
        let three = check(&view((0..3).map(exact).collect()));
        assert_eq!(invariants(&three), ["I2"]);
        assert!(three[0].detail.starts_with("V = 3"), "{}", three[0].detail);
        let five = check(&view((0..5).map(exact).collect()));
        assert!(
            five[0].detail.starts_with("below-goal"),
            "{}",
            five[0].detail
        );
        assert_eq!(five[0].code, "invariant:I2");
        assert_eq!(check(&view((0..6).map(exact).collect())), []);
        let mut low_floor = view((0..4).map(exact).collect());
        low_floor.floor = 4;
        assert_eq!(check(&low_floor), []);
    }

    #[test]
    fn i2_counts_an_equal_hash_one_time() {
        let mut items: Vec<_> = (0..5).map(exact).collect();
        items.push(exact(0));
        assert_eq!(view(items.clone()).v(), 5);
        assert_eq!(invariants(&check(&view(items))), ["I2"]);
    }

    fn p3(problem: &str) -> crate::late::kp_view::Item {
        item(problem, "See the solution.", json!({"kind": "none"}), None)
    }

    #[test]
    fn i4_permits_the_none_contract_in_one_place_only() {
        let mut items: Vec<_> = (0..6).map(exact).collect();
        items.push(p3("Write the full proof: the sum is even."));
        let mut proof = view(items.clone());
        proof.proof_kp = true;
        assert_eq!(check(&proof), []);
        assert_eq!(proof.u(), 1);
        // Not a proof KP.
        let found = check(&view(items.clone()));
        assert_eq!(invariants(&found), ["I4"]);
        assert!(
            found[0].detail.contains("contract: none"),
            "{}",
            found[0].detail
        );
        // Not the last exemplar.
        items.push(exact(6));
        proof.items = items.clone();
        assert_eq!(invariants(&check(&proof)), ["I4"]);
        // The last exemplar, but not the P3 problem text.
        items.pop();
        items.pop();
        items.push(p3("Explain the proof."));
        proof.items = items.clone();
        assert_eq!(invariants(&check(&proof)), ["I4"]);
        // Two items with no verdict: the one that is not last is a finding.
        items.push(p3("Write the full proof: the sum is even."));
        proof.items = items;
        assert_eq!(invariants(&check(&proof)), ["I4"]);
    }

    #[test]
    fn i4_names_a_key_that_does_not_validate() {
        let mut items: Vec<_> = (0..6).map(exact).collect();
        items.push(item(
            "Find the last one.",
            "three",
            json!({"kind": "exact"}),
            None,
        ));
        let found = check(&view(items));
        assert_eq!(invariants(&found), ["I4"]);
        assert!(
            found[0].detail.contains("contract: exact"),
            "{}",
            found[0].detail
        );
    }
}
