# Foundations authored reference visuals

## Verified scope

Baseline: 154 knowledge points required a visual and all 154 were missing one.
This slice authors 88 mathematically exact reference figures across 77 knowledge
points. All four supported families are represented. The visual-specific blocked
count is now 77. This count is independent of database approvals and other
readiness blockers; it does not mean 77 knowledge points are fully ready.

The figures are captioned independent reference examples. They support a concept
and are reused across its questions. They are not claimed to depict the current
randomized problem. The manifest keeps the objective, exact reference specification
and mathematical verification rationale for every knowledge point. Line segments
are finite segments, discrete relation points assert no invented curve, and the
square-metre conversion figure uses exact centimetre coordinates.

## Source and loading

Curriculum knowledge-point `visuals` is the existing source: readiness reads the
validated authored figures, and the serving payload renders them directly.
A `content_store.kind=visual` row or database migration is unnecessary.

The first real content load revealed that the schema pre-walk omitted visuals
from its whitelist, despite their presence in the typed model. This is fixed
with strict VisualSpec decoding. The canonical dump now includes nonempty visuals,
so edits affect the curriculum hash. Empty legacy visuals stay omitted.

## Reproduction and review

Run `python3 scripts/authoring/foundations_visuals.py` to verify the authored
manifest matches the YAML. Add `--write` to install missing reviewed entries;
existing differing figures are refused. No unrelated YAML is reformatted.

Run `cargo run -p cadus-core --example visual_inventory -- curriculum` to generate
the complete visual inventory, exact accessible equivalents and rendered SVG.
The exhaustive test checks all 88 figures, four families, exact reference-line
equations, deterministic SVG, accessibility frames, loader acceptance, canonical
dump inclusion and the literal remaining visual-blocked count.

No learner data, content approval or deployment is changed by this slice.
The remaining objectives need additional authored diagrams or renderer support
for curves, half-plane shading, rays, symbolic/irrational coordinates and angle
marks. Some are heuristic matches of words such as model rather than an explicit
visual requirement; they remain listed instead of being silently exempted.

## Remaining visual-blocked knowledge points

- `money-geometry-problems/kp1`: Money and cost setups
- `graphing-inequalities-number-line/kp1`: Open vs closed circle
- `graphing-inequalities-number-line/kp2`: Direction of the ray
- `graphing-inequalities-number-line/kp3`: Read the inequality from a graph
- `one-step-inequalities/kp3`: Solve and describe the graph
- `graphing-linear-inequalities/kp1`: Boundary line: solid or dashed
- `graphing-linear-inequalities/kp2`: Test a point to choose the shaded side
- `graphing-linear-inequalities/kp3`: Shade using the origin test
- `parabola-vertex-form/kp3`: Axis of symmetry and points on the graph
- `quadratic-graphs-vertex/kp1`: Vertex from standard form via x = -b/(2a)
- `quadratic-graphs-vertex/kp2`: Intercepts together with the vertex
- `quadratic-graphs-vertex/kp3`: All the features for a full sketch
- `exponential-functions/kp3`: Evaluate decay models
- `graphs-of-exponential-functions/kp1`: The y-intercept
- `graphs-of-exponential-functions/kp2`: Horizontal asymptote and range
- `graphs-of-exponential-functions/kp3`: Increasing or decreasing
- `exponential-growth-decay/kp1`: Growth models with an integer growth factor
- `exponential-growth-decay/kp2`: Decay models and half-life
- `compound-interest/kp3`: Write the compound interest model
- `graphs-of-logarithmic-functions/kp1`: Key points on a log graph
- `graphs-of-logarithmic-functions/kp2`: Domain, vertical asymptote, and monotonicity
- `graphs-of-logarithmic-functions/kp3`: The inverse-pair reflection with the exponential
- `natural-exponential-function/kp3`: The graph of y = e^x and the inverse pair with ln
- `continuous-growth-model/kp1`: From n compoundings per year to A = Pe^rt
- `continuous-growth-model/kp2`: Evaluating continuous growth and decay models
- `continuous-growth-model/kp3`: Doubling time and half-life via ln
- `inverse-variation/kp3`: The reciprocal parent graph
- `trig-ratios-definition/kp2`: Ratios of the other acute angle
- `solving-right-triangles-sides/kp1`: Multiply to find a leg
- `solving-right-triangles-sides/kp2`: Use tangent between the legs
- `solving-right-triangles-sides/kp3`: Divide to find the hypotenuse
- `right-triangle-trig/kp1`: Find angles from exact ratios
- `right-triangle-trig/kp2`: Find angles with a calculator (inverse trig keys)
- `right-triangle-trig/kp3`: Solve the whole right triangle
- `special-right-triangles/kp1`: 45-45-90 triangles
- `special-right-triangles/kp2`: 30-60-90 triangles
- `complementary-angle-trig/kp1`: Sine and cosine swap for complements
- `complementary-angle-trig/kp2`: Solve cofunction equations
- `complementary-angle-trig/kp3`: Complementary acute angles in one triangle
- `angle-of-elevation-depression/kp1`: Angle of elevation problems
- `angle-of-elevation-depression/kp2`: Angle of depression problems
- `angle-of-elevation-depression/kp3`: Find the angle
- `trig-applications/kp1`: Distances via trig ratios
- `trig-applications/kp2`: Multi-step problems
- `trig-applications/kp3`: Solve for angles in context
- `coterminal-angles/kp1`: Quadrant of an angle in standard position
- `coterminal-angles/kp2`: Coterminal angles in degrees
- `coterminal-angles/kp3`: Coterminal angles in radians
- `reference-angles/kp1`: Reference angles in degrees
- `reference-angles/kp2`: Reference angles in radians
- `reference-angles/kp3`: Negative and large angles
- `unit-circle-special-angles/kp1`: Quadrantal angles
- `unit-circle-special-angles/kp2`: First-quadrant special angles
- `unit-circle-special-angles/kp3`: Tangent at special angles
- `unit-circle/kp1`: Signs by quadrant (ASTC)
- `unit-circle/kp2`: Exact values via reference angles
- `unit-circle/kp3`: Negative angles and angles beyond 2π
- `reciprocal-trig-functions/kp1`: Definitions from triangle sides
- `reciprocal-trig-functions/kp3`: Values at special angles
- `sine-cosine-parent-graphs/kp1`: Key points of y = sin x
- `sine-cosine-parent-graphs/kp2`: Key points of y = cos x
- `sine-cosine-parent-graphs/kp3`: Domain, range, and period of the parent graphs
- `trig-graphs-basic/kp1`: Amplitude
- `trig-graphs-basic/kp2`: Period with a coefficient on x
- `trig-graphs-basic/kp3`: Amplitude and period together
- `trig-graphs-midline/kp1`: The midline
- `trig-graphs-midline/kp2`: Maximum and minimum with a shift
- `trig-graphs-midline/kp3`: Recover amplitude and midline from max and min
- `pythagorean-identity/kp2`: Find cosine from sine in Quadrant I
- `pythagorean-identity/kp3`: Use quadrant signs
- `law-of-sines/kp2`: Find the third angle first
- `law-of-sines/kp3`: Find an angle
- `law-of-cosines/kp2`: Obtuse included angles
- `law-of-cosines/kp3`: Find an angle (SSS)
- `triangle-area-sine/kp1`: The area formula
- `triangle-area-sine/kp2`: Exact areas with radicals
- `triangle-area-sine/kp3`: Obtuse included angles

## Verification result

88 focused tests pass: authored manifest/equations (2), readiness (10), visual
readiness (8), rendering (15), loader (13), strict loader (10), and built-in
visual validation (30). Targeted Clippy with denied warnings, formatting,
manifest idempotence and diff checks pass. No full-course completion or content
approval is claimed by this visual-specific result.
