/**
 * The one request wrapper (invariant F-36-1).
 *
 * Three rules, and all three are load-bearing:
 *
 *  1. A 401 outside demo mode drops the session and routes to sign-in. The session cookie is
 *     HttpOnly, so a mid-session 401 means the service expired or revoked it, and JavaScript
 *     holds nothing to clear (SEC-cookie).
 *  2. Any other failure toasts with a Retry that re-runs the request AND its continuation.
 *     Without the continuation, the awaiting caller already returned `undefined` and bailed
 *     by the time Retry fires, the retried response goes in the bin, and the view sits on its
 *     spinner forever. A caller whose request expires — a write of one served `problem_id` —
 *     adds `retryGate` and `onFail`; see `CallOptions`.
 *  3. A throw from the continuation PROPAGATES and raises no Retry. That is a view defect,
 *     not a transport failure. A retry re-sends a request the service already accepted, and
 *     for a consumed write — the served problem — the second post gives
 *     `404 unknown_problem` against an append-only log.
 *
 * THE AUTH EXCEPTION (invariant AUTH-inline, trap T14). The sign-in and password screens of
 * S6 call the API directly and never through this hook. `invalid_credentials` is also a 401,
 * and rule 1 would throw a learner out of the screen they are already on. Those screens read
 * `ApiError.code` and render the message inline.
 *
 * THE RETURN CONTRACT, which every view depends on: a failure resolves `undefined`, and a
 * view branches on `if (!res)`. A call that legitimately resolves a falsy value — `null`,
 * `0`, `''`, `false` — reads as a failure to that branch. Keep writing `if (!res)`, so the
 * views cannot diverge from one another.
 *
 * THE REACT RULE. In 1.0 the continuation closes over one long-lived view closure and over
 * live DOM nodes, so it is current whenever it runs. In React it closes over the values of
 * THAT render, and a Retry pressed three renders later replays stale data. So: anything
 * reachable from a retried `onOk` reads through a ref, a store, or a reducer dispatch, and
 * never through captured render state. The external store of `usePhase` satisfies the rule
 * for the phase; the rest belongs to the call site.
 */
import { useCallback, useEffect, useRef } from 'react';
import { ApiError } from '@/api';
import { toast } from '@/app/toast';

/** The line the learner sees when the session is gone. */
export const SESSION_EXPIRED_MESSAGE = 'Your session has expired — please sign in again.';

/** The line a foreign throw with no message falls back to. */
export const GENERIC_FAILURE_MESSAGE = 'Something went wrong.';

/** The line a Retry gets when the screen moved past the request it would re-send. */
export const RETRY_STALE_MESSAGE = 'That retry came too late. Continue from the screen.';

export type Call = <T>(
  fn: () => Promise<T>,
  onOk?: (value: T) => unknown,
  opts?: CallOptions,
) => Promise<T | undefined>;

/**
 * What a call site adds when it holds a lock, or when its request expires.
 *
 * A Retry arrives long after the failure, and `run` holds no view state: no phase, no live
 * problem, no test of validity. For a WRITE the view has since moved past — a `problem_id`
 * a later submit already spent — the retried post reaches an append-only log and the
 * service answers `404 unknown_problem`. Both fields are optional, and a call that omits
 * them behaves exactly as it did before.
 */
export interface CallOptions {
  /**
   * The Retry gate, called SYNCHRONOUSLY when the learner presses Retry. It re-takes the
   * caller's lock and answers whether the request is still valid. False refuses the retry
   * with a PLAIN toast: a refusal is not a way back, so it carries no action and expires
   * (F-36-1b), rather than arm another refusal forever.
   */
  retryGate?: () => boolean;
  /**
   * Called on EVERY failure — the first attempt and each retried one — before the toast, so
   * a lock the gate took is released whatever the outcome, and the learner meets a screen
   * that works.
   */
  onFail?: () => void;
}

export interface CallDeps {
  /** True under `?demo=1`, where a 401 does NOT route to sign-in. */
  demo: boolean;
  /** Called on a session-expired 401, before the navigation. */
  onUnauthorized: () => void;
}

export function useCall({ demo, onUnauthorized }: CallDeps): Call {
  // A thin wrapper over `run`, which is also what a Retry re-enters, so the first attempt and
  // every retry execute THE SAME code. A duplicate of the body in the retry path is two
  // chances for a contract this exact to drift.
  //
  // The deps go through a ref and are not captured: a Retry arrives long after the failure,
  // and the rule of this hook applies to the hook itself. The write happens in an EFFECT,
  // never during render, because a render-time ref write is what the react-hooks rule
  // forbids. The initializer already holds the right values, so the first render is correct
  // without a wait for the effect.
  const depsRef = useRef({ demo, onUnauthorized });
  useEffect(() => { depsRef.current = { demo, onUnauthorized }; }, [demo, onUnauthorized]);

  return useCallback(
    <T,>(fn: () => Promise<T>, onOk?: (value: T) => unknown, opts?: CallOptions) =>
      run(fn, onOk, depsRef, opts),
    [],
  );
}

type DepsRef = { current: CallDeps };

async function run<T>(
  fn: () => Promise<T>,
  onOk: ((value: T) => unknown) | undefined,
  deps: DepsRef,
  opts?: CallOptions,
): Promise<T | undefined> {
  let res: T;
  try {
    res = await fn();
  } catch (e) {
    const { demo, onUnauthorized } = deps.current;
    // FIRST, and on every failure path: the caller releases whatever it locked, so the
    // screen behind the toast works and a retry meets a view in a known phase.
    opts?.onFail?.();
    // `sessionExpired` is `status === 401` and nothing else. A 403 from the CSRF layer is a
    // different failure and takes the Retry path.
    if (e instanceof ApiError && e.sessionExpired && !demo) {
      onUnauthorized();
      toast(SESSION_EXPIRED_MESSAGE, { kind: 'info' });
      return undefined;
    }
    // `.message` off ANY thrown value: a foreign throw still shows what it carries instead of
    // collapsing to the generic line.
    const message = (e as { message?: string } | null)?.message || GENERIC_FAILURE_MESSAGE;
    toast(message, {
      label: 'Retry',
      onAction: () => {
        // THE RETRY GATE. This hook holds no view state, so the caller decides here whether
        // the request is still valid, and re-takes its lock in the same synchronous step.
        // A refusal toast is PLAIN: it expires, where another actionable toast would re-arm
        // itself on every refusal.
        if (opts?.retryGate && !opts.retryGate()) {
          toast(RETRY_STALE_MESSAGE, { kind: 'info' });
          return;
        }
        void run(fn, onOk, deps, opts);
      },
    });
    return undefined;
  }

  // OUTSIDE the try, deliberately — see rule 3 of the module docstring.
  if (onOk) await onOk(res);
  return res;
}
