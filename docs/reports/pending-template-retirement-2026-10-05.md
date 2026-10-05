# Pending-template retirement, 2026-10-05
The Foundations content commits of 2026-09-23 to 2026-10-05 rewrote the exemplars of the knowledge points below. Their reviewed pending templates still followed the old answer format, so the templates were removed from their pending sets. The curated exemplars are the source for these knowledge points. No template was approved or imported, and no database row changed.

Each `*-retired-pending-templates.json` report beside this note keeps every removed row whole (`body`), with its canonical SHA-256 (`previous_digest`), the pending artifact it came from (`source`), the curriculum commits that rewrote the knowledge point (`superseded_by`), and the verdict of the current production gate (`current_gate`, `refusal_code`, `refusal`). The worker tests of each set assert that the key is gone from the pending set, that the report holds the removed row byte for byte, and that the gate still gives the recorded verdict.

| Set | Knowledge point | Current gate | Rewritten by |
|---|---|---|---|
| symbolic-repair (shard 9) | absolute-value-equations/kp3 | accepted; the count question no longer matches the Solve items | 88a9a73c, 2d82c3f9 |
| symbolic-repair (shard 9) | equations-special-cases/kp1 | accepted; the count question no longer matches the Solve items | 88a9a73c, 2d82c3f9 |
| symbolic-repair (shard 10) | equations-special-cases/kp2 | accepted; the count question no longer matches the Solve items | 88a9a73c, 2d82c3f9 |
| symbolic-repair (shard 11) | equations-special-cases/kp3 | accepted; the "one solution" label no longer matches items keyed by their solution | c8a84e32, 88a9a73c |
| template36 | graphing-linear-inequalities/kp1 | refused: answer-contract | 62ca5934 |
| template36 | graphing-linear-inequalities/kp2 | refused: answer-contract | de905c7f |
| template36 | interpreting-graphs-qualitatively/kp1 | refused: answer-contract | c8a84e32 |
| unit05-inequalities | graphing-linear-inequalities/kp3 | refused: answer-contract | de905c7f |
| unit05-inequalities | solutions-of-inequalities/kp2 | refused: answer-contract | de905c7f |
| unit06-correction | pythagorean-converse/kp1 | refused: sample-eval | 4da99167 |
| unit06-correction | pythagorean-converse/kp2 | refused: sample-eval | 62ca5934 |
| unit07-complement | polynomial-basics/kp2 | refused: sample-eval | 62ca5934 |
| functions-exponentials | exponential-functions/kp1 | refused: answer-contract | 6e1947b9 |
| functions-exponentials | identifying-functions-vertical-line-test/kp1 | refused: answer-contract | f8e6f402 |
| functions-exponentials | one-to-one-functions/kp1 | refused: answer-contract | 6e1947b9 |

The unit06 rows were removed from both `unit06-correction/drafts.json` and the identical rows of `crates/worker/tests/fixtures/unit06-template-candidates.json`. The current technical receipt `unit06-correction/current-technical.json` was regenerated with `cargo run -p cadus-worker --example unit06_templates`. The functions-exponentials rows were per-knowledge-point files, and `functions-exponentials/manifest.json` no longer lists them. The unit05 checkpoint records (`production-gates.json`, `final-audit.json`) describe the checkpoint as reviewed and stay unchanged.
