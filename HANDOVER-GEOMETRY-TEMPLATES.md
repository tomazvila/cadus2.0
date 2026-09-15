# Handover — unblock geometry wave: the 6 blocked KP-1 templates

**For:** the next agent instance.
**State at handover:** foundations 100 % ✅. Geometry 0/87 — six wave-1 lessons
blocked on `practicable` (kp1 needs ≥ 3 practice items; each kp1 has 3 decidable
exemplars − 1 held-out = 2, and no approved template). One of the six
(`measuring-segments-angles/kp1`) also lacks a teach page entirely;
`parallelogram-trapezoid-area/kp1` has a pending LLM template + teach that still
need gate validation and approval. The LLM authoring passes have declined these repeatedly across ~6 waves
(geometry formula answers want variable coordinates inside `sqrt(...)`, which
the decidable grammar rejects). **Hand-authoring is the proven fix** (done
before for `opposites-of-integers/kp2`, `integer-multiplication-division/kp3`,
`adding-subtracting-negative-fractions/kp1+kp2` — all serving and grading).

The grind orchestrator (`cadus-grind-geo3`, systemd user unit, log
`/tmp/orchestrator.log`) keeps cycling meanwhile; it will pick the new templates
up on its next replan. Do NOT let it edit the same content.

---

## 1. Exactly what is missing (verified live)

| topic / kp1 | teach page | template | blockers shown |
|---|---|---|---|
| `distance-midpoint-formulas/kp1` | ✅ | ❌ | `practicable` |
| `circle-circumference/kp1` | ✅ | ❌ (kp2 has one) | `practicable` |
| `scale-drawings/kp1` | ✅ | ❌ (kp2 has one) | `practicable` |
| `translations-coordinate-plane/kp1` | ✅ | ❌ | `practicable` |
| `parallelogram-trapezoid-area/kp1` | 🟡 pending (LLM) | 🟡 pending (LLM) | `teachable`, `practicable` |
| `measuring-segments-angles/kp1` | ❌ | ❌ | `teachable`, `practicable` |

So: **5 templates + 1 teach page** to hand-author, plus gate-validate and
approve the pending `parallelogram-trapezoid-area/kp1` template + teach (only
hand-author replacements if the gate rejects them). One template per kp1 is
enough (3 decidable exemplars − 1 held-out + 1 template = 3 practice items ≥ 3).
kp2/kp3 of these topics have 3 exemplars each and pass practice-only — no
templates needed there.

## 2. The proven template schema (clone source: `scale-drawings/kp2`, which passed the gate)

```json
{"v": 1,
 "hints": ["...{param}...", "..."],
 "params": {"d": {"low": 10, "high": 200, "kind": "int"},
            "s": {"low": 2, "high": 20, "kind": "int"},
            "unit": {"kind": "choice", "values": ["km", "m"]}},
 "samples": [{"params": {"d": 10, "s": 2, "unit": "km"}, "expected": "5"}, …],
 "topic_id": "scale-drawings",
 "statement": "…{d}…{s}…",
 "space_size": <int>,
 "answer_expr": "<expr over params>",
 "answer_kind": "numeric" | "expression",
 "answer_contract": {"kind": "exact"},
 "solution_sketch": "…{param}…"}
```

`int` params with low/high + partial samples ARE gate-acceptable
(scale-drawings/kp2 passed that way; edge coverage of low/high is checked —
include the low and high value of every int param in `samples`).

## 3. Grammar rules learned the hard way (all of these refused real attempts)

1. **Exponents in `answer_expr` must be whole-number literals.** `x**(a-c)` →
   refused. `x**2` fine. Variable exponents are undecidable, full stop.
2. **LaTeX literal braces must be doubled.** `\frac{...}` in a statement is an
   unescaped-brace refusal; write `\frac{{...}}{{...}}`. A value placeholder
   wrapped in braces is `{{{a}}}` (triple, not quadruple).
3. **Space floor:** the declared param grid must produce ≥ 12 distinct problems.
   Size the choice lists / int ranges accordingly and count before writing.
4. **Sample agreement:** every sample's `expected` must equal `answer_expr`
   evaluated at those params (canonical form — reduced fractions, no units).
5. **Hidden parameters:** every param that appears in `answer_expr` must appear
   in `statement`.
6. **`answer_kind` must match the topic declaration** — see ISSUE-11 in
   `ISSUES.md`: `points-lines-planes` declared `multi-step` for numeric facts
   and 496 answers graded ungraded. The six topics do NOT all declare the same
   kind (verified in the YAML — verify again before writing):

   | topic | declared `answer_kind` | source |
   |---|---|---|
   | `distance-midpoint-formulas` | `expression` | `curriculum/geometry/05-measurement-coordinate.yaml` |
   | `circle-circumference` | `expression` | `curriculum/geometry/04-circles.yaml` |
   | `scale-drawings` | `numeric` | `curriculum/geometry/02-similarity.yaml` |
   | `translations-coordinate-plane` | `expression` | `curriculum/geometry/05-measurement-coordinate.yaml` |
   | `parallelogram-trapezoid-area` | `numeric` | `curriculum/geometry/05-measurement-coordinate.yaml` |
   | `measuring-segments-angles` | `numeric` | `curriculum/geometry/00-lines-angles.yaml` |

## 4. Suggested designs (decidable by construction)

- **distance-midpoint-formulas/kp1** — Pythagorean-triple design (kp1: "Distance
  between two points", constraints "coordinate differences form Pythagorean
  triples"; exemplar answers are plain numerals, but the topic declares
  `expression`):
  params `dx ∈ {3,6,8}`, `dy ∈ {4,8,15}` (kind int 3..15 is fine, but then
  include samples for non-triple pairs — simplest is `choice` lists with the
  pairs that make triples: (3,4)→5, (6,8)→10, (8,15)→17).
  `answer_expr: "sqrt(dx*dx+dy*dy)"`, `answer_kind: "expression"`,
  samples expected `"5"`, `"10"`, `"17"`.
  Do NOT fall back to a `numeric` squared-distance answer (`d²`) — that would
  violate rule 6. If the gate refuses `sqrt` of an expression, keep
  `answer_kind: "expression"` and pick different triple pairs until one form is
  accepted.
- **circle-circumference/kp1** — kp1 is "Circumference from the radius";
  exemplars are exact in terms of π (`$12\pi$`, `$8\pi$`), so target
  `expression` answers of the form `$N\pi$` with integer radii (one exemplar
  uses 3.14/nearest-tenth, but the exact-π form is the safe template core).
  Mirror the kp2 LLM template (read it first:
  `SELECT body FROM content_store WHERE kp_id='circle-circumference/kp2' AND
  kind='template'`).
- **scale-drawings/kp1** — actual→drawing or drawing→actual, integer answers
  (clone kp2 and flip the division).
- **translations-coordinate-plane/kp1** — image point of `(x,y)` under
  `(x+a, y+b)`. Tuple `answer_expr` support is UNVERIFIED — try
  `"({x}+{a}, {y}+{b})"` with `answer_kind: "expression"` first; if the gate
  refuses tuples, split into two templates asking for the x- and y-coordinate
  separately (numeric).
- **parallelogram-trapezoid-area/kp1** — kp1 is "Parallelogram area" (read the
  constraints in `curriculum/geometry/05-measurement-coordinate.yaml`; height is
  given explicitly, never a slant side). base/height numeric:
  `answer_expr: "{b}*{h}"`, `answer_kind: "numeric"`, statement supplies base
  and height. **A pending LLM template + teach already exist** in
  `content_store` — gate-validate and approve them first (§5); hand-author only
  if the gate rejects. If you do author the teach page, the schema is
  `{"concept": str, "worked_example": {"problem": str, "steps": [str, …]}}`
  (clone `radical-operations/kp2` teach).
- **measuring-segments-angles/kp1** — kp1 is "Segment addition with numbers"
  (constraints: integer lengths 1–50, point always between the endpoints;
  ruler-postulate coordinates between −20 and 20; read
  `curriculum/geometry/00-lines-angles.yaml`). Numeric designs:
  `AB + BC = AC` → `answer_expr: "{ab}+{bc}"`; missing piece →
  `answer_expr: "{pq}-{pm}"`. `answer_kind: "numeric"`.
  Also author its **teach page** (schema as above).

## 5. Insert → validate → approve flow (proven)

1. Compute the digest exactly:
   `sha256(kp_id + b"\0" + kind + b"\0" + body)[:16]` with prefix `sha256:`
   (see `crates/worker/src/authoring/job/mod.rs::document_digest`). The `body`
   is the exact JSON string you insert — byte-for-byte.
2. Insert as **pending**:
   `INSERT INTO content_store (digest, kp_id, kind, body, status,
   authoring_attempts, authoring_cost_usd) VALUES (…, 'pending', 0, NULL)
   ON CONFLICT (digest) DO NOTHING;` (via `docker exec cadus2-db psql -U
   cadus_admin -d cadus`).
3. **Validate through the gate** (this is the fastest feedback loop):
   `GET /api/admin/content/{digest}` (cookie auth as the admin user
   `learner.eval@example.com`, cookie jar `/tmp/cadus_cookies.txt`) → read
   `gate.rejected` (must be null) and `sample_instances` (check the rendered
   answers). Iterate on refusals — the message names the exact rule.
4. **Approve via the C6 API** (NOT direct SQL — it stamps currency and runs the
   re-gate): `POST /api/admin/content/{digest}/approve` with the body echoing
   `{"policy_digest":…, "template_context_digest":…, "curriculum_digest":…,
   "review_engine_digest":…}` exactly as the GET reported them.
   The approve also re-stamps sibling teach/hint rows (ISSUE-7 fix).
5. **Unblock check:** `GET /api/session/plan` → the six lessons leave
   `blocked`; then let `cadus-grind-geo3` grind (restart the unit if it has
   exited: `systemd-run --user --unit=cadus-grind-geoN --working-directory=/tmp
   /usr/bin/env python3 /tmp/orchestrator.py geometry 86400`).

## 6. Watch out for

- **Other agents' uncommitted work in the tree** (`crates/web/src/grade/*`,
  `serve/*`, `Cargo.*` — H-1/H-5 hint-stamp + drill-status fixes, mid-work,
  did not compile at handover). Build deploy images from a **clean worktree**
  (`git worktree add /tmp/cadus-build HEAD`) until they commit. Do not edit or
  commit their files.
- The curriculum is **baked into the image** — curriculum edits need a rebuild;
  the boot sweep auto-re-stamps approvals after any digest move (it logged
  1,674 re-stamps on 2026-09-15 — that is normal, not an error).
- Transient systemd units vanish when they stop — check
  `systemctl --user list-units 'cadus-*'` and relaunch if the grind stopped.
- `/tmp` does not survive reboots: `orchestrator.py`, cookie jar, and logs live
  there. Copy them somewhere durable if this box reboots.

## 7. Acceptance

- All six wave-1 geometry lessons serve and complete (`task_passed`), geometry
  `practiced` climbs past 6/87, and the orchestrator moves to wave 2.
- `cadus-worker readiness --course geometry` no longer lists the six kp1s as
  `practicable`-blocked (the report's store-half quirk is H-4 in
  `HANDOVER-OUTSTANDING.md` — compare against the session plan, not the report).
- No `CONTENT_MISMATCH` lines in `/tmp/orchestrator.log` (the driver submits
  the authored expected answer; any mismatch is a real checker/content bug).
