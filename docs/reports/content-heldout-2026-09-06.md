# Foundations held-out content audit

## Verified result

The exact-rational arithmetic oracle remains valid for the pure-numeric
curriculum family. Generated teach and hint documents remain `pending` review
candidates and have never been approved by this workflow.

Independent semantic review invalidated the curriculum-readiness claims of the
coarse operator-family generator. Same-shape operand redraws preserve syntax
and arithmetic but can violate knowledge-point constraints such as like
denominators, regrouping, reciprocal division, required cancellation, and
powers of ten. Generic solution text can also describe the wrong method even
when its final equality is true.

The correction therefore:

- removes all 68 held-out exemplars added by `d324681`/`3a3ecd3`;
- removes exactly the 105 generic solution sketches added by `b6f4c22`;
- removes the readiness fixtures that treated those rows as evidence; and
- makes both curriculum-mutation commands return exit 2 until reviewed,
  knowledge-point-specific recipes replace coarse generation.

The generated teach and hint packets remain useful inputs to the human digest
review gate. Their presence does not establish instructional quality,
solution coverage, held-out assessment coverage, or serving readiness.

## Verification

- The Python arithmetic, draft, and curriculum-patch suites pass.
- Both mutation entry points refuse with exit 2.
- Curriculum loading and final canonical parity are reverified by the
  integration gate after this correction is merged.
