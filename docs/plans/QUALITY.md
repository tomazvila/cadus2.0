# Quality gate: eleven code limits on the whole tree

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
| wave 3 | cross-unit clones, the full `scripts/quality.sh` | port 55434 |

## Mutation testing dropped

The owner dropped the mutant limit on 2026-09-05 ("Ditch mutant testing altogether"). The
cargo-mutants and Stryker runs took 16 hours and one hour per pass on this box. The tests
that the mutant units wrote before the stop stay in the tree: they pin exact values and
cost nothing. The gate has no mutant check, and the tooling is removed.
