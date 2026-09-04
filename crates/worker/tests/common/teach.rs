//! The teach page and the hint ladder of the perfect-squares knowledge point
//! (L4, L5).

use serde_json::{Value, json};

/// The tool arguments of a teach page the gate accepts.
pub fn teach_arguments() -> Value {
    json!({
        "concept": "Squaring a number multiplies it by itself.",
        "worked_example": {
            "problem": "Compute $6^2$.",
            "steps": ["Write $6^2$ as $6 \\times 6$.", "Multiply: $6 \\times 6 = 36$."]
        }
    })
}

/// The tool arguments of a hint ladder the gate accepts. No rung names 49.
pub fn ladder_arguments() -> Value {
    json!({
        "hints": [
            "What does the small 2 above the number ask you to do?",
            "A square is the number multiplied by itself.",
            "Write the base twice with a multiplication sign between them, then multiply."
        ]
    })
}

/// A ladder whose last rung states 81, which is the answer of the instance
/// `Compute $9^2$.` and is NOT the answer of any exemplar.
pub fn ladder_that_names_an_instance_answer() -> Value {
    json!({
        "hints": [
            "What does the small 2 above the number ask you to do?",
            "For a base of 9 the product is 81."
        ]
    })
}

/// The body the loop stores for [`teach_arguments`], character for character.
///
/// The stored text is the GATED document, so the field order is the document's
/// and no spare field survives. It carries no knowledge-point field at all,
/// which is why the digest below covers the knowledge point and the kind beside
/// the body (M6 review finding F1). Computed outside this tree with
///
/// ```sh
/// printf 'perfect-squares/squares\0teach\0%s' '<STORED_TEACH_BODY>' | sha256sum
/// # fc031ed7deaa7d60283410d81fe87f114375d02356f9ba5fe315fb9af1a362c2
/// ```
pub const STORED_TEACH_BODY: &str = r#"{"concept":"Squaring a number multiplies it by itself.","worked_example":{"problem":"Compute $6^2$.","steps":["Write $6^2$ as $6 \\times 6$.","Multiply: $6 \\times 6 = 36$."]}}"#;

/// The digest of [`STORED_TEACH_BODY`] under `perfect-squares/squares` and
/// kind `teach`.
pub const STORED_TEACH_DIGEST: &str = "sha256:fc031ed7deaa7d60";

/// The digest of [`STORED_TEACH_BODY`] under the SECOND knowledge point,
/// `perfect-cubes/cubes`, and kind `teach`.
///
/// ```sh
/// printf 'perfect-cubes/cubes\0teach\0%s' '<STORED_TEACH_BODY>' | sha256sum
/// # 642bb7b9c96c3add8a9d5536215f271fd914f47ecf0d5c18fdd4822a1841d5a0
/// ```
pub const OTHER_TEACH_DIGEST: &str = "sha256:642bb7b9c96c3add";

/// The body the loop stores for [`ladder_arguments`], character for character.
///
/// ```sh
/// printf 'perfect-squares/squares\0hint_ladder\0%s' '<STORED_LADDER_BODY>' | sha256sum
/// # 5c235ad773e2b639164134d43addb74700535dc11aa9973f494ff79b78853e5b
/// ```
pub const STORED_LADDER_BODY: &str = r#"{"hints":["What does the small 2 above the number ask you to do?","A square is the number multiplied by itself.","Write the base twice with a multiplication sign between them, then multiply."]}"#;

/// The digest of [`STORED_LADDER_BODY`] under `perfect-squares/squares` and
/// kind `hint_ladder`.
pub const STORED_LADDER_DIGEST: &str = "sha256:5c235ad773e2b639";

/// The gate's give-away sentence for the last rung of
/// [`ladder_that_names_an_instance_answer`].
pub const NAMES_AN_INSTANCE_ANSWER: &str = "rung 1 reads 'For a base of 9 the product is 81.', which \
names the answer '81' this knowledge point serves — a hint is a question, never the final step \
(Hard Rule 3)";
