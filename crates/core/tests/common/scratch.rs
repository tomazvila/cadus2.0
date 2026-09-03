//! A curriculum tree the test writes itself, for the forms no committed
//! fixture carries. The tree lives under the system temporary directory and
//! goes away with the value.

use std::path::{Path, PathBuf};

/// One temporary curriculum tree.
pub struct ScratchTree {
    root: PathBuf,
}

impl ScratchTree {
    /// Make an empty tree named after `label` and the process id.
    #[must_use]
    pub fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!("cadus-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("the temporary tree is writable");
        Self { root }
    }

    /// The root of the tree.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Write one file at `rel`, with its parent directories.
    pub fn write(&self, rel: &str, text: &str) -> &Self {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("the parent directory is writable");
        }
        std::fs::write(&path, text).expect("the file is writable");
        self
    }

    /// Make one directory at `rel`.
    pub fn dir(&self, rel: &str) -> &Self {
        std::fs::create_dir_all(self.root.join(rel)).expect("the directory is writable");
        self
    }

    /// Write a `courses.yaml` with one course per id, in id order.
    pub fn courses(&self, ids: &[&str]) -> &Self {
        let mut text = "courses:\n".to_owned();
        for (order, id) in ids.iter().enumerate() {
            text.push_str(&format!(
                "- id: {id}\n  name: {id}\n  order: {}\n",
                order + 1
            ));
        }
        self.write("courses.yaml", &text)
    }

    /// Write one unit file of course `course` at `rel` with one bare topic per
    /// `topics` entry. Each entry is the topic id followed by its extra YAML
    /// lines, indented by four spaces.
    pub fn unit(&self, rel: &str, course: &str, topics: &[(&str, &str)]) -> &Self {
        let mut text = format!("unit: u\ncourse: {course}\nmodule: M\ntopics:\n");
        for (id, extra) in topics {
            text.push_str(&format!(
                "  - id: {id}\n    name: {id}\n    difficulty: 0.3\n    answer_kind: numeric\n    \
                 expected_time_secs: 30\n{extra}"
            ));
        }
        self.write(rel, &text)
    }
}

impl Drop for ScratchTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
