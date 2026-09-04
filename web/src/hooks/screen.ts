/**
 * Two moves every study screen makes the same way.
 *
 * `closeWith` ends a screen on the reply of its closing request, and ends it too when the
 * request fails: the summary is a receipt, not the record, so a failed close never strands
 * the learner on a spinner. `releaseOnFail` is the `onFail` of a graded submit: a failure
 * hands the problem back to the learner instead of leaving every control disabled.
 *
 * Neither asks the view `Lifetime` first. A phase or a summary written after the view left
 * reaches no screen: React drops a state write of an unmounted component, and the phase
 * store has no subscriber left to tell.
 */
import type { Call } from './useCall';
import type { Gate } from './usePhase';

/** Close the screen on the reply of `request`, and close it too when the request fails. */
export function closeWith<T, P extends string>(
  call: Call,
  gate: Gate<P>,
  done: P,
  request: () => Promise<T>,
  setSummary: (summary: T) => void,
): void {
  // A failed close still ends the screen: the summary is a receipt, not the record.
  void call(request, setSummary).then(() => { gate.enter(done); });
}

/**
 * The `onFail` of a submit: return the view from `locked` to `open` on every failure — the
 * first attempt and each retried one alike — so the learner meets a screen that works. A
 * view that moved on while the submit was out stays where it moved to.
 */
export function releaseOnFail<P extends string>(gate: Gate<P>, locked: P, open: P): () => void {
  return () => { if (gate.is(locked)) gate.enter(open); };
}
