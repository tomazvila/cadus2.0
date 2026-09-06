# Unit04 complement: five live closures

This checkpoint is based on integration `d0be8747`. It contains pending-only content for the five Unit04 KPs that were still live when the external eight-KP patch arrived:

- `proportional-relationships/kp2`
- `graphing-proportional-relationships/kp3`
- `horizontal-vertical-slopes/kp3`
- `slope-as-rate-of-change/kp2`
- `graphing-linear-equations/kp3`

The patch's three already-integrated KPs were excluded: `constant-of-proportionality/kp2`, `slope/kp3`, and `reading-slope-intercept-equations/kp3`. Their current curriculum and recipes remain unchanged.

## Evidence

- `templates.json` contains five pending templates and 60 exhaustive, distinct instances.
- `gate-checkpoint.json` records production `verify_kind`, `Compiled::instantiate`, current `answer_for_contract`, 60 wrong-sample refusals, and five sub-floor refusals.
- The independent semantic test reconstructs every answer from learner-visible text and rejects field corruption, swapped answers, and generic sketches.
- `audit-checkpoint.json` records the current-head audit delta: issue KPs 59 to 54; absent recipes 59 to 54; fewer-than-four exemplars 47 to 42; missing sketches 18 to 16; undecidable answers 25 to 22. Duplicate, generic, singleton, and orphan counters do not regress.
- The templates use existing two-part multipart behavior, including nested coordinate pairs and current label writers. No evaluator or parser change is included.

No content was approved or imported. No database, model endpoint, deployment, or production state was used.

## Reproduce

Run from the repository root with Cargo restricted to one job:

```sh
python3 scripts/authoring/unit04_complement_first.py
CARGO_BUILD_JOBS=1 SQLX_OFFLINE=true cargo run -p cadus-worker --example unit04_complement_gate --locked --offline -- target/unit04-complement/gate.json
CARGO_BUILD_JOBS=1 SQLX_OFFLINE=true cargo run -p cadus-core --example content_audit_facts --locked --offline -- curriculum > target/unit04-complement/after-facts.json
python3 scripts/review/foundations_content_audit.py --facts target/unit04-complement/after-facts.json --content-root docs/content-foundations --output target/unit04-complement/after-audit.json
python3 scripts/authoring/test_unit04_complement.py target/unit04-complement/after-facts.json target/unit04-complement/gate.json -v
```
