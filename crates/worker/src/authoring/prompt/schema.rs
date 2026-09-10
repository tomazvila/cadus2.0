//! The tool schemas of the four kinds, `additionalProperties: false`.

use cadus_core::template::{MAX_CHOICES, MAX_DECIMAL_SCALE};
use cadus_model_client::ToolSpec;
use serde_json::{Value, json};

use super::{CONSTRAINT_OPS, Kind};
use crate::diagnosis::MODEL_ERROR_TAGS;

/// The `{"lit": n}` and nested-term grammar, stated in prose.
///
/// A term is recursive, and a recursive `$ref` is not read the same way by every
/// OpenAI-compatible endpoint, so the schema states the shape and the gate
/// decides it. The gate is the authority on every field here in any case: a
/// schema the provider honors still admits a constraint the core refuses.
const TERM_DESCRIPTION: &str = "A term: the parameter name as a bare string, {\"lit\": n} for an exact literal, or one of \
{\"add\": [term, ...]}, {\"sub\": [term, term]}, {\"mul\": [term, ...]}, {\"abs\": term}, \
{\"mod\": [term, term]}, {\"digit_sum\": term}.";

/// The distractor schema. The template tool and the distractor tool share every
/// field of it, and they ask for a different `answer`.
///
/// A template distractor is an EXPRESSION over the declared parameters, and the
/// gate evaluates it on each worked sample. A `diagnosis` document declares no
/// parameters — the knowledge point serves authored exemplars — so its
/// distractor answer is the wrong answer itself. One description for both kinds
/// spends attempts on documents the gate refuses (unit R7).
fn distractor_items(kind: Kind) -> Value {
    let answer = match kind {
        Kind::Diagnosis => {
            "The wrong answer itself, written the way a learner writes it. It is a literal \
answer, not a formula: this knowledge point declares no parameters."
        }
        Kind::Template | Kind::Teach | Kind::HintLadder => {
            "The wrong answer, as an expression over the declared parameters, so the server \
computes it per instance."
        }
    };
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["answer", "error_tag", "note"],
        "properties": {
            "answer": {
                "type": "string",
                "description": answer,
            },
            "error_tag": {"type": "string", "enum": MODEL_ERROR_TAGS},
            "note": {
                "type": "string",
                "description":
                    "1-2 sentences naming the misconception. It never states the correct answer.",
            },
        },
    })
}

/// The inclusive whole-number range of a rational domain's numerator and of its
/// denominator (`cadus_core::template::IntRange`).
fn int_range() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["low", "high"],
        "properties": {"low": {"type": "integer"}, "high": {"type": "integer"}},
    })
}

/// The parameter-domain schema: the four kinds of `cadus_core::template::Domain`.
fn params_schema() -> Value {
    json!({
        "type": "object",
        "description":
            "The placeholders and the values each one takes, by parameter name.",
        "additionalProperties": {
            "oneOf": [
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kind", "low", "high"],
                    "properties": {
                        "kind": {"const": "int"},
                        "low": {"type": "integer"},
                        "high": {"type": "integer"},
                    },
                },
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kind", "values"],
                    "properties": {
                        "kind": {"const": "choice"},
                        "values": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": MAX_CHOICES,
                            "items": {"type": ["string", "integer"]},
                        },
                    },
                },
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kind", "num", "den"],
                    "properties": {
                        "kind": {"const": "rational"},
                        "num": int_range(),
                        "den": int_range(),
                    },
                },
                {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["kind", "low", "high", "scale"],
                    "properties": {
                        "kind": {"const": "decimal"},
                        "low": {"type": "integer"},
                        "high": {"type": "integer"},
                        "scale": {"type": "integer", "minimum": 0, "maximum": MAX_DECIMAL_SCALE},
                    },
                },
            ],
        },
    })
}

/// The arguments schema of the template tool.
fn template_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "statement", "params", "constraints", "answer_expr",
            "solution_sketch", "hints", "distractors", "samples",
        ],
        "properties": {
            "answer_contract": {
                "type": "object",
                "description":
                    "A deterministic answer policy when the exemplars do not already share one. \
    The server validates it and the document remains pending for independent AI review. Use kind label with \
    explicit options for a closed choice, or kind multipart with ordered named parts for a flat \
    structured answer. A reviewed shared exemplar policy overrides this field.",
            },
            "statement": {
                "type": "string",
                "description":
                    "The problem statement with {name} placeholders for the values that vary, as \
    terminal-free prose with all math in $...$ KaTeX LaTeX. EVERY literal brace is doubled: write \
    $7^{{2}}$, because a single brace is a placeholder.",
            },
            "params": params_schema(),
            "constraints": {
                "type": "array",
                "description":
                    "The relations between parameters. Empty when the domains alone are right.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["op", "left", "right"],
                    "properties": {
                        "op": {"type": "string", "enum": CONSTRAINT_OPS},
                        "left": {"description": TERM_DESCRIPTION},
                        "right": {"description": TERM_DESCRIPTION},
                    },
                },
            },
            "answer_expr": {
                "type": "string",
                "description":
                    "The answer as an exact expression over the parameter names. The SERVER \
    computes every instance's answer from it, so it is exact over the whole domain. For a label \
    contract, use one text-valued choice parameter, equalitylabel(left, right), divisibilitylabel(number, divisor), \
    linearclass((a, b), (c, d)) for the solution-count class of ax+b=cx+d, or primeclass(number). \
    trianglelaw(given) selects the starting law from three distinct measurements a,b,c,A,B,C and \
    refuses AAA. For a quotient-and-remainder contract, use \
    quotientremainder(quotient, remainder). To preserve an unevaluated exact power, use \
    powerform(coefficient, [base, exponent]). Under a closed label contract, use \
    logequation(base, [exponent, result]) or expequation(base, [exponent, result]) for a proved \
    integer power identity. Under an approximate contract, use atandeg(ratio) for an \
    inverse-tangent angle in degrees. Use compounding(count) for the exact factor (1+1/n)^n \
    with a whole count from 1 through 64. \
    quarterextremum(curve, [lo, hi, direction]) returns the leftmost parent sine/cosine extremum \
    as coordinates for whole quarter turns 0 <= lo < hi <= 4. For an ordered exact-list contract, use \
    factorlist(number), firstmultiples(number, count), primefactors(number), or \
    repeatedfactors(number, count). For a multipart contract, use \
    multipart(part1, part2) with arguments in contract part order; each argument is a mathematical \
    expression or, for a label part, a supported label expression. For a unit contract, keep \
    this expression numeric; the server attaches the declared unit. For an inequality-union \
    contract, use excludepoint(variable, bound), lowerbound(variable, bound), or \
    upperbound(variable, bound), with a one-value text choice for variable. For a bidirectional \
    interval/inequality conversion, use convertnotation(source) with a \
    required_inequality_notation contract; the computed expected answer fixes the required \
    output notation for that instance. For one unevaluated numeric power, use \
    powerform(1,[base,exponent]) with a required_single_power contract. A \
    required_normalized_scientific_notation contract writes an exact terminating numeric \
    answer as a standard coefficient times 10 to an integer power. Use \
    signcase(selector, [negative, zero, positive]) for a bounded sign split.",
            },
            "solution_sketch": {
                "type": "string",
                "description":
                    "A 1-3 line method outline with the same {name} placeholders; math in $...$ \
    and literal braces doubled, exactly as in 'statement'.",
            },
            "hints": {
                "type": "array",
                "minItems": 1,
                "description":
                    "The hint ladder, widest rung first. No rung reveals the answer or the last \
    step.",
                "items": {"type": "string"},
            },
            "distractors": {
                "type": "array",
                "description":
                    "The wrong answers this knowledge point produces, with the mistake each one \
    names. Empty when you can name none.",
                "items": distractor_items(Kind::Template),
            },
            "samples": {
                "type": "array",
                "minItems": 1,
                "description":
                    "Worked instances that VERIFY your expression. For each, bind every parameter \
    and state the answer YOU compute by hand. The server evaluates answer_expr on the same bindings \
    and rejects the whole template if any one disagrees. COVERAGE IS CHECKED: the lowest AND the \
    highest value of every int parameter, every value of every choice parameter, and for every PAIR \
    of int parameters one crossed corner.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["params", "expected"],
                    "properties": {
                        "params": {
                            "type": "object",
                            "description": "A value for every declared parameter.",
                        },
                        "expected": {
                            "type": "string",
                            "description": "The answer for those values, computed by you.",
                        },
                    },
                },
            },
        },
    })
}

/// The arguments schema of one kind's tool, `additionalProperties: false`.
#[must_use]
pub fn tool_schema(kind: Kind) -> Value {
    match kind {
        Kind::Template => template_schema(),
        Kind::Teach => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["concept", "worked_example"],
            "properties": {
                "concept": {
                    "type": "string",
                    "description":
                        "1-2 sentences stating the method or the rule of this knowledge point \
        explicitly. Math in $...$ LaTeX.",
                },
                "worked_example": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["problem", "steps"],
                    "properties": {
                        "problem": {
                            "type": "string",
                            "description":
                                "A concrete example problem of this knowledge point's shape, with \
        DIFFERENT values from the exemplars.",
                        },
                        "steps": {
                            "type": "array",
                            "minItems": 1,
                            "description":
                                "The full solution, one step per entry, ending with the answer.",
                            "items": {"type": "string"},
                        },
                    },
                },
            },
        }),
        Kind::HintLadder => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["hints"],
            "properties": {
                "hints": {
                    "type": "array",
                    "minItems": 1,
                    "description":
                        "The rungs, widest first. No rung reveals the answer or the last step.",
                    "items": {"type": "string"},
                },
            },
        }),
        Kind::Diagnosis => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["distractors"],
            "properties": {
                "distractors": {
                    "type": "array",
                    "minItems": 1,
                    "items": distractor_items(Kind::Diagnosis),
                },
            },
        }),
    }
}

/// What the tool is for, in the model's words.
const fn tool_description(kind: Kind) -> &'static str {
    match kind {
        Kind::Template => {
            "Return the reusable STRUCTURE of this knowledge point's problems: the statement with \
placeholders, the values they range over, the relations between them, the answer as an \
expression, the hint ladder, the distractors, and the worked samples that verify it."
        }
        Kind::Teach => "Return the concept and one fully worked example for a knowledge point.",
        Kind::HintLadder => {
            "Return the ordered hint ladder of a knowledge point, widest rung first."
        }
        Kind::Diagnosis => {
            "Return the wrong answers a knowledge point produces, with the mistake each one names."
        }
    }
}

/// The forced tool of one kind.
#[must_use]
pub fn tool_spec(kind: Kind) -> ToolSpec {
    ToolSpec {
        name: kind.tool_name().to_owned(),
        description: tool_description(kind).to_owned(),
        parameters: tool_schema(kind),
    }
}
