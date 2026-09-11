# U07 complement — source checkpoint

Baseline: remote integration `69e4f650`, supplied as isolated commit `090a524`. Five exact `absent_pending_template_recipe` closures; all content remains pending.

| Knowledge point | Material exemplars | Pending instances | Distinct answers | Answer entropy (bits) |
|---|---:|---:|---:|---:|
| polynomial-basics/kp2 | 4 | 14 | 3 | 1.557 |
| difference-of-squares/kp1 | 4 | 12 | 12 | 3.585 |
| choosing-factoring-strategy/kp1 | 4 | 12 | 3 | 1.500 |
| parabola-vertex-form/kp2 | 4 | 12 | 12 | 3.585 |
| quadratic-graphs-vertex/kp3 | 4 | 12 | 12 | 3.585 |

## Audit and materiality

The fresh native facts and unchanged seven-code audit confirm each owned KP had only `absent_pending_template_recipe` before this checkpoint and has no findings afterward. Whole-course issue KPs and absent-pending KPs both fall **129 → 124**. Every other audit count and all **804 unowned KP fact records** are unchanged. Historical U07 reports and existing recipe writers remain in place.

Four exemplars per KP use different mathematical representations or reasoning tasks: direct expression, coefficient table, reconstruction from an area/tile/translation model, and combination or correction of an error. Independent reconstruction verifies all 20 answers. Classification uses 14 distinct nonzero-power supports: four monomials, six binomials and four trinomials. The strategy domain includes GCF-first, conjugate-factor and genuinely integer-factorable trinomial cases. Graph domains cover both opening directions, nonzero vertex heights, distinct real intercepts and changing vertex/intercept data.

Each pending family is exhaustively enumerated. Rendered statements independently reconstruct the polynomial; factor multiplication, coefficient GCFs, completed squares, zero substitution and symmetry verify the stored answers. There are 62 distinct semantic inputs, no within-KP authored/sibling semantic collisions, and no exact-text collisions against all curriculum exemplars or checked-in content/report problem records. Every instance rejects a perturbed mathematical prompt with its old answer. Additional controls reject wrong labels, reversed factors, altered multipart components, missing/extra fields, duplicate inputs, constant answers and capacity below 12. Entropy is at least 1.5 bits. Label families necessarily reuse their finite answer vocabulary.

The monic difference-of-squares constant ceiling is explicitly extended from 100 to 784. Its twelve template squares are 289 through 784; this changes the authored numeric range, preserving the integer, monic and no-GCF requirements. No readiness, capacity, entropy or audit threshold is lowered.

## Narrow contract changes

`signcase` can select a label branch through the existing closed-label contract, including nested choices. Selectors must be exact numbers; the selected result must belong to the contract. Existing numeric `signcase` behavior is preserved. Templates use one-letter parameter names permitted by the current grammar and avoid reserved expression unknowns.

The template-only `multipart` parser accepts 1–16 arguments, matching the existing contract schema. Evaluation still requires the exact number of named, flat contract parts. The learner grammar gains no template functions. Difference-of-squares returns ordered `lower_factor` and `upper_factor` monic linear expressions; independent multiplication verifies the factorisation without losing its structure to expansion. The full graph summary uses five consistently named parts in every exemplar.

The existing writer regression suites pass. Five equivalent divisibility predicates in the touched structured-writer module use `is_multiple_of` to satisfy Rust 1.95 Clippy; no writer output changes.

## Verification

- Core focused suites: **27 passed** (`unit07_label_multipart`, `answer_contract_content`, `structured_list_writers`, `polynomials_quadratics_negative_controls`).
- Worker suites: **4 passed** (`unit07_complement`, `unit07_semantic_filters`); all five real `verify_kind` gates and all 62 current `answer_for_contract` instances pass.
- Independent Python semantics: **5 passed**. The evidence packet is emitted only after the complete suite passes and is bound to SHA-256 digests of facts, recipes and production evidence.
- Production KaTeX 0.17.0: **226 text fields, 361 formulas, zero errors**.
- Focused Clippy: **passed with `-D warnings`**. Formatting and diff whitespace checks pass. Changed/new code files stay below 500 lines and functions at or below 70 lines.
- Broad core run: **1,433 passed, 39 failed**, across 141 test binaries/units. An untouched detached baseline reproduces **37 failing tests in 16 binaries**. The two remaining failures were an updated exemplar-value pin and an incorrect new test-fixture unit encoding; both pass in the final focused rerun. The full suite is therefore **not green**. Exact baseline failure names and log digests are in [audit.json](audit.json).

No database import, programmatic approval, production readiness run, release gate, browser interaction, push, integration or deployment was performed. Source acceptance does not establish serving readiness.

## Reproduction

Use Rust 1.95 or later and project-local/Nix-provided Python (`pyyaml`, `sympy`) and Node. Before applying this patch, emit baseline facts with the baseline's native `content_audit_facts` into `target/unit07-complement/baseline-facts.json`. After applying:

1. Run `python3 scripts/authoring/unit07/complement.py` to regenerate only the five owned exemplar blocks and pending recipes.
2. Run `cargo run --locked --offline -p cadus-core --bin content_audit_facts -- curriculum > target/unit07-complement/facts.json`.
3. Run the four focused core tests and two worker tests listed above. `unit07_complement` writes `target/unit07-complement/production-evidence.json` through the real worker gate.
4. Run `python3 scripts/authoring/unit07/check_complement.py` and `node scripts/authoring/unit07/check_render.cjs --complement`.
5. Run `python3 scripts/review/foundations_content_audit.py --facts target/unit07-complement/facts.json --output target/unit07-complement/audit.json`. Exit 1 records the 124 out-of-scope residual KPs.

[Production evidence](production-evidence.json) contains each canonical pending body, its real `document_digest`, and every rendered instance. [Audit evidence](audit.json) records exact closures, baseline comparisons, entropy and verification hashes. The source import bundle is [templates.json](../../content-foundations/unit07-complement/templates.json); human review is still required.
