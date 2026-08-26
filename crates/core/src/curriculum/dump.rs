//! The canonical curriculum dump and the curriculum hash (D1, R5, C5).
//!
//! The authority is `docs/reference/curriculum-1.0-spec.md` section 8, and the
//! oracle is `scripts/oracle/dump_curriculum_1_0.py`. The two dumps must be
//! byte-identical, so every rule below is a parity rule:
//!
//! - One line of JSON, compact separators `,` and `:`, no trailing newline.
//!   [`canonical_dump`] returns the line; the binary adds the newline.
//! - Non-ASCII text stays unescaped (the oracle sets `ensure_ascii=False`).
//! - Every object key is sorted (the oracle sets `sort_keys=True`). This module
//!   inserts each key in sorted order, so the output is sorted whatever map type
//!   `serde_json` is built with.
//! - Floats take the text of the Python `repr` of a float (parity trap 16).
//!   [`python_repr_f64`] writes it. `serde_json` writes a different text for the
//!   same value — it keeps fixed notation below `1e-4` and it writes an
//!   unpadded exponent — so [`canonical_dump`] renders the JSON itself with
//!   [`render`] and never calls `serde_json::to_string` on a number.
//! - `topics` is sorted by id. `prereq_edges` and `encompassing_edges` are
//!   sorted the way Python sorts lists: element by element, left to right.
//!
//! The dump is a semantic view, not a file view: it reads the arena, so it
//! carries the 1.0 graph rules with it — a dangling prerequisite is absent from
//! `prereq_edges`, and a weight-0 edge is absent from `encompassing_edges`
//! (parity traps 7 and 8).

use std::cmp::Ordering;

use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};

use super::arena::{Curriculum, TopicIdx};
use super::model::{AnkiSeed, Exemplar, KnowledgePoint, PrereqEdge, Topic};

/// The `schema` field of the dump (spec section 8).
pub const DUMP_SCHEMA: &str = "cadus-curriculum-dump/1";

/// The canonical dump of a curriculum: one line of JSON, no trailing newline.
///
/// The bytes of this string are the input of [`curriculum_hash`].
pub fn canonical_dump(c: &Curriculum) -> String {
    let prereq_edges = prereq_edges(c);
    let encompassing_edges = encompassing_edges(c);

    let mut root = Map::new();
    root.insert(
        "counts".to_owned(),
        counts(c, prereq_edges.len(), encompassing_edges.len()),
    );
    root.insert("courses".to_owned(), courses(c));
    root.insert("cycle".to_owned(), cycle(c));
    root.insert(
        "encompassing_edges".to_owned(),
        Value::Array(
            encompassing_edges
                .iter()
                .map(EncEdgeRow::to_value)
                .collect(),
        ),
    );
    root.insert(
        "prereq_edges".to_owned(),
        Value::Array(prereq_edges.iter().map(PrereqRow::to_value).collect()),
    );
    root.insert("schema".to_owned(), text(DUMP_SCHEMA));
    root.insert("topics".to_owned(), topics(c));
    root.insert("topo_order".to_owned(), topo_order(c));

    let mut out = String::new();
    render(&Value::Object(root), &mut out);
    out
}

// --------------------------------------------------------------------------- //
// The JSON text
// --------------------------------------------------------------------------- //

/// Append the canonical JSON text of one value.
///
/// The rules are the `json.dumps` arguments of the oracle: the separators are
/// `,` and `:`, and non-ASCII text stays unescaped. `serde_json` escapes a
/// string the same way `json.dumps(ensure_ascii=False)` does, so a string goes
/// through `serde_json`. A float does not: [`python_repr_f64`] writes it.
fn render(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => render_number(number, out),
        // A `&str` always serializes, so the fallback is unreachable. An empty
        // string fails the parity test loudly rather than passes silently.
        Value::String(item) => out.push_str(&json_string(item.as_str())),
        Value::Array(items) => {
            out.push('[');
            for (position, item) in items.iter().enumerate() {
                if position > 0 {
                    out.push(',');
                }
                render(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (position, (key, item)) in map.iter().enumerate() {
                if position > 0 {
                    out.push(',');
                }
                out.push_str(&json_string(key.as_str()));
                out.push(':');
                render(item, out);
            }
            out.push('}');
        }
    }
}

/// Append the canonical JSON text of one number.
///
/// A count is an integer and takes the plain decimal text. A `difficulty` or a
/// `weight` is a float and takes the Python `repr` text (parity trap 16).
fn render_number(number: &Number, out: &mut String) {
    if number.is_f64() {
        match number.as_f64() {
            Some(value) => out.push_str(&python_repr_f64(value)),
            None => out.push_str("null"),
        }
        return;
    }
    out.push_str(&number.to_string());
}

/// One JSON string, escaped the way `json.dumps(ensure_ascii=False)` escapes it.
fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// The text Python `repr` writes for a float (parity trap 16).
///
/// The digits are the shortest run that round-trips, which is what Rust `{:e}`
/// writes and what Python picks. The two formatters differ on one point: an
/// exact tie between the two shortest candidates. Rust rounds such a tie away
/// from zero; CPython `repr` (David Gay, mode 0) rounds it to the even last
/// digit. [`even_last_digit_on_a_tie`] puts the CPython rule back.
///
/// The notation follows the Python rule: fixed notation while the exponent is
/// at or above -4 and below 16, and `d.ddde±XX` with a signed exponent of at
/// least two digits outside that range. An integral value in fixed notation
/// keeps a `.0` tail, so `1` reads `1.0`. Scientific notation keeps no `.0`
/// tail, so `1e-05` has one digit.
///
/// `NaN` and the infinities give `nan`, `inf`, and `-inf`, the Python `repr`
/// text. They have no JSON form, so [`float`] maps them to `null` before the
/// dump ever reaches this function.
pub fn python_repr_f64(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-inf".to_owned()
        } else {
            "inf".to_owned()
        };
    }

    // Rust writes `-d.ddde<exp>` with the shortest digits that round-trip.
    let shortest = format!("{value:e}");
    let Some((mantissa, exponent_text)) = shortest.split_once('e') else {
        return shortest;
    };
    let Ok(exponent) = exponent_text.parse::<i32>() else {
        return shortest;
    };
    let (sign, unsigned) = match mantissa.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", mantissa),
    };
    let mut digits: String = unsigned.chars().filter(|item| *item != '.').collect();
    even_last_digit_on_a_tie(value, &mut digits, exponent);

    if (-4..16).contains(&exponent) {
        return fixed_notation(sign, &digits, exponent);
    }
    let separator = if exponent < 0 { '-' } else { '+' };
    let magnitude = exponent.unsigned_abs();
    let head = digits.get(..1).unwrap_or("0");
    let tail = digits.get(1..).unwrap_or("");
    let point = if tail.is_empty() { "" } else { "." };
    format!("{sign}{head}{point}{tail}e{separator}{magnitude:02}")
}

/// Apply the CPython tie rule to the shortest digit run of `value` (finding #1).
///
/// `digits` holds the significant digits with no point and no sign, and
/// `exponent` is the decimal exponent of the first digit, so the run stands for
/// the integer `N` scaled by `10^k` with `k = exponent - (digits.len() - 1)`.
///
/// Rust and CPython both take the shortest run and, inside that length, the
/// candidate closest to `value`. They differ only when `value` sits exactly
/// halfway between two candidates: Rust takes the one away from zero, which is
/// `N`, and CPython takes the one with the even last digit. `N` and `N - 1`
/// have last digits of opposite parity, so the rule fires only while the last
/// digit of `N` is odd, and then the answer is `N - 1`.
///
/// A tie needs two tests, and both are necessary.
///
/// 1. The halfway point of `N - 1` and `N` is the decimal `W * 10^(k - 1)` with
///    `W = 10 * N - 5`, whose digit run is `N - 1` followed by a `5`. A last
///    digit of `N` that is odd never borrows, so that run has the length of
///    `digits` plus one. [`is_exact_decimal`] tests `value == W * 10^(k - 1)`
///    exactly, in integer arithmetic.
/// 2. `N - 1` scaled by `10^k` parses back to `value`. A value on a binade
///    boundary has a rounding interval twice as wide above as below, so the
///    lower candidate can miss the value even while the halfway point is exact.
///    `2^-24` is such a value: it is exactly `5.9604644775390625e-08`, yet
///    `5.960464477539062e-08` parses to the float below it, and CPython writes
///    `5.960464477539063e-08`.
fn even_last_digit_on_a_tie(value: f64, digits: &mut String, exponent: i32) {
    let Some(last) = digits.chars().next_back() else {
        return;
    };
    let Some(units) = last.to_digit(10) else {
        return;
    };
    if units % 2 == 0 {
        return;
    }
    let length = i32::try_from(digits.len()).unwrap_or(0);
    let Some(k) = exponent.checked_sub(length.saturating_sub(1)) else {
        return;
    };
    let Some(j) = k.checked_sub(1) else {
        return;
    };

    // The midpoint digit run: the last digit down one, then a `5`.
    let head = digits.get(..digits.len().saturating_sub(1)).unwrap_or("");
    let mut midpoint = String::with_capacity(digits.len() + 1);
    midpoint.push_str(head);
    midpoint.push(char::from_digit(units - 1, 10).unwrap_or('0'));
    midpoint.push('5');
    let Ok(w) = midpoint.parse::<u128>() else {
        return;
    };
    if !is_exact_decimal(value, w, j) {
        return;
    }

    // Build the even neighbor. An odd last digit never borrows.
    let mut lower = String::with_capacity(digits.len());
    lower.push_str(head);
    lower.push(char::from_digit(units - 1, 10).unwrap_or('0'));

    // Test 2: the neighbor parses back to the same float. If it does not, keep
    // the digits of Rust.
    if format!("{lower}e{k}").parse::<f64>() != Ok(value.abs()) {
        return;
    }
    *digits = lower;
}

/// Test `|value| == w * 10^exponent` in exact arithmetic.
///
/// A finite `f64` is exactly `m * 2^p` with an integer `m`. Strip the factors of
/// two from `m` to get an odd `m_odd` and a corrected `e`, so
/// `|value| = m_odd * 2^e`. Because `m_odd` is odd, the equality can hold only
/// while `e` equals `exponent`, and it then reduces to a comparison of the odd
/// parts: `m_odd * 5^-exponent == w` below zero, and `m_odd == w * 5^exponent`
/// at or above zero. An overflow of the power of five means the two sides
/// cannot be equal, so the test gives `false`.
fn is_exact_decimal(value: f64, w: u128, exponent: i32) -> bool {
    let bits = value.abs().to_bits();
    let biased = (bits >> 52) & 0x7ff;
    let fraction = bits & 0x000f_ffff_ffff_ffff;
    let (mantissa, mut power) = if biased == 0 {
        (fraction, -1074i32)
    } else {
        // A normal number carries the hidden bit and a bias of 1023, less the
        // 52 fraction bits.
        (fraction | (1u64 << 52), (biased as i32) - 1075)
    };
    if mantissa == 0 {
        return false;
    }
    let shift = mantissa.trailing_zeros();
    let odd = u128::from(mantissa >> shift);
    power = match i32::try_from(shift)
        .ok()
        .and_then(|bits| power.checked_add(bits))
    {
        Some(sum) => sum,
        None => return false,
    };
    if power != exponent {
        return false;
    }
    if exponent < 0 {
        let Some(scale) = checked_power_of_five(exponent.unsigned_abs()) else {
            return false;
        };
        return odd.checked_mul(scale) == Some(w);
    }
    let Some(scale) = checked_power_of_five(exponent.unsigned_abs()) else {
        return false;
    };
    w.checked_mul(scale) == Some(odd)
}

/// `5^power` as a `u128`, or `None` when it does not fit.
fn checked_power_of_five(power: u32) -> Option<u128> {
    let mut total: u128 = 1;
    for _ in 0..power {
        total = total.checked_mul(5)?;
    }
    Some(total)
}

/// The fixed-notation form of `sign`, `digits`, and a decimal `exponent`.
///
/// `digits` holds the significant digits with no point and no sign. The point
/// goes after `exponent + 1` digits. Python pads with zeros on either side and
/// writes a `.0` tail when no digit falls after the point.
fn fixed_notation(sign: &str, digits: &str, exponent: i32) -> String {
    let point = exponent.saturating_add(1);
    if point <= 0 {
        let zeros = "0".repeat(usize::try_from(-point).unwrap_or(0));
        return format!("{sign}0.{zeros}{digits}");
    }
    let Ok(point) = usize::try_from(point) else {
        return format!("{sign}{digits}.0");
    };
    if point >= digits.len() {
        let zeros = "0".repeat(point - digits.len());
        return format!("{sign}{digits}{zeros}.0");
    }
    let head = digits.get(..point).unwrap_or(digits);
    let tail = digits.get(point..).unwrap_or("");
    format!("{sign}{head}.{tail}")
}

/// The curriculum hash: the lowercase hex SHA-256 of the [`canonical_dump`]
/// bytes (spec section 3).
pub fn curriculum_hash(c: &Curriculum) -> String {
    sha256_hex(canonical_dump(c).as_bytes())
}

/// The lowercase hex SHA-256 of a byte string.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        // Two lowercase hex digits per byte, high nibble first.
        out.push(hex_digit(byte >> 4));
        out.push(hex_digit(byte & 0x0f));
    }
    out
}

/// One lowercase hex digit of a nibble. A value above 15 cannot occur.
fn hex_digit(nibble: u8) -> char {
    const DIGITS: [char; 16] = [
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
    ];
    DIGITS.get(nibble as usize).copied().unwrap_or('0')
}

// --------------------------------------------------------------------------- //
// Sections
// --------------------------------------------------------------------------- //

/// `counts`: the eight totals of spec section 3.
fn counts(c: &Curriculum, prereq_edges: usize, encompassing_edges: usize) -> Value {
    let mut knowledge_points = 0usize;
    let mut exemplars = 0usize;
    let mut anki_seeds = 0usize;
    for topic in c.topics() {
        knowledge_points += topic.knowledge_points.len();
        anki_seeds += topic.anki_seeds.len();
        for kp in &topic.knowledge_points {
            exemplars += kp.exemplars.len();
        }
    }

    let mut map = Map::new();
    map.insert("anki_seeds".to_owned(), count(anki_seeds));
    map.insert("courses".to_owned(), count(c.courses().len()));
    map.insert("encompassing_edges".to_owned(), count(encompassing_edges));
    map.insert("exemplars".to_owned(), count(exemplars));
    map.insert("knowledge_points".to_owned(), count(knowledge_points));
    map.insert("prereq_edges".to_owned(), count(prereq_edges));
    map.insert("topics".to_owned(), count(c.topic_count()));
    map.insert("units".to_owned(), count(c.unit_count()));
    Value::Object(map)
}

/// `courses`: the catalog in file order, not in `order` order (parity trap 1).
/// `mastery_floor` is the authored list, with nothing dropped.
fn courses(c: &Curriculum) -> Value {
    let rows = c
        .courses()
        .iter()
        .map(|course| {
            let mut map = Map::new();
            map.insert("id".to_owned(), text(course.id.as_str()));
            map.insert(
                "mastery_floor".to_owned(),
                Value::Array(
                    course
                        .mastery_floor
                        .iter()
                        .map(|id| text(id.as_str()))
                        .collect(),
                ),
            );
            map.insert(
                "mastery_floor_course".to_owned(),
                course
                    .mastery_floor_course
                    .as_ref()
                    .map_or(Value::Null, |id| text(id.as_str())),
            );
            map.insert("name".to_owned(), text(&course.name));
            map.insert(
                "order".to_owned(),
                Value::Number(Number::from(course.order)),
            );
            Value::Object(map)
        })
        .collect();
    Value::Array(rows)
}

/// `cycle`: the topic ids of one prerequisite cycle, or `null` for a DAG.
fn cycle(c: &Curriculum) -> Value {
    match c.find_cycle() {
        None => Value::Null,
        Some(nodes) => Value::Array(nodes.iter().map(|idx| text(c.id_of(*idx))).collect()),
    }
}

/// `topics`: every topic, sorted by id.
fn topics(c: &Curriculum) -> Value {
    let mut order: Vec<TopicIdx> = (0..c.topic_count())
        .filter_map(|position| u32::try_from(position).ok())
        .map(TopicIdx::from_u32)
        .collect();
    // Topic ids are unique, so the id alone is a total order. Rust compares a
    // `str` byte by byte, which is the code-point order Python sorts by.
    order.sort_by(|a, b| c.id_of(*a).cmp(c.id_of(*b)));

    let rows = order
        .iter()
        .filter_map(|idx| c.topic(*idx).map(|topic| topic_value(c, *idx, topic)))
        .collect();
    Value::Array(rows)
}

/// One entry of `topics`.
fn topic_value(c: &Curriculum, idx: TopicIdx, topic: &Topic) -> Value {
    let mut map = Map::new();
    map.insert(
        "anki_seeds".to_owned(),
        Value::Array(topic.anki_seeds.iter().map(anki_seed_value).collect()),
    );
    map.insert("answer_kind".to_owned(), text(topic.answer_kind.as_str()));
    map.insert("core".to_owned(), Value::Bool(topic.core));
    map.insert("course".to_owned(), text(c.course_of(idx)));
    map.insert(
        "diagnostic_exemplar".to_owned(),
        topic
            .diagnostic_exemplar
            .as_ref()
            .map_or(Value::Null, exemplar_value),
    );
    map.insert("difficulty".to_owned(), float(topic.difficulty));
    map.insert("drill".to_owned(), Value::Bool(topic.drill));
    map.insert(
        "encompassings_extra".to_owned(),
        Value::Array(topic.encompassings_extra.iter().map(edge_value).collect()),
    );
    map.insert(
        "expected_time_secs".to_owned(),
        Value::Number(Number::from(topic.expected_time_secs)),
    );
    map.insert("id".to_owned(), text(topic.id.as_str()));
    map.insert(
        "knowledge_points".to_owned(),
        Value::Array(topic.knowledge_points.iter().map(kp_value).collect()),
    );
    map.insert("load_index".to_owned(), count(c.load_index(idx)));
    map.insert("module".to_owned(), text(c.module_of(idx)));
    map.insert("name".to_owned(), text(&topic.name));
    map.insert(
        "prerequisites".to_owned(),
        Value::Array(topic.prerequisites.iter().map(edge_value).collect()),
    );
    map.insert("unit".to_owned(), text(c.unit_of(idx)));
    Value::Object(map)
}

/// One prerequisite or `encompassings_extra` edge, as authored.
fn edge_value(edge: &PrereqEdge) -> Value {
    let mut map = Map::new();
    map.insert("id".to_owned(), text(edge.id.as_str()));
    map.insert("key".to_owned(), Value::Bool(edge.key));
    map.insert("weight".to_owned(), float(edge.weight));
    Value::Object(map)
}

/// One knowledge point.
fn kp_value(kp: &KnowledgePoint) -> Value {
    let mut map = Map::new();
    map.insert(
        "constraints".to_owned(),
        kp.constraints
            .as_ref()
            .map_or(Value::Null, |body| text(body)),
    );
    map.insert(
        "exemplars".to_owned(),
        Value::Array(kp.exemplars.iter().map(exemplar_value).collect()),
    );
    map.insert("id".to_owned(), text(kp.id.as_str()));
    map.insert(
        "key_prerequisites".to_owned(),
        Value::Array(
            kp.key_prerequisites
                .iter()
                .map(|id| text(id.as_str()))
                .collect(),
        ),
    );
    map.insert("name".to_owned(), text(&kp.name));
    Value::Object(map)
}

/// One exemplar.
fn exemplar_value(exemplar: &Exemplar) -> Value {
    let mut map = Map::new();
    map.insert("answer".to_owned(), text(&exemplar.answer));
    map.insert("problem".to_owned(), text(&exemplar.problem));
    map.insert(
        "solution_sketch".to_owned(),
        exemplar
            .solution_sketch
            .as_ref()
            .map_or(Value::Null, |body| text(body)),
    );
    Value::Object(map)
}

/// One Anki seed. The 1.0 wire name of the kind is `type`.
fn anki_seed_value(seed: &AnkiSeed) -> Value {
    let mut map = Map::new();
    map.insert("back".to_owned(), text(&seed.back));
    map.insert("front".to_owned(), text(&seed.front));
    map.insert("type".to_owned(), text(seed.kind.as_str()));
    Value::Object(map)
}

/// `topo_order`: the precomputed topological order, as topic ids.
fn topo_order(c: &Curriculum) -> Value {
    Value::Array(
        c.topo_order()
            .iter()
            .map(|idx| text(c.id_of(*idx)))
            .collect(),
    )
}

// --------------------------------------------------------------------------- //
// Edge rows
// --------------------------------------------------------------------------- //

/// One row of `prereq_edges`: `[child, parent, weight, key]`.
struct PrereqRow<'a> {
    child: &'a str,
    parent: &'a str,
    weight: f64,
    key: bool,
}

impl PrereqRow<'_> {
    fn to_value(&self) -> Value {
        Value::Array(vec![
            text(self.child),
            text(self.parent),
            float(self.weight),
            Value::Bool(self.key),
        ])
    }
}

/// One row of `encompassing_edges`: `[src, dst, weight]`.
struct EncEdgeRow<'a> {
    src: &'a str,
    dst: &'a str,
    weight: f64,
}

impl EncEdgeRow<'_> {
    fn to_value(&self) -> Value {
        Value::Array(vec![text(self.src), text(self.dst), float(self.weight)])
    }
}

/// `prereq_edges`: one row per authored prerequisite whose target is a topic
/// (parity trap 7), sorted the way Python sorts a list of lists.
fn prereq_edges(c: &Curriculum) -> Vec<PrereqRow<'_>> {
    let mut rows: Vec<PrereqRow<'_>> = Vec::with_capacity(c.prereq_edge_count());
    for (position, topic) in c.topics().iter().enumerate() {
        let Ok(raw) = u32::try_from(position) else {
            continue;
        };
        let child = c.id_of(TopicIdx::from_u32(raw));
        for edge in &topic.prerequisites {
            let Some(parent) = c.idx_of(edge.id.as_str()) else {
                continue;
            };
            rows.push(PrereqRow {
                child,
                parent: c.id_of(parent),
                weight: edge.weight,
                key: edge.key,
            });
        }
    }
    // Python compares element by element and stops at the first difference:
    // string, string, float, then bool with `False` below `True`.
    rows.sort_by(|a, b| {
        a.child
            .cmp(b.child)
            .then_with(|| a.parent.cmp(b.parent))
            .then_with(|| compare_float(a.weight, b.weight))
            .then_with(|| a.key.cmp(&b.key))
    });
    rows
}

/// `encompassing_edges`: the forward encompassing map, sorted the same way.
///
/// The walk covers every node, so a phantom target keeps its string id. A
/// phantom node declares no edge of its own, the same as 1.0, where `_enc` holds
/// one key per topic only.
fn encompassing_edges(c: &Curriculum) -> Vec<EncEdgeRow<'_>> {
    let mut rows: Vec<EncEdgeRow<'_>> = Vec::with_capacity(c.enc_forward_count());
    for node in 0..c.enc_node_count() {
        let Ok(raw) = u32::try_from(node) else {
            continue;
        };
        let src_node = super::arena::EncNode::from_u32(raw);
        let src = c.enc_node_id(src_node);
        for link in c.enc_forward(src_node) {
            rows.push(EncEdgeRow {
                src,
                dst: c.enc_node_id(link.target),
                weight: link.weight,
            });
        }
    }
    rows.sort_by(|a, b| {
        a.src
            .cmp(b.src)
            .then_with(|| a.dst.cmp(b.dst))
            .then_with(|| compare_float(a.weight, b.weight))
    });
    rows
}

// --------------------------------------------------------------------------- //
// Scalars
// --------------------------------------------------------------------------- //

/// A JSON string.
fn text(value: &str) -> Value {
    Value::String(value.to_owned())
}

/// A count as a JSON number.
fn count(value: usize) -> Value {
    u64::try_from(value).map_or(Value::Null, |n| Value::Number(Number::from(n)))
}

/// A float as a JSON number. [`render_number`] gives it the Python `repr` text.
///
/// `NaN` and the infinities have no JSON form. Python writes the non-standard
/// `NaN` and `Infinity` there; this dump writes `null`. The loader rejects both
/// forms, so no curriculum value reaches this point: `difficulty` and `weight`
/// both hold a finite number in 0..=1.
fn float(value: f64) -> Value {
    Number::from_f64(value).map_or(Value::Null, Value::Number)
}

/// Order two weights the way Python orders two floats (finding #2).
///
/// Python compares with `<` and `>`, where `-0.0 == 0.0`, so a pair of zeros
/// that differ only in sign is equal and the comparison falls through to the
/// next element of the row. `f64::total_cmp` puts `-0.0` below `0.0` and swaps
/// such a pair, so this function uses `partial_cmp` instead.
///
/// `partial_cmp` gives `None` for a `NaN` only. The loader rejects `NaN` in a
/// `weight`, so no dump reaches that arm; `Equal` there keeps the fall-through
/// rule and keeps the sort total.
fn compare_float(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}
