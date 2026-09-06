# Foundations Template Completion — 2026-09-06
This change adds 21 deterministic practice template recipes. Each recipe uses an audited, closed expression family.

## Result
- Previous distinct template coverage: 89 of 809 knowledge points.
- New pending templates: 21.
- Current distinct template coverage: 110 of 809 knowledge points.
- Residual without a pending or approved template: 699 knowledge points.
- Model API cost: 0 micro-USD.
- Approval changes: 0.

## Covered families
- Mixed numbers: `mixed-numbers/kp1`, `mixed-numbers/kp2`, and `mixed-numbers/kp3`.
- Signed mixed numbers: `negative-fractions-decimals/kp3`.
- Decimal operations: `decimal-multiplication-powers-of-ten/kp1`, `decimal-multiplication-powers-of-ten/kp3`, `decimal-addition-subtraction/kp1`, and `decimal-operations/kp1`.
- Signed decimals: `signed-decimal-operations/kp1` and `signed-decimal-operations/kp2`.
- Additive inverses: `adding-integers/kp3`.
- Exact roots: `perfect-square-roots/kp2`, `perfect-square-roots/kp3`, `square-roots/kp2`, and `square-roots/kp3`.
- Radical operations: `radical-operations/kp1` and `dividing-radicals/kp1`.
- Rational exponents: `radical-exponent-conversion/kp3`, `rational-exponents/kp1`, and `rational-exponents/kp2`.
- Opposite factors: `rational-expressions/kp3`.

## Proof
The focused Rust test ran each recipe through `verify_kind`. Every recipe produced at least 12 exact practice items.

The same test compared two generator runs byte for byte. It also found no practice text equal to an authored exemplar.

The disposable database was `cadus2_manual_20260906` in container `cadus2-testdb`. The first import stored 21 pending rows with no refusal.

The second missing-only import stored 0 rows and skipped 21 rows. It made 0 endpoint calls and reported 0 micro-USD.

The output bundle remains at `/home/deploy/.cache/cadus2_orchestration/template-completion-first` for integration review. Its file digests are:
- `drafts.json`: `38e4277ff2d379a6e3c1544ea9777158c5d439598bcdba172d0e1a098be7f975`
- `manifest.json`: `5d977511ad4fa9d37c8a9c21354ffaac1a1c2782825134917497ef1866fe8a36`
- `review.json`: `52f6cc5622caf3238ba902c2ce39fa62e277f26979a983538817429b3e23ef92`
- `summary.json`: `227c13e8e52bd94b419f02e89d6a26c5731819535781f853985cff3ea64e945a`

## Boundary
The residual 699 knowledge points need semantic models beyond the audited closed-expression families. This tool records each refusal and leaves each content slot empty.

Every imported row has `pending` status. A human digest review remains necessary before approval.
