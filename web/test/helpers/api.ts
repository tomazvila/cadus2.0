/**
 * The failures a stubbed route answers, shared by every screen test.
 *
 * `busy()` is the rejection the Retry path handles, as opposed to the 401 that routes to
 * sign-in. `flakyOnce` builds the service that fails ONCE and then answers: the shape every
 * stale-Retry test starts from.
 */
import { ApiError } from '@/api';

/** The 503 every Retry test raises first. */
export function busy(): ApiError {
  return new ApiError(503, 'unavailable', 'The service is busy.');
}

/** A thrown `Error` with no envelope: the network went away. */
export function networkFailure(): Error {
  return new Error('the network went away');
}

/**
 * The append-only log of the service: the first grade fails with `busy()`, the next grade
 * of a problem is accepted, and every later post of that same `problem_id` is
 * `404 unknown_problem`.
 */
export function appendOnlyGrade<R>(reply: () => R): (problemId: string) => R {
  const spent = new Set<string>();
  let attempts = 0;
  return (problemId) => {
    attempts += 1;
    if (attempts === 1) throw busy();
    if (spent.has(problemId)) {
      throw new ApiError(404, 'unknown_problem', 'That problem is no longer open.');
    }
    spent.add(problemId);
    return reply();
  };
}

/**
 * A route whose FIRST call throws `busy()` and whose later calls answer `reply(attempt)`.
 *
 * The attempt count starts at 1, so `reply(2)` is the first reply that lands. Wrap it in
 * `vi.fn<…>()` at the call site, so the mock carries the route's own signature.
 */
export function flakyOnce<R>(reply: (attempt: number) => R): () => Promise<R> {
  let attempts = 0;
  return async () => {
    attempts += 1;
    if (attempts === 1) throw busy();
    return reply(attempts);
  };
}
