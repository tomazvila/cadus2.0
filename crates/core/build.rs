use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn collect_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, files)?;
        } else if path.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR")
            .ok_or_else(|| std::io::Error::other("CARGO_MANIFEST_DIR is unavailable"))?,
    );
    let workspace = manifest.parent().and_then(Path::parent).ok_or_else(|| {
        std::io::Error::other("cadus-core is not inside the workspace crates directory")
    })?;
    let mut files = Vec::new();
    collect_files(&manifest.join("src"), &mut files)?;
    files.extend([
        manifest.join("Cargo.toml"),
        manifest.join("build.rs"),
        workspace.join("Cargo.toml"),
        workspace.join("Cargo.lock"),
        workspace.join("crates/worker/Cargo.toml"),
        workspace.join("crates/worker/src/bin/content_template_gate.rs"),
        workspace.join("crates/worker/src/bin/content_instruction_gate.rs"),
    ]);
    files.sort_by(|left, right| left.to_string_lossy().cmp(&right.to_string_lossy()));
    files.dedup();

    let mut hash = Sha256::new();
    hash.update(b"cadus-review-engine-v1\0");
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let relative = path.strip_prefix(workspace).unwrap_or(&path);
        let bytes = fs::read(&path)?;
        hash.update(relative.to_string_lossy().as_bytes());
        hash.update([0]);
        hash.update(bytes.len().to_le_bytes());
        hash.update(bytes);
        hash.update([0]);
    }
    println!(
        "cargo:rustc-env=CADUS_REVIEW_ENGINE_DIGEST={:x}",
        hash.finalize()
    );
    Ok(())
}
