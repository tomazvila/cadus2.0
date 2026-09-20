//! `selftest`: the static check of the seeded trees against the list of their
//! seeded defects (`cadus.selftest.v1`).
//!
//! Each entry of the expected file has a `tree` field (pack v9). The default
//! is the name of the `--tree` directory; a different name is a directory next
//! to `--tree`. Each tree loads separately, because one refused file stops a
//! full load. The results join by (`tree`, `kp`). A tree that the loader
//! refuses gives one finding `invariant:I1` with no KP.
//!
//! Each exemplar of a seeded tree is a new exemplar, so I12, I13 and I14 run
//! for each one.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{Value, json};

use super::kp_view::Tree;
use super::{Fail, Opts, Outcome, read_json, report};
use crate::output::Finding;

const I1: &str = "invariant:I1";

/// One expected defect.
struct Defect {
    n: i64,
    tree: String,
    place: String,
    code: String,
}

impl Defect {
    /// True if the finding of the tree `tree` is the finding of this defect.
    /// The I1 finding has no KP, so the tree and the code identify it.
    fn is(&self, tree: &str, finding: &Finding) -> bool {
        self.tree == tree
            && self.code == finding.code
            && (self.code == I1 || self.place == finding.kp)
    }
}

/// The findings of the full static check of one tree.
fn findings_of(dir: &str) -> Result<Vec<Finding>, Fail> {
    let tree = match Tree::load(dir) {
        Ok(tree) => tree,
        Err(fail) if fail.exit == 3 => {
            return Ok(vec![Finding {
                ck: "CK12".to_owned(),
                code: I1.to_owned(),
                invariant: Some("I1".to_owned()),
                kp: String::new(),
                item: None,
                hash: None,
                detail: fail.text,
            }]);
        }
        Err(fail) => return Err(fail),
    };
    let mut views = tree.views(None);
    for item in views.iter_mut().flat_map(|view| &mut view.items) {
        item.is_new = true;
    }
    let by_kp = report::findings_by_kp(&views, None);
    Ok(by_kp.into_iter().flatten().collect())
}

/// The expected defects. `default_tree` is the name of the `--tree` directory.
fn expected_defects(doc: &Value, default_tree: &str) -> Result<Vec<Defect>, Fail> {
    let list = doc["defects"]
        .as_array()
        .ok_or_else(|| Fail::input("the expected file has no `defects` list"))?;
    list.iter()
        .map(|entry| {
            let text = |key: &str| entry[key].as_str().map(str::to_owned);
            let fields = entry["n"].as_i64().zip(text("where")).zip(text("code"));
            let ((n, place), code) =
                fields.ok_or_else(|| Fail::input("a defect needs `n`, `where` and `code`"))?;
            let tree = text("tree").unwrap_or_else(|| default_tree.to_owned());
            Ok(Defect {
                n,
                tree,
                place,
                code,
            })
        })
        .collect()
}

/// Run `selftest`. Exit 0 only if each defect has a finding and no other
/// finding exists.
pub fn run(args: &[String]) -> Outcome {
    let opts = Opts::read(args, &["tree", "expected"], &[])?;
    let dir = Path::new(opts.need("tree")?);
    let name_of = |path: &Path| {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
    };
    let main_name =
        name_of(&dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf())).unwrap_or_default();
    let expected = expected_defects(&read_json(opts.need("expected")?)?, &main_name)?;
    let mut names: BTreeSet<&str> = expected.iter().map(|defect| defect.tree.as_str()).collect();
    names.insert(&main_name);
    let mut found: Vec<(&str, Finding)> = Vec::new();
    for name in names {
        let tree_dir = if name == main_name {
            dir.to_path_buf()
        } else {
            dir.parent().unwrap_or(Path::new(".")).join(name)
        };
        let findings = findings_of(&tree_dir.to_string_lossy())?;
        found.extend(findings.into_iter().map(|finding| (name, finding)));
    }
    let missing: Vec<i64> = expected
        .iter()
        .filter(|defect| !found.iter().any(|(tree, finding)| defect.is(tree, finding)))
        .map(|defect| defect.n)
        .collect();
    let extra: Vec<&Finding> = found
        .iter()
        .filter(|(tree, finding)| !expected.iter().any(|defect| defect.is(tree, finding)))
        .map(|(_, finding)| finding)
        .collect();
    let pass = missing.is_empty() && extra.is_empty();
    let doc = json!({"schema": "cadus.selftest.v1", "pass": pass, "expected": expected.len(),
        "flagged": expected.len() - missing.len(), "missing": missing, "extra": extra});
    Ok((doc, u8::from(!pass)))
}
