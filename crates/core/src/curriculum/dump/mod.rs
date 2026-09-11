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

mod float;

use std::cmp::Ordering;

use serde_json::{Map, Number, Value};

pub(crate) use float::render as render_json;
pub use float::{python_repr_f64, sha256_hex};

use super::arena::{Curriculum, EncNode, TopicIdx};
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
    float::render(&Value::Object(root), &mut out);
    out
}

/// The curriculum hash: the lowercase hex SHA-256 of the [`canonical_dump`]
/// bytes (spec section 3).
pub fn curriculum_hash(c: &Curriculum) -> String {
    sha256_hex(canonical_dump(c).as_bytes())
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
    if !kp.visuals.is_empty() {
        map.insert("visuals".to_owned(), serde_json::json!(kp.visuals));
    }
    Value::Object(map)
}

/// One exemplar.
fn exemplar_value(exemplar: &Exemplar) -> Value {
    let mut map = Map::new();
    map.insert("answer".to_owned(), text(&exemplar.answer));
    if let Some(contract) = &exemplar.answer_contract {
        map.insert("answer_contract".to_owned(), serde_json::json!(contract));
    }
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
        let child = c.id_of(TopicIdx::from_u32(node(position)));
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
    for node in (0..c.enc_node_count()).map(node) {
        let src_node = EncNode::from_u32(node);
        let src = c.enc_node_id(src_node);
        for link in c.enc_forward(src_node) {
            rows.push(EncEdgeRow {
                src,
                dst: c.enc_node_id(link.target),
                weight: link.weight,
            });
        }
    }
    // The forward map holds one edge per `(src, dst)` pair, so the two ids
    // order the rows and no weight comparison is needed.
    rows.sort_by(|a, b| a.src.cmp(b.src).then_with(|| a.dst.cmp(b.dst)));
    rows
}

/// The node number of a load position. The arena holds at most `u32::MAX`
/// topics, so the conversion never saturates.
fn node(position: usize) -> u32 {
    u32::try_from(position).unwrap_or(u32::MAX)
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
