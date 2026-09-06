# Foundations authored reference visuals

## Verified scope

Baseline: 154 knowledge points required a visual and all 154 were missing one.
A first slice authored 88 figures across 77 knowledge points, four families
(number line, fraction, coordinate, geometry), and left 77 visual-blocked.
This slice closes 72 of those 77: it adds five renderer/model families (rays,
shaded half-planes, curves, angle marks, and exact-radical special triangles)
and authors 72 more reference figures. **5 visual-blocked knowledge points
remain, explicitly enumerated below with the reason each is not addressed.**
This count is independent of database approvals and other readiness
blockers; it does not mean 149 knowledge points are fully ready.

The figures are captioned independent reference examples. They support a
concept and are reused across its questions, including across sibling
knowledge points of the same topic where several previous entries in this
manifest already reused one figure verbatim. They are not claimed to depict
the current randomized problem.

## The five renderer families this slice adds

- **Rays** (`NumberLineFigure.rays`): one exact drawn origin, filled or open,
  with an arrowhead drawn to the frame edge in the authored direction, so a
  bounded viewport is never mistaken for a bounded interval. Closes the
  inequality-on-a-number-line family.
- **Shaded half-planes** (`CoordinateFigure.shaded_half_planes`): the
  boundary is fixed by two points (never an author-typed equation); the
  shaded side is fixed by a third point checked to sit strictly off that
  line. The render clips the visible rectangle against the half-plane with
  Sutherland-Hodgman and draws the boundary solid or dashed. Closes the
  linear-inequality family.
- **Curves** (new `VisualSpec::Curve`, six `CurveKind` variants: polynomial,
  exponential, logarithm, reciprocal, sine, cosine): the drawn line is a
  sampled approximation, like any grapher's, but every authored key point is
  checked exactly — rational arithmetic for a polynomial or a reciprocal off
  its asymptote, the integer steps of the base for an exponential or a
  logarithm (searched exactly, bounded to a magnitude of 64), the
  quarter-period values for a sine or cosine. A key point the family cannot
  confirm exactly is refused, never silently accepted. Closes the parabola,
  exponential, logarithmic, reciprocal, and trig-graph families (except the
  base-*e* residual below).
- **Angle marks** (`GeometryShape::Polygon.angle_marks`, and a new
  `GeometryShape::Angle` for a standalone standard-position diagram): a
  polygon vertex mark names which angle a question means (a letter, θ, or a
  given measure), trusted the same way an existing vertex or segment label
  already is, since the figure is a reference example and not a per-problem
  rendering. The standalone `Angle` shape draws two rays from a vertex at
  given degree directions (negative or beyond 360 allowed) with an arc and a
  label. Closes the right-triangle-trig and the standard-position/coterminal/
  reference-angle families.
- **Exact-radical special triangles** (new `VisualSpec::SpecialTriangle`): a
  45-45-90 or 30-60-90 triangle is authored with *one rational leg*; every
  other side is *computed* by the renderer from the fixed exact ratio of the
  family and only ever displayed, never author-typed, so no radical can be
  authored wrong. A `sas_area` shape computes the exact area of a
  two-sides-and-an-included-angle triangle from a small table of angles with
  a known exact sine (30°, 45°, 60°, 90°, 120°, 135°, 150°) and refuses any
  other angle rather than falling back to a numeric approximation. Closes
  the symbolic/irrational-coordinate family.

## Source and loading

Unchanged from the first slice: curriculum knowledge-point `visuals` is the
existing source; readiness reads the validated authored figures, and the
serving payload renders them directly. No migration or `content_store.kind`
change was needed for this slice either.

## Reproduction and review

Run `python3 scripts/authoring/foundations_visuals.py` to verify the
authored manifest (now 149 entries) matches the YAML. Add `--write` to
install missing reviewed entries; existing differing figures are refused. No
unrelated YAML is reformatted.

Run `cargo run -p cadus-core --example visual_inventory -- curriculum` to
generate the complete visual inventory, exact accessible equivalents, and
rendered SVG, over the real curriculum tree.

`cargo test -p cadus-core --test foundations_visuals` is the exhaustive
readiness regression: it re-derives the manifest's entry count (149), the
canonical dump's `"visuals"` occurrence count (149), the total visual count
(160, since a few knowledge points keep two figures), the family count (6:
number_line, fraction, coordinate, geometry, curve, special_triangle), and —
the number this slice exists to move — **the literal visual-blocked count,
asserted to be exactly 5**. Every one of the 149 manifest entries is also
cross-referenced against the real curriculum knowledge point it names,
exactly equality-checked against the authored `visuals` field (so the
manifest and the YAML can never silently drift), validated
(`VisualSpec::validate`), rendered (`role="img"`, a `<desc>` with the
accessible equivalent, byte-identical on a second render), and required to
carry a caption starting with `Reference example:`.

`crates/core/src/visual/curve.rs`, `special_triangle.rs`, `geometry.rs`,
`number_line.rs`, and `plane.rs` carry their own unit tests for the new
constructs (exact key-point verification, degenerate-parameter refusal,
accessible text) independent of the curriculum content. The render tests in
`crates/core/tests/visual_render.rs` cover the SVG output of every new
construct: the ray's arrowhead and end cap, the shaded region's clipped
polygon and solid/dashed boundary, the curve's broken polyline around an
asymptote, the polygon's angle-mark arc, the standalone angle diagram, and
the special triangle's computed radical label.

Two curriculum-content digests changed with the new `visuals:` bytes and
were refreshed with their actual, oracle-verified values (never guessed):
`crates/core/tests/parity.rs` and `crates/core/tests/parity_oracle.rs`
(`DUMP_LEN`, `TREE_HASH`). `the_rust_dump_equals_the_live_1_0_dump` (the live
1.0 Python-oracle byte-for-byte comparison) still passes against the new
tree, and the `counts` object (topics, knowledge points, exemplars, ...) is
unchanged, since this slice adds only `visuals:` bytes and no knowledge
point, topic, or exemplar.

No learner data, content approval, or deployment is changed by this slice.

## Remaining visual-blocked knowledge points (5, explicitly enumerated)

- `money-geometry-problems/kp1`: **Money and cost setups.** Not addressed —
  no visual is mathematically required. The two exemplars (`Tickets cost €8
  each and you spent €56...`, `A taxi charges a €3 base fare plus €2 per
  kilometre...`) are plain one-variable linear-equation word problems with no
  graph, shape, or diagram in their content. The heuristic in
  `crates/core/src/readiness/visual.rs` flags every knowledge point of this
  topic because the *topic id* contains the substring `geometry` (the topic
  bundles this KP with a sibling perimeter KP); the heuristic has no way to
  read a single knowledge point's own exemplars. This is the heuristic
  false positive the first slice's report already flagged as a residual
  category, confirmed by reading the exemplars directly.
- `natural-exponential-function/kp3`: **The graph of y = e^x and the
  inverse pair with ln.** Not addressed — genuinely unsafe to fake. This
  slice's `CurveKind::Exponential` requires a *rational* base `b`, because
  every key point is checked by raising `b` to an exact integer power with
  `BigRational` arithmetic; `e` is irrational, so no rational `b` makes
  `y = e^x` the actual function drawn. Substituting a rational base (e.g. `2`)
  would render a curve of the right *shape* but a mathematically wrong
  function for a knowledge point specifically about `e`, which is exactly
  the kind of mismatch this task was told to avoid. Closing this needs an
  irrational-base (or named-constant) extension to `CurveKind`, out of this
  slice's scope.
- `continuous-growth-model/kp1`: **From n compoundings per year to
  A = Pe^rt.** Not addressed, same reason as above: the formula is defined
  in terms of `e`.
- `continuous-growth-model/kp2`: **Evaluating continuous growth and decay
  models.** Not addressed, same reason: evaluating `A = Pe^rt` needs the
  irrational base `e`.
- `continuous-growth-model/kp3`: **Doubling time and half-life via ln.**
  Not addressed, same reason: the inverse relationship this knowledge point
  teaches is specifically the `e`/`ln` pair.

## Verification result

`cargo test -p cadus-core --no-fail-fast` (excluding the pre-existing,
environment-only failures in `crates/worker`/`crates/store` that need
`CADUS_TEST_DATABASE_URL`, unrelated to this slice and unrelated to
visuals): 0 failures. Targeted `cargo clippy -p cadus-core --all-targets --
-D warnings` and `cargo fmt -p cadus-core -- --check` pass. `cargo build
--workspace` passes. Manifest idempotence (`foundations_visuals.py` with no
`--write` reports zero files needing a change once applied) holds.
