//! Count the figures of a curriculum tree, per unit.
//!
//! Usage: `cargo run -p cadus-core --example visual_count [TREE] [--list]`
//!
//! The table has one row per unit: knowledge points, knowledge points with
//! figures, specs by family, reference figures (entries that no exemplar names)
//! and `broken_visuals` from the readiness index. The program exits 1 when one
//! figure is broken. With `--list` it prints only the serving key of each
//! knowledge point that holds a reference figure, one per line. That list is a
//! file for `cadus-worker author --kp-file`.
use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use cadus_core::curriculum::load_curriculum;
use cadus_core::readiness::{EmptyContent, ReadinessIndex};

#[derive(Default)]
struct Row {
    kps: usize,
    with_figures: usize,
    families: BTreeMap<&'static str, usize>,
    reference: usize,
    broken: usize,
}

fn main() -> ExitCode {
    let mut root = "curriculum".to_owned();
    let mut list = false;
    for argument in std::env::args().skip(1) {
        if argument == "--list" {
            list = true;
        } else {
            root = argument;
        }
    }
    let (curriculum, findings) = match load_curriculum(Path::new(&root)) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("visual_count: {error}");
            return ExitCode::FAILURE;
        }
    };
    if !findings.is_empty() {
        eprintln!("visual_count: the tree has {} findings", findings.len());
        return ExitCode::FAILURE;
    }
    let readiness = ReadinessIndex::build(&curriculum).resolve(&EmptyContent);
    let mut rows: BTreeMap<String, Row> = BTreeMap::new();
    let mut keys: Vec<String> = Vec::new();
    for topic in curriculum.topics() {
        let Some(idx) = curriculum.idx_of(topic.id.as_str()) else {
            continue;
        };
        let unit = format!("{}/{}", curriculum.course_of(idx), curriculum.unit_of(idx));
        let row = rows.entry(unit).or_default();
        for kp in &topic.knowledge_points {
            let key = format!("{}/{}", topic.id, kp.id);
            row.kps += 1;
            if !kp.visuals.is_empty() {
                row.with_figures += 1;
            }
            for spec in &kp.visuals {
                *row.families.entry(spec.kind()).or_default() += 1;
            }
            let reference = (0..kp.visuals.len())
                .filter(|at| !kp.exemplars.iter().any(|e| e.visual == Some(*at)))
                .count();
            row.reference += reference;
            if reference > 0 {
                keys.push(key.clone());
            }
            row.broken += readiness.get(&key).map_or(0, |r| r.broken_visuals);
        }
    }
    if list {
        for key in keys {
            println!("{key}");
        }
    } else {
        println!("unit\tkps\tkps_with_figures\tspecs_by_family\treference_figures\tbroken_visuals");
        for (unit, row) in &rows {
            let families: Vec<String> = row
                .families
                .iter()
                .map(|(family, count)| format!("{family}={count}"))
                .collect();
            println!(
                "{unit}\t{}\t{}\t{}\t{}\tbroken_visuals = {}",
                row.kps,
                row.with_figures,
                families.join(","),
                row.reference,
                row.broken
            );
        }
        println!(
            "total\t{}\t{}\t\t{}\tbroken_visuals = {}",
            rows.values().map(|r| r.kps).sum::<usize>(),
            rows.values().map(|r| r.with_figures).sum::<usize>(),
            rows.values().map(|r| r.reference).sum::<usize>(),
            rows.values().map(|r| r.broken).sum::<usize>()
        );
    }
    if rows.values().any(|row| row.broken > 0) {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
