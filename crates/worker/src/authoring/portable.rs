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
        request.user.push_str(TEMPLATE_RULES);
        request.user.push_str("\nVALID COMPLETE JSON EXAMPLE (adapt the mathematics and bounds to the requested KP):\n");
        request.user.push_str(TEMPLATE_EXAMPLE);
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

/// Gate-compatible arithmetic example, also exercised through the fake provider.
pub const TEMPLATE_EXAMPLE: &str = r#"{"statement":"Compute ${a}^{{2}}$.","params":{"a":{"kind":"int","low":1,"high":12}},"constraints":[],"answer_expr":"a**2","solution_sketch":"${a} \\times {a}$ gives the answer.","hints":["What does squaring a number mean?"],"distractors":[],"samples":[{"params":{"a":1},"expected":"1"},{"params":{"a":12},"expected":"144"}]}"#;

const TEMPLATE_RULES: &str = r#"
DETERMINISTIC TEMPLATE RULES:
- params must have named domains, never {}. Use short lowercase single-letter parameter names a,b,c,f,g,h,m,p,q,s. Avoid descriptive names like base, count, dividend, exp, and reserved e/i/pi. For expression answers, k,n,r,t,theta,u,v,w,x,y,z are free unknowns and must NOT be parameter names. Every {name} placeholder and constraint variable must be declared in params.
- answer_expr is a STRING in a restricted mathematical grammar, not Python. Use +, -, *, /, parentheses, and powers with a literal whole-number exponent (a**2, not a**b). Available evaluated functions with exact arity: abs(a), sqrt(a), gcd(a,b), lcm(a,b), floor(a), ceiling(a), min(a,b), max(a,b), factorial(a), binomial(a,b), signcase(selector,[negative,zero,positive]), equalitylabel(left,right), divisibilitylabel(number,divisor), linearclass((a,b),(c,d)), primeclass(number), quotientremainder(quotient,remainder), powerform(coefficient,[base,exponent]), logequation(base,[exponent,result]), expequation(base,[exponent,result]), atandeg(ratio), factorlist(number), firstmultiples(number,count), primefactors(number), and repeatedfactors(number,count). Use floor(a/b) for integer quotient and a-b*floor(a/b) for remainder. No //, %, str(), round(), ** with a variable exponent, Python conditionals, comparisons, assignment, indexing, strings, or units in answer_expr. The answer must fit the requested numeric/expression kind; never concatenate quotient/remainder prose.
- When exemplars do not share a reviewed policy, answer_contract may state the deterministic policy for this pending template. A label uses one text choice parameter, equalitylabel(left,right), divisibilitylabel(number,divisor), linearclass((a,b),(c,d)), primeclass(number), logequation(base,[exponent,result]), or expequation(base,[exponent,result]) as answer_expr; the equation writers prove the integer power identity and require the label vocabulary to enumerate the result. A quotient-and-remainder contract uses quotientremainder(quotient,remainder). An exact contract may preserve an unevaluated power with powerform(coefficient,[base,exponent]). An approximate contract may use atandeg(ratio) for a certified inverse-tangent angle in degrees. An ordered exact-list contract uses factorlist(number), firstmultiples(number,count), primefactors(number), or repeatedfactors(number,count). A flat multipart answer uses multipart(part1, part2), with arguments in contract part order; each argument is math or a text choice for a label part. For a unit contract, keep answer_expr numeric; the server attaches the declared unit. For an inequality-union contract, use excludepoint(variable,bound), lowerbound(variable,bound), or upperbound(variable,bound), with a one-value text choice for variable. The server validates it and never approves the row.
- statement, solution_sketch, each hint, sample.expected and answer_expr are STRINGS. params and each sample.params are objects. constraints, hints, distractors and samples are arrays. Never turn answer_expr into an object.
- Text placeholders are ONLY {a}, {b}, etc. Expressions such as {a-b} or {b**c} are invalid placeholders. Write substituted operands explicitly, e.g. ${a} - {b}$, and put calculations in answer_expr. Double every literal LaTeX brace: ${a}^{{2}}$, not ${a}^{2}$. Avoid numeric grouping braces such as {,}.
- Samples bind EVERY declared parameter and satisfy EVERY constraint. Include each parameter's declared minimum and maximum in valid samples; for every two varying parameters include a crossed corner (one low while the other is high), whenever constraints admit it. Matching all-low/all-high samples alone miss swapped operands. Compute sample.expected exactly from the requested mathematics. Choose small domains with many satisfying tuples; do not rely on finding extremely rare tuples in huge Cartesian products.
- Use distractors: [] unless you can prove the expression is wrong for EVERY satisfying tuple. a-b collides with a+b when b=0; zero/zero is especially unsafe. Hints ask a method question and must not state an instantiated answer. The solution sketch explains the method using declared placeholders.
"#;
