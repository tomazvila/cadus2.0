//! `diff`: one KP before and after a change (`cadus.diff.v1`), and the diff
//! rule I16. Before = `curriculum/` of a git ref. After = a tree directory.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use super::invariants::{counts, finding};
use super::kp_view::{Item, KpView, Tree};
use super::{Fail, Opts, Outcome};

/// A scratch directory. The drop removes it.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self, Fail> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let name = format!("content_check-{}-{nanos}", std::process::id());
        let path = std::env::temp_dir().join(name);
        std::fs::create_dir_all(&path)
            .map_err(|error| Fail::input(format!("cannot make a scratch directory: {error}")))?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Run one command. A start error or a failed status is an input error.
fn run_command(command: &mut Command, what: &str) -> Result<(), Fail> {
    let output = command
        .output()
        .map_err(|error| Fail::input(format!("{what}: cannot start: {error}")))?;
    if output.status.success() {
        Ok(())
    } else {
        let text = String::from_utf8_lossy(&output.stderr);
        Err(Fail::input(format!("{what}: {}", text.trim())))
    }
}

/// Load `curriculum/` of `git_ref` in the repository `repo`.
pub fn tree_at(repo: &str, git_ref: &str) -> Result<Tree, Fail> {
    let scratch = Scratch::new()?;
    let tar = scratch.0.join("tree.tar");
    let what = format!("git archive {git_ref} curriculum");
    let mut archive = Command::new("git");
    archive
        .arg("-C")
        .arg(repo)
        .arg("archive")
        .arg("-o")
        .arg(&tar);
    run_command(archive.arg(git_ref).arg("curriculum"), &what)?;
    let mut unpack = Command::new("tar");
    unpack.arg("-xf").arg(&tar).arg("-C").arg(&scratch.0);
    run_command(&mut unpack, "tar")?;
    Tree::load(&scratch.0.join("curriculum").to_string_lossy())
}

fn hashes(items: &[Item], verdict_only: bool) -> BTreeSet<&str> {
    items
        .iter()
        .filter(|item| item.verdict || !verdict_only)
        .map(|item| item.hash.as_str())
        .collect()
}

/// The first item with the hash.
fn item_of<'a>(items: &'a [Item], hash: &str) -> Option<&'a Item> {
    items.iter().find(|item| item.hash == hash)
}

/// The I16 breach texts of one KP.
fn i16_breaches(before: &[Item], after: &KpView, v_before: usize) -> Vec<String> {
    let u_before = before.iter().filter(|item| !item.verdict).count();
    let (v_after, u_after) = (after.v(), after.u());
    let old = hashes(before, false);
    let new_p3 = after.items.iter().enumerate().any(|(index, item)| {
        !item.verdict && !old.contains(item.hash.as_str()) && counts::is_p3(after, index)
    });
    let now = hashes(&after.items, false);
    let lost = hashes(before, true)
        .into_iter()
        .any(|hash| !now.contains(hash));
    let replaced = hashes(&after.items, true)
        .into_iter()
        .any(|hash| !old.contains(hash));
    [
        (v_after < v_before).then(|| format!("V went down: {v_before} to {v_after}")),
        (u_after > u_before + usize::from(new_p3))
            .then(|| format!("U went up: {u_before} to {u_after} (one new P3 item is legal)")),
        (lost && !replaced).then(|| {
            "a verdict exemplar of the base is not there, and the KP has no new verdict exemplar"
                .to_owned()
        }),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Run `diff`. The command gives exit 0 when it ran; `i16` has the result.
pub fn run(args: &[String]) -> Outcome {
    let opts = Opts::read(args, &["base", "kp", "repo", "tree"], &[])?;
    let (git_ref, id) = (opts.need("base")?, opts.need("kp")?);
    let repo = opts.get("repo").unwrap_or(".");
    let default_tree = Path::new(repo).join("curriculum");
    let after_dir = opts.get("tree").map_or_else(
        || default_tree.to_string_lossy().into_owned(),
        str::to_owned,
    );
    let after = Tree::load(&after_dir)?.view(id)?;
    // A KP that the base does not have is a KP with no exemplar there.
    let (before, v_before) = tree_at(repo, git_ref)?
        .view(id)
        .map_or((Vec::new(), 0), |view| (view.items.clone(), view.v()));
    let (old, now) = (hashes(&before, false), hashes(&after.items, false));
    let changed: Vec<&str> = old
        .intersection(&now)
        .copied()
        .filter(|hash| {
            let exemplar = |items| item_of(items, hash).map(|item: &Item| &item.exemplar);
            exemplar(&before) != exemplar(&after.items)
        })
        .collect();
    let findings: Vec<_> = i16_breaches(&before, &after, v_before)
        .into_iter()
        .map(|text| finding(&after, "invariant:I16", "I16", None, format!("I16: {text}")))
        .collect();
    let doc = json!({"schema": "cadus.diff.v1", "kp": after.kp,
        "V_before": v_before, "V_after": after.v(),
        "U_before": before.iter().filter(|item| !item.verdict).count(), "U_after": after.u(),
        "exemplars_before": before.len(), "exemplars_after": after.items.len(),
        "kept": old.intersection(&now).collect::<Vec<_>>(),
        "removed": old.difference(&now).collect::<Vec<_>>(),
        "added": now.difference(&old).collect::<Vec<_>>(),
        "changed_bytes": changed, "i16": findings.is_empty(), "findings": findings});
    Ok((doc, 0))
}
