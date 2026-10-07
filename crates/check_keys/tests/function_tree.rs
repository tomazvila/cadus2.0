//! `check_keys` on a small tree with `function` items.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const COURSES: &str = "courses:\n- id: c\n  name: c\n  order: 1\n";

/// One unit file; `{ITEMS}` is the exemplar list of the one knowledge point.
const UNIT: &str = r"unit: u
course: c
module: M
topics:
  - id: chain-rule
    name: The chain rule
    difficulty: 0.5
    answer_kind: expression
    expected_time_secs: 60
    knowledge_points:
      - id: kp1
        name: Formulas
        exemplars:
{ITEMS}";

const GOOD_ITEMS: &str = r#"          - problem: 'Differentiate $\sqrt{x^2+9}$.'
            answer_contract: {kind: function, vars: [x]}
            answer: "x/sqrt(x^2+9)"
          - problem: 'Differentiate $1/(3x+1)$.'
            answer_contract: {kind: function, vars: [x]}
            answer: "-3/(3x+1)^2"
          - problem: 'Integrate $x$.'
            answer_contract: {kind: function, vars: [x], up_to_constant: true}
            answer: "y = x^2/2 + C"
          - problem: 'Give the two first partial derivatives of $x^2 y$.'
            answer_contract:
              kind: multipart
              parts:
                - {name: fx, contract: {kind: function, vars: [x, y]}}
                - {name: fy, contract: {kind: function, vars: [x, y]}}
            answer: "fx = 2xy; fy = x^2"
"#;

// Grader pass 3 made `ln(x - 5)` finite on a fallback interval and gave `log`
// one reading (base 10), so the refused keys are a function with no finite
// value anywhere and a key that names a letter outside its variables.
const BAD_ITEMS: &str = r#"          - problem: 'Differentiate $\sqrt{-x^2 - 1}$.'
            answer_contract: {kind: function, vars: [x]}
            answer: "-x/sqrt(-x^2 - 1)"
          - problem: 'Differentiate $ax^2$.'
            answer_contract: {kind: function, vars: [x]}
            answer: "2ax"
"#;

/// Write the tree below the target directory of the build (not below `/tmp`).
fn tree(label: &str, items: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(label);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("c")).unwrap();
    fs::write(root.join("courses.yaml"), COURSES).unwrap();
    fs::write(root.join("c/00-u.yaml"), UNIT.replace("{ITEMS}", items)).unwrap();
    root
}

/// Run the tool with no database; give the exit code and the two outputs.
fn run(root: &Path) -> (Option<i32>, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_check_keys"))
        .arg("--curriculum")
        .arg(root)
        .env_remove("DATABASE_URL")
        .env_remove("CADUS_CURRICULUM")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.code(), text)
}

#[test]
fn a_tree_with_good_function_items_reports_no_failure() {
    let (code, text) = run(&tree("function-good", GOOD_ITEMS));
    assert_eq!(code, Some(0), "{text}");
    assert!(text.contains("failed=0"), "{text}");
    assert!(text.contains("checked=4"), "{text}");
    assert!(!text.contains("no numeric component to mutate"), "{text}");
}

#[test]
fn a_function_key_that_the_contract_refuses_is_a_failure() {
    let (code, text) = run(&tree("function-bad", BAD_ITEMS));
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("failed=2"), "{text}");
    assert!(
        text.contains("fewer than six finite sample points"),
        "{text}"
    );
    assert!(text.contains("name outside its variables"), "{text}");
}
