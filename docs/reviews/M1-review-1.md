# M1 adversarial review — round 1 (2026-08-26)

Run on the tree at commit 128b032 (M1 U1–U4 integrated). Two find/refute rounds, major+ only: 30 raised, 27 confirmed. Assigned to FIXM1a (loader), FIXM1b (arena/graph/dump), FIXM1c (lint fixtures).

## Orchestrator rulings (binding for the fix units)

- YAML dialect. 1.0 read YAML with PyYAML `safe_load` (YAML 1.1 resolution); 2.0 reads
  with `serde_norway` (YAML 1.2 core schema). 2.0 does NOT emulate PyYAML. Rule: the 2.0
  loader REJECTS every YAML 1.1-only form with an explicit finding of its own wording
  (code `schema`, message names the form and the fix), and ACCEPTS a UTF-8 BOM and a
  whole-number float in an integer field (pydantic lax behavior that content generators
  produce). Rejected forms: unquoted `yes/no/on/off/y/n` in a bool field, octal `060`,
  underscore `1_200`, sexagesimal `1:30`, merge keys `<<`, duplicate mapping keys, integers
  outside i64. A string field keeps `yes`/`2020-01-01` as strings (2.0 is more permissive
  there than 1.0 and that is fine: a string is what the author wrote). Document every
  difference in `docs/reference/curriculum-1.0-spec.md` §7 under a new "2.0 strictness"
  heading. The checked-in tree uses none of the rejected forms (pin that with a test).
- NaN and infinities are out of range: `difficulty`/`weight` must be finite and in 0..=1.
- Duplicate course id: last entry wins, as 1.0 (no new lint code).
- `load_curriculum` returns `Err` when the parse stage has a fatal finding, as 1.0 `Graph.load`.
- Float text in the dump follows the Python `repr` rule exactly (shortest round-trip digits,
  scientific notation when the exponent is < -4 or >= 16, exponent with sign and at least
  two digits).
- Equivalent mutants become fixtures with 1.0-generated expected output.

| # | Sev | File | Unit | Title |
|---|---|---|---|---|
| 1 | blocker | `crates/core/src/curriculum/load.rs:516` | FIXM1a | A NaN weight or NaN difficulty passes the range check, so the Rust lint calls a tree clean that 1.0 rejects, and the dump writes null where a number belongs |
| 2 | blocker | `crates/core/src/curriculum/load.rs:299` | FIXM1a | Loader silently drops hidden and symlinked unit files that 1.0 loads |
| 3 | major | `crates/core/src/curriculum/dump.rs:421` | FIXM1b | The canonical dump formats floats with ryu, not with the Python repr rule, so the dump and the curriculum hash diverge for any legal value below 1e-4 |
| 4 | major | `crates/core/src/curriculum/load.rs:363` | FIXM1a | A float value in an integer field passes the hand-written schema walk but fails the serde backstop, so the whole file is dropped with a schema message that has no location |
| 5 | major | `crates/core/src/curriculum/load.rs:293` | FIXM1a (dup of #2) | A symlinked unit file is invisible to the loader, so its topics vanish from the tree while 1.0 reads them |
| 6 | major | `crates/core/src/curriculum/load.rs:516` | FIXM1a (dup of #1) | NaN passes the 0..=1 range check, so the port accepts content 1.0 rejects and dumps `null` for a float |
| 7 | major | `crates/core/src/curriculum/arena.rs:291` | FIXM1b | A repeated course id resolves to the first catalog entry; 1.0 resolves to the last |
| 8 | major | `crates/core/src/curriculum/load.rs:317` | FIXM1a | A duplicated YAML mapping key drops the whole unit file; 1.0 accepts it (last value wins) |
| 9 | major | `crates/core/src/curriculum/arena.rs:291` | FIXM1b (dup of #7) | Arena course_by_id keeps the first duplicate course id; 1.0 keeps the last, so mastery_floor diverges |
| 10 | major | `crates/core/src/curriculum/load.rs:293` | FIXM1a (dup of #2) | Loader hides symlinked and dot-prefixed unit files that 1.0 loads; no fixture covers either |
| 11 | major | `crates/core/tests/arena.rs:375` | FIXM1b | Parity trap 12 (mastery_floor_course order union) is an equivalent mutant: deleting the rule keeps all 71 tests green |
| 12 | major | `crates/core/src/bin/dump_curriculum.rs:43` | FIXM1b | Parity trap 13 lives only in the dump binary and is pinned by no test; the library load_curriculum builds a partial arena where 1.0 raises |
| 13 | major | `crates/core/src/curriculum/arena.rs:650` | FIXM1b (dup of #12) | load_curriculum never blocks on a fatal parse-stage finding, so a broken unit file yields a silently truncated arena |
| 14 | major | `crates/core/src/curriculum/load.rs:299` | FIXM1a (dup of #2) | Unit files whose name starts with a dot, and symlinked unit files, are skipped with no finding |
| 15 | major | `crates/core/src/curriculum/load.rs:747` | FIXM1a (dup of #4) | An integer written as a whole-number float passes the schema walk and then fails serde, producing a schema finding with an empty location and dropping the whole file |
| 16 | major | `crates/core/src/curriculum/load.rs:506` | FIXM1a | The schema walk rejects the scalar coercions 1.0 pydantic performs, and reuses pydantic's message text for inputs pydantic accepts |
| 17 | major | `crates/core/src/curriculum/load.rs:317` | FIXM1a | A YAML 1.1 plain scalar in a string field is read as a string, so the port loads and hashes a tree 1.0 refuses |
| 18 | major | `crates/core/src/curriculum/load.rs:739` | FIXM1a | YAML 1.1 integer forms (octal, underscore, sexagesimal) drop the whole file, or the whole catalog |
| 19 | major | `crates/core/src/curriculum/load.rs:466` | FIXM1a | A YAML merge key is not flattened, so an anchored unit file is dropped with a bogus '<<' extra-key finding |
| 20 | major | `crates/core/src/curriculum/load.rs:314` | FIXM1a | A UTF-8 BOM splits the document, so every BOM-prefixed unit file is dropped |
| 21 | major | `crates/core/src/curriculum/load.rs:317` | FIXM1a | An integer literal outside the i64/u64 range makes the loader reject a file 1.0 loads cleanly |
| 22 | major | `crates/core/src/curriculum/arena.rs:735` | FIXM1b | Every repeated-edge rule of Graph.__init__ is an equivalent mutant: no fixture declares one edge twice |
| 23 | major | `crates/core/src/curriculum/graph.rs:239` | FIXM1b | graph::closure's start-exclusion rule (1.0 `_closure`) survives deletion: no test calls ancestors/descendants on a cyclic fixture |
| 24 | major | `crates/core/src/curriculum/lint.rs:291` | FIXM1c | The lint's id-sorting rules are unpinned: every fixture's load order already equals its id order |
| 25 | major | `crates/core/tests/lint.rs:130` | FIXM1c | The whole inter-rule ordering of lint_curriculum is unpinned: each of the 19 fixtures triggers exactly one lint code |
| 26 | major | `docs/reference/curriculum-1.0-spec.md:112` | FIXM1a | Spec section 2 and trap 8 state the wrong weight-0 edge count and invent a duplicated edge pair |
| 27 | major | `crates/core/src/curriculum/load.rs:496` | FIXM1a | A whole-number bool value gets a lint message that 1.0 does not write |

## FIXM1a

### #1 [blocker] A NaN weight or NaN difficulty passes the range check, so the Rust lint calls a tree clean that 1.0 rejects, and the dump writes null where a number belongs

File: `crates/core/src/curriculum/load.rs:516` — IDs: R5, C5, D1

**Claim.** Checker::check_unit_interval tests `number < 0.0` and `number > 1.0`, and both tests are false for NaN, so `difficulty: .nan` and `weight: .nan` produce no finding at all, while 1.0 pydantic `Field(ge=0.0, le=1.0)` reports two fatal findings for the same file.

**Evidence.**

```
crates/core/src/curriculum/load.rs:506-521
    fn check_unit_interval(&mut self, value: &Value) {
        let Some(number) = as_f64(value) else { ... };
        if number < 0.0 {
            self.report_range("Input should be greater than or equal to 0");
        } else if number > 1.0 {
            self.report_range("Input should be less than or equal to 1");
        }
    }

Fixture c/00.yaml: topic `a` with `difficulty: .nan`, topic `b` with `prerequisites: [{id: a, weight: .nan}]`.

1.0 (/home/deploy/dev/cadus/.venv/bin/python, cadus.graph):
  schema | topics.0.difficulty: Input should be less than or equal to 1 | fatal= True
  weight_out_of_range | topics.1.prerequisites.0.weight: Input should be less than or equal to 1 | fatal= True
  load raises: CurriculumError [schema] topics.0.difficulty: ...; [weight_out_of_range] topics.1.prerequisites.0.weight: ...

Port (./target/debug/lint_curriculum FIXTURE):
  OK: .../fx/nan is a valid curriculum (0 findings).
  rc=0

Port (./target/debug/dump_curriculum FIXTURE) exits 0 and writes:
  "prereq_edges":[["b","a",null,false]]
  "difficulty":null

The 1.0 oracle on the same tree exits non-zero (CurriculumError traceback), so no dump exists to compare.

The same file also carries two comments that the input falsifies:
  dump.rs:419-420  "No curriculum value is one of them: `difficulty` and `weight` both hold a number in 0..=1."
  dump.rs:425-426  "Order two weights. No weight is `NaN`, so the total order of `f64` matches the Python comparison."
```

**Failure scenario.** An author writes `difficulty: .nan` (or `weight: .nan`, or `-.nan`) in a unit file — a YAML 1.2 and YAML 1.1 float literal that a generator or a spreadsheet export produces from an empty cell. `lint_curriculum` prints `OK: curriculum is a valid curriculum (0 findings).` and exits 0, so the C5 content gate passes the file into git. 1.0 refuses the same tree with two fatal findings. The arena then holds `difficulty = NaN` and an encompassing edge of weight NaN: `dump.rs::float` turns each into JSON `null`, so `canonical_dump` emits `"difficulty":null` and `["b","a",null,false]` — a dump the schema of spec section 8 does not allow — and `curriculum_hash` is taken over those bytes. Downstream, `compare_float` sorts the rows with `total_cmp`, which puts NaN outside the Python order, and `graph::relax` multiplies by NaN, so every reach weight through that edge becomes NaN.

**Refuter.** I could not refute the claim; I reproduced it end to end. `Checker::check_unit_interval` (/home/deploy/dev/cadus2.0/crates/core/src/curriculum/load.rs:506-521) is the only range gate for `difficulty` and `weight`. `as_f64` (load.rs:731-736) returns `Some(f64::NAN)` for the YAML scalar `.nan`, because `serde_norway` parses it as `Value::Number`. Both `number < 0.0` and `number > 1.0` are false for NaN, so the checker emits no finding, and no later stage compensates: `weight_out_of_range` is produced only at load.rs:406, from the same checker, and `Topic.difficulty` / `Prerequisite.weight` are plain `f64` in model.rs:185 and model.rs:155 with no NaN guard. `.inf` is caught (inf > 1.0), so NaN is the single hole. The divergence from 1.0 is real, and the two source comments in dump.rs:418-426 ("No curriculum value is one of them", "No weight is `NaN`") are false for this input, so `float()` 

### #2 [blocker] Loader silently drops hidden and symlinked unit files that 1.0 loads

File: `crates/core/src/curriculum/load.rs:299` — IDs: C5, R5, D1

**Claim.** unit_file_names() skips directory entries whose name starts with '.' and entries whose read_dir file type is not a regular file, so a hidden `*.yaml` unit file and a symlink to a `*.yaml` unit file are dropped from the load with no finding, while 1.0's `sorted(course_dir.glob("*.yaml"))` reads both.

**Evidence.**

```
load.rs:293 `if !entry.file_type()?.is_file() { continue; }` (read_dir file types do NOT follow symlinks) and load.rs:299 `if name.starts_with('.') || !name.ends_with(UNIT_EXTENSION) { continue; }`. Python 3.13 pathlib does not exclude dotfiles or symlinks: `sorted(x.name for x in p.glob('*.yaml'))` -> `['.hidden.yaml', 'dir.yaml', 'link.yaml', 'normal.yaml']`. On a tree with c/.00-hidden.yaml, c/01-plain.yaml and c/02-linked.yaml (a symlink), 1.0 `load_curriculum` reports `topics ['hid', 'linked', 'plain'] / units 3`; `target/debug/dump_curriculum` on the same tree reports `topics ['plain'] / units 1`. No finding of any code is emitted by the port. The doc comment at load.rs:285-288 cites only parity traps 2 and 3 (code-point sort, non-recursive `*.yaml`); neither trap nor docs/DECISIONS.md nor any test mentions hidden files or symlinks.
```

**Failure scenario.** A deployment mounts `curriculum/<course>/NN-unit.yaml` as a symlink (a common container/volume or content-overlay layout), or an author checks in a file whose name begins with a dot. 1.0 serves those topics; 2.0 loads zero of them, emits no finding, so `lint_curriculum` exits 0 on a tree that is missing whole units. Because load order fixes every TopicIdx, the missing file also shifts every later load_index, which changes `topo_order`, `load_index` in the dump, and therefore the curriculum hash — silently, with no signal anywhere.

**Refuter.** The defect is demonstrable, so I cannot refute it.

Both filters diverge from 1.0. `/home/deploy/dev/cadus2.0/crates/core/src/curriculum/load.rs:293` rejects an entry whose `read_dir` file type is not a regular file, and `read_dir` file types do not follow symlinks. Line 299 rejects a name that starts with a dot. 1.0 `cadus/graph.py:593` uses `sorted(course_dir.glob("*.yaml"))`, which applies neither filter.

I confirmed the Python side on the oracle interpreter (Python 3.13.5). On a directory that holds `normal.yaml`, `.hidden.yaml`, a directory `dir.yaml`, and a symlink `link.yaml`, `sorted(x.name for x in p.glob('*.yaml'))` returns `['.hidden.yaml', 'dir.yaml', 'link.yaml', 'normal.yaml']`. `pathlib.Path.glob` excludes neither dotfiles nor symlinks.

I confirmed the divergence on a real curriculum tree. I built a course `c` with `.00-hidden.yaml`, `01-plain.yaml`, and `02-linked.yaml`

### #4 [major] A float value in an integer field passes the hand-written schema walk but fails the serde backstop, so the whole file is dropped with a schema message that has no location

File: `crates/core/src/curriculum/load.rs:363` — IDs: R5, C5

**Claim.** `as_i64` deliberately accepts a float with no fractional part, to copy the pydantic lax coercion, but `Unit`/`Catalog` declare `i64`, so `T::deserialize` then rejects the same document and the residual arm emits a `schema` finding whose message starts with `": "` — an empty dotted location that 1.0 never produces — while 1.0 loads the file.

**Evidence.**

```
crates/core/src/curriculum/load.rs:361-364
    // A residual error means the walk and the types disagree. Report it rather
    // than drop the file in silence.
    T::deserialize(document.clone())
        .map_err(|error| vec![Finding::new("schema", format!(": {error}")).with_file(file)])
crates/core/src/curriculum/load.rs:739-751  (as_i64: "A float with no fractional part counts.")

Case A — `expected_time_secs: 60.0` in a unit file:
$ ./target/debug/dump_curriculum FIXTURE
dump_curriculum: 1 fatal finding(s):
  [schema] : invalid type: floating point `60.0`, expected i64
$ .venv/bin/python scripts/oracle/dump_curriculum_1_0.py FIXTURE | grep -o '"expected_time_secs":[^,]*'
"expected_time_secs":60
$ .venv/bin/python scripts/oracle/dump_lint_1_0.py FIXTURE
[]

Case B — `order: 1.0` in courses.yaml:
$ ./target/debug/dump_curriculum FIXTURE
dump_curriculum: 1 fatal finding(s):
  [schema] : invalid type: floating point `1.0`, expected i64
$ .venv/bin/python scripts/oracle/dump_curriculum_1_0.py FIXTURE
sha256=a78b139e2f1b16fba97c746d79053c334e4137bbb9e20c597fd633927d319010  (exit 0)

Case C — `expected_time_secs: 9223372036854775808` (as_i64 saturates through the f64 branch and passes the `> 0` test):
$ ./target/debug/dump_curriculum FIXTURE
  [schema] : invalid value: integer `9223372036854775808`, expected i64
$ .venv/bin/python ... | grep -o '"expected_time_secs":[^,]*'
"expected_time_secs":9223372036854775808

Spec section 5 rule 2 fixes the shape of a `schema` message as `{loc}: {msg}` with a dotted path; the port writes an empty `loc` here.
```

**Failure scenario.** An author writes `order: 1.0` for one course in `curriculum/courses.yaml`. 1.0 coerces it to the integer 1, loads all 13 courses, and `lint_curriculum` reports 0 findings. The port reports `[schema] : invalid type: floating point `1.0`, expected i64`, `parse_curriculum` returns `catalog: None`, and every unit file of every course is dropped — the whole curriculum stops loading. The same happens per file for `expected_time_secs: 60.0`, which drops that unit's topics only. The message names no field, so an author reading `[schema] : invalid type: floating point `1.0`, expected i64` cannot tell which line to fix. The `Checker` was written to accept these values (`as_i64` handles `fract() == 0.0`), so the walk and the model contradict each other.

**Refuter.** I could not refute the claim. I reproduced all three cases against the real curriculum tree and the 1.0 oracle, and the port diverges exactly as the reviewer describes.

The mechanism is real and is a contradiction inside the port. `as_i64` (crates/core/src/curriculum/load.rs:739-751) accepts a YAML float with no fractional part, and `check_int` / `check_positive_int` (load.rs:523-539) use it. That copies pydantic lax mode: /home/deploy/dev/cadus/cadus/model.py declares `expected_time_secs: int = Field(gt=0)` (line 141) and `order: int` (line 163) under `model_config = ConfigDict(extra="forbid")` (line 29) with no `strict=True`, so pydantic coerces 1.0 to 1. But crates/core/src/curriculum/model.rs:189 and :223 declare `pub expected_time_secs: i64` and `pub order: i64`, and serde has no float-to-integer coercion. The Checker collects nothing, `checker.out` is empty, and control reaches th

### #5 [major] A symlinked unit file is invisible to the loader, so its topics vanish from the tree while 1.0 reads them

File: `crates/core/src/curriculum/load.rs:293` — IDs: R5, C5, D1 — duplicate of #2

**Claim.** `unit_file_names` skips every directory entry whose `file_type()` is not a regular file, and `fs::read_dir` does not follow symlinks, so a `*.yaml` symlink is dropped; 1.0 `sorted(course_dir.glob("*.yaml"))` returns the symlink and opens it.

**Evidence.**

```
crates/core/src/curriculum/load.rs:289-296
fn unit_file_names(course_dir: &Path) -> Result<Vec<String>, std::io::Error> {
    let mut names = Vec::new();
    for entry in fs::read_dir(course_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }

1.0 (cadus/graph.py:593): `unit_files = sorted(course_dir.glob("*.yaml"))` — glob applies no file-type filter.

Fixture: c/00.yaml is a symlink to a real unit file that holds one topic.
$ ls -l FIXTURE/c/
lrwxrwxrwx 1 deploy deploy 19 Aug 26 08:47 00.yaml -> ../shared/real.yaml

$ ./target/debug/dump_curriculum FIXTURE | grep -o '"topics":[0-9]*,"units":[0-9]*'
"topics":0,"units":0
$ .venv/bin/python scripts/oracle/dump_curriculum_1_0.py FIXTURE | grep -o '"topics":[0-9]*,"units":[0-9]*'
"topics":1,"units":1

$ ./target/debug/lint_curriculum FIXTURE
  [empty_course] course c has no unit files
$ .venv/bin/python scripts/oracle/dump_lint_1_0.py FIXTURE
[]

Spec section 1 and parity traps 2 and 3 name only the sort order and the non-recursive `*.yaml` glob; neither names a file-type filter.
```

**Failure scenario.** A deployment mounts or checks out `curriculum/` with one course directory whose unit files are symlinks (a shared unit reused across two courses, a git checkout with `core.symlinks`, or a container volume that links content in). The port reads zero unit files for that course. `empty_course` is advisory, so `load_curriculum` returns an arena with the course present and no topics — a learner enrolled in it gets an empty frontier at run time, exactly the degradation 1.0 documents as the reason for reporting `missing_course_dir`. 1.0 on the same tree loads 1 unit and 1 topic and lints clean. The content is dropped by the port and only by the port.

**Refuter.** The claim is correct and reproducible. I cannot refute it. `DirEntry::file_type()` on Unix reports the type of the link, not the type of the target, so `!entry.file_type()?.is_file()` at crates/core/src/curriculum/load.rs:293 drops a `*.yaml` symlink. Python `Path.glob("*.yaml")` at /home/deploy/dev/cadus/cadus/graph.py:593 returns the symlink and 1.0 opens it. On one fixture the port produced topics 0 / units 0 with an advisory `empty_course` finding, and the 1.0 oracle produced topics 1 / units 1 with an empty lint. A control run over the identical tree with the symlink replaced by a regular file made the Rust dump hash equal the Python dump hash exactly, so the symlink is the only variable. The port also contradicts itself: line 85 uses `Path::is_file()`, which follows a symlink, so a symlinked courses.yaml loads while a symlinked unit file does not. No authority permits the filter. S

### #6 [major] NaN passes the 0..=1 range check, so the port accepts content 1.0 rejects and dumps `null` for a float

File: `crates/core/src/curriculum/load.rs:516` — IDs: C5, R5, D1 — duplicate of #1

**Claim.** check_unit_interval() tests the range with `number < 0.0` / `number > 1.0`; both comparisons are false for NaN, so `difficulty: .nan` and `weight: .nan` pass the schema walk and are deserialized into the arena, while pydantic rejects both with a fatal finding.

**Evidence.**

```
load.rs:516-520 `if number < 0.0 { self.report_range("Input should be greater than or equal to 0"); } else if number > 1.0 { self.report_range("Input should be less than or equal to 1"); }`. 1.0 (pydantic 2.13.4) on the same values: `Topic(difficulty=nan)` -> `difficulty: Input should be less than or equal to 1`; `PrereqEdge(weight=nan)` -> same. On a fixture with `difficulty: .nan` on topic `a` and `weight: .nan` on b's prerequisite, `scripts/lint_curriculum.py` prints `FAIL: 2 curriculum finding(s)` (`[schema] topics.0.difficulty: ...`, `[weight_out_of_range] topics.1.prerequisites.0.weight: ...`), while `target/debug/lint_curriculum` prints `OK: ... is a valid curriculum (0 findings).` and `dump_curriculum` emits `"prereq_edges":[["b","a",null,false]]` and `"difficulty":null` (dump.rs:421 `Number::from_f64(value).map_or(Value::Null, ...)`).
```

**Failure scenario.** An author writes `difficulty: .nan` (or a templating step emits it). 1.0 refuses the load with a fatal finding. 2.0 lints clean, builds the arena, and produces a canonical dump in which a field the schema declares as a float is JSON `null` — so the dump no longer round-trips, the curriculum hash covers a malformed document, and every downstream consumer of `difficulty`/`weight` gets NaN (which silently makes `relax`'s `candidate > *slot` always false, i.e. the edge behaves as weight 0 with no diagnostic).

**Refuter.** The claim is demonstrable, and I found no defense for it. `check_unit_interval` (crates/core/src/curriculum/load.rs:506-521) is the only range gate on `difficulty` and `weight`, and it tests the bounds with `number < 0.0` / `else if number > 1.0`. Both comparisons are false for NaN, so a NaN scalar passes the schema walk with no finding. The YAML crate is `serde_norway` (a `serde_yaml` fork), whose `Value::Number` holds a real f64, so `.nan` reaches `as_f64` as NaN instead of failing the `Value::Number` match. There is no `is_nan` or `is_finite` guard anywhere under crates/core/src (grep for `is_nan\|is_finite` returns nothing), and the later lint stage adds none. The 1.0 oracle rejects the same input, because `cadus/model.py:118` declares `weight: float = Field(ge=0.0, le=1.0)` (and the same bounds on `difficulty`), and pydantic 2 fails `le` for NaN. So 2.0 accepts a document that 1.0 r

### #8 [major] A duplicated YAML mapping key drops the whole unit file; 1.0 accepts it (last value wins)

File: `crates/core/src/curriculum/load.rs:317` — IDs: C5, R5

**Claim.** serde_norway rejects a duplicate mapping key as a parse error, so read_document turns a repeated key into a fatal `yaml` finding and the unit is discarded, while PyYAML's safe_load accepts the document and keeps the last value, so 1.0 loads every topic in that file.

**Evidence.**

```
load.rs:317-319 `let value: Value = serde_norway::from_str(&text).map_err(|error| Box::new(Finding::new("yaml", format!("{rel}: {error}")).with_file(rel)))?;` — the finding is fatal (finding.rs:56, `Finding::new` sets `fatal: true`) and the caller at load.rs:152-157 does `continue`, so the file never reaches `units`. On a unit file whose topic repeats `name:`, 1.0 reports `topics: ['alpha'] name: Alpha renamed`; the port loads zero topics (`topics_in_course("c")` empty) and `lint_curriculum` reports `[yaml] c/00.yaml: topics[0]: duplicate entry with key "name" at line 5 column 5`. No spec trap covers duplicate mapping keys, and no fixture or test in crates/core/tests pins this choice.
```

**Failure scenario.** An author duplicates a key while hand-editing a unit file — the commonest YAML mistake there is, and one PyYAML has always swallowed. Under 1.0 the tree loads and serves normally with the second value. Under 2.0 the entire unit file is dropped from the arena: every topic in it disappears, every prerequisite pointing into it becomes dangling, every later topic's load_index shifts, and the curriculum hash changes. `load_curriculum` still returns `Ok`, so `dump_curriculum` produces a truncated curriculum with exit status 0; only the separate lint run surfaces the cause.

**Refuter.** The claim is demonstrable and I reproduced it exactly. I built a two-topic unit file whose first topic repeats `name:`. 1.0 (`/home/deploy/dev/cadus/.venv/bin/python`, `cadus.graph.load_curriculum`) loaded both topics and kept the last value: `1.0 topics: ['alpha', 'beta']`, `1.0 alpha.name: Alpha renamed`, and its lint returned only content findings (`no_kp`, `missing_diagnostic_exemplar` for each topic). The 2.0 port dropped the whole file: `lint_curriculum` printed exactly one finding, `[yaml] c/00.yaml: topics[0]: duplicate entry with key "name" at line 5 column 5`, and exited 1. `serde_norway::from_str` rejects the duplicate mapping key, `read_document` (load.rs:317-319) wraps that error in `Finding::new("yaml", ...)`, `Finding::new` sets `fatal: true` (finding.rs:53-62), and the caller (load.rs:150-157) does `continue`, so the unit never reaches `units`. PyYAML `safe_load` accepts 

### #10 [major] Loader hides symlinked and dot-prefixed unit files that 1.0 loads; no fixture covers either

File: `crates/core/src/curriculum/load.rs:293` — IDs: R5, C5, D1 — duplicate of #2

**Claim.** `unit_file_names` rejects any directory entry that is not a regular file and any name starting with `.`, but 1.0 uses `sorted(course_dir.glob("*.yaml"))`, which follows symlinks and returns dotfiles, so the port silently builds a smaller graph than the oracle for the same tree.

**Evidence.**

```
crates/core/src/curriculum/load.rs:293-301
        if !entry.file_type()?.is_file() {
            continue;
        }
        ...
        if name.starts_with('.') || !name.ends_with(UNIT_EXTENSION) {
            continue;
        }

1.0 behavior, measured:
$ python -c "from pathlib import Path; print(sorted(p.name for p in Path(d).glob('*.yaml')))"
['.hidden.yaml', 'normal.yaml', 'sub.yaml']

Tree whose only unit file is a symlink (c/00-mounted.yaml -> ../real-unit.yaml, one topic `a`):
$ python scripts/oracle/dump_lint_1_0.py $D
[]
$ target/debug/lint_curriculum $D
FAIL: 1 curriculum finding(s) in .../demo-symlink:
  [empty_course] course c has no unit files

$ target/debug/dump_curriculum $D | jq .counts
{'topics': 0, 'knowledge_points': 0, 'exemplars': 0, 'units': 0, ...}
$ python scripts/oracle/dump_curriculum_1_0.py $D | jq .counts
{'topics': 1, 'knowledge_points': 1, 'exemplars': 1, 'units': 1, ...}

The same divergence reproduces with a dot-prefixed regular file `c/.00-hidden.yaml`.

No fixture under crates/core/tests/fixtures/ contains a symlink or a dot-prefixed `.yaml`; `unit_files_load_in_code_point_order_and_not_recursively` (crates/core/tests/loader.rs:378) exercises only `.yml` and a subdirectory, which the spec does name (traps 2 and 3). Dotfiles and symlinks are named nowhere in docs/reference/curriculum-1.0-spec.md.
```

**Failure scenario.** Mount `curriculum/` from a Kubernetes ConfigMap or any volume that materializes entries as symlinks (the standard `..data/<file>` pattern), or check the tree out with a tool that symlinks unit files. 1.0 loads all 88 unit files and 1090 topics; the Rust loader sees zero regular files, reports `empty_course` for all 13 courses, and `load_curriculum` returns an arena with 0 topics. The full test suite stays green because every fixture and `curriculum/` itself hold only plain regular files, so the parity test compares two dumps of the one shape the bug cannot reach.

**Refuter.** The claim is demonstrable, and I reproduced both halves of it with the shipped binaries and the R5 oracle.

1. The 1.0 source is exactly as quoted. /home/deploy/dev/cadus/cadus/graph.py:593 reads `unit_files = sorted(course_dir.glob("*.yaml"))`. `pathlib.Path.glob` does not special-case hidden names (unlike the stdlib `glob` module) and does not filter by file type, so it returns dot-prefixed names, symlinks, and directories.

2. The port applies two extra filters that 1.0 does not have. /home/deploy/dev/cadus2.0/crates/core/src/curriculum/load.rs:293 rejects every entry whose `file_type()` is not a regular file. `fs::read_dir` returns the `lstat`-style type, so a symlink to a unit file has `is_symlink() == true` and `is_file() == false`, and the loader drops it. Line 299 then rejects every name that starts with `.`.

3. Neither filter has a parity justification. The doc comment at load.

### #14 [major] Unit files whose name starts with a dot, and symlinked unit files, are skipped with no finding

File: `crates/core/src/curriculum/load.rs:299` — IDs: C5, D1 — duplicate of #2

**Claim.** unit_file_names filters out directory entries whose name begins with '.' and entries that are not regular files, but 1.0's `sorted(course_dir.glob("*.yaml"))` matches dotfiles and follows symlinks, so the port loses whole unit files silently and also shifts every load_index.

**Evidence.**

```
crates/core/src/curriculum/load.rs:293-301 —
        if !entry.file_type()?.is_file() { continue; }
        ...
        if name.starts_with('.') || !name.ends_with(UNIT_EXTENSION) { continue; }
Demonstrated on two trees, each with `c/00.yaml` (topic `a`) plus one extra unit file holding topic `h`:
  extra file named `c/.hidden.yaml` : rust -> OK topics=1 units=1 fatal_findings=0 ; 1.0 -> OK topics ['a','h'] units 2
  extra file `c/01-linked.yaml` -> symlink to a real unit file : rust -> OK topics=1 units=1 fatal_findings=0 ; 1.0 -> OK topics ['a','h'] units 2
The doc comment above the function justifies only traps 2 and 3 ("a subdirectory and a `.yml` file stay invisible"); neither the dot rule nor the is_file() rule appears in the spec, in docs/DECISIONS.md, or in any test — crates/core/tests/fixtures/file-order covers `.yml`, `nested/`, and `Z-upper.yaml` but no dot-prefixed or symlinked file.
```

**Failure scenario.** A course directory contains `c/.01-draft.yaml`, or a unit file kept as a symlink into a shared tree. 1.0's lint gate and loader see its topics; the port loads zero of them and reports zero findings, so `lint_curriculum` prints "OK: ... (0 findings)" on a tree that is missing ~12 topics. Because a dot sorts before every digit, such a file also changes the load index of every topic in that course, which moves `topics[].load_index`, `topo_order`, and therefore `curriculum_hash`.

**Refuter.** The claim is demonstrable, and I reproduced both halves of it against the 1.0 oracle.

1. **The 1.0 behavior is as the reviewer states.** `/home/deploy/dev/cadus/cadus/graph.py:593` is `unit_files = sorted(course_dir.glob("*.yaml"))`. `pathlib.Path.glob` is not the `glob` module: it does not special-case a leading dot, and it does not filter by file type. A direct probe with the 1.0 interpreter returned `['.hidden.yaml', 'a.yaml', 'link.yaml']`, so 1.0 reads dot-prefixed unit files and symlinked unit files.

2. **The port drops both.** `/home/deploy/dev/cadus2.0/crates/core/src/curriculum/load.rs:293` rejects every non-regular entry, and line 299 rejects every name that starts with `.`. Neither rule has a counterpart in 1.0.

3. **The divergence is real, not theoretical.** On two trees built from `crates/core/tests/fixtures/file-order`, the Rust dump and the 1.0 dump disagree on unit cou

### #15 [major] An integer written as a whole-number float passes the schema walk and then fails serde, producing a schema finding with an empty location and dropping the whole file

File: `crates/core/src/curriculum/load.rs:747` — IDs: C5, R5 — duplicate of #4

**Claim.** as_i64 deliberately accepts a float with no fractional part, but the model fields are `i64`, so `expected_time_secs: 30.0` and `order: 1.0` clear the Checker and then hit the residual-error branch, which emits a `schema` finding whose message has no dotted location and drops the file (or the entire catalog) — while 1.0 accepts both with zero findings.

**Evidence.**

```
crates/core/src/curriculum/load.rs:744-749 —
    let float = number.as_f64()?;
    if float.fract() == 0.0 && float >= i64::MIN as f64 && float <= i64::MAX as f64 {
        return Some(float as i64);
    }
but crates/core/src/curriculum/model.rs:189 declares `pub expected_time_secs: i64` and model.rs:223 `pub order: i64`, so load.rs:363-364 fires:
    T::deserialize(document.clone())
        .map_err(|error| vec![Finding::new("schema", format!(": {error}")).with_file(file)])
Measured, one topic with `expected_time_secs: 30.0`:
  1.0  : (no findings)
  rust : {"code":"schema","fatal":true,"file":"c/00.yaml","message":": invalid type: floating point `30.0`, expected i64"}
and with `order: 1.0` in courses.yaml:
  1.0  : (no findings)
  rust : {"code":"schema","fatal":true,"file":"courses.yaml","message":": invalid type: floating point `1.0`, expected i64"}
For a value 1.0 also rejects (`expected_time_secs: 3.0e1`) the two messages still disagree: 1.0 says `topics.0.expected_time_secs: Input should be a valid integer, unable to parse string as an integer`, the port says `: invalid type: floating point ...`.
```

**Failure scenario.** An author writes `expected_time_secs: 30.0` (or a generator emits `1.0` for a course `order`). 1.0 loads the tree clean. The port drops the whole unit file — every topic in it disappears from the graph — and the finding it prints is `[schema] : invalid type: floating point \`30.0\`, expected i64`, with no field path, so the author cannot tell which key is wrong. Spec §5 rule 2 fixes the shape of a `schema` message at `{loc}: {msg}` with a dotted path. For `order: 1.0` the catalog becomes None, so lint_curriculum returns that single finding and reports nothing else about the tree at all.

**Refuter.** The claim is demonstrable and reproduces exactly as described. `as_i64` (crates/core/src/curriculum/load.rs:739-750) deliberately accepts a YAML float with no fractional part, and both `check_int` (line 523) and `check_positive_int` (line 531) use it. The Checker therefore records nothing for `expected_time_secs: 30.0` and `order: 1.0`. But `Topic.expected_time_secs` (crates/core/src/curriculum/model.rs:189) and `Course.order` (model.rs:223) are `i64`, and serde_yaml does not coerce a float into an `i64`. The residual-error branch in `validate` (load.rs:363-364) then fires and makes one `schema` finding with the message `format!(": {error}")` — an empty dotted location. This breaks spec section 5, rule 2, which fixes a `schema` message at `{loc}: {msg}` with a dotted path, and it breaks parity with 1.0, which accepts both values and reports zero schema findings. The failure is a real div

### #16 [major] The schema walk rejects the scalar coercions 1.0 pydantic performs, and reuses pydantic's message text for inputs pydantic accepts

File: `crates/core/src/curriculum/load.rs:506` — IDs: C5, R5

**Claim.** check_unit_interval, check_bool, check_int and check_positive_int require an exact YAML scalar type, but 1.0 validates in pydantic lax mode, which coerces str->float, str->int, str->bool and int->bool; the port therefore turns files 1.0 loads cleanly into fatal schema findings, and prints pydantic's "unable to parse" wording for values pydantic parses fine.

**Evidence.**

```
crates/core/src/curriculum/load.rs:506-521 (check_unit_interval) and 494-504 (check_bool) accept only Value::Number / Value::Bool. Measured pairs (1.0 first, port second), each a one-topic tree:
  weight: "0.5"            1.0: (none)  port: weight_out_of_range? no — {"code":"schema",...,"message":"topics.1.prerequisites.0.weight: Input should be a valid number, unable to parse string as a number"}
  difficulty: "0.3"        1.0: (none)  port: "topics.0.difficulty: Input should be a valid number, unable to parse string as a number"
  expected_time_secs: "30" 1.0: (none)  port: "topics.0.expected_time_secs: Input should be a valid integer, unable to parse string as an integer"
  core: 1                  1.0: (none)  port: "topics.0.core: Input should be a valid boolean"
  core: "true"             1.0: (none)  port: "topics.0.core: Input should be a valid boolean, unable to interpret input"
  expected_time_secs: 030  1.0: (none, PyYAML 1.1 octal -> 24)  port: "topics.0.expected_time_secs: Input should be a valid integer, unable to parse string as an integer"
The one comparable divergence that IS pinned — YAML 1.1 `core: yes` — carries a written justification and a guard test (crates/core/tests/loader.rs:403-437). These scalar coercions have no counterpart: nothing in docs/reference/curriculum-1.0-spec.md §7, docs/plans/M1.md, or docs/DECISIONS.md records them.
```

**Failure scenario.** An author quotes a number in a curriculum file — `weight: "0.5"` or `expected_time_secs: "30"` — or writes a zero-padded `030`. The 1.0 lint gate passes the file; the port emits a fatal `schema` finding, `lint_curriculum` exits 1, and the loader drops every topic of that file. The message tells the author the value cannot be parsed as a number when it plainly can be, so the reported cause is wrong as well as the verdict.

**Refuter.** The claim is correct and fully demonstrable. I reproduced all six measured pairs end to end.

1.0 side. `/home/deploy/dev/cadus/cadus/model.py:26-29` sets only `ConfigDict(extra="forbid")`. No model, and no field, sets `strict=True`. Pydantic v2 therefore validates the curriculum models in lax mode. Direct `model_validate` calls on the PyYAML output accept every value: `weight: "0.5"` -> 0.5, `difficulty: "0.3"` -> 0.3, `expected_time_secs: "30"` -> 30, `core: 1` -> True, `core: "true"` -> True, `expected_time_secs: 030` -> 24 (PyYAML 1.1 octal).

Port side. `crates/core/src/curriculum/load.rs:494-504` (`check_bool`) matches only `Value::Bool`, and `as_f64` (:731-736) and `as_i64` (:739-751) return `None` for anything that is not `Value::Number`. `check_unit_interval` (:506-521), `check_int` (:523-528) and `check_positive_int` (:530-540) therefore reject every string scalar and every non

### #17 [major] A YAML 1.1 plain scalar in a string field is read as a string, so the port loads and hashes a tree 1.0 refuses

File: `crates/core/src/curriculum/load.rs:317` — IDs: R5, C5, D1

**Claim.** serde_norway resolves the YAML 1.2 core schema, so `no`, `yes`, `on`, `off` and `2020-01-01` stay strings and pass `check_string`, while PyYAML resolves them to bool/date and 1.0 pydantic rejects them with a fatal `schema` finding.

**Evidence.**

```
Fixture `demo/01.yaml` holds `diagnostic_exemplar: {problem: q, answer: no}`.

Rust: `target/debug/lint_curriculum <dir>` ->
  `OK: <dir> is a valid curriculum (0 findings).`  (exit 0)
1.0: `dump_lint_1_0.py <dir>` ->
  `[{"code": "schema", "fatal": true, "file": "demo/01.yaml", "message": "topics.0.diagnostic_exemplar.answer: Input should be a valid string"}]`
The Rust `dump_curriculum` writes a full dump with `sha256=b9757ec9bd6edff1ca2c0bda21d73efd2df78a87bb6f178495cd2aaff8686994`; the 1.0 oracle raises `CurriculumError: [schema] topics.0.diagnostic_exemplar.answer: Input should be a valid string` and writes nothing.
The same holds for `name: 2020-01-01` (Rust dump sha256=da2c1a30...; 1.0 raises `topics.0.name: Input should be a valid string`).
load.rs:317 `let value: Value = serde_norway::from_str(&text)` is the resolver boundary. The only test that covers the 1.1/1.2 gap is `crates/core/tests/loader.rs:403 a_yaml_1_1_boolean_yes_is_a_schema_error`, which covers the opposite direction (a 1.1 bool in a bool field); its guard `the_checked_in_tree_writes_no_yaml_1_1_boolean` only counts `core`/`drill`, so it cannot see a 1.1 scalar in any `str` field.
```

**Failure scenario.** An author writes an exemplar answer as the bare word `no` (a legal answer for a proof or yes/no topic), or a unit `name` that looks like a date. 1.0 rejects the file loudly; the Rust loader accepts it, the lint prints `OK ... (0 findings)`, and the curriculum hash of the tree is one 1.0 can never produce. R5 parity is broken in the silent direction: CI is green on content 1.0 will not load.

**Refuter.** The claim is demonstrable, and I reproduced it end to end with the checked-in binaries and the live 1.0 oracle. `serde_norway` at crates/core/src/curriculum/load.rs:317 resolves the YAML 1.2 core schema, so the 1.1 bool words `yes`, `no`, `on`, `off`, `Off` and an ISO date `2020-01-01` stay `Value::String` and pass `check_string` (load.rs:477). PyYAML `safe_load` in 1.0 resolves the same scalars to bool/date, and pydantic then raises a fatal `schema` finding. The divergence runs in the silent direction: Rust prints `OK ... (0 findings)` with exit 0 and `dump_curriculum` writes a full graph plus a sha256, while 1.0 raises `CurriculumError` and writes nothing. This breaks R5 ("Divergence is a bug in 2.0 until proven otherwise") and reaches C5/D1, because the accepted file gives a curriculum hash 1.0 can never produce. The reviewer's account of the test coverage is also correct: the only te

### #18 [major] YAML 1.1 integer forms (octal, underscore, sexagesimal) drop the whole file, or the whole catalog

File: `crates/core/src/curriculum/load.rs:739` — IDs: R5, C5

**Claim.** `060`, `1_200` and `1:30` are integers to PyYAML but strings to serde_norway, so `as_i64` returns `None` and the port reports `Input should be a valid integer, unable to parse string as an integer` and drops the file, while 1.0 loads 48, 1200 and 90.

**Evidence.**

```
Fixture with `expected_time_secs: 060` on topic `a` and `expected_time_secs: 1_200` on topic `b`.

Rust `lint_curriculum` ->
  `FAIL: 2 curriculum finding(s) in <dir>:`
  `  [schema] topics.0.expected_time_secs: Input should be a valid integer, unable to parse string as an integer`
  `  [schema] topics.1.expected_time_secs: Input should be a valid integer, unable to parse string as an integer`
1.0 oracle dump -> `[('a', 48), ('b', 1200)]`, `dump_lint_1_0.py` -> `[]`.

`expected_time_secs: 1:30` gives the same Rust finding; the 1.0 dump gives 90.
With `order: 010` in `courses.yaml`, Rust reports `[schema] courses.0.order: Input should be a valid integer, unable to parse string as an integer` and returns `catalog: None`, so NO course is read at all; 1.0 reads `order: 8` and loads the whole tree.
An integer past i64 escalates further: `expected_time_secs: 99999999999999999999` yields the wrong code entirely — `[yaml] demo/01.yaml: topics[0].expected_time_secs: invalid type: integer ... as u128, expected any YAML value at line 9 column 25` — while 1.0 loads it.
```

**Failure scenario.** An author writes `expected_time_secs: 090` (a leading zero for column alignment) or `1_800` for readability. 1.0 stores 72 and 1800; the port drops the entire unit file, and because `load_curriculum` does not block on a fatal parse finding the arena silently loses every topic of that file. The same literal in `courses.yaml` `order` empties the catalog and the tree loads as zero courses.

**Refuter.** The claim is demonstrable. I reproduced every part of it with a fresh fixture, the real `lint_curriculum` bin, and the 1.0 oracle.

Root cause: `serde_norway` reads YAML 1.2, and PyYAML reads YAML 1.1. `060`, `1_200` and `1:30` are `Value::String` in Rust and `int` in Python. `as_i64` (`/home/deploy/dev/cadus2.0/crates/core/src/curriculum/load.rs:739`) matches only `Value::Number`, so it returns `None`. `int_message` (`:751`) then selects the `Value::String` arm and prints the pydantic string message. The file goes to a `schema` finding and never enters `units`.

Three points make the refutation fail:

1. The project pinned YAML 1.2 for booleans on purpose. `crates/core/tests/loader.rs:402-417` states the pin, and `:419-436` guards it with a test that counts `core`/`drill` in the checked-in tree. No equivalent pin and no equivalent guard exist for integers. The pin covers `yes`/`no` only

### #19 [major] A YAML merge key is not flattened, so an anchored unit file is dropped with a bogus '<<' extra-key finding

File: `crates/core/src/curriculum/load.rs:466` — IDs: R5, C5

**Claim.** PyYAML's SafeConstructor flattens `<<: *anchor` into the mapping; serde_norway leaves `<<` as a literal key, so `Checker::extras` reports `Extra inputs are not permitted` for `<<` and every merged field is reported missing.

**Evidence.**

```
Fixture `demo/01.yaml`: topic `a` carries `&base`, topic `b` is `- <<: *base` plus `id: b` and `name: B`.

Rust `lint_curriculum` ->
  `FAIL: 4 curriculum finding(s) in <dir>:`
  `  [schema] topics.1.difficulty: Field required`
  `  [schema] topics.1.answer_kind: Field required`
  `  [schema] topics.1.expected_time_secs: Field required`
  `  [schema] topics.1.<<: Extra inputs are not permitted`
1.0 oracle dump -> `['a', 'b']` (2 topics, sha256=5fea6b31b8b09487a257c9f6444086636e29510e075d33faa195860a8737d304); `dump_lint_1_0.py` -> `[]`.
load.rs:466 `fn extras(&mut self, map: &Mapping, known: &[&str])` walks the raw mapping keys, so it sees `<<`.
```

**Failure scenario.** An author uses the standard YAML merge idiom to share a topic skeleton across a unit file (a natural move for 88 hand-written files). 1.0 loads every topic; the port drops the whole file and reports three phantom 'Field required' errors plus an error on a key the author never invented. C5 content review is misled about what is wrong.

**Refuter.** The claim is demonstrable. I reproduced it exactly with a fresh fixture, and I could not find a defense for the port.

What I did a check of:

1. Merge-key handling in the port. `crates/core/src/curriculum/load.rs` never calls `Value::apply_merge()`. A grep for `merge` over `crates/core/src/curriculum/**` returns no hit. `read_document` (load.rs:313-325) calls `serde_norway::from_str` and returns the raw `Value` unchanged. `serde_norway` resolves plain aliases, but it keeps `<<` as a literal mapping key until `apply_merge()` runs. `Checker::extras` (load.rs:466) then walks the raw keys and reports `<<` as an extra input, and `Checker::field` reports every merged field as missing.

2. PyYAML behavior. `yaml.safe_load` flattens the merge and returns two complete topic mappings.

3. Rust output against 1.0 output on the same tree. The two disagree. 1.0 loads both topics. The port drops the 

### #20 [major] A UTF-8 BOM splits the document, so every BOM-prefixed unit file is dropped

File: `crates/core/src/curriculum/load.rs:314` — IDs: R5, C5

**Claim.** `fs::read_to_string` keeps the leading U+FEFF, which shifts the first key to column 3, so libyaml ends the document at the second top-level key and serde_norway reports 'more than one document'; Python's `open(encoding="utf-8")` plus PyYAML reads the same file with no complaint.

**Evidence.**

```
`demo/01.yaml` = `EF BB BF` then `unit: U\ncourse: demo\nmodule: M\ntopics: []\n` (verified with `od -c`).

Rust `lint_curriculum` ->
  `FAIL: 1 curriculum finding(s) in <dir>:`
  `  [yaml] demo/01.yaml: deserializing from YAML containing more than one document is not supported`
1.0 `dump_lint_1_0.py` -> `[]`; the 1.0 dump loads the file (`"units":1`).

The trigger is any BOM file with more than one top-level key:
  `[bom, unit only]  [schema] module: Field required`   (one key: parses)
  `[bom, unit+module]  [yaml] demo/01.yaml: deserializing from YAML containing more than one document is not supported`
  `[bom, zz+yy]  [yaml] demo/01.yaml: deserializing from YAML containing more than one document is not supported`
Every real unit file has four top-level keys, so every BOM-prefixed unit file hits this. load.rs:314 `let text = fs::read_to_string(path)` never strips the BOM.
```

**Failure scenario.** A content author edits one unit file on Windows or in an editor that writes a BOM. 1.0 reads it and the topics stay in the graph. The port drops the file, and because `load_curriculum` does not block on a fatal parse finding the arena comes up with those topics missing; the reported reason ('more than one document') names neither the BOM nor anything the author can see in the file.

**Refuter.** The claim is demonstrable. I reproduced every step. `read_document` at /home/deploy/dev/cadus2.0/crates/core/src/curriculum/load.rs:314 calls `fs::read_to_string`, which keeps the U+FEFF, and no code in the repo strips it (a grep for "bom", "feff", and "utf-8-sig" over the tree returns no hit). `serde_norway::from_str` then rejects the text with "deserializing from YAML containing more than one document is not supported" for any BOM file with two or more top-level keys. The 1.0 oracle accepts the same file: PyYAML strips the leading U+FEFF in its reader, so `open(encoding="utf-8")` plus `yaml.safe_load` returns the mapping. This is an R5 divergence (1.0 is the oracle; a difference is a 2.0 bug) and a C5 defect (a linted content file that 1.0 accepts is refused by 2.0 with a message that names neither the BOM nor a visible position). The spec does not exempt a BOM — docs/reference/curricu

### #21 [major] An integer literal outside the i64/u64 range makes the loader reject a file 1.0 loads cleanly

File: `crates/core/src/curriculum/load.rs:317` — IDs: D1, C5, R5

**Claim.** `read_document` turns any YAML integer literal that does not fit `i64`/`u64` into a fatal `yaml` (malformed-file) finding, so the whole unit file — or, in `courses.yaml`, the whole catalog — is dropped, while 1.0 parses the same tree with zero findings.

**Evidence.**

```
crates/core/src/curriculum/load.rs:317
    let value: Value = serde_norway::from_str(&text).map_err(|error| {
        Box::new(Finding::new("yaml", format!("{rel}: {error}")).with_file(rel))
    })?;

`serde_norway::Value::Number` holds only i64/u64/f64, so the document never parses and the error is reported with the 1.0 `yaml` code, which is `fatal`.

Fixture: a one-course, one-topic tree whose only unusual value is `demo/01.yaml:9  expected_time_secs: 18446744073709551616`.

  1.0 lint  -> []
  1.0 oracle dump (scripts/oracle/dump_curriculum_1_0.py) -> exit=0

  rust lint -> [{"code":"yaml","fatal":true,"file":"demo/01.yaml","message":"demo/01.yaml: topics[0].expected_time_secs: invalid type: integer `18446744073709551616` as u128, expected any YAML value at line 9 column 25"}]
  rust dump_curriculum -> exit=2
     dump_curriculum: 1 fatal finding(s):
       [yaml] demo/01.yaml: topics[0].expected_time_secs: invalid type: integer `18446744073709551616` as u128, expected any YAML value at line 9 column 25

Same tree with the value moved to the catalog (`courses.yaml: order: 18446744073709551616`):
  1.0  -> topics: 1, order: 18446744073709551616
  rust -> `catalog: None`, arena topic count 0, finding `courses.yaml: courses[0].order: invalid type: integer `18446744073709551616` as u128, ...`

A negative literal below i64::MIN behaves the same (`... as i128`).

The neighboring case that stays inside u64 (`expected_time_secs: 18446744073709551615`) takes the schema-walk path instead and yields a `schema` finding, so the two ranges even report different codes for the same class of value.
```

**Failure scenario.** An author writes an integer with 20 or more digits anywhere in a YAML file — `expected_time_secs: 18446744073709551616`, or an unquoted large numeric answer/name. 1.0 loads the tree and `lint_curriculum` reports 0 findings. The port instead emits a fatal `yaml` finding whose message names a Rust type (`invalid type: integer ... as u128`), a shape no 1.0 message ever has, and drops the entire unit file: every topic in it disappears from the arena, `dump_curriculum` exits 2, and the R5 parity diff cannot run at all. When the literal is in `courses.yaml`, `parse_curriculum` returns `catalog: None`, so `load_curriculum` hands back an arena with 0 topics and no error — a library caller that does not inspect the findings serves an empty curriculum where 1.0 serves the full one.

**Refuter.** The claim is correct and reproducible. `read_document` (crates/core/src/curriculum/load.rs:317) deserializes into `serde_norway::Value`, whose `Number` holds only i64/u64/f64. A YAML integer literal outside that range makes `serde_norway::from_str` fail, so the whole document becomes one fatal `yaml` finding and the file is dropped. Python 1.0 uses `yaml.safe_load`, and a Python `int` has arbitrary precision, so 1.0 loads the same tree.

I built the fixture and ran both sides. Result for `expected_time_secs: 18446744073709551616` in a unit file: 1.0 lint returns `[]` and the 1.0 oracle dump exits 0 with `expected_time_secs 18446744073709551616`; the Rust `dump_curriculum` exits 2 and `lint_curriculum` exits 1 with one fatal `yaml` finding. Result for the same literal at `courses.yaml: order`: 1.0 dumps 1 topic and `order 18446744073709551616`; Rust exits 2. `parse_curriculum` (load.rs:11

### #26 [major] Spec section 2 and trap 8 state the wrong weight-0 edge count and invent a duplicated edge pair

File: `docs/reference/curriculum-1.0-spec.md:112` — IDs: C5, R5

**Claim.** The spec says the tree holds 81 weight-0 encompassing edges and 1 duplicated (src, dst) pair; the tree holds 82 weight-0 edges and no duplicated pair, so trap 8 sends a later port to a rule that no real data exercises.

**Evidence.**

```
docs/reference/curriculum-1.0-spec.md:112 -- "Counts: 3200 forward, 3282 reverse, from 3282 declared edges (81 weight-0 edges, 1 duplicated pair)." and :211 -- "8. `_enc` / `_enc_rev` asymmetry for weight-0 edges (81 real edges)."  A load of the tree with the 1.0 code prints: `zero weight edges 82`, `total edges 3282 distinct pairs 3282`, `enc fwd 3200 enc_rev 3282`.  The shipped test says the opposite of the spec -- crates/core/tests/arena.rs:86: "forward encompassing entries: 3282 declared edges less the 82 that stay at weight 0".
```

**Failure scenario.** An M2/M3 engineer reads trap 8 and writes the arena assertion the spec dictates: 3282 declared edges minus 81 weight-0 edges minus 1 duplicate. The count of weight-0 edges is 82 and the count of duplicated pairs is 0, so the assertion fails on the checked-in tree. The engineer also believes the max-keeps-the-larger branch of `_add_enc` (`cadus/graph.py:314-321`) is exercised by real content and pins it with a tree-level test; that branch never runs on the tree, because all 3282 declared pairs are distinct.

**Refuter.** The claim is correct and demonstrable. A 1.0 load of the checked-in tree at /home/deploy/dev/cadus2.0/curriculum gives 82 weight-0 declared edges and 0 duplicated (src, dst) pairs. The spec at docs/reference/curriculum-1.0-spec.md:112 states 81 weight-0 edges and 1 duplicated pair. Both sub-counts are wrong.

The spec sentence is also self-inconsistent. `_enc_rev` holds one entry per distinct (src, dst) pair. If 1 pair were duplicated, the count of distinct pairs is 3281, and the same sentence cannot also say "3282 reverse". The measured value is 3282, which is only possible if all 3282 declared pairs are distinct. The totals 3200 / 3282 stay correct; only the parenthetical decomposition is wrong.

Trap 8 at docs/reference/curriculum-1.0-spec.md:211 repeats the wrong number ("81 real edges"). The shipped Rust test contradicts the spec: crates/core/tests/arena.rs:86 says "3282 declared ed

### #27 [major] A whole-number bool value gets a lint message that 1.0 does not write

File: `crates/core/src/curriculum/load.rs:496` — IDs: R5, C5

**Claim.** `check_bool` appends "unable to interpret input" only for a YAML string, but 1.0 pydantic appends it for an integer too, so `core: 2` and `drill: 3` produce a different `schema` message in the two implementations.

**Evidence.**

```
crates/core/src/curriculum/load.rs:494-503 -- `let message = if matches!(value, Value::String(_)) { "Input should be a valid boolean, unable to interpret input" } else { "Input should be a valid boolean" };`  Same tree, `core: -1` in the only topic:
  1.0  : [schema] topics.0.core: Input should be a valid boolean, unable to interpret input
  rust : [schema] topics.0.core: Input should be a valid boolean
The same split appears for `core: 2` and for `drill: 3`; `core: []`, `core: {}`, `core: 0.5` and `core: null` agree.
```

**Failure scenario.** An author writes `core: 2` in a unit file. The 1.0 gate prints `topics.0.core: Input should be a valid boolean, unable to interpret input`; the Rust `lint_curriculum` binary prints `topics.0.core: Input should be a valid boolean`. A lint fixture built for that input fails the byte-for-byte oracle comparison of crates/core/tests/lint.rs, and R5 counts the divergence as a 2.0 bug. This is a message-text split on an input that both implementations reject, and not the already-recorded case where the Rust walk rejects a scalar that pydantic coerces.

**Refuter.** The claim is demonstrable end to end and I could not refute it. `check_bool` (/home/deploy/dev/cadus2.0/crates/core/src/curriculum/load.rs:494-503) appends ", unable to interpret input" only when the YAML value is `Value::String(_)`. pydantic 2.13.4 selects the same tail by error type, not by Python type: it raises `bool_parsing` (with the tail) for `str` AND for any `int` outside {0, 1}, and `bool_type` (no tail) for `float`, `list`, `dict` and `None`. A YAML integer therefore takes the wrong branch in Rust. I reproduced the split on a real tree with both binaries. No pinned-deviation record covers it; the only bool deviation on record is the YAML 1.1 `yes` case at crates/core/tests/loader.rs:404, which is a different input class. Under R5 ("Divergence is a bug in 2.0 until proven otherwise") the divergence counts as a 2.0 defect. The fix is to key the tail on "string OR integer not in 

## FIXM1b

### #3 [major] The canonical dump formats floats with ryu, not with the Python repr rule, so the dump and the curriculum hash diverge for any legal value below 1e-4

File: `crates/core/src/curriculum/dump.rs:421` — IDs: R5, D1, C5

**Claim.** `float()` hands the value to `serde_json`, whose ryu formatter chooses a different exponent threshold and writes an unpadded exponent, while the oracle uses `json.dumps`, which writes the Python `repr` with a two-digit exponent; the two dumps differ for `difficulty` and `weight` values that sit inside the legal 0.0..=1.0 range.

**Evidence.**

```
crates/core/src/curriculum/dump.rs:13-14 (module comment)
//! - Floats take the shortest form that round-trips, the same rule as the
//!   Python `repr` of a float (parity trap 16).
crates/core/src/curriculum/dump.rs:421-423
fn float(value: f64) -> Value {
    Number::from_f64(value).map_or(Value::Null, Value::Number)
}

Fixture: topic `a` difficulty 0.00001, topic `b` difficulty 0.0000001234, edge `b -> a` weight 0.000015. Both loaders exit 0.

$ ./target/debug/dump_curriculum FIXTURE | grep -o '"difficulty":[^,]*'
"difficulty":0.00001
"difficulty":1.234e-7
$ .venv/bin/python scripts/oracle/dump_curriculum_1_0.py FIXTURE | grep -o '"difficulty":[^,]*'
"difficulty":1e-05
"difficulty":1.234e-07

$ ./target/debug/dump_curriculum FIXTURE | grep -o '"encompassing_edges":\[\[[^]]*\]\]'
"encompassing_edges":[["b","a",0.000015]]
$ .venv/bin/python scripts/oracle/dump_curriculum_1_0.py FIXTURE | grep -o '"encompassing_edges":\[\[[^]]*\]\]'
"encompassing_edges":[["b","a",1.5e-05]]

$ sha256sum rust.json python.json
b5bfe2fe5ca6bef5bc1c191608d0e5639713b6ea56d40fd2a46ff58397d028e3  rust.json
614ae80f8bff181ade50c38ca935274ce09fcce785ed981f09e5bdf37611852d  python.json

The same formatting gap is live inside the checked-in tree's own weights. A probe over `curriculum/` prints 229,894 reach-weight and upward-weight pairs, and Rust and Python agree on every bit (`float.hex()` comparison: identical), but their text differs, for example `6.300000000000001e-5` against `6.300000000000001e-05`. Only the dump's own values happen to stay above the threshold today, which is why crates/core/tests/parity.rs (which pins one tree hash and one length) stays green.
```

**Failure scenario.** An author gives one topic `difficulty: 0.00001` — a value spec section 1 allows, because the rule is `0.0..=1.0`. `dump_curriculum` and `scripts/oracle/dump_curriculum_1_0.py` both exit 0 and both report success, but the two dumps differ byte for byte and the two curriculum hashes differ (`b5bfe2f...` against `614ae80...`). R5 says a divergence is a bug in 2.0, and D1 pins the semantic hash to the canonical dump, so every consumer that binds content to the hash sees a different digest for the same curriculum depending on which implementation computed it. The parity test does not catch this, because it only checks the one checked-in tree, whose values all fall above the point where the two formatters agree.

**Refuter.** I tried to refute the claim and failed. The defect reproduces on a lint-clean fixture with values that the spec allows.

The module comment at /home/deploy/dev/cadus2.0/crates/core/src/curriculum/dump.rs:13-14 states the parity rule: "Floats take the shortest form that round-trips, the same rule as the Python `repr` of a float (parity trap 16)." The code at dump.rs:421-423 does not implement that rule. `float()` builds a `serde_json::Number`, and `serde_json` formats it with ryu. Ryu and the Python `repr` differ in two ways:

1. Exponent threshold. Python `repr` moves to exponential form below 1e-4, so it writes `1e-05` and `1.5e-05`. Ryu keeps the plain decimal form, so the dump writes `0.00001` and `0.000015`.
2. Exponent padding. Python pads the exponent to two digits (`1.234e-07`). Ryu writes an unpadded exponent (`1.234e-7`).

I built the fixture the reviewer describes, then added k

### #7 [major] A repeated course id resolves to the first catalog entry; 1.0 resolves to the last

File: `crates/core/src/curriculum/arena.rs:291` — IDs: D1, C5, R5

**Claim.** Curriculum::build indexes the catalog with `.or_insert(position)`, keeping the FIRST entry for a repeated course id, while 1.0 builds `{c.id: c for c in catalog.courses}`, which keeps the LAST; `Curriculum::course()` and `Curriculum::mastery_floor()` therefore return a different course and a different mastery floor than 1.0 for the same file.

**Evidence.**

```
arena.rs:287-292 `for (position, course) in catalog.courses.iter().enumerate() { course_by_id.entry(course.id.as_str().to_owned()).or_insert(position); }` vs `cadus/graph.py:236` `self.course_by_id: dict[str, Course] = {c.id: c for c in catalog.courses}`. The port's own lint module states the correct rule and implements it the other way: lint.rs:384-389 `// 1.0 writes {c.id: c for c in catalog.courses}, so a repeated course id keeps the LAST entry.` followed by `course_by_id.insert(...)`. Demonstrated on a catalog with `c(order 1)`, `d(order 2)`, `c(order 3, mastery_floor_course: d)`: 1.0 prints `course_by_id[c] -> C-second order 3` and `mastery_floor(c) = ['t-d']`; the port prints `course(c) -> order=1 name=C-first` and `mastery_floor(c) = []`. Course-id uniqueness is not a lint rule (spec section 5, "Not enforced by lint"), so nothing reports the duplicate.
```

**Failure scenario.** courses.yaml gains a duplicated course id (a merge conflict resolved by keeping both blocks, or a course re-declared to change its `order`/floor). 1.0 gives every learner enrolled in that course the floor of the later entry; 2.0 gives them the earlier entry's floor — here the empty set instead of `{t-d}`. PEDAGOGY 1 mastery is then wrong for every learner in that course, and the arena disagrees with its own lint: `unreachable_from_floor` grounds on the last entry (lint.rs:388) while `Curriculum::mastery_floor` grounds on the first, so lint passes while the served floor is empty.

**Refuter.** The defect is real and I reproduced it on the same tree with both implementations. `Curriculum::build` (crates/core/src/curriculum/arena.rs:287-291) writes `course_by_id.entry(...).or_insert(position)`, so a repeated course id keeps the FIRST catalog entry. 1.0 writes `self.course_by_id = {c.id: c for c in catalog.courses}` (cadus/graph.py:243), which keeps the LAST entry, and `Graph.mastery_floor` reads that same map (cadus/graph.py:401-404, `_mastery_floor_of` at graph.py:201-220). The port's own lint states and implements the 1.0 rule (crates/core/src/curriculum/lint.rs:384-389, `course_by_id.insert(...)` = last wins), so the arena disagrees with the lint that grounds `unreachable_from_floor`. Neither side rejects a repeated course id: the 1.0 model has no uniqueness validator (cadus/model.py:158-171), the 2.0 schema checker has none (load.rs `check_catalog`/`check_course`, load.rs:55

### #9 [major] Arena course_by_id keeps the first duplicate course id; 1.0 keeps the last, so mastery_floor diverges

File: `crates/core/src/curriculum/arena.rs:291` — IDs: R5, D1, C5 — duplicate of #7

**Claim.** Curriculum::build indexes the catalog with `.or_insert(position)`, which keeps the FIRST entry for a repeated course id, while 1.0 `Graph.__init__` builds `{c.id: c for c in catalog.courses}` and keeps the LAST — so `Curriculum::course()` and the PEDAGOGY-1 `Curriculum::mastery_floor()` return a different course row and a different floor than the oracle, silently and with no lint code to catch it.

**Evidence.**

```
crates/core/src/curriculum/arena.rs:287-291
```rust
let mut course_by_id = HashMap::with_capacity(catalog.courses.len());
for (position, course) in catalog.courses.iter().enumerate() {
    course_by_id
        .entry(course.id.as_str().to_owned())
        .or_insert(position);   // FIRST wins
}
```
1.0 oracle, /home/deploy/dev/cadus/cadus/graph.py:243
```python
self.course_by_id: dict[str, Course] = {c.id: c for c in catalog.courses}   # LAST wins
```
The port already knows the right rule elsewhere — crates/core/src/curriculum/lint.rs:384-388:
```rust
// 1.0 writes `{c.id: c for c in catalog.courses}`, so a repeated course
// id keeps the LAST entry.
let mut course_by_id: HashMap<&str, &Course> = HashMap::new();
for course in &catalog.courses {
    course_by_id.insert(course.id.as_str(), course);
```
so arena.rs and lint.rs compute two different mastery floors for the same tree.

Demonstrated on a fixture whose courses.yaml declares `a` (order 1), `b` (order 2, "B first"), `b` again (order 5, "B second", duplicate id, directory absent so no duplicate topic id blocks the build), and `c` (order 3, `mastery_floor_course: b`), with topics a1,a2 in a/ and c1,c2 in c/. Both implementations load the tree with the same two non-fatal `missing_course_dir` findings and the same 4 topics:

```
RUST findings: [("missing_course_dir", "no unit directory b/ for course", false),
                ("missing_course_dir", "no unit directory b/ for course", false)]
RUST topics: ["a1", "a2", "c1", "c2"]
RUST course("b") -> name="B first" order=2
RUST mastery_floor("c") = ["a1", "a2"]
=========== 1.0 ORACLE ===========
PY topics: ['a1', 'a2', 'c1', 'c2']
PY course_by_id['b'] -> name='B second' order=5
PY mastery_floor('c') = ['a1', 'a2', 'c1', 'c2']
```
(release build via `cargo build --release`; oracle via /home/deploy/dev/cadus/.venv/bin/python importing `cadus.graph.Graph.load`.)
```

**Failure scenario.** An author adds a second `courses.yaml` entry that reuses an existing course id — a duplicate-`id` catalog entry is legal content: neither `serde` (`deny_unknown_fields` does not check value uniqueness) nor any of the 16 lint codes flags it, and `docs/reference/curriculum-1.0-spec.md` §5 lists only `order` uniqueness as unenforced, so `lint_curriculum` still exits 0. If that duplicated course has no unit directory (or an empty one), `Curriculum::build` succeeds — no `DuplicateTopicId` — and `Curriculum::course("b")` resolves to the first entry (order 2) instead of the last (order 5). Trap 12 then unions only the courses with `order <= 2`, so `mastery_floor("c")` returns `[a1, a2]` where 1.0 returns `[a1, a2, c1, c2]`. Every topic in the difference is auto-considered mastered by 1.0 and NOT by 2.0, so the M3/M4 scheduler built on this arena will schedule, serve, and grade topics the 1.0 learner never sees — an R5 divergence in the pedagogy core that no M1 test or lint run reports, and that only surfaces later as unexplained scheduling drift against the 1.0 event streams.

**Refuter.** The claim is demonstrable and I reproduced it independently. crates/core/src/curriculum/arena.rs:287-291 indexes the catalog with `.entry(id).or_insert(position)`, so a repeated course id keeps the FIRST entry, while 1.0 `Graph.__init__` (/home/deploy/dev/cadus/cadus/graph.py:243) writes `{c.id: c for c in catalog.courses}` and keeps the LAST. Both `Curriculum::course()` (arena.rs:447) and `Curriculum::mastery_floor()` (arena.rs:463) read that map, so trap 12 unions the wrong set of courses. A duplicate course id is legal, unvalidated input: `Course`/`Catalog` in model.rs:217-236 declare no uniqueness, `Checker::check_course` in load.rs:566-587 checks types only, `grep -rn duplicate_course` over the repo returns nothing, and docs/reference/curriculum-1.0-spec.md line 34 plus lines 189-190 never require course-id uniqueness. `missing_course_dir` is advisory (load.rs:125-130), so the build

### #11 [major] Parity trap 12 (mastery_floor_course order union) is an equivalent mutant: deleting the rule keeps all 71 tests green

File: `crates/core/tests/arena.rs:375` — IDs: R5, D1, C5

**Claim.** `mastery_floor_sizes_are_the_spec_literals`, the only test under the header "mastery floors (parity trap 12)", cannot distinguish "union every course with `order <= ref.order`" from "union only the referenced course", because every `mastery_floor_course` in the tree and in every fixture points at `foundations`, whose `order` is 1.

**Evidence.**

```
curriculum/courses.yaml: all 11 referencing courses write `mastery_floor_course: foundations`, and `foundations` has `order: 1`. The only other `mastery_floor_course` in the repo is crates/core/tests/fixtures/lint/mastery_floor_ambiguous/courses.yaml:8 (`mastery_floor_course: c1`, c1 has `order: 1`), and that fixture asserts only the ambiguity finding.

1.0 floors, measured: foundations 3, proofs 7, every other course 285 — exactly `len(topics_in_course('foundations'))`.

Mutation applied to crates/core/src/curriculum/arena.rs:473-477, replacing
            for other in &self.courses {
                if other.order <= reference.order {
                    floor.extend_from_slice(self.topics_in_course(other.id.as_str()));
                }
            }
with
            floor.extend_from_slice(self.topics_in_course(reference.id.as_str()));

$ cargo test -p cadus-core --tests
test result: ok. 21 passed ... ok. 21 passed ... ok. 17 passed ... ok. 10 passed ... ok. 2 passed; 0 failed

The twin rule at crates/core/src/curriculum/lint.rs:407-413 survives the same mutation (verified separately, SURVIVED). Control mutation on the same harness (topo min-heap -> max-heap) was KILLED by two tests, so the harness detects real breakage.
```

**Failure scenario.** An author adds a course that grounds on a mid-ordered course, e.g. `precalculus` with `mastery_floor_course: geometry` (order 2). 1.0 grounds it on foundations + geometry (372 topics); a port that dropped the order union grounds it on geometry alone (87 topics). Every topic of `precalculus` whose only path runs through a foundations topic then becomes `unreachable_from_floor` in lint and un-floored in the scheduler, and the M1 suite reports nothing, because the test that names trap 12 only ever measures a floor whose reference course has order 1.

**Refuter.** The claim holds. The union rule at crates/core/src/curriculum/arena.rs:473-477 collapses to "the referenced course alone" for every input the M1 suite supplies, so the one test that names parity trap 12 has no power over it. In curriculum/courses.yaml, `foundations` is the unique minimum order (order: 1) and all 11 `mastery_floor_course` values name `foundations`. The set {other : other.order <= reference.order} is therefore exactly {foundations} for every reference in the tree. The only other `mastery_floor_course` in the repo (crates/core/tests/fixtures/lint/mastery_floor_ambiguous/courses.yaml:8) names `c1`, which also has order: 1, and that fixture test (crates/core/tests/lint.rs:294-299) asserts the finding-code list only. No other test touches the floor: a grep for `mastery_floor` across crates/ finds calls only in crates/core/tests/arena.rs:378-407, and crates/core/src/curriculum/

### #12 [major] Parity trap 13 lives only in the dump binary and is pinned by no test; the library load_curriculum builds a partial arena where 1.0 raises

File: `crates/core/src/bin/dump_curriculum.rs:43` — IDs: R5, D1

**Claim.** `load_curriculum` (crates/core/src/curriculum/arena.rs:650) returns `Ok` with a silently truncated arena for a tree whose parse stage produced a fatal finding, where 1.0 `Graph.load` raises `CurriculumError`; the only place the port implements trap 13 is the guard at dump_curriculum.rs:43-50, and deleting that guard keeps every test green.

**Evidence.**

```
Probe test compiled against the unmodified crate:
    let (c, f) = load_curriculum("tests/fixtures/broken-yaml").expect(...);
PROBE ok=true topics=1 findings=[("yaml", true)]
PROBE2 (fixtures/unknown-key) topics=0 findings=[("schema", true)]

1.0 on the same two trees:
broken-yaml RAISED CurriculumError: ['yaml']
unknown-key RAISED CurriculumError: ['schema']

(1.0 cadus/graph.py:203-206 — `if catalog is None or any(f.fatal for f in findings): raise CurriculumError(...)`; spec section 7 trap 13.)

Mutation applied to crates/core/src/bin/dump_curriculum.rs:43-50, deleting the whole fatal-findings block:
$ cargo test -p cadus-core --tests
J_dump_no_fatal_guard: SURVIVED  (no test failed)

The test that claims to cover trap 13, crates/core/tests/arena.rs:609-621 `a_graph_stage_defect_does_not_stop_the_build` ("Only the parse stage can block a load"), asserts only `load_curriculum(...).is_ok()` on cycle-3, cycle-entered, dangling-refs and zero-weight-edge — four trees with zero parse-stage findings. No test loads a tree that carries a fatal parse finding, and `the_binary_exits_2_on_a_tree_with_no_courses_file` (parity.rs:159) exercises the `CurriculumNotFound` path, which returns before the guard.
```

**Failure scenario.** One unit file of `curriculum/` gets a YAML syntax error or an unknown key in a content PR. 1.0 refuses to load the tree at all. `load_curriculum` returns `Ok` with the remaining 87 files, so any M4/M5 caller that does not itself inspect the returned `Vec<Finding>` serves a curriculum missing every topic of that file — the frontier, the mastery floors and `curriculum_hash` all shift silently. The M1 suite stays green, and a later refactor that drops the guard in dump_curriculum.rs:43 also stays green, at which point the parity oracle would happily emit a dump for a tree 1.0 rejects.

**Refuter.** The claim is fully demonstrable and I could not refute it. load_curriculum (crates/core/src/curriculum/arena.rs:650) returns Ok with a truncated arena for a tree whose parse stage produced a fatal finding, where 1.0 Graph.load raises CurriculumError (/home/deploy/dev/cadus/cadus/graph.py:333, with the comment at line 328 giving the exact reason: dropped content makes the graph misrepresent the curriculum). The only enforcement of parity trap 13 in the port is the guard at crates/core/src/bin/dump_curriculum.rs:43-50, and I confirmed by mutation that deleting it kills no test: 71 tests pass with the block removed. The M1 plan states the decision as "only parse-stage fatal findings stop a load" (docs/plans/M1.md:19), which the loader does not do, and HANDOVER.md:69 makes an unkilled mutant a rejected suite; PROGRESS.md records "5 mutations red" for U1/U2/U3 and no mutation result for U4, t

### #13 [major] load_curriculum never blocks on a fatal parse-stage finding, so a broken unit file yields a silently truncated arena

File: `crates/core/src/curriculum/arena.rs:650` — IDs: C5, D1, R5 — duplicate of #12

**Claim.** The library entry point cadus_core::curriculum::load_curriculum returns Ok with a partially built Curriculum when the parse stage produced a fatal finding, where 1.0 Graph.load raises; only the dump_curriculum binary re-implements the gate, so every other caller gets a curriculum that silently omits the topics of the dropped file.

**Evidence.**

```
crates/core/src/curriculum/arena.rs:650-654 —
    pub fn load_curriculum(root: &Path) -> Result<(Curriculum, Vec<Finding>), LoadError> {
        let (raw, findings) = load_raw_curriculum(root)?;
        let curriculum = Curriculum::build(raw)?;
        Ok((curriculum, findings))
    }
`findings` is never inspected. The gate lives only in crates/core/src/bin/dump_curriculum.rs:43-50. Run on the committed fixture `crates/core/tests/fixtures/broken-yaml`:
  rust : OK topics=1 units=1 fatal_findings=1  ->  [yaml] fatal=true demo/01-bad.yaml: did not find expected ',' or ']' ...  topics: ["alpha"]
  1.0  : RAISED CurriculumError [yaml] demo/01-bad.yaml: while parsing a flow sequence ...
crates/core/tests/arena.rs:611 asserts the opposite in prose ("Only the parse stage can block a load") but no test exercises a fatal parse finding through load_curriculum.
```

**Failure scenario.** A content author lands a unit file with a YAML typo or an unknown key. 1.0 refused to start (`graph.py:592`: "Only *fatal* findings block a load: those mean content was dropped, so the graph would silently misrepresent the curriculum"). In 2.0 the M4 web tier calling load_curriculum gets Ok(...) and serves a curriculum missing every topic of that file — no error, no startup failure. Spec docs/reference/curriculum-1.0-spec.md §5 ("the only findings Graph.load blocks on when fatal") and §7 trap 13 ("only parse-stage fatal findings block a load") both require the block.

**Refuter.** The claim is demonstrable and I could not refute it. cadus_core::curriculum::load_curriculum (crates/core/src/curriculum/arena.rs:650-654) binds `findings` and never inspects them; Curriculum::build errors only on a duplicate topic id, so a fatal parse-stage finding yields Ok with a curriculum missing every topic of the dropped file. 1.0's same-named entry point raises (graph.py:618-620 -> Graph.load, gate at graph.py:328-334). The gate exists only in crates/core/src/bin/dump_curriculum.rs:43-50. The usual defense — that the doc comment delegates the decision to the caller because "the lint runner of U3 fails on any finding at all" — does not hold: lint_curriculum (crates/core/src/curriculum/lint.rs:44-50) calls parse_curriculum directly and never uses load_curriculum, so no in-repo consumer needs the ungated form and no gated library API is offered. Three authorities require the block: 

### #22 [major] Every repeated-edge rule of Graph.__init__ is an equivalent mutant: no fixture declares one edge twice

File: `crates/core/src/curriculum/arena.rs:735` — IDs: D1, R5, C5

**Claim.** No fixture and no tree in `curriculum/` declares the same (src, dst) edge twice, so the three merge rules the port copies from 1.0 `_add_enc` and `Graph.__init__` — the max-merge of the forward map (arena.rs:718), the max-merge of the reverse map (arena.rs:735), and the set-dedup of the prerequisite adjacency (arena.rs:277) — are pinned by nothing; deleting the reverse max-merge and the dedup keeps all 71 tests green while the arena diverges from 1.0.

**Evidence.**

```
Tree `demo/00.yaml` with `b.prerequisites: [{id: a, weight: 0.2}, {id: a, weight: 0.7}]`.
1.0 oracle: `_enc_rev {'a': {'b': 0.7}}`, `prereqs {'b': ['a']}`.
Unmutated port agrees (`dump_curriculum` sha256 identical to `dump_curriculum_1_0.py`: 017998d82ede9fa93f5c264ed424ab15e82d189f50cf09213119f11acf2504d9).
Mutation: arena.rs:735 `Some(edge) => { if weight > edge.weight { edge.weight = weight; } }` -> `Some(_edge) => {}`, and arena.rs:277 `list.dedup();` deleted.
  cargo test -p cadus-core --tests -> 21 + 21 + 17 + 10 + 2 passed; 0 failed (SURVIVED)
  enc_rev[a] = [("b", 0.2)]   (1.0: 0.7)
  upward_weights(a) = [1.0, 0.2]  (1.0: 0.7)
  prereq_edge_count = 2      (1.0 len(g.prereqs['b']) = 1)
```

**Failure scenario.** An author writes the same prerequisite id twice on one topic (or lists a prerequisite target again under `encompassings_extra`) with two different weights — legal 1.0 input that the schema accepts. A future edit that drops the max-merge, or an M3 rewrite of `EncBuilder`, lowers `W(a -> b)` from 0.7 to 0.2 for that pair and doubles `prereq_edge_count`. The whole M1 suite still reports 71 passed, and the parity dump does not see it either, because the canonical dump exposes only the forward encompassing map — the corrupted reverse map is what M3's `upward_weights` (PEDAGOGY 4 failure penalty) reads.

**Refuter.** The claim is demonstrable. I reproduced every step in a copy of the tree (build outside the repo; the repo stayed unchanged).

1. The premise holds. A YAML scan of all files in `crates/core/tests/fixtures/` and `curriculum/` found zero topics that name the same prerequisite id twice, and zero topics that name one id in both `prerequisites` and `encompassings_extra`. No test builds a `RawCurriculum` in code either (`cargo test -p cadus-core --tests` shows `running 0 tests` for the lib target), so no duplicate `(src, dst)` edge reaches `EncBuilder::add` anywhere in M1.

2. The three merge rules are unpinned. The mutations survive:
   - reverse max-merge (arena.rs:735) + `list.dedup()` (arena.rs:277) deleted together: 21 + 21 + 17 + 10 + 2 = 71 passed, 0 failed.
   - forward max-merge (arena.rs:718) deleted alone: 71 passed, 0 failed, and the live differential test `the_rust_dump_equals_the

### #23 [major] graph::closure's start-exclusion rule (1.0 `_closure`) survives deletion: no test calls ancestors/descendants on a cyclic fixture

File: `crates/core/src/curriculum/graph.rs:239` — IDs: C5, R5, D1

**Claim.** The `if node == start { continue; }` guard that copies 1.0 `_closure` (`cadus/graph.py:148-158`, "excluding ``start``") only fires when a cycle leads back to the start node, and every test that reaches `closure` uses a DAG — the `cycle-3` and `cycle-entered` fixtures exist but no test ever calls `ancestors`, `descendants`, or a key-prerequisite lint rule on them — so deleting the guard keeps all 71 tests green while the lint silently loses a finding that 1.0 reports.

**Evidence.**

```
Mutation: delete graph.rs:239-241 (`if node == start { continue; }`).
  cargo test -p cadus-core --tests -> 21 + 21 + 17 + 10 + 2 passed; 0 failed (SURVIVED)
Tree: prerequisite cycle a -> b -> c -> a, with topic `b` whose kp1 lists `key_prerequisites: [b]`.
  1.0 dump_lint_1_0.py:
    [cycle] prerequisite cycle: a -> b -> c -> a
    [key_prereq_not_ancestor] key_prerequisite 'b' in b.kp1 is neither an ancestor nor an encompassings_extra target
  unmutated Rust lint_curriculum: same 2 findings
  mutated Rust lint_curriculum: FAIL: 1 curriculum finding(s) — the key_prereq_not_ancestor finding is gone
```

**Failure scenario.** A curriculum tree contains a prerequisite cycle (the `cycle` fixtures show this is reachable content) and a topic on that cycle names itself, directly or transitively, as a key prerequisite. 1.0 reports `key_prereq_not_ancestor`; a port whose `closure` includes `start` does not. Nothing in the suite distinguishes the two, so the regression ships as a silently missing lint code. The same guard also governs `Curriculum::ancestors`/`descendants`, which M3's scheduler walks: on `cycle-3`, 1.0 gives `ancestors('a') == {'b','c'}` while the mutant gives `{'a','b','c'}`.

**Refuter.** The claim is demonstrable in full, so I cannot refute it. I confirmed all three parts. (1) The guard at crates/core/src/curriculum/graph.rs:239-241 is the only line that excludes `start` from `closure`: `seen[start]` is never set, so without the guard a `start` that a cycle reaches back is pushed into `out`. The guard is load-bearing for parity with 1.0 `_closure` (`cadus/graph.py:148-158`, `if node in seen or node == start`). (2) The mutation survives. After I deleted lines 239-241, `cargo test -p cadus-core --tests` gave 21 + 21 + 17 + 10 + 2 passed and 0 failed — the same 71 tests as the unmutated tree. The curriculum modules hold no `#[cfg(test)]` unit tests, so the guard has no unit-level pin either. (3) The coverage gap is real. `crates/core/tests/arena.rs` touches `cycle-3` and `cycle-entered` only at lines 466, 476, 483 and 614-615, for `find_cycle`, `topo_order`, and load tolera

## FIXM1c

### #24 [major] The lint's id-sorting rules are unpinned: every fixture's load order already equals its id order

File: `crates/core/src/curriculum/lint.rs:291` — IDs: C5, R5

**Claim.** The three places where the lint reproduces a 1.0 `sorted()` — `core_dependents.sort_unstable()` (lint.rs:291), `Table::sorted_positions` (lint.rs:503, the 1.0 `for tid in sorted(topics)` walk), and `unreachable.sort_unstable()` (lint.rs:449) — are all equivalent mutants, because in every fixture the topics are authored in ascending id order, so the sorted result equals the load-index result; deleting all three keeps 71/71 green while the finding order and the topic named in the message diverge from 1.0.

**Evidence.**

```
Mutation 1: delete lint.rs:291 and lint.rs:503.
  cargo test -p cadus-core --tests -> 21 + 21 + 17 + 10 + 2 passed; 0 failed (SURVIVED)
  Tree, load order zbase(non-core), abase(non-core), core1, core0:
    1.0 dump_lint_1_0.py -> abase first (context ['core0']), then zbase, message "... of core topic 'core0' (+1 more)", context ['core0','core1']
    mutated Rust -> zbase first, message "... of core topic 'core1' (+1 more)"; abase second
Mutation 2: delete lint.rs:449.
  cargo test -p cadus-core --tests -> SURVIVED
  Tree with c2 authoring topics zzz then aaa, both unreachable:
    1.0 -> 'aaa' then 'zzz'
    mutated Rust -> 'zzz' then 'aaa'
```

**Failure scenario.** A curriculum unit file authors topics in any order other than alphabetical (the real `curriculum/` tree does exactly this — load index 0 is `single-digit-addition`, not the alphabetically first id) and two of them trip the same lint rule. The port then emits `noncore_ancestor_of_core` findings in load order instead of id order, names the wrong `example` topic in the message text, and orders `unreachable_from_floor` findings wrongly. U3's acceptance check is byte-for-byte equality with the committed 1.0 output, and none of the 19 fixtures can see any of it.

**Refuter.** The claim is demonstrable and I reproduced every part of it. All three sorts are load-bearing, and no test in scope kills any of them. I copied the tree to a scratchpad, deleted lint.rs:291 (core_dependents.sort_unstable), lint.rs:449 (unreachable.sort_unstable), and lint.rs:503 (the sort inside Table::sorted_positions) together, and cargo test -p cadus-core --tests stayed at 21+21+17+10+2 = 71 passed, 0 failed. All three mutants survive. The divergence is real: graph::closure ends with out.sort_unstable() (graph.rs:252), so it yields load-index order, while 1.0 uses id order (cadus/graph.py:751 `for tid in sorted(topics)`, :753 `core_dependents = sorted(...)`, :833 `for tid in sorted(course_topics - reachable)`). On two constructed trees the mutated Rust differs from the 1.0 oracle in finding order, in context order, and in the topic named in the message, while the unmutated Rust matche

### #25 [major] The whole inter-rule ordering of lint_curriculum is unpinned: each of the 19 fixtures triggers exactly one lint code

File: `crates/core/tests/lint.rs:130` — IDs: C5, R5

**Claim.** Every committed lint fixture yields findings of exactly one code (the largest, `missing_ref`, yields three findings of one code), so `every_fixture_matches_the_committed_1_0_output_byte_for_byte` never observes the order in which the lint's rule stages append their findings; two whole rule blocks can be swapped and all 71 tests stay green while the output order stops matching 1.0.

**Evidence.**

```
Distinct codes per fixture expected.json: all 19 trees give distinct_codes <= 1 (clean=0, missing_ref=3 findings / 1 code, every other=1 finding / 1 code).
Mutation: move the `-- exactly one mastery-floor form per course --` block (lint.rs:350-367) ahead of the `-- module names --` block (lint.rs:314-348).
  cargo test -p cadus-core --tests -> 21 + 21 + 17 + 10 + 2 passed; 0 failed (SURVIVED)
Tree with a blank module name and a course setting both mastery-floor forms:
  1.0 dump_lint_1_0.py codes -> ['module_inconsistent', 'mastery_floor_ambiguous']
  unmutated Rust    -> module_inconsistent, then mastery_floor_ambiguous
  mutated Rust      -> mastery_floor_ambiguous, then module_inconsistent
```

**Failure scenario.** A real curriculum tree fails more than one lint rule at once — the normal case during authoring. 1.0 and the port then print the findings in different orders, so `scripts/oracle/dump_lint_1_0.py` output and `lint_curriculum` output no longer diff to empty, and the R5 oracle comparison that U3 rests on gives a false negative. The suite cannot detect the reordering because no fixture ever produces two different codes; there is no multi-code fixture and no test that pins the stage sequence.

**Refuter.** The claim is demonstrable, and I reproduced every step of it.

1. The premise is true. All 19 committed fixtures under /home/deploy/dev/cadus2.0/crates/core/tests/fixtures/lint/ hold at most one distinct code in expected.json. `clean` holds 0 findings. `missing_ref` holds 3 findings of the one code `missing_ref`. The other 17 hold 1 finding each. No fixture tree makes two different lint codes.

2. The consequence is true. `every_fixture_matches_the_committed_1_0_output_byte_for_byte` (crates/core/tests/lint.rs:130) compares a per-fixture list. With one code per list, the comparison pins the order inside a rule block only. It never pins the sequence of the rule blocks. No other test lints a multi-code tree: `lint_curriculum` appears only in crates/core/tests/lint.rs and crates/core/src/bin/lint_curriculum.rs, and scripts/gate.sh runs no lint-oracle diff.

3. The mutation survives. I copie
