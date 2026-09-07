//! M6 R1 acceptance: the authoring prompts and the digest.
//!
//! The section 7 row R1 of `docs/reference/authoring-and-spa-1.0-spec.md` names
//! three checks, and each one is a test here:
//!
//! 1. the retry block reproduces the 1.0 wording of `prompts.py:766-774`;
//! 2. the digest is stable across a re-serialization, and changes when the
//!    schema changes;
//! 3. a golden file pins the rendered user message for one knowledge point.
//!
//! Every expected value is a LITERAL: the literal three lines of the 1.0 retry
//! block and a golden file on disk. Nothing is re-derived from the code under
//! test. `authoring_schema.rs` holds the checks of the four kinds, the tools
//! and the schemas.
//!
//! No test here reaches a model. This unit builds a request and makes no call.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_worker::authoring::prompt::{
    DIGEST_CHARS, KINDS, Kind, RETRY_HEADER, digest_material, digest_of, prompt_digest,
    retry_block, user_message,
};
use serde_json::{Value, json};

use common::golden_spec;

/// The rendered first-attempt message of the golden knowledge point.
const GOLDEN_FIRST: &str = include_str!("fixtures/authoring/template_user_first.txt");

/// The rendered retry message of the same knowledge point.
const GOLDEN_RETRY: &str = include_str!("fixtures/authoring/template_user_retry.txt");

/// The gate message the golden retry carries. It is one literal rejection of
/// `cadus_core::template::gate` ("edge-coverage").
const GOLDEN_FEEDBACK: &str = "no worked sample uses the low end of a (10) — the edges are where an expression stops being right";

// --------------------------------------------------------------------------- //
// Check 1: the retry block
// --------------------------------------------------------------------------- //

/// The template retry block, byte for byte from `prompts.py:766-774`.
///
/// 1.0 appends four `lines.append` calls: the header, the reason at a
/// four-space indent, and the fix sentence, which its two source lines join
/// into one. The reason goes in VERBATIM, because 1.0 measured that the
/// rejection message is what rescues the template.
#[test]
fn the_retry_block_reproduces_the_1_0_wording() {
    let block = retry_block(Kind::Template, "template text is missing or empty");
    let lines: Vec<&str> = block.split('\n').collect();
    assert_eq!(lines.len(), 3, "the 1.0 block is three lines");
    assert_eq!(
        lines[0],
        "YOUR PREVIOUS ATTEMPT WAS REFUSED. The server's exact reason was:"
    );
    assert_eq!(lines[1], "    template text is missing or empty");
    assert_eq!(
        lines[2],
        "Fix that specifically. Do not restate the same template — change the domains, the samples, or the expression so the reason no longer applies."
    );
}

/// The other three kinds keep the 1.0 frame and name their own fields.
///
/// "change the domains, the samples, or the expression" instructs a teach-page
/// author to edit fields a teach page does not have, so each kind pins its own
/// closing line and every line is literal here.
#[test]
fn the_other_kinds_keep_the_frame_and_name_their_own_fields() {
    let expected = [
        (
            Kind::Teach,
            "Fix that specifically. Do not restate the same page — change the concept, the worked problem, or its steps so the reason no longer applies.",
        ),
        (
            Kind::HintLadder,
            "Fix that specifically. Do not restate the same ladder — change the rungs so the reason no longer applies.",
        ),
        (
            Kind::Diagnosis,
            "Fix that specifically. Do not restate the same list — change the answers, the tags, or the notes so the reason no longer applies.",
        ),
    ];
    for (kind, fix) in expected {
        let block = retry_block(kind, "a reason");
        let lines: Vec<&str> = block.split('\n').collect();
        assert_eq!(lines.len(), 3, "{} block is three lines", kind.as_str());
        assert_eq!(
            lines[0],
            "YOUR PREVIOUS ATTEMPT WAS REFUSED. The server's exact reason was:"
        );
        assert_eq!(lines[1], "    a reason");
        assert_eq!(lines[2], fix, "{} closing line", kind.as_str());
    }
}

/// A rejection message reaches the model unchanged, newlines included.
#[test]
fn the_rejection_message_is_carried_verbatim() {
    let reason = "sample 0 binds a=9, which its own domain cannot produce — a sample outside the domain verifies nothing";
    let block = retry_block(Kind::Template, reason);
    assert!(block.contains(reason), "the message is not verbatim");
}

/// The first attempt carries no retry block at all.
#[test]
fn the_first_attempt_carries_no_retry_block() {
    let message = user_message(Kind::Template, &golden_spec(), None);
    assert!(!message.contains(RETRY_HEADER));
    assert!(!message.contains("Fix that specifically."));
}

// --------------------------------------------------------------------------- //
// Check 2: the digest
// --------------------------------------------------------------------------- //

/// The digest survives a JSON round trip of its own material.
///
/// `serde_json` holds an object in a key-sorted map, so the text is the same
/// bytes at every depth after a parse and a re-print. That is 1.0's
/// `sort_keys=True`.
#[test]
fn the_digest_is_stable_across_a_re_serialization() {
    for kind in KINDS {
        let material = digest_material(kind);
        let text = material.to_string();
        let reread: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            digest_of(&material),
            digest_of(&reread),
            "{} digest moved on a re-serialization",
            kind.as_str()
        );
        assert_eq!(reread.to_string(), text, "{} text moved", kind.as_str());
        assert_eq!(
            prompt_digest(kind),
            digest_of(&material),
            "{} digest is not the digest of its material",
            kind.as_str()
        );
    }
}

/// One added schema property gives a different digest.
#[test]
fn the_digest_changes_when_the_schema_changes() {
    for kind in KINDS {
        let before = prompt_digest(kind);
        let mut material = digest_material(kind);
        material["schema"]["properties"]["invented_field"] = json!({"type": "string"});
        let after = digest_of(&material);
        assert_ne!(
            before,
            after,
            "{} digest did not move when the schema gained a field",
            kind.as_str()
        );
    }
}

/// One edited word of the system message gives a different digest.
#[test]
fn the_digest_changes_when_the_system_message_changes() {
    for kind in KINDS {
        let mut material = digest_material(kind);
        material["system"] = json!("a different instruction");
        assert_ne!(
            prompt_digest(kind),
            digest_of(&material),
            "{} digest did not move when the system message changed",
            kind.as_str()
        );
    }
}

/// The digest is 16 lowercase hex characters, and the four kinds differ.
#[test]
fn the_digest_is_sixteen_hex_characters_and_one_per_kind() {
    assert_eq!(DIGEST_CHARS, 16);
    let mut seen: Vec<String> = Vec::new();
    for kind in KINDS {
        let digest = prompt_digest(kind);
        assert_eq!(digest.len(), 16, "{} digest length", kind.as_str());
        assert!(
            digest
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "{} digest is not lowercase hex: {digest}",
            kind.as_str()
        );
        assert!(
            !seen.contains(&digest),
            "{} shares a digest with an earlier kind",
            kind.as_str()
        );
        seen.push(digest);
    }
}

/// The four digests of this checkout, pinned.
///
/// 1.0 pins the same value in its own survey
/// (`docs/reference/serving-1.0-spec.md:113`: `f322b85a40b9ac50`). The pin makes
/// every prompt edit and every schema edit a visible diff, and it is the value
/// `cadus_worker::authoring::job::store_pending` writes into
/// `content_store.prompt_digest` (M6 review finding F4).
#[test]
fn the_prompt_digests_of_this_checkout_are_pinned() {
    let pinned = [
        (Kind::Template, "31a0f1e0010115bd"),
        (Kind::Teach, "dfc6329034bf030f"),
        (Kind::HintLadder, "c8e040f53fce1881"),
        // Unit R7 moved this one: the `emit_distractors` tool asks for a
        // literal answer, and the `emit_template` tool keeps its expression, so
        // the template digest above stands.
        (Kind::Diagnosis, "164baef458d55e4f"),
    ];
    for (kind, digest) in pinned {
        assert_eq!(
            prompt_digest(kind),
            digest,
            "the {} prompt changed; a prompt edit marks the affected rows for re-authoring (C6), so update this pin with it",
            kind.as_str()
        );
    }
}

/// The digest covers the prompt, never the knowledge point.
///
/// The user message carries the knowledge point. A fold of it gives one digest
/// per knowledge point instead of one digest per prompt.
#[test]
fn the_digest_material_holds_three_keys_and_no_user_message() {
    let material = digest_material(Kind::Template);
    let object = material.as_object().unwrap();
    let keys: Vec<&str> = object.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["schema", "system", "tool"]);
    assert_eq!(object["tool"], json!("emit_template"));
    assert!(!material.to_string().contains("sub-borrow-two-digit"));
}

// --------------------------------------------------------------------------- //
// Check 3: the golden user message
// --------------------------------------------------------------------------- //

/// The rendered first-attempt user message of one knowledge point.
#[test]
fn the_rendered_user_message_is_pinned() {
    assert_eq!(
        user_message(Kind::Template, &golden_spec(), None),
        GOLDEN_FIRST
    );
}

/// The rendered retry message of the same knowledge point.
#[test]
fn the_rendered_retry_message_is_pinned() {
    assert_eq!(
        user_message(Kind::Template, &golden_spec(), Some(GOLDEN_FEEDBACK)),
        GOLDEN_RETRY
    );
}

/// A knowledge point that states no constraints and holds no exemplars renders
/// the two 1.0 stand-in lines (`prompts.py:667`, `:750`).
#[test]
fn the_empty_spec_renders_the_1_0_stand_in_lines() {
    let mut spec = golden_spec();
    spec.constraints = None;
    spec.exemplars = Vec::new();
    let message = user_message(Kind::Template, &spec, None);
    assert!(
        message.contains("\nConstraints: none stated.\n"),
        "{message}"
    );
    assert!(
        message.contains("\n(no exemplars provided — infer a reasonable problem for this topic)\n"),
        "{message}"
    );
    assert!(
        message.contains("Difficulty target: standard review (80-85% expected accuracy)"),
        "{message}"
    );
}
