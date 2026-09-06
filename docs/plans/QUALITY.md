# Quality gate: ten code limits on the whole tree

Owner request (2026-09-03): hold these limits on the whole codebase.

| Limit | Value | Rust check | Web check |
|---|---|---|---|
| Lines per file | < 500 | `scripts/quality/loc.py` | same |
| Cyclomatic complexity | < 22 | rust-code-analysis | ESLint `complexity` |
| Cognitive complexity | < 22 | rust-code-analysis | eslint-plugin-sonarjs |
| Halstead difficulty | < 80 | rust-code-analysis | `web/scripts/halstead.mjs` |
| Test coverage | 100% | cargo llvm-cov, `src/` only | Vitest v8, `src/` only |
| CRAP | < 25 | `rust_coverage.py` | `web-coverage.mjs` |
| Dead code | 0 | clippy, `rust_dead.py`, cargo-machete | knip |
| Redundant code | 0 | jscpd (50 tokens, 5 lines) | jscpd |
| `any` or `unknown` | 0 | not applicable | ESLint `no-restricted-syntax` |

`scripts/quality.sh` runs every check and prints one PASS or FAIL line per check.
Lines per file counts physical lines. Coverage measures `src/` files: the tests are the
instrument, not the subject. CRAP is `cc^2 * (1 - coverage)^3 + cc` per function.

## Baseline at `d740f68` (2026-09-03)

| Measure | Rust (169 files, 115,779 lines) | Web (99 files, 18,393 lines) |
|---|---|---|
| Files at or over 500 lines | 92 (39 src, 53 tests) | 7 (2 src, 5 tests) |
| Functions over a complexity limit | 41 | 4 |
| Halstead difficulty, max | 63 | 36 |
| Unused public items / knip items | 8 | 36 |
| Clone pairs | 511 | 61 |
| Coverage | lines 92.1%, functions 87.7%, regions 90.6% | statements 96.4%, branches 87.2%, functions 79.1% |
| Files below 100% coverage | 82 of 88 | 38 of 49 |
| Functions with CRAP >= 25 | 569 | see coverage |
| `any` or `unknown` sites | not applicable | 81 |

## Units

Each unit works in a worktree on a branch from `quality/gate`, owns its files alone, and
reports before-and-after numbers. Prompts: `~/.cache/cadus2_scripts/quality/`.

| Unit | Scope | Database |
|---|---|---|
| u1-core-answer | `core/src/answer`, `core/src/template`, `numeric.rs`, their tests | none |
| u2-core-curriculum | `core/src/curriculum`, `pool`, `bin`, `config`, `instruction`, `learner`, `xp`, their tests | none |
| u3-core-selector | `selector`, `projector`, `fire`, `event`, `diagnostic`, their tests, `tests/common/mod.rs` | none |
| u4-store | `crates/store`, `crates/model-client` | port 55435 |
| u5-web | `crates/web` | port 55436 |
| u6-worker | `crates/worker` | port 55434 |
| u7-spa | `web/` | none |
| u8, u9 | mutant kills in core, model-client, worker, store (stopped 2026-09-05; the tests stay) | 55435 |
| u10-scripts | `scripts/` (`check_ops.sh` split into `check_ops.d/`, the oracle scripts split and shared) | none |
| u11-cross-clones | clones that span two crates: the `cadus-testkit` crate, `cadus_store::shutdown`, paged review | 55435 |
| u12-store-gap | one store test that read the maintenance database's state | 55434, 55435 |

## Mutation testing dropped

The owner dropped the mutant limit on 2026-09-05 ("Ditch mutant testing altogether"). The
cargo-mutants and Stryker runs took 16 hours and one hour per pass on this box. The tests
that the mutant units wrote before the stop stay in the tree: they pin exact values and
cost nothing. The gate has no mutant check, and the tooling is removed.

## Result at `9f59c98` (2026-09-06)

`scripts/quality.sh` with `CADUS_TEST_DATABASE_URL=postgresql://test:test@127.0.0.1:55434/cadus2_gate`
prints `QUALITY GATE PASSED` (log `target/quality/gate-run3.txt`). The PASS lines:

| Check | Line |
|---|---|
| loc | `loc: 723 files, 0 at or over 500 lines` |
| rust-complexity | `rust complexity: 7745 functions, 0 over a limit` (Rust and Python) |
| rust-dead | `rust dead code: 0 unused public items` |
| rust-unused-deps | cargo-machete: none |
| rust-clippy | clean at `-D warnings` |
| rust-clones | `Found 0 clones.` over `crates` and `scripts` |
| rust-coverage | `lines 100.00% functions 100.00% regions 100.00%; 0 files below 100%; 0 functions with CRAP >= 25` |
| web-lint | clean: complexity, cognitive complexity, no `any`, no `unknown` |
| web-types | clean |
| web-halstead | `functions=2912 over_limit=0 max=25.9` |
| web-dead | knip: none |
| web-clones | `Found 0 clones.` |
| web-coverage | `60 files, 0 below 100%; 0 functions with CRAP >= 25` |

Before and after, whole tree:

| Measure | Baseline `d740f68` | Result `9f59c98` |
|---|---|---|
| Source files | 268 | 723 |
| Files at or over 500 lines | 99 | 0 |
| Functions over a complexity limit | 45 | 0 |
| Unused public items and knip items | 44 | 0 |
| Clone pairs | 572 | 0 |
| Rust coverage, lines | 92.1% | 100% |
| Web coverage, statements | 96.4% | 100% |
| Functions with CRAP >= 25 | 569 | 0 |
| `any` or `unknown` sites | 81 | 0 |
| Rust tests | 1,818 | 2,312 |

New crate: `crates/testkit` (`cadus-testkit`), the shared test instruments (bench harness,
purity guards, source scan, fake HTTP replies, process helpers), a dev-dependency of the
five crates. New module `cadus_store::shutdown` serves both binaries. Scripts: `scripts/check_ops.d/`
holds the parts of `check_ops.sh`; the oracle scripts share `_common` modules.

Scope notes. Coverage and CRAP measure Rust `src/` and web `src/`; `scripts/` has no test
harness, so the line, complexity (Python) and clone limits apply there and coverage does
not. Bash has the line and shellcheck limits only. The tests are the instrument and are
held to the line, complexity, dead-code and clone limits, not to coverage.
