# Cadus 1.0 curriculum — M1 port specification (D1, D2, C5)

Source: survey of `/home/deploy/dev/cadus` on 2026-08-26 with a real load of the
checked-in tree. The same tree is copied to `curriculum/` in this repo (C5). Line
citations point at 1.0 files.

## 1. File layout and formats

```
curriculum/
  courses.yaml                    # required; absence = CurriculumNotFound
  <course-id>/                    # one directory per course id in courses.yaml
    NN-<unit-slug>.yaml           # unit files, glob "*.yaml", non-recursive
```

- `courses.yaml` is mandatory (`cadus/graph.py:570-572`). The directory name comes
  from `course.id` (`graph.py:583`). Unit files: `sorted(course_dir.glob("*.yaml"))`
  (`graph.py:593`) — non-recursive, `.yaml` only; `.yml` is invisible. The name is
  the only test `pathlib.Path.glob` applies: it returns a dot-prefixed name and it
  follows a symlink, so the port filters on neither.
- Real tree: 13 course directories, 88 unit files, 89 YAML files.
- Every model forbids unknown keys (`model.py:26-29`, `extra="forbid"`). Rust:
  `#[serde(deny_unknown_fields)]` on every struct.
- `Slug`: string, whitespace stripped, min length 1 after the strip (`model.py:22-23`).
  Kebab-case is a convention, not a rule.
- YAML: `yaml.safe_load`, UTF-8. An empty document is `{}`. A YAML int is not a
  string: every `str` field must be a YAML string (0 violations in the tree).

### courses.yaml (`model.py:158-171`)

| Key | Type | Required | Default |
|---|---|---|---|
| `courses` | list of Course | no | `[]` |
| `courses[].id` | Slug | yes | — |
| `courses[].name` | str | yes | — |
| `courses[].order` | int | yes | — (1..13; no uniqueness check) |
| `courses[].mastery_floor` | list of Slug | no | `[]` |
| `courses[].mastery_floor_course` | Slug or null | no | `null` |

Both floor forms on one course = lint `mastery_floor_ambiguous`. Real file: 2 list
form, 11 reference form.

### Unit file (`model.py:149-155`)

| Key | Type | Required |
|---|---|---|
| `unit` | str (not Slug) | yes |
| `course` | Slug | yes |
| `module` | str (not Slug) | yes |
| `topics` | list of Topic | no, default `[]` |

The `course` field is NOT cross-checked against the directory; the loader trusts the
field (`graph.py:602-614`, `:272`). All 88 files agree. Copy this behavior.

### Topic (`model.py:132-146`)

| Key | Type | Required | Default / constraint |
|---|---|---|---|
| `id` | Slug | yes | globally unique |
| `name` | str | yes | |
| `core` | bool | no | `true` |
| `difficulty` | float | yes | `0.0..=1.0` |
| `drill` | bool | no | `false` |
| `answer_kind` | enum | yes | see §4 |
| `expected_time_secs` | int | yes | `> 0` |
| `prerequisites` | list of PrereqEdge | no | `[]` |
| `encompassings_extra` | list of PrereqEdge | no | `[]` (used by 1 topic) |
| `knowledge_points` | list of KnowledgePoint | no | `[]` (lint: >= 1) |
| `diagnostic_exemplar` | Exemplar or null | no | `null` (lint: required) |
| `anki_seeds` | list of AnkiSeed | no | `[]` |

### PrereqEdge (`model.py:112-119`)

| Key | Type | Required | Constraint |
|---|---|---|---|
| `id` | Slug | yes | target topic id |
| `weight` | float | yes | `0.0..=1.0` (encompassing weight) |
| `key` | bool | no | `false` |

Distinct weights in the tree: 0.0, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.85, 0.9, 1.0.

### KnowledgePoint, Exemplar, AnkiSeed (`model.py:96-129`)

| Model | Key | Type | Required | Default |
|---|---|---|---|---|
| KnowledgePoint | `id` | Slug | yes | (unique inside the topic only; values kp1..kp4) |
| | `name` | str | yes | |
| | `key_prerequisites` | list of Slug | no | `[]` |
| | `exemplars` | list of Exemplar | no | `[]` (lint: >= 1) |
| | `constraints` | str or null | no | `null` (free text; carry as opaque String) |
| Exemplar | `problem` | str | yes | LaTeX inside `$...$` |
| | `answer` | str | yes | |
| | `solution_sketch` | str or null | no | `null` |
| AnkiSeed | `front` | str | yes | |
| | `back` | str | yes | |
| | `type` | enum `basic`/`cloze` | no | `basic` |

## 2. Identifiers and graph construction

- Topic id: globally unique across the tree. Duplicate: `Graph.__init__` raises
  `duplicate_topic_id` (`graph.py:259-270`); lint records it and keeps the first
  (`graph.py:652-663`).
- KP address in messages: `"<topic-id>.<kp-id>"`. Persist a KP as `(topic_id, kp_id)`.
- A prerequisite is a bare global topic id; cross-course references are normal
  (791 of 3281). `topic.prerequisites[i].id` is the parent; the declaring topic is
  the child. `prereqs[child] = {parent}`, `dependents[parent] = {child}`
  (`graph.py:292-293`).
- A prerequisite id with no topic is dropped from the adjacency at load
  (`graph.py:291`) and reported only by lint (`missing_ref`). A dangling
  `encompassings_extra` target is NOT filtered (`graph.py:295-296`).
- Encompassing maps (`graph.py:286-321`): `_enc[src][dst]` keeps only weights that
  beat the 0.0 default (weight-0 edges absent); `_enc_rev[dst][src]` records every
  edge, weight 0 included, because `neighborhood()` reads its keys. Counts: 3200
  forward, 3282 reverse, from 3282 declared edges (3281 prerequisites and 1
  `encompassings_extra`). Of the 3282, 82 carry weight 0, which is exactly the
  forward/reverse difference: 3200 + 82 = 3282. All 3282 `(src, dst)` pairs are
  distinct, so no declared edge repeats and the "keep the larger weight" branch of
  `_add_enc` (`graph.py:314-321`) never runs on the checked-in tree; a test of that
  branch needs a synthetic fixture. Measured with the 1.0 code on the tree in this
  repository: `weight-0 declared edges 82`, `duplicated (src, dst) pairs 0`,
  `_enc forward entries 3200`, `_enc_rev entries 3282`.
- `W(a -> b)` = max over paths of the product of edge weights, by a stack-based
  relaxation (`graph.py:178-198`), memoized per source; `W(a, a) = 1.0`; strict `>`
  comparison, LIFO pop order over an insertion-ordered map.
- Load order = `courses.yaml` list order (NOT sorted by `order`) x sorted unit files
  (code-point order) x `topics:` order x per-topic list orders.
- Every set-returning API in 1.0 returns an unordered set. Rust returns sorted `Vec`s
  or consumes order-independently.

## 3. Facts from a real load (verbatim)

```
courses: 13 / units: 88 / topics: 1090 / knowledge_points: 3138
kp count distribution: {2: 133, 3: 956, 4: 1}   (the 4-KP topic is category-definition)
prereq edges (declared): 3281 / in adjacency: 3281 / encompassings_extra: 1
enc forward edges (positive): 3200 / enc_rev edges (all): 3282
cycle: None
answer_kind: expression 221, multi-step 483, numeric 257, proof 129
core: True 725, False 365 / drill: True 22, False 1068
exemplars total: 6800 / anki_seeds total: 2144 / key_prerequisites total: 4417
prereq edges with key=true: 1820 / cross-course prereq edges: 791 / dangling refs: 0
lint findings: 0
mastery floor sizes: foundations 3, proofs 7, every other course 285
```

Topological order: 1.0 precomputes none. Rule for 2.0 (D1): Kahn's algorithm with a
min-heap on the authored load index. First entries: `single-digit-addition`,
`subtraction-facts`, `multiplication-tables`, `division-facts`, `perfect-squares`;
last: `cartesian-closed-categories`, `presheaves-intro`, `toposes-glimpse`.

Curriculum hash: 1.0 computes none (`curriculum_hash` is always NULL). 2.0 defines it
in M1: the semantic hash = SHA-256 of the canonical dump (§8), value on the checked-in
tree `f121f9baf73f29e89679a3304c5405352f0ff47f163a0140bd0e92102a0a8b9e`; a byte
hash over sorted `relpath \0 bytes \0` is provenance only
(`882d7b41127ef40bb0294c68a9872e0beb351257e00026b13e746cdf866ff448`).

## 4. Answer kinds (`model.py:37-42`)

| Rust variant | wire value | topics |
|---|---|---|
| Numeric | `numeric` | 257 |
| Expression | `expression` | 221 |
| MultiStep | `multi-step` (hyphen) | 483 |
| Proof | `proof` | 129 |

Verifiable kinds (V1/V2, M2): numeric and expression — 478 topics.

## 5. Lint rules (`graph.py:628-842`; runner `scripts/lint_curriculum.py`)

Runner output: exit 0 `OK: {path} is a valid curriculum (0 findings).`; exit 1
`FAIL: {n} curriculum finding(s) in {path}:` then `  [{code}]{ (topic)} {message}`.
`{x!r}` = Python repr with single quotes. Finding fields: `code`, `message`, `topic?`,
`file?`, `context?` (list), `fatal` (default true).

Parse stage (the only findings `Graph.load` blocks on when fatal):
1. `yaml` — `courses.yaml: {exc}` or `{course}/{file}: {exc}`.
2. `schema` — `{loc}: {msg}` (dotted path); missing key, wrong type, unknown key.
3. `weight_out_of_range` — same shape, when `weight` in loc and the error is a bound.
4. `missing_course_dir` (advisory) — `no unit directory {course.id}/ for course`.
5. `empty_course` (advisory) — `course {course.id} has no unit files`.

Graph stage (lint only; the graph loads regardless):
6. `duplicate_topic_id` — `topic id {id!r} defined more than once`.
7. `missing_ref` — `prerequisite {edge.id!r} of {tid!r} does not exist` /
   `encompassings_extra {edge.id!r} of {tid!r} does not exist` /
   `key_prerequisite {key_id!r} in {tid}.{kp.id} does not exist`.
8. `cycle` — `prerequisite cycle: a -> b -> c -> a`; context = cycle nodes.
9. `no_kp` — `topic {tid!r} has no knowledge_points`.
10. `no_exemplar` — `KP {tid}.{kp.id} has no exemplars`.
11. `missing_diagnostic_exemplar` — `topic {tid!r} has no diagnostic_exemplar`.
12. `key_prereq_not_ancestor` — `key_prerequisite {key_id!r} in {tid}.{kp.id} is neither an ancestor nor an encompassings_extra target` (skipped when already missing_ref).
13. `noncore_ancestor_of_core` — `non-core topic {tid!r} is a prerequisite (ancestor) of core topic {example!r}{extra}`, `extra` = ` (+{n-1} more)`; iteration over sorted topics; context = sorted core dependents.
14. `module_inconsistent` — `topic {tid!r} has an empty module name` / `module {module!r} spans multiple courses: ['a', 'b']` (Python list repr; sorted).
15. `mastery_floor_ambiguous` — `course {id!r} sets both a mastery_floor list and mastery_floor_course {ref!r}; a course must use exactly one mastery-floor form`.
16. `unreachable_from_floor` — `topic {tid!r} is not reachable from course {course.id}'s floor/roots`; grounding = floor ∩ known ∪ roots; skipped entirely when a `cycle` or a `duplicate_topic_id` finding exists.
Graph-only code: `empty` — `no curriculum found`.

Not enforced by lint: kebab-case; KP id uniqueness; duplicate/self prerequisite
edges; `course` field vs directory; `order` uniqueness.

## 6. Exemplars and constraints

Exemplar = `{problem, answer, solution_sketch?}`, LaTeX inside `$...$`, YAML
single-quoted scalars; 85 of 88 files contain non-ASCII. `constraints` is a free-text
string on every KP (3138/3138) — the M1 loader carries it as an opaque String; A1/D-S4
structured constraints are a 2.0 addition, not a port. The one `encompassings_extra`:
`foundations/07-polynomials-quadratics.yaml:950-954` (`difference-of-squares`, 0.3).
A double-quoted YAML scalar with `\\mid` resolves to one backslash
(`proofs/01-proof-techniques.yaml:47`).

## 7. Parity traps

1. Catalog order = file order, not `order` (proofs is second in the file, fourth by order).
2. Unit-file order = code-point sort of the path string, not locale, not readdir.
3. Non-recursive `*.yaml` glob.
4. Unknown key = error at every level.
5. `multi-step` wire value.
6. `Slug` strips whitespace before matching.
7. Dangling prerequisites dropped; dangling `encompassings_extra` kept.
8. `_enc` / `_enc_rev` asymmetry for weight-0 edges (82 real edges; every declared
   pair is distinct, so the max-keeps-the-larger branch needs a synthetic fixture).
9. Float products in `W` are order-sensitive in the last bit; replay the LIFO order
   or compare with a tolerance in the parity test.
10. No order dependence on hash-set iteration.
11. Cycle detection with an explicit stack; the cycle list starts at the re-entered node.
12. `mastery_floor_course` unions every course with `order <= ref.order`.
13. `Graph.load` tolerates every graph-stage lint code; only parse-stage fatal
    findings block a load.
14. `Graph.__init__` raises on duplicate topic id; lint does not.
15. UTF-8 everywhere; no case folding.
16. JSON floats: shortest round-trip in both languages; verify on the whole dump.
17. Do not port the mtime cache; load once into an `Arc` arena.
18. The curriculum hash is a 2.0 decision (see §3).

### 2.0 strictness

1.0 reads YAML with PyYAML `safe_load`, which resolves the YAML 1.1 tag set, and it
validates with pydantic in lax mode. 2.0 reads YAML with `serde_norway`, which
resolves the YAML 1.2 core schema, and 2.0 does not emulate PyYAML. This section
lists every deliberate difference between the two loaders. The 2.0 loader reports
each rejected form with the `schema` code and a message that names the form and the
fix. The pydantic message text stays reserved for the values pydantic also rejects,
so a 1.0 message never appears on a value 1.0 accepts.

Rejected by 2.0, accepted by 1.0:

| Form | 1.0 reads | 2.0 message |
|---|---|---|
| `core: yes` (also `no`, `on`, `off`, `y`, `n`, in any case) | `True` | `topics.0.core: YAML 1.1 boolean 'yes' is not accepted; write true or false` |
| `core: "true"` (a string pydantic coerces) | `True` | `topics.0.core: string 'true' is not accepted; write true or false` |
| `core: 1`, `core: 0` | `True`, `False` | `topics.0.core: number 1 is not accepted; write true or false` |
| `expected_time_secs: 030` (octal) | `24` | `topics.0.expected_time_secs: integer 030 is not accepted; write 24` |
| `expected_time_secs: 1_200` (underscore) | `1200` | `topics.0.expected_time_secs: integer 1_200 is not accepted; write 1200` |
| `expected_time_secs: 1:30` (sexagesimal) | `90` | `topics.0.expected_time_secs: integer 1:30 is not accepted; write 90` |
| `expected_time_secs: "30"` (a quoted integer) | `30` | `topics.0.expected_time_secs: string '30' is not accepted; write 30` |
| `difficulty: "0.3"` (a quoted number) | `0.3` | `topics.0.difficulty: string '0.3' is not accepted; write the number unquoted` |
| `difficulty: true` | `1.0` | `topics.0.difficulty: boolean true is not accepted; write a number` |
| an integer literal outside `i64` | the Python integer | `topics.0.expected_time_secs: integer literal outside the 64-bit range` |
| a repeated mapping key | the last value | `c/00.yaml: duplicate mapping key 'name' at line 5` |
| `<<: *anchor` (a merge key) | the merged fields | `topics.1.<<: merge keys are not accepted; write the fields out` |

An integer literal outside `i64` takes two paths inside the parser — one literal
above `i64::MAX` still fits `u64`, one past `u64` fits no number at all — and both
paths report the one message above, with the `schema` code and the dotted location.

Accepted by 2.0, the same as 1.0:

- A UTF-8 BOM. The loader removes it before the parse. Python removes it in the
  reader, and libyaml does not.
- A whole-number float in an integer field. `expected_time_secs: 60.0` is 60 and
  `order: 1.0` is 1. This is the pydantic lax rule, and content generators write it.
- A dot-prefixed unit file and a symlinked unit file. `pathlib.Path.glob` returns
  both, so both belong to the load and to the load index.

Accepted by 2.0, rejected by 1.0:

- A YAML 1.1 scalar in a string field. `name: no` is the string `no`, and
  `name: 2020-01-01` is the string `2020-01-01`. PyYAML resolves the first one to a
  boolean and the second one to a date, and pydantic then reports `Input should be a
  valid string`. 2.0 keeps the text the author wrote.
- A directory, or a symlink to a missing file, named `*.yaml` inside a course
  directory. 1.0 raises an uncaught `OSError` and reads nothing; 2.0 reports a `yaml`
  finding and reads the other files. The loader never panics on content.

Parity, not strictness: NaN and the two infinities are out of range for `difficulty`
and `weight`. pydantic reports `Input should be less than or equal to 1` for NaN and
for `.inf`, and `Input should be greater than or equal to 0` for `-.inf`. 2.0 reports
the same three messages.

The checked-in tree writes none of the rejected forms. The test
`the_checked_in_tree_uses_no_yaml_1_1_form` (`crates/core/tests/loader.rs`) reads all
89 files and lists every line that writes one.

## 8. Parity oracle

`scripts/oracle/dump_curriculum_1_0.py` (runs with `/home/deploy/dev/cadus/.venv/bin/python`,
imports the 1.0 loader from `/home/deploy/dev/cadus`). Output: one line of canonical
JSON (`sort_keys=True, ensure_ascii=False, separators=(",", ":")`), sha256 on stderr.

Dump fields: `schema` = `cadus-curriculum-dump/1`; `courses` (catalog order);
`topics` (sorted by id) with `id, name, core, difficulty, drill, answer_kind,
expected_time_secs, course, module, unit, load_index, prerequisites[], encompassings_extra[],
knowledge_points[{id,name,key_prerequisites,constraints,exemplars[]}], diagnostic_exemplar,
anki_seeds[]`; `prereq_edges` = sorted `[child, parent, weight, key]` (existing targets
only); `encompassing_edges` = sorted `[src, dst, weight]` (forward map);
`topo_order`; `counts`; `cycle`.

Verified on the checked-in tree and on the copy in this repo:
`sha256=f121f9baf73f29e89679a3304c5405352f0ff47f163a0140bd0e92102a0a8b9e`,
4,052,882 bytes.

Lint oracle: `scripts/lint_curriculum.py` in 1.0 prints
`OK: <path> is a valid curriculum (0 findings).` on the clean tree. Port the lint
against synthetic broken fixtures (1.0 `tests/test_graph.py:518-797` builds them) and
diff `{code, message, topic, file, context, fatal}` as JSON.
