# Timing policy wiring
Ordinary and integrated submissions persist the policy reading (`outcome`, `ratio`, `claim`) and its reliability. Idempotent replies reuse the recorded reading. Legacy events default the additive fields to absent.

## Policy
- Correctness and task progression continue to read grading evidence. A slow correct reasoning answer remains correct.
- Reliable unaided routine answers can record `routine_fluency`.
- Missing/zero budgets, zero/future clocks, observed re-serves, ungraded answers, and elapsed times above the policy ceiling support no speed claim. Existing ordinary elapsed-time caps preserve the unreliable marker.
- Ordinary expected time comes from the actual served topic. Integrated expected time is the sum of authored component-topic budgets; any missing budget excludes the reading. Integrated items always use reasoning mode. This aggregate budget is a conservative implementation assumption, not a calibrated item-specific estimate.

## Verification
- Core timing policy and persisted reading round trips: 7 passed.
- Web timing HTTP regressions: 2 passed (routine fluency, slow-correct KP advance, interrupted/overlong/future clocks, event decoding).
- Integrated HTTP regressions: 13 passed, including slow-correct completion, frozen idempotent timing, and reload exclusion.
- Grade event wire-shape regressions: 2 passed.
- Browser visibility telemetry is outside this slice; server re-serves and the existing elapsed-time ceiling supply interruption evidence.
