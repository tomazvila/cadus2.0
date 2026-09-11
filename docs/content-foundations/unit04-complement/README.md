# Unit04 complement: historical checkpoint and current suite

The historical checkpoint is based on integration `d0be8747`. It contains pending-only content for the five Unit04 KPs that were still live when the external eight-KP patch arrived:

- `proportional-relationships/kp2`
- `graphing-proportional-relationships/kp3`
- `horizontal-vertical-slopes/kp3`
- `slope-as-rate-of-change/kp2`
- `graphing-linear-equations/kp3`

The patch's three already-integrated KPs were excluded: `constant-of-proportionality/kp2`, `slope/kp3`, and `reading-slope-intercept-equations/kp3`. Their current curriculum and recipes remain unchanged.

The current source adds six independently checked families: `coordinate-plane/kp1`, `horizontal-vertical-slopes/kp1`, `horizontal-vertical-slopes/kp2`, `solutions-of-two-variable-equations/kp1`, `graphing-from-a-table/kp2`, and `slopes-of-parallel-perpendicular-lines/kp3`. `scope.json` remains the immutable five-family baseline; the test pins the union of that baseline and these six additions.

## Evidence

- `templates.json` contains eleven pending templates and 132 exhaustive, distinct instances.
- `gate-checkpoint.json` and `audit-checkpoint.json` retain the historical five-family checkpoint evidence. A current gate file must be regenerated from the checked-out source before the current semantic suite runs.
- The independent semantic test reconstructs every answer from learner-visible text and rejects field corruption, swapped answers, and generic sketches.
- `audit-checkpoint.json` records the historical checkpoint audit delta: issue KPs 59 to 54; absent recipes 59 to 54; fewer-than-four exemplars 47 to 42; missing sketches 18 to 16; undecidable answers 25 to 22. Duplicate, generic, singleton, and orphan counters do not regress.
- The templates use existing two-part multipart behavior, including nested coordinate pairs and current label writers. No evaluator or parser change is included.

No content was approved or imported. No database, model endpoint, deployment, or production state was used.

## Reproduce

Run from the repository root with Cargo restricted to one job:

```sh
CARGO_BUILD_JOBS=1 SQLX_OFFLINE=true cargo run -p cadus-worker --example unit04_complement_gate --locked --offline -- target/unit04-complement/gate.json
CARGO_BUILD_JOBS=1 SQLX_OFFLINE=true cargo run -p cadus-core --bin content_audit_facts --locked --offline -- curriculum > target/unit04-complement/after-facts.json
python3 scripts/review/foundations_content_audit.py --facts target/unit04-complement/after-facts.json --content-root docs/content-foundations --output target/unit04-complement/after-audit.json
python3 scripts/authoring/test_unit04_complement.py target/unit04-complement/after-facts.json target/unit04-complement/gate.json -v
```
