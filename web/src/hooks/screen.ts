/**
 * Two moves every study screen makes the same way.
 *
 * `closeWith` ends a screen on the reply of its closing request, and ends it too when the
 * request fails: the summary is a receipt, not the record, so a failed close never strands
 * the learner on a spinner. `releaseOnFail` is the `onFail` of a graded submit: a failure
 * hands the problem back to the learner instead of leaving every control disabled.
 *
 * Both take the view `Lifetime` first, so a reply that lands after the view left changes
 * nothing (F-37-1b).
 */
import type { Call } from './useCall';
import type { Lifetime } from './useLifetime';
import type { Gate } from './usePhase';

/** Close the screen on the reply of `request`, and close it too when the request fails. */
export function closeWith<T, P extends string>(
  call: Call,
  life: Lifetime,
  gate: Gate<P>,
  done: P,
  request: () => Promise<T>,
  setSummary: (summary: T | null) => void,
): void {
  void call(request, (summary) => {
    if (life.alive()) { setSummary(summary); gate.enter(done); }
  }).then((summary) => {
    // A failed close still ends the screen: the summary is a receipt, not the record.
    if (!summary && life.alive()) { setSummary(null); gate.enter(done); }
  });
}

/**
 * The `onFail` of a submit: return the view from `locked` to `open` on every failure — the
 * first attempt and each retried one alike — so the learner meets a screen that works.
 */
export function releaseOnFail<P extends string>(
  life: Lifetime,
  gate: Gate<P>,
  locked: P,
  open: P,
): () => void {
  return () => { if (life.alive() && gate.is(locked)) gate.enter(open); };
}
