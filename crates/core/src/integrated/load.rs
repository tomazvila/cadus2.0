//! Reading the authored integrated set from `curriculum/<course>/integrated/`.
//!
//! The unit loader reads `*.yaml` DIRECTLY in a course directory
//! (`curriculum::load::unit_entries`), so the `integrated/` subdirectory is
//! invisible to it and the two schemas never meet.
//!
//! One file holds one item. A file that does not parse, or an item that breaks a
//! fatal rule of [`validate`](super::validate), is dropped and reported; the
//! rest of the set still loads, the same way the curriculum loader treats a bad
//! unit file.

use std::fs;
use std::path::Path;

use crate::curriculum::{Curriculum, Finding};

use super::model::IntegratedItem;
use super::validate::{check_against, check_item};

/// The subdirectory of a course that holds its integrated tasks.
pub const INTEGRATED_DIR: &str = "integrated";

/// The file extension of an authored item.
const YAML_EXTENSION: &str = ".yaml";

/// The loaded integrated tasks and every defect found on the way.
#[derive(Debug, Default)]
pub struct IntegratedSet {
    /// The items that passed every fatal rule, in file order.
    items: Vec<IntegratedItem>,
    /// Every finding, fatal or advisory.
    findings: Vec<Finding>,
}

impl IntegratedSet {
    /// A set with no item. A deployment with no authored file uses this.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Validate `items` and keep the ones with no fatal finding.
    #[must_use]
    pub fn from_items(items: Vec<IntegratedItem>) -> Self {
        let mut set = Self::empty();
        for item in items {
            set.push(item, None);
        }
        set.check_ids();
        set
    }

    /// Read every `<root>/<course>/integrated/*.yaml` file.
    ///
    /// `root` is the curriculum root, the directory that holds `courses.yaml`.
    #[must_use]
    pub fn load(root: &Path) -> Self {
        let mut set = Self::empty();
        for course in sorted_names(root) {
            let dir = root.join(&course).join(INTEGRATED_DIR);
            if !dir.is_dir() {
                continue;
            }
            for name in sorted_names(&dir) {
                if !name.ends_with(YAML_EXTENSION) {
                    continue;
                }
                let rel = format!("{course}/{INTEGRATED_DIR}/{name}");
                set.read_file(&dir.join(&name), &rel);
            }
        }
        set.check_ids();
        set
    }

    /// Read one authored file into the set.
    fn read_file(&mut self, path: &Path, rel: &str) {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                self.findings.push(
                    Finding::new("integrated_read", format!("{rel}: {error}")).with_file(rel),
                );
                return;
            }
        };
        match serde_norway::from_str::<IntegratedItem>(&text) {
            Ok(item) => self.push(item, Some(rel)),
            Err(error) => self
                .findings
                .push(Finding::new("integrated_schema", format!("{rel}: {error}")).with_file(rel)),
        }
    }

    /// Validate one item and keep it when no fatal rule is broken.
    fn push(&mut self, item: IntegratedItem, rel: Option<&str>) {
        let findings = check_item(&item);
        let fatal = findings.iter().any(|finding| finding.fatal);
        for finding in findings {
            self.findings.push(match rel {
                Some(file) => finding.with_file(file),
                None => finding,
            });
        }
        if !fatal {
            self.items.push(item);
        }
    }

    /// Report a repeated item id and drop the later item.
    fn check_ids(&mut self) {
        let mut seen: Vec<String> = Vec::new();
        let mut findings = Vec::new();
        self.items.retain(|item| {
            let id = item.id.as_str().to_owned();
            if seen.contains(&id) {
                findings.push(Finding::new(
                    "integrated_item_id",
                    format!("two integrated items share the id {id}"),
                ));
                return false;
            }
            seen.push(id);
            true
        });
        self.findings.append(&mut findings);
    }

    /// The loaded items.
    #[must_use]
    pub fn items(&self) -> &[IntegratedItem] {
        &self.items
    }

    /// Every finding of the load.
    #[must_use]
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// The item with this id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&IntegratedItem> {
        self.items.iter().find(|item| item.id.as_str() == id)
    }

    /// The item a multi-step task over `components` serves (D-F10).
    ///
    /// The first item in file order that covers every component wins, so the
    /// answer is deterministic. `None` keeps the old per-component path.
    #[must_use]
    pub fn for_components(&self, components: &[String]) -> Option<&IntegratedItem> {
        self.items.iter().find(|item| item.covers(components))
    }

    /// Every item that records against `topic`.
    #[must_use]
    pub fn for_topic(&self, topic: &str) -> Vec<&IntegratedItem> {
        self.items
            .iter()
            .filter(|item| item.topic.as_str() == topic)
            .collect()
    }

    /// Check every loaded item against the curriculum arena.
    ///
    /// The build calls this after both loads: an item that names a topic or a
    /// knowledge point the arena does not hold is reported here, and the caller
    /// decides whether the deployment starts.
    #[must_use]
    pub fn check_curriculum(&self, graph: &Curriculum) -> Vec<Finding> {
        self.items
            .iter()
            .flat_map(|item| check_against(item, graph))
            .collect()
    }
}

/// Read one authored item from YAML text.
///
/// The loader above reads files; this reads a document a caller already holds,
/// which is what a review tool and a test fixture need. The rules of
/// [`check_item`] are NOT applied here: the caller decides what to do with a
/// document that parses and breaks a rule.
///
/// # Errors
///
/// The message of the parser, when the document is not one integrated item.
pub fn parse_item(text: &str) -> Result<IntegratedItem, String> {
    serde_norway::from_str(text).map_err(|error| error.to_string())
}

/// The entry names of `dir` in byte order, or an empty list when it cannot be
/// read. The order makes the loaded set reproducible.
fn sorted_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}
