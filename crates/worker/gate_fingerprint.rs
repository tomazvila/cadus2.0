//! Stable source fingerprints shared by the build and offline audit adapter.

use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub fn collect(directory: &Path, extension: &str, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, extension, files)?;
        } else if path.extension().is_some_and(|value| value == extension) {
            files.push(path);
        }
    }
    Ok(())
}

pub fn fingerprint(root: &Path, mut files: Vec<PathBuf>) -> io::Result<String> {
    files.sort();
    let mut digest = Sha256::new();
    for path in files {
        let relative = path.strip_prefix(root).map_err(io::Error::other)?;
        digest.update(relative.to_string_lossy().as_bytes());
        digest.update([0]);
        digest.update(std::fs::read(path)?);
        digest.update([0]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
