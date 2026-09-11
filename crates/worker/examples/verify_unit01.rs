//! Offline unit01 authoring verification using the unchanged production worker.
#![allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "unit01/verify.rs"]
mod verify;

fn main() {
    let path = std::env::args().nth(1).expect("pass a draft JSON file");
    let output = std::env::args().nth(2).expect("pass an evidence directory");
    let report = verify::run(std::path::Path::new(&path), std::path::Path::new(&output));
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
