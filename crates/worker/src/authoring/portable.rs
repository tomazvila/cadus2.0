//! Provider-portable tool transport; production document gates remain authoritative.
use crate::authoring::{job::document_digest, prompt::Kind};
use cadus_core::{instruction::ServedInstance, template::Rejection};
use cadus_model_client::{ChatRequest, ModelError};
use serde_json::{Value, json};

/// Wrap the logical tool schema in a single string field.
/// The logical schema remains text in the request and retains its prompt digest.
pub fn prepare(request: &mut ChatRequest, kind: Kind, instances: &[ServedInstance]) {
    let logical = request.tool.parameters.to_string();
    request.user.push_str("\n\nTRANSPORT: emit document_json as a string containing the COMPLETE original JSON document. Do not omit any original fields. Original document schema:\n");
    request.user.push_str(&logical);
    if kind == Kind::Template {
        request.user.push_str("\nA params dictionary must contain named domains, never {}. Example shape: {\"a\":{\"kind\":\"int\",\"low\":0,\"high\":9},\"b\":{\"kind\":\"int\",\"low\":0,\"high\":9}}. Adapt bounds and constraints to this knowledge point. Each sample.params supplies a value for EVERY declared name. Use distractors: [] when no wrong-answer expression is wrong for EVERY satisfying tuple. In particular, a-b equals a+b when b=0; such a distractor needs a constraint that excludes b=0, or omit it.");
    }
    if kind == Kind::Teach {
        request.user.push_str("\nThe worked problem must have different operand values from every existing served problem below. Use a recognizable direct calculation when that is the target shape. Put only the final result, or one correct final equation, in the last step. Existing served problems:\n");
        for instance in instances.iter().take(100) {
            request.user.push_str(&instance.problem);
            request.user.push('\n');
        }
    }
    request.tool.parameters = json!({
        "type": "object", "additionalProperties": false,
        "required": ["document_json"],
        "properties": {"document_json": {"type": "string", "description": "The complete authored document as valid JSON text. Follow the original schema in the user message, including all dynamic parameter names, constraints and sample bindings."}}
    });
}

/// Read the portable envelope before the unchanged document gate.
///
/// # Errors
/// Reject an absent string, malformed JSON or a non-object document.
pub fn unpack(envelope: Value) -> Result<Value, ModelError> {
    let raw = envelope
        .get("document_json")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ModelError::Reply("portable tool needs document_json as a string".to_owned())
        })?;
    let value: Value = serde_json::from_str(raw).map_err(|_| {
        ModelError::Reply("document_json is not valid JSON; emit the complete document".to_owned())
    })?;
    if !value.is_object() {
        return Err(ModelError::Reply(
            "document_json must contain an object".to_owned(),
        ));
    }
    Ok(value)
}

/// Save one rejected draft. The artifact has no client configuration or credentials.
pub fn snapshot(
    directory: &std::path::Path,
    kp: &str,
    kind: Kind,
    attempt: u32,
    arguments: &Value,
    refusal: &Rejection,
) {
    let body = json!({"kp_id": kp, "kind": kind.as_str(), "attempt": attempt,
        "arguments": arguments, "refusal": {"code": refusal.code, "message": refusal.message}});
    let digest = document_digest(kp, kind, &arguments.to_string());
    let file = directory.join(format!(
        "{}-{}-{attempt}.json",
        kind.as_str(),
        digest.trim_start_matches("sha256:")
    ));
    let result =
        std::fs::create_dir_all(directory).and_then(|()| std::fs::write(&file, body.to_string()));
    if let Err(error) = result {
        tracing::error!(%error, "declined draft artifact did not write");
    }
}
