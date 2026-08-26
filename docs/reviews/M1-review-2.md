# M1 adversarial review — round 2 (2026-08-26)

Run on commit 7d49756 (after FIXM1a–c). One find/refute round, major+ only: 15 raised, 15 confirmed. Assigned to FIXM1d (dump: float text, -0.0 order, repeated-edge fixture) and FIXM1e (loader numeric forms, duplicate-key line, non-UTF-8 names, lint fixtures, spec). M1 closes after this wave on the two-round cap.

## Orchestrator rulings (binding)

- Float text: implement the CPython `repr` tie rule (shortest round-trip digits; on an
  exact tie between two shortest candidates choose the even last digit). `-0.0` prints
  as `-0.0`, sorts equal to `0.0`, and the comparison falls through to the next element.
- Numeric literal forms in `order`, `difficulty`, `expected_time_secs`, `weight`: 2.0
  accepts a plain decimal integer (`-?[0-9]+` with no leading zero except `0` itself) and a
  plain decimal float (`-?[0-9]+\.[0-9]+`, optional signed exponent with a decimal point as
  YAML 1.1 requires). Every other spelling — leading zero `08`/`060`, `1e3`, `1.0e2`,
  `0o17`, `0b101`, `0x1F`, `1_000`, `0.7_5`, `1:30`, `.nan`, `.inf` — is a `schema` finding
  with the 2.0 message `<loc>: numeric literal form '<raw>' is not accepted; write a plain
  decimal number`. Detect the raw spelling with a line-level pre-scan of the YAML text for
  those four keys (the resolved `Value` has lost the spelling); document the scan and its
  limit (a value written on a continuation line is not scanned) in spec §7.
- A duplicate root-level mapping key reports its line by a raw-text scan when the parser
  gives no location.
- A unit-file name that is not valid UTF-8 is a `yaml` finding, never a silent skip.
- The spec §7 "2.0 strictness" lists are the complete truth after these fixes; recount
  every number in §6 with a quoted command.

| # | Sev | File | Unit | Title |
|---|---|---|---|---|
| 1 | major | `crates/core/src/curriculum/dump.rs:161` | FIXM1d | python_repr_f64 breaks shortest-digit ties away from zero; CPython repr breaks them to even, so the dump and the curriculum hash diverge for a legal difficulty/weight |
| 2 | major | `crates/core/src/curriculum/dump.rs:563` | FIXM1d | compare_float sorts prereq_edges with total_cmp, which orders -0.0 below 0.0; Python treats them equal and falls through to the key flag, so the dump rows swap |
| 3 | major | `crates/core/src/curriculum/load.rs:876` | FIXM1e | The YAML 1.1 recognizer misses underscore-separated floats and hex/binary literals, so the loader drops a file 1.0 loads clean and writes the pydantic message text the spec reserves |
| 4 | major | `crates/core/src/curriculum/load.rs:798` | FIXM1e | Exponent-form and YAML 1.2 octal/binary integers load in 2.0 where 1.0 reports a fatal schema finding |
| 5 | major | `crates/core/src/curriculum/load.rs:873` | FIXM1e | The pydantic 'unable to parse' text is emitted for underscore and hex forms that 1.0 accepts |
| 6 | major | `crates/core/src/curriculum/load.rs:819` | FIXM1e | An integer literal past u128 reports a third message, not the one spec section 7 fixes |
| 7 | major | `crates/core/src/curriculum/load.rs:1003` | FIXM1e | A duplicate mapping key in the root mapping of a unit file or of courses.yaml yields a schema finding with no line number ('at line ?') |
| 8 | major | `crates/core/src/curriculum/load.rs:784` | FIXM1e | A YAML 1.2-only number form in an integer field (1e2, 1.0e2, 0o17) passes the 2.0 lint gate but makes 1.0 refuse the whole curriculum |
| 9 | major | `crates/core/tests/fixtures/arena-repeated-edge/demo/01-basics.yaml:19` | FIXM1d | The repeated-edge "keep the larger weight" rule is still an equivalent mutant: the new fixture writes the two weights in ascending order |
| 10 | major | `crates/core/src/curriculum/load.rs:798` | FIXM1e | The loader silently accepts YAML 1.2-only numeric scalars (1e3, 1.0e2, 0o17) that 1.0 rejects, and no test or spec section covers the direction |
| 11 | major | `crates/core/src/curriculum/lint.rs:388` | FIXM1e | The lint's "a repeated course id keeps the LAST catalog entry" rule is unpinned; the round-1 fix pinned only the arena copy |
| 12 | major | `crates/core/src/curriculum/lint.rs:336` | FIXM1e | The `sorted(module_courses.items())` site of the lint is unpinned, so the `order_not_id_order` fixture's "every sorted() site" claim is false |
| 13 | major | `crates/core/src/curriculum/load.rs:315` | FIXM1e | A unit file whose name is not valid UTF-8 is silently dropped from the load, with no finding |
| 14 | major | `docs/reference/curriculum-1.0-spec.md:243` | FIXM1e | Spec section 7 claims to list every deliberate loader difference, but at least two integer-literal forms diverge and appear in neither table |
| 15 | major | `docs/reference/curriculum-1.0-spec.md:204` | FIXM1e | Spec section 6 states a wrong non-ASCII file count and cites the wrong line for the escaped-backslash example |

## FIXM1d

### #1 [major] python_repr_f64 breaks shortest-digit ties away from zero; CPython repr breaks them to even, so the dump and the curriculum hash diverge for a legal difficulty/weight

File: `crates/core/src/curriculum/dump.rs:161` — IDs: R5, C5, D1

**Claim.** python_repr_f64 takes its digits from Rust's `format!("{value:e}")`, whose shortest round-trip algorithm rounds a last-digit tie away from zero, while CPython `repr` (David Gay, mode 0) rounds such a tie to even, so the two write different text for f64 values that are legal in 0..=1 and the canonical dump is no longer byte-identical.

**Evidence.**

```
crates/core/src/curriculum/dump.rs:160-161
    // Rust writes `-d.ddde<exp>` with the shortest digits that round-trip.
    let shortest = format!("{value:e}");

Fixture c/00.yaml: topic `a` with `difficulty: 0.50000762939453125` (exact f64, both candidate 16-digit texts round-trip).

$ /home/deploy/dev/cadus/.venv/bin/python scripts/oracle/dump_curriculum_1_0.py FIXTURE | grep -o '"difficulty":[^,}]*'
"difficulty":0.5000076293945312
sha256=ff49be2230777cf6be08a93f4deb91fccdb573d95b932a3abeecb67d4d5b3322

$ ./target/debug/dump_curriculum FIXTURE | grep -o '"difficulty":[^,}]*'
"difficulty":0.5000076293945313
sha256=d4306bb0382d08b9275e03863d2a7db14391dce17b177b11de62a88cf8a3d4cd

The same happens in the scientific branch, e.g. `weight: 0.000093936920166015625`:
  1.0  "prereq_edges":[["b","a",9.393692016601562e-05,false]]
  2.0  "prereq_edges":[["b","a",9.393692016601563e-05,false]]

Exhaustive check over 510,602 f64 values in [0,1] (400k uniform, plus m/2^s ladders): 101 values where python_repr_f64 and CPython repr disagree; all disagreements are last-digit ties, both texts round-trip.
```

**Failure scenario.** An author (or a generator that emits full-precision decimals) writes `difficulty: 0.50000762939453125` in a unit file. Both loaders accept the value and lint reports 0 findings on both sides. `dump_curriculum` writes `"difficulty":0.5000076293945313` while `scripts/oracle/dump_curriculum_1_0.py` writes `"difficulty":0.5000076293945312`, so the R5 parity gate (`diff rust.json python.json`) fails and `curriculum_hash` is d4306bb0… on the Rust side against ff49be22… on the 1.0 side — a hash mismatch with no finding anywhere to explain it.

**Refuter.** The claim is demonstrable and I reproduced it end to end. `python_repr_f64` takes its digit run from `format!("{value:e}")` (crates/core/src/curriculum/dump.rs:161). Rust's shortest-round-trip formatter breaks an exact last-digit tie away from zero; CPython `repr` (David Gay dtoa mode 0, ROUND_BIASED undefined) breaks it to even. For f64 values that are exactly midway between two 16-digit candidates, the two implementations write different text. `check_unit_interval` (crates/core/src/curriculum/load.rs:536-565) accepts any finite f64 in 0..=1 with no precision limit, so such values load cleanly and lint reports nothing. The result is a non-byte-identical canonical dump and a different `curriculum_hash`, which breaks R5 parity and the D1/C5 canonical-dump guarantee. docs/reference/curriculum-1.0-spec.md:231 binds the dump to the Python `repr` rule, so this is a spec violation, not a toler

### #2 [major] compare_float sorts prereq_edges with total_cmp, which orders -0.0 below 0.0; Python treats them equal and falls through to the key flag, so the dump rows swap

File: `crates/core/src/curriculum/dump.rs:563` — IDs: R5, C5

**Claim.** prereq_edges is sorted with compare_float = f64::total_cmp, which makes -0.0 strictly less than 0.0, while the 1.0 oracle sorts Python lists where -0.0 == 0.0 and the comparison falls through to the next element, so two rows that share (child, parent) and differ only in the sign of a zero weight come out in the opposite order.

**Evidence.**

```
crates/core/src/curriculum/dump.rs:560-564
    /// Order two weights. The loader rejects `NaN`, so the total order of `f64`
    /// matches the Python comparison.
    fn compare_float(a: f64, b: f64) -> Ordering {
        a.total_cmp(&b)
    }

The comment is false for the pair (0.0, -0.0): total_cmp says Greater, Python says equal.

Fixture c/00.yaml: topic `b` with
    prerequisites:
      - {id: a, weight: 0.0, key: false}
      - {id: a, weight: -0.0, key: true}
Both weights pass the 0..=1 gate on both sides (`-0.0 < 0.0` is false; pydantic `ge=0` accepts -0.0).

$ /home/deploy/dev/cadus/.venv/bin/python scripts/oracle/dump_curriculum_1_0.py FIXTURE
"prereq_edges":[["b","a",0.0,false],["b","a",-0.0,true]]
sha256=aa120076e6f1b3605e4c291dcce70ad3964b630771a40f29abc8407ea3514ae8

$ ./target/debug/dump_curriculum FIXTURE
"prereq_edges":[["b","a",-0.0,true],["b","a",0.0,false]]
sha256=b37b4ce53416c2d31fa6a3a178c7fe3e0f36daa0c8107d84ce367b7a9f7c6a3d

lint_curriculum reports 0 findings on both sides, and the arena agrees (`REV a [b=0.0]` on both), so only the dump order differs.
```

**Failure scenario.** A content generator emits a weight of -0.0 (a rounding pipeline or a spreadsheet export produces it routinely) on one of two prerequisite edges that a topic declares twice against the same parent — the repeated-edge case parity trap 8 calls out as unexercised by the checked-in tree. Both loaders load the tree cleanly with 0 lint findings, but `prereq_edges` comes out in the opposite order, the canonical dumps differ byte for byte, and `curriculum_hash` is b37b4ce5… against 1.0's aa120076…. The same swap happens when both rows carry the same `key` flag, because Python's sort is stable on equal keys and total_cmp is not.

**Refuter.** The claim is demonstrable end to end, and I reproduced it on both sides with two independent fixtures. Every link in the chain holds:

1. `-0.0` passes the port's range gate. `/home/deploy/dev/cadus2.0/crates/core/src/curriculum/load.rs:540-545` (`check_unit_interval`) rejects a weight only when `number.is_nan() || number > 1.0` or `number < 0.0`. For `-0.0` both tests are false, so the port accepts it. 1.0 pydantic `ge=0` accepts it for the same reason (`-0.0 >= 0` is True). Neither side normalizes the sign, and both dumps print `"weight":-0.0` inside `topics[].prerequisites`, so the value survives the load on both sides.

2. A topic can declare the same parent twice. This is not a hypothetical shape: the checked-in fixture `/home/deploy/dev/cadus2.0/crates/core/tests/fixtures/arena-repeated-edge/demo/01-basics.yaml` already has topic `b` with two `id: a` prerequisites, and its comment 

### #9 [major] The repeated-edge "keep the larger weight" rule is still an equivalent mutant: the new fixture writes the two weights in ascending order

File: `crates/core/tests/fixtures/arena-repeated-edge/demo/01-basics.yaml:19` — IDs: D1, C5

**Claim.** Round-1 finding #22 ("every repeated-edge rule of Graph.__init__ is an equivalent mutant") is not fixed: the fixture added by the fix declares the repeated edge b->a as weight 0.2 then 0.7, so 1.0's `if weight > self._enc[src].get(dst, 0.0)` and a "last weight wins" implementation give the same answer, and replacing the max rule in `arena.rs:755` and `arena.rs:772` with `edge.weight = weight;` leaves all 94 tests green.

**Evidence.**

```
Fixture (ascending, so max == last):
  17:    prerequisites:
  18:      - id: a
  19:        weight: 0.2
  20:      - id: a
  21:        weight: 0.7

Mutant applied to both branches of `EncBuilder::add` (arena.rs:755, :772):
  -                    if weight > edge.weight { edge.weight = weight; }
  +                    edge.weight = weight;
$ cargo test -p cadus-core --tests
test result: ok. 27 passed ... ok. 25 passed ... ok. 28 passed ... ok. 12 passed ... ok. 2 passed  (94/94 green)

Same two edges written in descending order (0.7 then 0.2) in a scratch tree:
$ .venv/bin/python scripts/oracle/dump_curriculum_1_0.py $R  -> encompassing_edges [['b','a',0.7]]
$ target/debug/dump_curriculum $R (repo)                     -> [['b', 'a', 0.7]]
$ target/debug/dump_curriculum $R (mutant)                   -> [['b', 'a', 0.2]]
```

**Failure scenario.** An author writes the same prerequisite twice with the stronger weight first (`- id: a, weight: 0.7` then `- id: a, weight: 0.2`). 1.0 keeps 0.7 in `_enc`/`_enc_rev`; a port that dropped the max comparison keeps 0.2, so every `W(b -> a)` credit-flow value and the `encompassing_edges` row of the dump (hence `curriculum_hash`) differ from 1.0 — and no test in the suite reports it. The test named `a_repeated_edge_keeps_the_maximum_weight_in_both_encompassing_maps` (arena.rs:646) only pins "keeps the later weight".

**Refuter.** The claim is demonstrable and I reproduced every step of it. The fix for round-1 finding #22 added the fixture `crates/core/tests/fixtures/arena-repeated-edge/demo/01-basics.yaml`, and that fixture declares the repeated edge b -> a in ASCENDING weight order (0.2 at line 19, then 0.7 at line 21). For an ascending pair, "keep the maximum" and "keep the last weight" give the same result, so the fixture pins the wrong property. A YAML scan of the whole tree (`curriculum/` plus every directory under `crates/core/tests/fixtures/`) finds exactly ONE topic with a repeated prerequisite id in the entire repository, and it is that fixture, so no other test data can separate the two rules. I copied the tree to a scratch directory, replaced both max comparisons in `EncBuilder::add` (arena.rs:755 forward map, arena.rs:772 reverse map) with `edge.weight = weight;`, and ran `cargo test -p cadus-core --t

## FIXM1e

### #3 [major] The YAML 1.1 recognizer misses underscore-separated floats and hex/binary literals, so the loader drops a file 1.0 loads clean and writes the pydantic message text the spec reserves

File: `crates/core/src/curriculum/load.rs:876` — IDs: R5, C5, D1

**Claim.** string_number_message and int_message recognize only the YAML 1.1 integer forms that yaml_1_1_integer covers (octal, decimal underscore, sexagesimal); an underscore-separated float (`0.7_5`), an underscore-separated hex/binary literal (`0x1_F`, `0b1_01`) and a sexagesimal float (`0:00.5`) fall through to the pydantic message text, which spec section 7 reserves for values pydantic also rejects — while 1.0 reads all four as valid in-range numbers and loads the file.

**Evidence.**

```
crates/core/src/curriculum/load.rs:868-880
fn string_number_message(text: &str) -> String {
    match yaml_1_1_integer(text) {
        Yaml11Integer::Value(number) => format!("integer {text} is not accepted; write {number}"),
        Yaml11Integer::OutOfRange => OUT_OF_RANGE_INTEGER.to_owned(),
        Yaml11Integer::No => {
            if text.trim().parse::<f64>().is_ok() {
                format!("string '{text}' is not accepted; write the number unquoted")
            } else {
                "Input should be a valid number, unable to parse string as a number".to_owned()
            }
        }
    }
}
(load.rs:833 is the same fall-through for integer fields.)

docs/reference/curriculum-1.0-spec.md:245 — "The pydantic message text stays reserved for the values pydantic also rejects, so a 1.0 message never appears on a value 1.0 accepts."  Neither `0.7_5` nor `0x1_F` nor `0b1_01` appears in the section 7 "Rejected by 2.0" table.

$ /home/deploy/dev/cadus/.venv/bin/python -c "import yaml;print(yaml.safe_load('a: 0x1_F\nb: 0b1_01\nc: 0.7_5\n'))"
{'a': 31, 'b': 5, 'c': 0.75}

Fixture, `difficulty: 0.7_5`:
  1.0 lint: []                      1.0 dump: "difficulty":0.75
  2.0 lint: [schema] topics.0.difficulty: Input should be a valid number, unable to parse string as a number
  2.0 dump: rc=2, no dump written

Fixture, `expected_time_secs: 0x1_F`:
  1.0 lint: []                      1.0 dump: "expected_time_secs":31
  2.0 lint: [schema] topics.0.expected_time_secs: Input should be a valid integer, unable to parse string as an integer

Fixture, `expected_time_secs: 0b1_01`:  1.0 -> 5;  2.0 -> same pydantic message.
Fixture, `difficulty: 0:00.5`:          1.0 -> 0.5; 2.0 -> same pydantic message.

The unqualified forms behave correctly: `0x1F` and `0b101` both read 31 and 5 on both sides, and `030` / `1_200` / `1:30` get the intended 2.0 wording.
```

**Failure scenario.** An author writes `difficulty: 0.7_5` (PyYAML's underscore grouping, legal 1.0 content that reads 0.75). 1.0 loads the unit file and `scripts/lint_curriculum.py` prints `OK: ... (0 findings).`. The Rust loader reports `[schema] topics.0.difficulty: Input should be a valid number, unable to parse string as a number`, `load_curriculum` returns Err on the fatal finding, and `dump_curriculum` exits 2 — the whole tree stops loading. The message is the reserved pydantic text on a value pydantic accepts, so an author who greps the 1.0 wording concludes the value itself is malformed, and it names no fix, unlike every form the spec's strictness table covers.

**Refuter.** I could not refute the claim; I reproduced every part of it end to end.

`yaml_1_1_integer` (/home/deploy/dev/cadus2.0/crates/core/src/curriculum/load.rs:906-928) recognizes exactly three PyYAML 1.1 forms: sexagesimal integer, leading-`0` octal, and a leading-digit decimal underscore group. PyYAML's implicit resolvers cover more than that. Its int regex also accepts `0x[0-9a-fA-F_]+` and `0b[0-1_]+`, and its float regex also accepts `[0-9][0-9_]*\.[0-9_]*` and the sexagesimal float `[0-9][0-9_]*(:[0-5]?[0-9])+\.[0-9_]*`. serde_norway resolves the YAML 1.2 core schema, so all four of those forms arrive as `Value::String`, `yaml_1_1_integer` answers `No`, the `parse::<f64>()` / `parse::<i64>()` retry fails on the underscore or the radix prefix, and the code emits the pydantic fall-through text at load.rs:876 (floats) and load.rs:833-834 (integers).

That text is exactly what spec section 7

### #4 [major] Exponent-form and YAML 1.2 octal/binary integers load in 2.0 where 1.0 reports a fatal schema finding

File: `crates/core/src/curriculum/load.rs:798` — IDs: R5, C5

**Claim.** int_message accepts a Value::Number that serde_norway resolved from an exponent form (1e3) or a YAML 1.2 octal/binary form (0o17, 0b101), so the 2.0 lint gate passes and load_curriculum yields a value, while 1.0 PyYAML leaves the scalar a string and pydantic rejects it with a fatal schema finding; spec section 7 lists neither form in its 'Accepted by 2.0, rejected by 1.0' table.

**Evidence.**

```
Tree with `expected_time_secs: 1e3`:

$ target/debug/lint_curriculum <tree>
OK: <tree> is a valid curriculum (0 findings).   (rc=0)

$ .venv/bin/python scripts/oracle/dump_lint_1_0.py <tree>
[ { "code": "schema", "fatal": true, "file": "c/00.yaml",
    "message": "topics.0.expected_time_secs: Input should be a valid integer, unable to parse string as an integer" } ]

$ target/debug/dump_curriculum <tree> | jq '.topics[0].expected_time_secs'
1000

load.rs:798-801
        Value::Number(number) => {
            if number.as_i64().is_some() {
                return None;
            }

Same divergence measured for 1E3, 1e+3, 6e2, 2e5, 1e1, 1e2 (all -> 2.0 accepts, 1.0 rejects), 0o17 -> 15, 0b101 -> 5, and `difficulty: 0o1` -> 1.0.
```

**Failure scenario.** A content author (or a generator) writes `expected_time_secs: 1e3` in a unit file. `lint_curriculum` exits 0 and the CI content gate passes. `load_curriculum` builds the arena with `expected_time_secs = 1000`. The same file run through 1.0 `lint_curriculum` fails with a fatal `schema` finding, and 1.0 `Graph.load` raises `CurriculumError`, so the two implementations disagree on whether the tree is a valid curriculum. No test pins the form and spec section 7 does not name it, so the divergence is invisible.

**Refuter.** The core claim is demonstrable and I reproduced it with the checked-in binaries and the live 1.0 oracle. serde_norway resolves the YAML 1.2 core schema, so `expected_time_secs: 1e3` becomes Value::Number(1000.0) and `expected_time_secs: 0o17` becomes Value::Number(15). `as_i64` (load.rs:776) and `int_message` (load.rs:796-800) accept both, `lint_curriculum` exits 0 with 0 findings, and `dump_curriculum` writes 1000 and 15. PyYAML leaves both scalars strings, and 1.0 pydantic then reports a fatal `schema` finding, so `Graph.load` refuses the tree. Spec section 7 states "This section lists every deliberate difference between the two loaders", and its "Accepted by 2.0, rejected by 1.0" list names neither form. The guard test `the_checked_in_tree_uses_no_yaml_1_1_form` (tests/loader.rs:768) states the invariant "the tree must write no form the two versions read differently", but its helper `

### #5 [major] The pydantic 'unable to parse' text is emitted for underscore and hex forms that 1.0 accepts

File: `crates/core/src/curriculum/load.rs:873` — IDs: R5, C5

**Claim.** yaml_1_1_integer recognizes `_` groups only inside a plain decimal integer and never inside a float literal or after `0x`, so string_number_message and int_message fall through to str::parse and report the reserved pydantic text on values that 1.0 parses successfully, which spec section 7 forbids: 'The pydantic message text stays reserved for the values pydantic also rejects, so a 1.0 message never appears on a value 1.0 accepts.'

**Evidence.**

```
1.0 vs 2.0 on the same unit file (1.0 value from cadus.graph.load_curriculum, 2.0 text from dump_curriculum/lint_curriculum):

  difficulty: 6_0e-2   1.0 -> 0.6      2.0 -> [schema] topics.0.difficulty: Input should be a valid number, unable to parse string as a number
  difficulty: 1_0e-1   1.0 -> 1.0      2.0 -> same message
  difficulty: .5_0     1.0 -> 0.5      2.0 -> same message
  difficulty: 0_1.0    1.0 -> 1.0      2.0 -> same message
  difficulty: 5e-1_0   1.0 -> 5e-10    2.0 -> same message
  expected_time_secs: 0x_1F  1.0 -> 31  2.0 -> [schema] topics.0.expected_time_secs: Input should be a valid integer, unable to parse string as an integer

load.rs:872-877
        Yaml11Integer::No => {
            if text.trim().parse::<f64>().is_ok() {
                format!("string '{text}' is not accepted; write the number unquoted")
            } else {
                "Input should be a valid number, unable to parse string as a number".to_owned()
            }

load.rs:920-923 (yaml_1_1_integer) covers only `0`-prefixed octal and `_` in a leading-digit decimal group:
    } else if let Some(octal) = digits.strip_prefix('0').filter(|_| digits.len() > 1) {
        radix_value(octal, 8)
    } else if digits.contains('_') && leads {
        radix_value(digits, 10)
```

**Failure scenario.** An author migrating a 1.0 unit file that writes `difficulty: 6_0e-2` (1.0 reads 0.6) runs the 2.0 lint. The message says the value cannot be parsed as a number, which is untrue of the value 1.0 read and gives no fix. Spec section 7 promises the opposite contract: every form 2.0 rejects but 1.0 accepts must carry a 2.0 message that names the form and the fix. The author has no way to tell an unparseable value from a deliberate 2.0 rejection.

**Refuter.** The claim reproduces exactly, on both the float path and the radix path, and it breaks the contract sentence in spec section 7.

`yaml_1_1_integer` (crates/core/src/curriculum/load.rs:908-931) recognizes three YAML 1.1 forms only: a `:` sexagesimal, a `0`-prefixed octal, and a `_` group in a decimal that starts with a digit. It has no branch for `0x`, no branch for `0b`, and no branch for a `_` inside a float literal. Every other 1.1 numeric form returns `Yaml11Integer::No`, so `string_number_message` (load.rs:873) falls through to `text.trim().parse::<f64>()` and `int_message` (load.rs:831) falls through to `text.trim().parse::<i64>()`. Rust `str::parse` rejects `_`, so both emit the reserved pydantic text.

I built a fixture tree and ran both loaders. 1.0 loads all eight topics with zero findings and reads real values (0.6, 1.0, 0.5, 1.0, 5e-10, 31, 31, 5). 2.0 reports seven `schema` f

### #6 [major] An integer literal past u128 reports a third message, not the one spec section 7 fixes

File: `crates/core/src/curriculum/load.rs:819` — IDs: R5

**Claim.** serde_norway resolves an integer literal above u128::MAX to an f64 rather than failing with the BIG_INTEGER_MARKERS text, so int_message takes its f64 out-of-range branch and reports 'Unable to parse input string as an integer, exceeded maximum size' instead of the OUT_OF_RANGE_INTEGER text that spec section 7 fixes for every out-of-i64 literal.

**Evidence.**

```
$ target/debug/lint_curriculum <tree>   # expected_time_secs: 18446744073709551616 (20 digits)
  [schema] topics.0.expected_time_secs: integer literal outside the 64-bit range

$ target/debug/lint_curriculum <tree>   # expected_time_secs: 340282366920938463463374607431768211456 (2^128)
  [schema] topics.0.expected_time_secs: Unable to parse input string as an integer, exceeded maximum size

load.rs:818-822
            if !(I64_MIN_AS_F64..I64_MAX_EXCLUSIVE_AS_F64).contains(&float) {
                return Some(
                    "Unable to parse input string as an integer, exceeded maximum size".to_owned(),
                );
            }

Spec section 7: 'An integer literal outside `i64` takes two paths inside the parser ... and both paths report the one message above, with the `schema` code and the dotted location.' The committed fixture crates/core/tests/fixtures/loader-huge-integer/c/00.yaml uses 18446744073709551616, so only the u128-representable path is pinned. 1.0 accepts both literals (Python integers have no upper bound), so this pydantic text also lands on a value 1.0 accepts.
```

**Failure scenario.** A unit file writes `expected_time_secs: 99999999999999999999999999999999999999999`. 1.0 loads it as a Python integer. 2.0 rejects it, but with a message the spec does not allow, and the message differs from the one the same tree gets for a 20-digit literal. A downstream tool or docs page that keys on the fixed OUT_OF_RANGE_INTEGER text to explain the 64-bit limit never matches this case.

**Refuter.** The claim is demonstrable, and the divergence is larger than the claim states. serde_norway 0.9.42 tries u64, i64, u128, i128 in visit_int (src/de.rs:1095-1112). Only the u128/i128 step makes the text that BIG_INTEGER_MARKERS (load.rs:44) matches, so only a literal of 20 to 38 digits reaches OUT_OF_RANGE_INTEGER through parse_finding (load.rs:1010-1017). Above u128::MAX, visit_int returns Err and visit_untagged_scalar falls through to parse_f64 (src/de.rs:1133-1137), which accepts a plain digit string and keeps a finite f64 (src/de.rs:1080-1084). The scalar becomes Value::Number with an f64, so int_message (load.rs:794-822) passes the as_i64, as_u64, is_finite and fract tests and takes the out-of-range f64 branch at load.rs:818-822. Above about 309 digits, parse_f64 overflows to infinity and returns None, the scalar becomes Value::String, and int_message takes a third branch. Result: thr

### #7 [major] A duplicate mapping key in the root mapping of a unit file or of courses.yaml yields a schema finding with no line number ('at line ?')

File: `crates/core/src/curriculum/load.rs:1003` — IDs: C5, R5

**Claim.** FIXM1a's duplicate-key finding reads the line number out of the `serde_norway` error text, but that text carries a location only when the duplicate sits inside a nested mapping; for a duplicate in the document's root mapping the parser writes `duplicate entry with key "topics"` with no location, so `parse_finding` falls back to the literal `?` and the loader reports a fatal finding that names no place in the file, contradicting the message format that spec section 7 documents (`c/00.yaml: duplicate mapping key 'name' at line 5`).

**Evidence.**

```
crates/core/src/curriculum/load.rs:996-1008
    fn parse_finding(rel: &str, error: &str) -> Finding {
        if let Some((_, tail)) = error.split_once(DUPLICATE_KEY_MARKER)
            && let Some((key, tail)) = tail.split_once('"')
        {
            let line = tail
                .split_once("at line ")
                .and_then(|(_, tail)| tail.split(' ').next())
                .unwrap_or("?");            // <- no location in the root case

Raw serde_norway 0.9.42 error text (probe built against the same crate version):
  nested   : ERR topics[0]: duplicate entry with key "name" at line 5 column 3
  toplevel : ERR duplicate entry with key "module"
  toplevel2: ERR duplicate entry with key "unit"

Tree with `topics:` written twice at the root of c/00.yaml:
  $ lintjson r1
  [ { "code": "schema", "fatal": true, "file": "c/00.yaml",
      "message": "c/00.yaml: duplicate mapping key 'topics' at line ?" } ]
  1.0: lint: [('no_kp', "topic 'b' has no knowledge_points"), ...]; loaded topics: ['b']

The only committed fixture, crates/core/tests/fixtures/loader-duplicate-key/c/00.yaml, puts the repeated key inside `topics[0]`, so the test at crates/core/tests/loader.rs:561 pins only the branch that does carry a line.
```

**Failure scenario.** An author merges two edits of `curriculum/calculus-1/03-limits.yaml` and leaves two `topics:` blocks at the root (1.0 silently keeps the last block; 2.0 rejects the file, which is the intended strictness). The 2.0 lint prints `c/00.yaml: duplicate mapping key 'topics' at line ?`. The message names no line, so in a 600-line unit file the author has no location to go to, and the same happens for a repeated `courses:` key in `courses.yaml`, where the whole catalog is dropped from one location-less finding.

**Refuter.** The claim is demonstrable end to end and I cannot refute it.

1. Parser text. I built an independent probe against serde_norway 0.9.42 (the version in Cargo.lock) and confirmed the reviewer's raw output. A duplicate inside a nested mapping carries a location; a duplicate in the root mapping does not. The root case is the bare text `duplicate entry with key "topics"`.

2. Code path. `parse_finding` at crates/core/src/curriculum/load.rs:996-1008 splits on `DUPLICATE_KEY_MARKER` (load.rs:41 = `duplicate entry with key "`), then looks for the substring `at line ` in the tail. The root text holds no such substring, so `unwrap_or("?")` at load.rs:1003 supplies the literal `?`.

3. Real binary. I built `lint_curriculum` from the repo and ran it on two trees I made in the scratchpad. A file with two root `topics:` blocks prints `[schema] c/00.yaml: duplicate mapping key 'topics' at line ?`. A `c

### #8 [major] A YAML 1.2-only number form in an integer field (1e2, 1.0e2, 0o17) passes the 2.0 lint gate but makes 1.0 refuse the whole curriculum

File: `crates/core/src/curriculum/load.rs:784` — IDs: C5, R5

**Claim.** The FIXM1a/FIXM1b lax-integer rule (`as_i64` plus `LaxI64::visit_f64`) accepts any YAML 1.2 float with no fractional part in `expected_time_secs` and `order`, but 1.0 accepts a whole-number float only when PyYAML also resolves the scalar as a float: PyYAML's YAML 1.1 float regex requires a decimal point and a signed exponent, so `1e2`, `1.0e2` and `6e1` resolve to strings and `int()` refuses them, and `0o17` is not a YAML 1.1 integer either — 2.0 therefore lints clean on content that makes 1.0 `Graph.load` raise, a divergence direction that spec section 7's 'Accepted by 2.0, rejected by 1.0' list does not name.

**Evidence.**

```
crates/core/src/curriculum/load.rs:776-788 (as_i64) and crates/core/src/curriculum/model.rs:40-48 (LaxI64::visit_f64) accept every finite float with `fract() == 0.0` in an integer field.

Tree r2, topic `a` with `expected_time_secs: <value>`:
  1e2      2.0 lint findings=0  1.0 Graph.load RAISED CurriculumError
  1.0e2    2.0 lint findings=0  1.0 Graph.load RAISED CurriculumError
  0o17     2.0 lint findings=0  1.0 Graph.load RAISED CurriculumError
  6e1      2.0 lint findings=0  1.0 Graph.load RAISED CurriculumError

Full 1.0 text for `1e2`:
  lint: [('schema', 'topics.0.expected_time_secs: Input should be a valid integer, unable to parse string as an integer', True)]
  Graph.load RAISED CurriculumError [schema] topics.0.expected_time_secs: Input should be a valid integer, unable to parse string as an integer

The same holds in courses.yaml:
  order: 1e2 -> 2.0 findings 0 | 1.0 Graph.load RAISED CurriculumError [schema] courses.0.order: Input should be a valid integer, unable to parse string as an integer

2.0 also writes the value into the dump (`"expected_time_secs":100`) and into the curriculum hash, so the two implementations disagree on the tree, not only on the finding list.

spec section 7 lists only two entries under 'Accepted by 2.0, rejected by 1.0' (a YAML 1.1 scalar in a string field; a directory or broken symlink named `*.yaml`); it lists the lax whole-number float under 'Accepted by 2.0, the same as 1.0', which is false for these forms.
```

**Failure scenario.** An authoring script writes `expected_time_secs: 6e1` (or a hand edit writes `0o17`) into a unit file. `lint_curriculum` reports 0 findings, so the C5 git gate passes the file. The 1.0 deployment still serves learners during the port and loads the graph on every authenticated request: `Graph.load` raises `CurriculumError` for that tree, so every request fails until the file is reverted — a content change the 2.0 gate declared clean takes the running system down.

**Refuter.** The claim reproduces exactly and I found no ground to refute it. `as_i64` (crates/core/src/curriculum/load.rs:776-788) and `LaxI64::visit_f64` (crates/core/src/curriculum/model.rs:40-48) accept any finite float with `fract() == 0.0`, and neither consults the raw scalar text. serde_norway resolves `1e2`, `1.0e2`, `6e1` and `0o17` to numbers under the YAML 1.2 core schema, so `lint_curriculum` reports 0 findings and `dump_curriculum` writes the value into the tree and the hash. PyYAML's YAML 1.1 float regex needs a decimal point and a signed exponent, and its integer regex has no `0o` form, so all four scalars resolve to strings; pydantic lax `int()` then refuses them and 1.0 `load_curriculum` raises `CurriculumError`. The decimal-point form the spec bullet actually names (`60.0`, `order: 1.0`) does match 1.0, so the bullet's examples are right, but the exponent and `0o` forms are an undoc

### #10 [major] The loader silently accepts YAML 1.2-only numeric scalars (1e3, 1.0e2, 0o17) that 1.0 rejects, and no test or spec section covers the direction

File: `crates/core/src/curriculum/load.rs:798` — IDs: C5, D1

**Claim.** PyYAML's YAML 1.1 resolver leaves `1e3`, `1.0e2` (unsigned exponent) and `0o17` as strings, which pydantic then rejects in an integer field, while `serde_norway` resolves all three to numbers that `int_message`/`as_i64` accept — so 2.0 loads, lints clean, and hashes a tree that 1.0 refuses to load at all. Spec section 7 "2.0 strictness" claims to list "every deliberate difference between the two loaders" and lists none of these, and the guard test `the_checked_in_tree_uses_no_yaml_1_1_form` cannot see them (`is_yaml_1_1_form`, loader.rs:748, tests only the six boolean words, `0[0-7]+`, `_` groups and `:` groups).

**Evidence.**

```
Tree with `expected_time_secs: 0o17` (topic a) and `expected_time_secs: 1e3` (topic b):
$ .venv/bin/python scripts/oracle/dump_lint_1_0.py $R
  [schema] topics.0.expected_time_secs: Input should be a valid integer, unable to parse string as an integer
  [schema] topics.1.expected_time_secs: Input should be a valid integer, unable to parse string as an integer
$ target/debug/lint_curriculum $R
  OK: ... is a valid curriculum (0 findings).   exit=0
$ target/debug/dump_curriculum $R -> [('a', 15), ('b', 1000)]
$ .venv/bin/python scripts/oracle/dump_curriculum_1_0.py $R -> CurriculumError, exit=1

Same result for `expected_time_secs: 1.0e2` (YAML 1.1 requires a signed exponent, so PyYAML reads the string "1.0e2"): 1.0 = schema finding, 2.0 = accepted as 100. The reverse case is not symmetric: `1.0e30` is accepted by 1.0 (pydantic lax int of a whole float) and rejected by 2.0 with the pydantic text "Unable to parse input string as an integer, exceeded maximum size", which spec section 7 forbids ("a 1.0 message never appears on a value 1.0 accepts").
```

**Failure scenario.** A content generator writes `expected_time_secs: 1e3` (or `1.0e2`, or `0o17`) into one unit file. The 2.0 lint gate reports 0 findings, the file loads, and `curriculum_hash` is computed over a tree that 1.0 drops the file from — the two loaders disagree with no diagnostic on either the code or the C5 review gate, and the tree-guard test stays green because its scanner does not recognize the forms.

**Refuter.** Not refuted — the defect reproduces exactly as described. PyYAML's YAML 1.1 resolver leaves `0o17`, `1e3` and `1.0e2` as strings (its int regex has no `0o` form; its float regex needs a dot and a signed exponent), so 1.0 emits a fatal `schema` finding and `Graph.load` raises. serde_norway resolves all three to numbers, `as_i64` (load.rs:776) and the `Value::Number` branch of `int_message` (load.rs:798) accept them, so 2.0 loads 15, 1000 and 100, lints with 0 findings, and hashes a tree that 1.0 cannot load at all. Spec section 7 "2.0 strictness" claims to list every deliberate loader difference and lists none of these forms, and the tree guard `is_yaml_1_1_form` (tests/loader.rs:748) cannot detect them, so the C5 review gate has no diagnostic. The only part of the claim that fails is the secondary reverse case: 1.0 rejects `1.0e30` AND `1.0e+30`, so the "a 1.0 message never appears on a 

### #11 [major] The lint's "a repeated course id keeps the LAST catalog entry" rule is unpinned; the round-1 fix pinned only the arena copy

File: `crates/core/src/curriculum/lint.rs:388` — IDs: C5

**Claim.** Round-1 findings #7/#9 (1.0 writes `{c.id: c for c in catalog.courses}`, so a repeated course id resolves to the last entry) were fixed with the arena fixture `arena-course-duplicate`, but `lint_curriculum` holds its own `course_by_id` map at lint.rs:386-389 and no lint fixture declares a repeated course id, so turning that line into first-wins leaves all 94 tests green while changing the `unreachable_from_floor` output.

**Evidence.**

```
Mutant at lint.rs:388:
  -            course_by_id.insert(course.id.as_str(), course);
  +            course_by_id.entry(course.id.as_str()).or_insert(course);
$ cargo test -p cadus-core --tests  ->  94/94 green (no FAILED line)

Tree: courses.yaml declares c1 (order 1), mid (order 2), c1 again (order 3), c2 (order 4, mastery_floor_course: c1); c1/ holds no unit file; mid holds topic mm; c2 holds topic b whose prerequisite is mm.
$ .venv/bin/python scripts/oracle/dump_lint_1_0.py $R
   empty_course course c1 has no unit files
   empty_course course c1 has no unit files
$ target/debug/lint_curriculum $R (repo)   -> the same two findings
$ target/debug/lint_curriculum $R (mutant) -> the same two, plus
  [unreachable_from_floor] (b) topic 'b' is not reachable from course c2's floor/roots
```

**Failure scenario.** A catalog repeats a course id with a different `order` (a copy-paste in courses.yaml) and another course grounds its floor on it with `mastery_floor_course`. 1.0 resolves the reference to the last entry and unions every course at or below order 3; a first-wins port unions only order 1 and reports a spurious `unreachable_from_floor` finding, failing the C5 review gate on a tree 1.0 calls clean. The fixture set cannot distinguish the two implementations.

**Refuter.** The claim is demonstrable and I reproduced every step of it independently. `lint_curriculum` builds a second, private `course_by_id` map at /home/deploy/dev/cadus2.0/crates/core/src/curriculum/lint.rs:386-389 (`course_by_id.insert(course.id.as_str(), course);`). That map is the only thing that decides which catalog row a `mastery_floor_course` reference resolves to for the `unreachable_from_floor` rule (parity trap 12, the union over `other.order <= reference.order`). The lint deliberately does not use the arena ("No arena", lint.rs:15-20), so the round-1 fix in arena.rs and the fixture `arena-course-duplicate` give this line no protection. The lint oracle test walks the 22 names in `FIXTURES` (crates/core/tests/lint.rs:40-63); I read every `courses.yaml` under crates/core/tests/fixtures/lint/ and none declares a course id twice. Only three fixture trees use `mastery_floor_course` at all

### #12 [major] The `sorted(module_courses.items())` site of the lint is unpinned, so the `order_not_id_order` fixture's "every sorted() site" claim is false

File: `crates/core/src/curriculum/lint.rs:336` — IDs: C5

**Claim.** Round-1 finding #24 was fixed with a fixture whose comment states it makes "every `sorted()` site of the lint" visible, but no fixture holds two modules that each span multiple courses, so the ordering of the `module_inconsistent` "spans multiple courses" findings (1.0 `for module, courses in sorted(module_courses.items())`) is untested: reversing it keeps all 94 tests green.

**Evidence.**

```
Mutant at lint.rs:336:
  -    for (module, courses) in &module_courses {
  +    for (module, courses) in module_courses.iter().rev() {
$ cargo test -p cadus-core --tests  ->  94/94 green

Tree: c1 and c2 each hold one unit in module "Zeta" (authored first) and one in module "Alpha".
$ .venv/bin/python scripts/oracle/dump_lint_1_0.py $R
   module_inconsistent module 'Alpha' spans multiple courses: ['c1', 'c2']
   module_inconsistent module 'Zeta' spans multiple courses: ['c1', 'c2']
$ target/debug/lint_curriculum $R (repo)   -> Alpha, then Zeta   (matches 1.0)
$ target/debug/lint_curriculum $R (mutant) -> Zeta, then Alpha   (diverges, tests green)

The three multi-rule fixtures (`many_codes`, `cycle_duplicate_missing_ref`, `order_not_id_order`) each trip the "spans multiple courses" rule at most once, so the byte-for-byte comparison never sees this ordering.
```

**Failure scenario.** A tree in which two module names each appear under two courses (a realistic shared-module refactor) is linted: 1.0 emits the two findings in ascending module-name order, a port that walks the map in any other order emits them swapped, and the runner output that the C5 review reads no longer matches 1.0 — with the whole fixture suite still green.

**Refuter.** The claim is demonstrable in all three parts, so I cannot refute it.

1. The premise is true. 1.0 `cadus/graph.py:780` holds a fourth `sorted()` site — `for module, courses in sorted(module_courses.items())` — that is separate from the three sites named in round-1 finding #24 (`sorted(topics)`, `sorted(core_dependents)`, `sorted(course_topics - reachable)`). Only three fixture trees trip the "spans multiple courses" rule (`module_inconsistent`, `many_codes`, `cycle_duplicate_missing_ref`), and each one emits exactly one such finding, always for module `'Shared'` with courses `['c1', 'c2']`. No fixture holds two modules that each span two courses, so the byte-for-byte comparison in `every_fixture_matches_the_committed_1_0_output_byte_for_byte` iterates a one-element result at that site. A reversal of a one-element walk is a no-op.

2. The mutant survives. I copied the tree to a scratchpad

### #13 [major] A unit file whose name is not valid UTF-8 is silently dropped from the load, with no finding

File: `crates/core/src/curriculum/load.rs:315` — IDs: R5, C5, D1

**Claim.** FIXM1a removed the `is_file()` and dot-prefix filters from `unit_file_names` but left the `entry.file_name().to_str()` filter, so a unit file whose name is not valid UTF-8 is skipped with no finding — the same silent content drop that review-1 blocker #2 was raised for, and the doc comment the fix added on line 308 ("The name test is the only test") is false in the port.

**Evidence.**

```
crates/core/src/curriculum/load.rs:313-320
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !name.ends_with(UNIT_EXTENSION) {
            continue;
        }

Demonstrated on a tree holding `c/00-plain.yaml` and `c/01-caf\xe9.yaml` (the second name is a Latin-1 byte, not UTF-8):

  $ python  # 1.0
  topics: ['latin', 'plain'] units: 2

  $ ./target/debug/dump_curriculum <tree>
  topics: ['plain'] units: 1
  sha256=e9d0efdb1330099aae4bb4066680733e4b04e154d67e86be89927bb459bfb8c1

  $ ./target/debug/lint_curriculum <tree>
  OK: <tree> is a valid curriculum (0 findings).

Python `pathlib.Path.glob` decodes directory names with surrogateescape, so `*.yaml` matches the name and `open()` reads the file; 1.0 loads both units. Spec section 1 (line 18-20) states the name test is the only test the port applies.
```

**Failure scenario.** A course directory holds a unit file whose name carries a non-UTF-8 byte (a file copied from a Latin-1 filesystem, or an archive extracted with a legacy encoding). 1.0 loads it: 2 units, topics `latin` and `plain`. 2.0 skips it with no `yaml` and no `schema` finding: `lint_curriculum` prints `OK: ... (0 findings)`, `dump_curriculum` reports 1 unit and 1 topic, and `curriculum_hash` silently changes. Every topic in that file, and every prerequisite edge into it, vanishes from the arena while the CI gate stays green.

**Refuter.** I tried to refute the claim and failed. The divergence reproduces end to end on the current binaries against the 1.0 oracle.

What the code does. /home/deploy/dev/cadus2.0/crates/core/src/curriculum/load.rs:313-319 applies two tests, not one: `entry.file_name().to_str()` (a UTF-8 test) and `name.ends_with(UNIT_EXTENSION)` (the name test). A directory entry whose name is not valid UTF-8 hits the `else { continue; }` on lines 315-317. That arm pushes no `Finding`, and `unit_file_names` has no access to the findings vector (signature: `Result<Vec<String>, std::io::Error>`), so the drop cannot be reported from there. `ParsedUnit.file_name`, `RawUnit.file_name`, and `RawTopic.file_name` are all `String`, so the UTF-8-only domain is structural, not incidental.

What 1.0 does. `cadus/graph.py:593` is `sorted(course_dir.glob("*.yaml"))` and `cadus/graph.py:539-541` is `path.open(encoding="utf-8"

### #14 [major] Spec section 7 claims to list every deliberate loader difference, but at least two integer-literal forms diverge and appear in neither table

File: `docs/reference/curriculum-1.0-spec.md:243` — IDs: R5, C5

**Claim.** The "2.0 strictness" section states it "lists every deliberate difference between the two loaders", yet `0o17` (YAML 1.2 octal — accepted by 2.0, rejected by 1.0) and `08` (a leading-zero decimal — rejected by 2.0, accepted by 1.0) both flip the accept/reject decision and appear in neither of the section's three lists.

**Evidence.**

```
docs/reference/curriculum-1.0-spec.md:241-244
  "2.0 reads YAML with `serde_norway`, which resolves the YAML 1.2 core schema, and
   2.0 does not emulate PyYAML. This section lists every deliberate difference
   between the two loaders."

Case A — `expected_time_secs: 0o17` (plus 0x1f and 0b1010 in the same file):
  1.0 lint: [schema] topics.2.expected_time_secs: Input should be a valid integer,
            unable to parse string as an integer      <- whole file dropped, 3 topics lost
  2.0 lint: OK (0 parse findings); dump shows  oct12 15,  hex 31,  bin 10
  yaml.safe_load confirms PyYAML reads 0x1f=31, 0b1010=10, '0o17'=the string "0o17".

Case B — `expected_time_secs: 08`:
  1.0 lint: [no_kp] / [missing_diagnostic_exemplar] only  -> file loads, value 8
  2.0 lint: [schema] topics.0.expected_time_secs: string '08' is not accepted; write 8
            -> whole file dropped

The section's "Rejected by 2.0, accepted by 1.0" table (line 248) names 030, 1_200, 1:30 and quoted integers; its "Accepted by 2.0, rejected by 1.0" list (line 278) names only YAML 1.1 scalars in string fields and a directory named *.yaml.
```

**Failure scenario.** A content author writes `expected_time_secs: 08` in a unit file. The 1.0 gate accepts the tree (the value is 8); the 2.0 gate drops the whole unit file with a `schema` finding, so every topic in it disappears. Conversely `expected_time_secs: 0o17` passes the 2.0 gate as 15 while 1.0 refuses the file. A reader who trusts the exhaustiveness sentence has no way to predict either outcome from the spec, and the M1 rule "a divergence from 1.0 is a finding unless spec section 7 names it" cannot be applied to these two forms.

**Refuter.** The claim is demonstrable. I reproduced both cases end to end with `target/debug/lint_curriculum`, `target/debug/dump_curriculum`, and the 1.0 oracle at /home/deploy/dev/cadus.

Case A (`0o17`) is a real accept/reject flip in the silent direction, and no list in the section names it. 2.0 reads `0o17` as 15 and reports 0 parse findings; 1.0 drops the whole file with a fatal `schema` finding, and the 1.0 dump raises `CurriculumError`, so 3 topics disappear on the 1.0 side. The third list (docs/reference/curriculum-1.0-spec.md:278) names only a YAML 1.1 scalar in a string field and a directory named `*.yaml`. `0x1f` and `0b1010` do not flip (both loaders give 31 and 10), so `0o17` is the one form of the three that diverges.

Case B (`08`) is a real flip in the other direction. 1.0 reads the string `08` and pydantic lax mode coerces it to 8, so the file loads and only graph findings appear. 

### #15 [major] Spec section 6 states a wrong non-ASCII file count and cites the wrong line for the escaped-backslash example

File: `docs/reference/curriculum-1.0-spec.md:204` — IDs: C5, R5

**Claim.** Section 6 claims "85 of 88 files contain non-ASCII" when 32 of the 88 unit files do, and cites `proofs/01-proof-techniques.yaml:47` for the double-quoted `\\mid` scalar when line 47 is a plain `name:` field and the scalar is on line 56 — the same class of wrong-measurement defect as confirmed review-1 finding #26.

**Evidence.**

```
docs/reference/curriculum-1.0-spec.md:203-209
  "...YAML single-quoted scalars; 85 of 88 files contain non-ASCII."
  "A double-quoted YAML scalar with `\\mid` resolves to one backslash
   (`proofs/01-proof-techniques.yaml:47`)."

Measured on the checked-in tree:
  $ for f in $(find curriculum -mindepth 2 -name '*.yaml'); do
      LC_ALL=C grep -qP '[\x80-\xff]' "$f" && c=$((c+1)); done
  32 of 88            (courses.yaml has 0 non-ASCII lines)

  $ sed -n '47p' curriculum/proofs/01-proof-techniques.yaml
          name: "Direct proofs about divisibility"

  $ grep -n '\\\\mid' curriculum/proofs/01-proof-techniques.yaml
  56:        constraints: "use the definition $a \\mid b$ means $b = ak$ for some integer $k$"

(Every other section-3 fact and every graph.py/model.py line citation I checked does hold: 13/88/1090/3138/6800/2144, 3281 declared prereq edges, 3200 forward / 3282 reverse, 82 weight-0 edges, 0 duplicated pairs, key_prerequisites 4417, key=true 1820, cross-course 791, floors 3/7/285, and graph.py:570-572, :583, :593, :259-270, :291, :292-293, :295-296, :314-321, :178-198.)
```

**Failure scenario.** A maintainer verifying the escaped-backslash parity rule — the rule that makes `\\mid` render as one backslash in the canonical dump, and therefore feeds the curriculum hash — opens `curriculum/proofs/01-proof-techniques.yaml:47`, finds an ordinary plain `name:` scalar with no backslash, and concludes either that the rule is obsolete or that the tree changed under the spec. The wrong 85-of-88 figure likewise misstates how much of the tree the UTF-8 path actually exercises, so a reviewer sizing the risk of an encoding change reads a number nearly three times the truth.

**Refuter.** The claim is demonstrable on both counts, and both are wrong-measurement defects of the same class as confirmed review-1 finding #26.

1. Non-ASCII count (docs/reference/curriculum-1.0-spec.md:204). The spec states "85 of 88 files contain non-ASCII". A byte scan of the 88 unit files finds 32 files with a byte in [0x80-0xff]. The count is 32 for the full 89-file glob too, so courses.yaml adds nothing. The figure 85 matches no other property of the tree that I tested: all 88 unit files contain `$`, `exemplars:`, `solution_sketch`, `key_prerequisites`, and a double-quoted scalar; 81 contain a single-quoted scalar; 38 contain a backslash. So 85 is not a mislabeled measurement of a neighboring fact — it is simply wrong, and it overstates the UTF-8 exposure of the tree by a factor of about 2.7.

2. Line citation (docs/reference/curriculum-1.0-spec.md:209). The spec cites `proofs/01-proof-techn
