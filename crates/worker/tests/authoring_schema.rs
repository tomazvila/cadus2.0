//! M6 R1 acceptance: the four kinds, their tools, and their schemas (A2, T5).
//!
//! Every expected value is a LITERAL: the literal wire names of the four kinds,
//! the literal required list of each schema, and the literal sentences the
//! prompts state. Nothing is re-derived from the code under test.
//!
//! No test here reaches a model. This unit builds a request and makes no call.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::template::Cmp;
use cadus_worker::authoring::prompt::{
    CONSTRAINT_OPS, KINDS, Kind, request, system_prompt, tool_schema, tool_spec, user_message,
};
use cadus_worker::diagnosis::MODEL_ERROR_TAGS;
use serde_json::{Value, json};

use common::golden_spec;

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
