# Unit05 systems tail: isolated source checkpoints

Baseline `47f2532` is the supplied source mapping of live `7bebe2aa`. The checked-in baseline audit confirms all 13 assigned keys lack pending recipes. Live state was not queried.

## Checkpoint 1: mixtures

Closed source-audit keys: `systems-mixture-problems/kp1`, `kp2`, `kp3`. Each has four distinct exemplars and 12 exhaustive pending instances. Course issue/absent-pending counts move from 93 to 90. No content is approved or imported.

The setup contract records `(T,p,q,M)` for the explicitly supplied equation forms `x+y=T`, `px+qy=M`. Solving contracts return ordered ingredient amounts in the units requested by the prompt. Tests independently parse the actual quantities, reconstruct amount and substance/value equations, solve with rational arithmetic, and reject perturbed quantities and answers. Collision checks include parseable two-equation curriculum exemplars and prior pending systems. All 12 answers per recipe are distinct (entropy log2(12)); changing either numeric axis changes the result.

Verification commands (from this checkout):

- `nix-shell -p cargo rustc --run 'cargo run --ignore-rust-version --quiet -p cadus-core --bin content_audit_facts -- curriculum > /tmp/cadus-u05-current-facts.json'`
- `python3 scripts/authoring/test_unit05_tail_mixtures.py /tmp/cadus-u05-current-facts.json` — 3 tests pass.
- `nix-shell -p cargo rustc --run 'SQLX_OFFLINE=true cargo test --ignore-rust-version -p cadus-worker --test unit05_systems_tail'` — 2 tests pass, including 36 exhaustive instances through production `verify_kind` and `answer_for_contract`, false samples, malformed answers, and constant/cancelling expressions.
- Rustfmt on the added Rust test, Python AST function lengths <=70, each added code file <500 lines, and `git diff --check` pass.

The available Nix compiler is Rust 1.92; the repository declares 1.94. These focused builds passed with Cargo's rust-version compatibility override; no source/toolchain requirement was changed. Verification on declared Rust 1.94, the full database-backed release gate, browser rendering, integration, deployment, and live readiness remain unverified. Nothing was pushed, integrated, deployed, or approved.
