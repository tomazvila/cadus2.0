# Unit05 systems tail: source acceptance

All 13 assigned KPs are source-audit clean: 52 authored exemplars and 190 exhaustive pending instances. The course audit moves from 93 to 80 unresolved KPs. These are source closures; content remains pending and has not been imported or approved.

Baseline `47f2532` is the supplied source mapping of live `7bebe2aa`. `baseline-audit.json` confirms all 13 keys were `absent_pending_template_recipe` before editing. Live state was not queried. `final-audit.json` contains exact per-KP counts, entropy, audit counts, compiler identity, and the SHA-256 of fresh production-adapter facts.

## Exact closures

| KP | Exemplars | Pending instances | Distinct answers |
|---|---:|---:|---:|
| checking-systems-solutions/kp1 | 4 | 12 | 12 |
| checking-systems-solutions/kp2 | 4 | 22 | 22 |
| checking-systems-solutions/kp3 | 4 | 12 | 12 |
| graphing-systems/kp3 | 4 | 12 | 12 |
| systems-special-cases/kp1 | 4 | 12 | 2 |
| systems-special-cases/kp2 | 4 | 24 | 3 |
| systems-special-cases/kp3 | 4 | 24 | 3 |
| systems-of-linear-inequalities/kp1 | 4 | 12 | 12 |
| systems-of-linear-inequalities/kp2 | 4 | 12 | 12 |
| systems-of-linear-inequalities/kp3 | 4 | 12 | 12 |
| systems-mixture-problems/kp1 | 4 | 12 | 12 |
| systems-mixture-problems/kp2 | 4 | 12 | 12 |
| systems-mixture-problems/kp3 | 4 | 12 | 12 |

Every declared numeric axis affects the mathematical task and can change its answer. Numeric/coordinate records are distinct across every satisfying tuple. Classification has its natural two- or three-answer vocabulary: KP1 has 9 none and 3 infinite instances; KP2 and KP3 each have 18 unique, 3 none, and 3 infinite instances. Entropies are 0.811278 and 1.061278 bits respectively. No text-choice axis pads the domain. The independent suite rejects constant and cancelling output families.

## Contracts and scope

- Equation and inequality verification: `residuals = (first,second); solution = yes/no`. Both substituted left-minus-right differences are assessed. For the rejection KP, exactly one equation passes on all 22 satisfying tuples.
- Candidate selection: an exact ordered coordinate pair. Both candidate positions are successful across pending instances. Inequality candidates are off both boundaries.
- Special-case KP1: `infinite` and `none` are genuine yes/no propositions with exactly one true. KP2 and KP3 use `one` and `infinite`; both false means no solution, and both true is invalid. Independent determinant/rank reconstruction checks the classification of nondegenerate linear equations. Missing or malformed fields and contradictory classifications fail grading.
- Overlap regions: `solid = (m,c,d); dashed = (m,c,d)`, with boundary `y=mx+c` and direction `d=1` above or `d=-1` below. The solid boundary is included and the dashed boundary excluded. This is a bounded representation of the intersection of two nonvertical, nonparallel half-planes, one strict and one inclusive. The oracle reconstructs both oriented boundaries, strictness, and exact membership at/on/either side of the crossing. It refuses out-of-scope geometry and wrong direction codes. It does not claim support for arbitrary region prose or polygons.
- Mixture setup: `(T,p,q,M)` completes the explicit forms `x+y=T` and `px+qy=M`. Solving records give ordered ingredient amounts in the requested units. Independent parsing handles percentages, pure fractions, dilution, total substance/value, cents, and grams; rational balance equations prove feasible positive amounts.

All contracts and expressions use the existing `answer_for_contract`. The two-argument multipart parser limit is preserved. No grading implementation, production threshold, unrelated KP, diagnostic exemplar, or reference visual changed.

## Verification

Run from this checkout with Cargo, rustc, rustfmt, and Clippy available:

`python3 scripts/authoring/check_unit05_tail.py`

The available Nix compiler is Rust 1.92; the repository declares 1.94. The recorded run explicitly used the compatibility override:

`nix-shell -p cargo rustc rustfmt clippy --run 'python3 scripts/authoring/check_unit05_tail.py --ignore-rust-version'`

Result: `UNIT05 SOURCE ACCEPTANCE OK: 13 closed; 190 pending; 80 course residuals. No content approved.`

`check_unit05_tail.py` reproduces the historical source-acceptance checkpoint, including its exact baseline source-scope assertions. For a current-head semantic recheck, generate facts from the same checkout and pass that file explicitly to each tracked semantic entrypoint:

```sh
mkdir -p target/unit05-current
CARGO_BUILD_JOBS=1 cargo run --quiet -p cadus-core --bin content_audit_facts -- curriculum > target/unit05-current/facts.json
python3 scripts/authoring/test_unit05_residual.py target/unit05-current/facts.json -v
python3 scripts/authoring/test_unit05_residual_second.py target/unit05-current/facts.json -v
python3 scripts/authoring/test_unit05_tail_mixtures.py target/unit05-current/facts.json -v
python3 scripts/authoring/test_unit05_tail_systems.py target/unit05-current/facts.json -v
sha256sum target/unit05-current/facts.json
```

Checked-in `authored-facts.json`, baseline audit files, and checkpoint facts are historical evidence. They are not inputs to this current-head recheck.

- 23 Python tests pass: 8 new semantic/materiality tests, 6 prior Unit05 regression tests, and 9 audit tests. New tests independently reconstruct all 52 exemplars and 190 pending instances, check normalized collisions against parseable curriculum/prior pending systems, exhaust domains, perturb mathematical inputs and answers, and check entropy and active axes.
- 6 Rust tests pass: the production authoring gate and current answer evaluator cover the 190 new instances plus 204 prior Unit05 instances. Well-formed perturbed answers grade incorrect; malformed answers, false samples, and constant/cancelling expressions are rejected.
- Whole-workspace `cargo fmt --all --check`, focused worker Clippy with `-D warnings`, `git diff --check`, unchanged-outside-target-exemplar-blocks verification, and code limits pass. Largest added code file: 224 lines; largest function: 54 lines.

Verification on declared Rust 1.94, full database-backed release/quality gates, browser rendering, live readiness, and human content review remain unverified. Nothing was pushed, integrated, deployed, imported, or approved. No test processes remain running.

## Patch checkpoints

Checkpoint 1 was committed immediately after its focused green tests as `b81a78d`: the three mixture KPs, 12 exemplars, and 36 pending instances. `checkpoint-1-audit.json` records the interim course residual of 90. Its exported patch is `patches/0001-Author-and-verify-Unit05-mixture-systems-pending-sou.patch`.

The subsequent patch contains the remaining ten KPs, the aggregate production gate, independent geometry/semantic tests, and the final source acceptance report. Apply the patches in their numbered order when integration is authorized.
