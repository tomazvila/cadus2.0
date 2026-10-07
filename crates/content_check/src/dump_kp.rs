//! `dump-kp`: the parsed KP as JSON (`cadus.kp.v1`), with the hash, the
//! verdict and the status of each exemplar.

use serde_json::{Value, json};

use super::db::Store;
use super::kp_view::{KpView, Tree};
use super::{Opts, Outcome};

/// The `exemplars` list of `cadus.kp.v1`.
fn exemplars(view: &KpView) -> Vec<Value> {
    view.items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let (status, reason) = view.status(index);
            json!({"index": index, "hash": item.hash, "problem": item.exemplar.problem,
                "answer": item.exemplar.answer, "answer_contract": item.contract,
                "solution_sketch": item.exemplar.solution_sketch, "verdict": item.verdict,
                "status": status, "reason": reason})
        })
        .collect()
}

/// The `cadus.kp.v1` document of one view.
pub fn doc(view: &KpView) -> Value {
    json!({"schema": "cadus.kp.v1", "kp": view.kp, "file": view.file, "topic": view.topic,
        "kp_block": view.kp_block, "V": view.v(), "U": view.u(), "W": view.w(), "exemplars": exemplars(view)})
}

/// Run `dump-kp`. The command gives exit 0 when it ran.
pub fn run(args: &[String]) -> Outcome {
    let opts = Opts::read(args, &["kp", "base", "db"], &[])?;
    let id = opts.need("kp")?;
    let mut view = Tree::load(opts.need("base")?)?.view(id)?;
    if let Some(dsn) = opts.get("db") {
        let store = Store::read(dsn)?;
        view.set_teach_problem(store.teach_problem(&view.store_key));
    }
    Ok((doc(&view), 0))
}
