//! `report`: the static check of the campaign (`cadus.report.v1`).
//!
//! The report counts the verdict exemplars of each KP and compares each KP
//! with its floor. It has no course average: "no verdict" is never "pass".

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::db::Store;
use super::invariants::{self, counts, file_rule, finding, label};
use super::kp_view::{HARD_FLOOR, KpView, Tree};
use super::{Fail, Opts, Outcome, diff};
use crate::output::Finding;

/// I15: the KP has an approved teach page. The rule runs only with `--db`.
fn i15(view: &KpView, store: Option<&Store>) -> Option<Finding> {
    store
        .filter(|store| !store.has_teach(&view.store_key))
        .map(|_| {
            let detail = "I15: the KP has no approved teach page".to_owned();
            finding(view, "invariant:I15", "I15", None, detail)
        })
}

/// The findings of each view, in the order of `views`: the KP rules, I15, and
/// the file rule I10 on the first KP of each unit file.
pub fn findings_by_kp(views: &[KpView], store: Option<&Store>) -> Vec<Vec<Finding>> {
    let mut files: BTreeMap<usize, Vec<&KpView>> = BTreeMap::new();
    for view in views {
        files.entry(view.unit).or_default().push(view);
    }
    let mut file_findings: BTreeMap<String, Finding> = files
        .values()
        .filter_map(|file| file_rule::check(file))
        .map(|found| (found.kp.clone(), found))
        .collect();
    views
        .iter()
        .map(|view| {
            let mut found = invariants::check(view);
            found.extend(file_findings.remove(&view.kp));
            found.extend(i15(view, store));
            found
        })
        .collect()
}

/// The `kps` entry of one view.
fn kp_doc(view: &KpView, findings: &[Finding], store: Option<&Store>) -> Value {
    let existing: Vec<Value> = view
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            json!({"hash": item.hash, "status": view.status(index).0,
                "verdict": item.verdict, "kind": item.kind()})
        })
        .collect();
    let v = view.v();
    json!({"kp": view.kp, "file": view.file, "V": v, "U": view.u(), "W": view.w(),
        "T": store.map(|store| store.templates(&view.store_key)),
        "teach": store.map(|store| store.has_teach(&view.store_key)),
        "floor": view.floor, "goal": v >= view.floor, "serves": v >= HARD_FLOOR,
        "proof_kp": view.proof_kp, "existing": existing, "findings": findings})
}

/// The counts of one course. Each count is a count of KPs or of exemplars.
#[derive(Default)]
struct CourseSum {
    floor: usize,
    kps: usize,
    at_goal: usize,
    v4_to_floor: usize,
    below_4: usize,
    exemplars: usize,
    unmarked: usize,
    unmarked_p3: usize,
    written: usize,
    label_items: usize,
    no_teach: usize,
    with_finding: usize,
    breaches: BTreeMap<String, usize>,
}

impl CourseSum {
    fn add(&mut self, view: &KpView, findings: &[Finding], store: Option<&Store>) {
        let v = view.v();
        self.floor = view.floor;
        self.kps += 1;
        self.at_goal += usize::from(v >= view.floor);
        self.v4_to_floor += usize::from((HARD_FLOOR..view.floor).contains(&v));
        self.below_4 += usize::from(v < HARD_FLOOR);
        self.exemplars += view.items.len();
        self.unmarked += view.u();
        self.written += view.w();
        let p3 = |index: &usize| !view.items[*index].verdict && counts::is_p3(view, *index);
        self.unmarked_p3 += (0..view.items.len()).filter(p3).count();
        self.label_items += view
            .items
            .iter()
            .filter(|item| label::is_label(item))
            .count();
        self.no_teach += usize::from(store.is_some_and(|s| !s.has_teach(&view.store_key)));
        self.with_finding += usize::from(!findings.is_empty());
        for found in findings {
            let rule = found.invariant.clone().unwrap_or_default();
            *self.breaches.entry(rule).or_insert(0) += 1;
        }
    }

    fn doc(&self, course: &str, db_read: bool) -> Value {
        let result = if self.with_finding == 0 {
            "PASS"
        } else {
            "FAIL"
        };
        json!({"course": course, "result": result, "floor": self.floor, "kps": self.kps,
            "kps_at_goal": self.at_goal, "kps_v4_to_floor": self.v4_to_floor,
            "kps_below_4": self.below_4, "exemplars": self.exemplars,
            "unmarked": self.unmarked, "unmarked_p3": self.unmarked_p3, "written": self.written,
            "label_items": self.label_items,
            "kps_no_teach_page": db_read.then_some(self.no_teach),
            "breaches": self.breaches})
    }
}

/// `--since`: an exemplar whose hash is not in the same KP of the base tree is
/// a new exemplar, so I12, I13 and I14 run for it.
fn mark_new(views: &mut [KpView], base: &Tree) {
    let known: BTreeMap<String, BTreeSet<String>> = base
        .views(None)
        .into_iter()
        .map(|view| {
            (
                view.kp,
                view.items.into_iter().map(|item| item.hash).collect(),
            )
        })
        .collect();
    for view in views {
        let hashes = known.get(&view.kp);
        for item in &mut view.items {
            item.is_new = !hashes.is_some_and(|hashes| hashes.contains(&item.hash));
        }
    }
}

/// The course filter of the command line: `--all`, or one `--course`.
fn course_filter<'a>(opts: &'a Opts, tree: &Tree) -> Result<Option<&'a str>, Fail> {
    match (opts.has("all"), opts.get("course")) {
        (true, None) => Ok(None),
        (false, Some(course)) if tree.has_course(course) => Ok(Some(course)),
        (false, Some(course)) => Err(Fail::input(format!(
            "the course `{course}` is not in the tree"
        ))),
        _ => Err(Fail::input("give `--all` or `--course <id>`, not the two")),
    }
}

/// Run `report`. Exit 0 if the result is PASS, exit 1 if it is FAIL.
pub fn run(args: &[String]) -> Outcome {
    let opts = Opts::read(args, &["course", "base", "db", "since", "repo"], &["all"])?;
    let base = opts.need("base")?;
    let tree = Tree::load(base)?;
    let mut views = tree.views(course_filter(&opts, &tree)?);
    if let Some(git_ref) = opts.get("since") {
        mark_new(
            &mut views,
            &diff::tree_at(opts.get("repo").unwrap_or("."), git_ref)?,
        );
    }
    let store = opts.get("db").map(Store::read).transpose()?;
    for view in &mut views {
        let problem = store
            .as_ref()
            .and_then(|s| s.teach_problem(&view.store_key));
        view.set_teach_problem(problem);
    }
    let findings = findings_by_kp(&views, store.as_ref());
    let mut order: Vec<&str> = Vec::new();
    let mut sums: BTreeMap<&str, CourseSum> = BTreeMap::new();
    for (view, found) in views.iter().zip(&findings) {
        if !sums.contains_key(view.course.as_str()) {
            order.push(&view.course);
        }
        sums.entry(&view.course)
            .or_default()
            .add(view, found, store.as_ref());
    }
    let pass = findings.iter().all(Vec::is_empty);
    let courses: Vec<Value> = order
        .iter()
        .map(|course| sums[course].doc(course, store.is_some()))
        .collect();
    let kps: Vec<Value> = views
        .iter()
        .zip(&findings)
        .map(|(view, found)| kp_doc(view, found, store.as_ref()))
        .collect();
    let doc = json!({"schema": "cadus.report.v1", "base": base, "db_read": store.is_some(),
        "result": if pass { "PASS" } else { "FAIL" }, "courses": courses, "kps": kps});
    Ok((doc, u8::from(!pass)))
}
