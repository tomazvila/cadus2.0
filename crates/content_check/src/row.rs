//! `row`: the check of one row against the base tree (`cadus.rowcheck.v1`).
//!
//! The command builds the KP "as if the row was applied": a `keep` hash
//! becomes the base exemplar, a new item becomes an exemplar, in row order.
//! Then the KP rules run, and the file rule I10 runs for the file with this KP
//! replaced. The command writes nothing.

use std::collections::BTreeSet;

use cadus_core::curriculum::Exemplar;
use serde_json::{Value, json};

use super::db::Store;
use super::invariants::{self, file_rule, finding, label};
use super::kp_view::{Item, KpView, Tree};
use super::{Fail, Opts, Outcome, read_json};
use crate::output::Finding;

/// One new item of the row as an item of the view.
fn new_item(entry: &Value, view: &KpView) -> Result<Item, Fail> {
    let text = |key: &str| entry[key].as_str().map(str::to_owned);
    let id = text("id").unwrap_or_else(|| "-".to_owned());
    let bad = |what: &str| Fail::input(format!("the row item `{id}`: {what}"));
    let contract = entry.get("answer_contract").cloned().unwrap_or(Value::Null);
    let answer_contract = serde_json::from_value(contract.clone())
        .map_err(|error| bad(&format!("the contract does not parse: {error}")))?;
    let exemplar = Exemplar {
        problem: text("problem").ok_or_else(|| bad("no `problem` text"))?,
        answer_contract,
        answer: text("answer").ok_or_else(|| bad("no `answer` text"))?,
        solution_sketch: text("solution_sketch"),
        visual: None,
    };
    let mut item = Item::new(exemplar, contract, view.answer_kind);
    item.id = Some(id);
    item.rule = text("rule");
    item.is_new = true;
    Ok(item)
}

/// The items of the applied state, and the `keep` hashes with no base exemplar.
fn applied_items(row: &Value, base: &KpView) -> Result<(Vec<Item>, Vec<String>), Fail> {
    let entries = row["items"]
        .as_array()
        .ok_or_else(|| Fail::input("the row has no `items` list (a blocked row has no check)"))?;
    let mut items = Vec::new();
    let mut unknown = Vec::new();
    for entry in entries {
        match entry["keep"].as_str() {
            None => items.push(new_item(entry, base)?),
            Some(hash) => match base.items.iter().find(|item| item.hash == hash) {
                Some(item) => items.push(item.clone()),
                None => unknown.push(hash.to_owned()),
            },
        }
    }
    Ok((items, unknown))
}

/// CK2: a base exemplar with the status `keep` that the row does not keep.
fn dropped_keeps(base: &KpView, view: &KpView) -> Vec<Finding> {
    let applied: BTreeSet<&str> = view.items.iter().map(|item| item.hash.as_str()).collect();
    (0..base.items.len())
        .filter(|index| base.status(*index).0 == "keep")
        .map(|index| &base.items[index])
        .filter(|item| !applied.contains(item.hash.as_str()))
        .map(|item| {
            let detail = "the row does not keep a base exemplar with the status `keep`";
            finding(view, "base-coverage", "CK2", Some(item), detail.to_owned())
        })
        .collect()
}

/// CK7: a new problem that is equal to an exemplar of a different KP of the
/// course.
fn course_duplicates(view: &KpView, others: &[KpView]) -> Vec<Finding> {
    let taken: BTreeSet<&str> = others
        .iter()
        .filter(|other| other.kp != view.kp)
        .flat_map(|other| &other.items)
        .map(|item| item.hash.as_str())
        .collect();
    view.items
        .iter()
        .filter(|item| item.is_new && taken.contains(item.hash.as_str()))
        .map(|item| {
            let detail = "I5: the problem is equal to an exemplar of a different KP of the course";
            finding(view, "duplicate", "I5", Some(item), detail.to_owned())
        })
        .collect()
}

/// I10 for the file state after the row. The row is the cause only if it adds
/// a label item, so the finding has the KP of the row.
fn file_finding(view: &KpView, tree: &Tree) -> Option<Finding> {
    let adds_label = view
        .items
        .iter()
        .any(|item| item.is_new && label::is_label(item));
    let file = tree.file_views(view.unit);
    let state: Vec<&KpView> = file
        .iter()
        .map(|other| if other.kp == view.kp { view } else { other })
        .collect();
    let found = file_rule::check(&state).filter(|_| adds_label)?;
    Some(Finding {
        kp: view.kp.clone(),
        ..found
    })
}

/// The teach-page problem: from `--packet`, or from `--db`.
fn teach_problem(opts: &Opts, view: &KpView) -> Result<Option<String>, Fail> {
    if let Some(path) = opts.get("packet") {
        let packet = read_json(path)?;
        let problem = packet["teach_page"]["worked_example"]["problem"].as_str();
        return Ok(problem.map(str::to_owned));
    }
    let store = opts.get("db").map(Store::read).transpose()?;
    Ok(store.and_then(|store| store.teach_problem(&view.store_key).map(str::to_owned)))
}

/// Run `row`. Exit 0 if the row passes, exit 1 if it has a finding.
pub fn run(args: &[String]) -> Outcome {
    let opts = Opts::read(args, &["row", "base", "packet", "db"], &[])?;
    let row = read_json(opts.need("row")?)?;
    let tree = Tree::load(opts.need("base")?)?;
    let id = row["kp"]
        .as_str()
        .ok_or_else(|| Fail::input("the row has no `kp` text"))?;
    let base = tree.view(id)?;
    let (items, unknown) = applied_items(&row, &base)?;
    let mut view = KpView {
        items,
        ..base.clone()
    };
    view.set_teach_problem(teach_problem(&opts, &view)?.as_deref());
    let mut findings: Vec<Finding> = unknown
        .iter()
        .map(|hash| Finding {
            hash: Some(hash.clone()),
            ..finding(
                &view,
                "base-coverage",
                "CK2",
                None,
                "the `keep` hash is not in the KP".to_owned(),
            )
        })
        .collect();
    findings.extend(dropped_keeps(&base, &view));
    findings.extend(invariants::check(&view));
    findings.extend(course_duplicates(&view, &tree.views(Some(&view.course))));
    findings.extend(file_finding(&view, &tree));
    let pass = findings.is_empty();
    let doc = json!({"schema": "cadus.rowcheck.v1", "kp": view.kp, "pass": pass,
        "V_before": base.v(), "V_after": view.v(), "U_before": base.u(), "U_after": view.u(),
        "floor": view.floor, "findings": findings});
    Ok((doc, u8::from(!pass)))
}
