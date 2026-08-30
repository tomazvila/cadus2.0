//! M6 R1 acceptance: the authoring prompts, the tool schemas, and the digest.
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
//! block, the literal wire names of the four kinds, the literal required list of
//! each schema, and a golden file on disk. Nothing is re-derived from the code
//! under test.
//!
//! No test here reaches a model. This unit builds a request and makes no call.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use cadus_core::curriculum::{AnswerKind, Exemplar};
use cadus_core::template::Cmp;
use cadus_worker::authoring::prompt::{
    AuthoringSpec, CONSTRAINT_OPS, DIGEST_CHARS, KINDS, Kind, RETRY_HEADER, digest_material,
    digest_of, prompt_digest, request, retry_block, system_prompt, tool_schema, tool_spec,
    user_message,
};
use cadus_worker::diagnosis::MODEL_ERROR_TAGS;
use serde_json::{Value, json};

/// The rendered first-attempt message of the golden knowledge point.
const GOLDEN_FIRST: &str = include_str!("fixtures/authoring/template_user_first.txt");

/// The rendered retry message of the same knowledge point.
const GOLDEN_RETRY: &str = include_str!("fixtures/authoring/template_user_retry.txt");

/// The gate message the golden retry carries. It is one literal rejection of
/// `cadus_core::template::gate` ("edge-coverage").
const GOLDEN_FEEDBACK: &str = "no worked sample uses the low end of a (10) — the edges are where an expression stops being right";

/// The knowledge point both golden files render.
///
/// It is the shape 1.0 failed to template: "subtraction with borrowing" needs
/// `a > b` between two parameters, and 1.0 had no constraint language for it
/// (`problem_templates.py:59-66`).
fn golden_spec() -> AuthoringSpec {
    AuthoringSpec {
        kp_id: "sub-borrow-two-digit".to_owned(),
        kp_name: "Two-digit subtraction with borrowing".to_owned(),
        topic_id: "subtraction-borrowing".to_owned(),
        topic_name: "Subtraction with borrowing".to_owned(),
        answer_kind: AnswerKind::Numeric,
        difficulty_target: None,
        constraints: Some(
            "the ones digit of the first number is smaller than the ones digit of the second"
                .to_owned(),
        ),
        exemplars: vec![
            Exemplar {
                problem: "Compute $52 - 27$.".to_owned(),
                answer: "25".to_owned(),
                solution_sketch: Some("Borrow one ten, then subtract the ones column.".to_owned()),
            },
            Exemplar {
                problem: "Compute $81 - 46$.".to_owned(),
                answer: "35".to_owned(),
                solution_sketch: None,
            },
        ],
    }
}

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
/// unit R2 writes onto the stored row.
#[test]
fn the_prompt_digests_of_this_checkout_are_pinned() {
    let pinned = [
        (Kind::Template, "a40cdfe609d76a83"),
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

// --------------------------------------------------------------------------- //
// The four kinds, the tools, and the schemas
// --------------------------------------------------------------------------- //

/// The wire values are the four the `content_store` CHECK admits.
#[test]
fn the_kind_wire_values_are_the_schema_check_list() {
    let names: Vec<&str> = KINDS.iter().map(|kind| kind.as_str()).collect();
    assert_eq!(names, vec!["template", "teach", "hint_ladder", "diagnosis"]);
    for kind in KINDS {
        assert_eq!(Kind::from_wire(kind.as_str()), Some(kind));
    }
    assert_eq!(Kind::from_wire("multistep"), None);
    assert_eq!(Kind::from_wire("Template"), None);
    assert_eq!(Kind::from_wire(""), None);
}

/// Each kind forces its own tool, and the request carries all three parts.
#[test]
fn each_kind_forces_its_own_tool() {
    let expected = [
        (Kind::Template, "emit_template"),
        (Kind::Teach, "emit_teach"),
        (Kind::HintLadder, "emit_hint_ladder"),
        (Kind::Diagnosis, "emit_distractors"),
    ];
    for (kind, name) in expected {
        assert_eq!(tool_spec(kind).name, name);
        let built = request(kind, &golden_spec(), None);
        assert_eq!(built.tool.name, name);
        assert_eq!(built.system, system_prompt(kind));
        assert_eq!(built.user, user_message(kind, &golden_spec(), None));
        assert!(
            built.user.ends_with(&format!("via the {name} tool.")),
            "{} does not ask for its own tool",
            kind.as_str()
        );
        assert!(
            built.system.contains(name),
            "{} system message does not name its tool",
            kind.as_str()
        );
    }
}

/// The template tool asks for the authored fields and for no server field.
#[test]
fn the_template_schema_requires_the_authored_fields_only() {
    let schema = tool_schema(Kind::Template);
    assert_eq!(
        schema["required"],
        json!([
            "statement",
            "params",
            "constraints",
            "answer_expr",
            "solution_sketch",
            "hints",
            "distractors",
            "samples"
        ])
    );
    let properties = schema["properties"].as_object().unwrap();
    for server_field in [
        "v",
        "topic_id",
        "answer_kind",
        "space_size",
        "kp_id",
        "status",
    ] {
        assert!(
            !properties.contains_key(server_field),
            "the model is asked for the server field {server_field}"
        );
    }
}

/// The other three tools ask for the fields their stored bodies hold.
#[test]
fn the_other_schemas_require_their_stored_fields() {
    assert_eq!(
        tool_schema(Kind::Teach)["required"],
        json!(["concept", "worked_example"])
    );
    assert_eq!(
        tool_schema(Kind::Teach)["properties"]["worked_example"]["required"],
        json!(["problem", "steps"])
    );
    assert_eq!(tool_schema(Kind::HintLadder)["required"], json!(["hints"]));
    assert_eq!(
        tool_schema(Kind::Diagnosis)["required"],
        json!(["distractors"])
    );
    assert_eq!(
        tool_schema(Kind::Diagnosis)["properties"]["distractors"]["items"]["required"],
        json!(["answer", "error_tag", "note"])
    );
}

/// Every object of every schema refuses an unknown key.
///
/// A model that invents a field must get a rejection, not a silently dropped
/// instruction — the same rule `TemplateDoc` states with
/// `#[serde(deny_unknown_fields)]`.
#[test]
fn every_schema_object_refuses_unknown_fields() {
    fn walk(node: &Value, path: &str) {
        if let Some(object) = node.as_object() {
            if object.contains_key("properties") {
                assert_eq!(
                    object.get("additionalProperties"),
                    Some(&json!(false)),
                    "{path} admits unknown fields"
                );
            }
            for (key, value) in object {
                walk(value, &format!("{path}.{key}"));
            }
        }
        if let Some(list) = node.as_array() {
            for (index, value) in list.iter().enumerate() {
                walk(value, &format!("{path}[{index}]"));
            }
        }
    }
    for kind in KINDS {
        walk(&tool_schema(kind), kind.as_str());
    }
}

// --------------------------------------------------------------------------- //
// The prompt and the code are one statement
// --------------------------------------------------------------------------- //

/// The two prompts that invite an error tag name every tag the filter keeps.
///
/// The 1.0 defect this closes: a tag the prompt invited and the filter lacked
/// was dropped in silence (`prompts.py:529-536`).
#[test]
fn the_prompts_name_every_tag_the_filter_keeps() {
    for kind in [Kind::Template, Kind::Diagnosis] {
        let prompt = system_prompt(kind);
        let schema = tool_schema(kind).to_string();
        for tag in MODEL_ERROR_TAGS {
            assert!(
                prompt.contains(tag),
                "the {} prompt does not name {tag}",
                kind.as_str()
            );
            assert!(
                schema.contains(tag),
                "the {} schema does not name {tag}",
                kind.as_str()
            );
        }
    }
}

/// Every comparison the schema offers is a comparison the core reads.
#[test]
fn constraint_ops_parse_as_the_core_grammar() {
    assert_eq!(
        CONSTRAINT_OPS,
        [
            "eq", "ne", "lt", "le", "gt", "ge", "divides", "coprime", "carries"
        ]
    );
    for op in CONSTRAINT_OPS {
        let parsed: Cmp = serde_json::from_value(json!(op))
            .unwrap_or_else(|error| panic!("the core refuses the op {op}: {error}"));
        assert_eq!(parsed.as_str(), op);
    }
    assert_eq!(
        tool_schema(Kind::Template)["properties"]["constraints"]["items"]["properties"]["op"]["enum"],
        json!(CONSTRAINT_OPS)
    );
}

/// The template prompt states the constraint rule 1.0 had no words for.
///
/// The 1.0 measurement: "subtraction with borrowing" needs `a > b`, the model
/// kept authoring two independent 10..99 ranges, and the re-prompt did not
/// rescue it (`problem_templates.py:59-66`).
#[test]
fn the_template_prompt_states_the_constraint_rule() {
    let prompt = system_prompt(Kind::Template);
    assert!(prompt.contains("STATE A CONSTRAINT, DO NOT NARROW A DOMAIN TO FAKE ONE."));
    assert!(prompt.contains("carries"));
    assert!(prompt.contains("at least 12 distinct problems"));
    assert!(prompt.contains("at most 10000 values"));
    assert!(prompt.contains("at most 24"));
}

/// The template prompt carries the two 1.0 incident reports.
///
/// 1.0 records that the prompt reads as instruction and not as policy because
/// of them (spec section 2.1, `prompts.py:472-482`).
#[test]
fn the_template_prompt_carries_the_two_incident_reports() {
    let prompt = system_prompt(Kind::Template);
    assert!(prompt.contains("graded a correct learner wrong on 18 of 30 problems"));
    assert!(prompt.contains("served a wrong answer to half of every learner's problems"));
}

/// No prompt asks a model to compute a served answer (T1).
#[test]
fn the_template_prompt_puts_the_answer_on_the_server() {
    let prompt = system_prompt(Kind::Template);
    assert!(prompt.contains("the server computes each instance's answer from it"));
    assert!(prompt.contains("The problem statement NEVER contains the answer or the method"));
}

/// The hint prompt states the L5 rule, and the teach prompt states the L4 shape.
#[test]
fn the_hint_and_teach_prompts_state_their_rules() {
    let hint = system_prompt(Kind::HintLadder);
    assert!(hint.contains("no rung reveals the final answer"));
    assert!(hint.contains("the last step that produces"));
    let teach = system_prompt(Kind::Teach);
    assert!(teach.contains("'concept' states the rule or the method"));
    assert!(teach.contains("'worked_example.steps' is the COMPLETE solution"));
}
