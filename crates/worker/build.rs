//! Bind offline gate evidence to the Rust sources actually compiled.

mod gate_fingerprint;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let root = manifest
        .parent()
        .and_then(std::path::Path::parent)
        .ok_or("workspace root absent")?;
    let mut files = [
        "Cargo.toml",
        "Cargo.lock",
        "crates/core/Cargo.toml",
        "crates/worker/Cargo.toml",
        "crates/worker/build.rs",
        "crates/worker/gate_fingerprint.rs",
    ]
    .map(|name| root.join(name))
    .to_vec();
    for directory in ["crates/core/src", "crates/worker/src"] {
        let path = root.join(directory);
        println!("cargo:rerun-if-changed={}", path.display());
        gate_fingerprint::collect(&path, "rs", &mut files)?;
    }
    for file in &files {
        println!("cargo:rerun-if-changed={}", file.display());
    }
    println!(
        "cargo:rustc-env=CADUS_TEMPLATE_GATE_SOURCE_HASH={}",
        gate_fingerprint::fingerprint(root, files)?
    );
    Ok(())
}
