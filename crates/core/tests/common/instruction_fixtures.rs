//! The fixtures of the instruction gate tests: one exemplar, two served
//! instances, and one accepted body of each kind.

use cadus_core::curriculum::Exemplar;
use cadus_core::instruction::{InstructionSpec, ServedInstance};

/// The one exemplar every test judges against: a problem that does NOT carry its
/// own answer, so the give-away rule reads every rung.
#[must_use]
pub fn exemplars() -> Vec<Exemplar> {
    vec![Exemplar {
        problem: "Compute $7^2$.".to_owned(),
        answer: "49".to_owned(),
        solution_sketch: None,
    }]
}

/// The spec of the knowledge point under test: no approved template, so the
/// exemplar answers are the whole served set.
#[must_use]
pub fn spec(exemplars: &[Exemplar]) -> InstructionSpec<'_> {
    InstructionSpec {
        exemplars,
        instance_answers: Vec::new(),
    }
}

/// The spec of a knowledge point whose stored templates render these instances.
///
/// Each pair is one served problem and the answer that problem expects. The
/// teach gate reads the pair; the hint gate reads the answer alone.
#[must_use]
pub fn spec_with_instances<'a>(
    exemplars: &'a [Exemplar],
    instances: &[(&str, &str)],
) -> InstructionSpec<'a> {
    InstructionSpec {
        exemplars,
        instance_answers: instances
            .iter()
            .map(|(problem, answer)| ServedInstance {
                problem: (*problem).to_owned(),
                answer: (*answer).to_owned(),
            })
            .collect(),
    }
}

/// The two instances of every test that judges against served material: the
/// squares of 8 and of 9.
pub const INSTANCES: [(&str, &str); 2] = [("Compute $8^2$.", "64"), ("Compute $9^2$.", "81")];

/// A teach body the gate accepts.
pub const GOOD_TEACH: &str = r#"{
    "concept": "Squaring a number multiplies it by itself.",
    "worked_example": {
        "problem": "Compute $6^2$.",
        "steps": ["Write $6^2$ as $6 \\times 6$.", "Multiply: $6 \\times 6 = 36$."]
    }
}"#;

/// A hint ladder the gate accepts. No rung names 49.
pub const GOOD_LADDER: &str = r#"{
    "hints": [
        "What does the small 2 above the number ask you to do?",
        "A square is the number multiplied by itself.",
        "Write the base twice with a multiplication sign between them, then multiply."
    ]
}"#;
