/**
 * The read path of the two operator screens.
 *
 * WHY THIS IS NOT `useCall`, and the precedent it follows. `useCall` turns every failure
 * that is not a 401 into a toast with a Retry, and it resolves `undefined` to the caller.
 * That is right for a study screen, where a failed read is a transport fault and the screen
 * itself is still the screen the learner asked for. It is wrong here: a `403 forbidden` IS
 * the answer to "may this account see the review queue", and the screen has to render the
 * refusal in place of the queue. A toast that says "forbidden" over an empty operator table
 * tells a learner nothing and offers a Retry that will fail the same way forever.
 *
 * The auth screens of S6 take the same exception for the same reason (AUTH-inline): a
 * failure the screen is ABOUT is rendered by the screen, not toasted past it.
 *
 * The 401 rule is kept whole. A session that expired mid-read still calls `onUnauthorized`
 * and drops the view, outside demo mode, exactly as `useCall` would.
 *
 * THE PAYLOAD ON SCREEN STAYS WHILE THE NEXT ONE LOADS (the S11 rule). A reload after an
 * approve does not blank the queue: blanking it takes the selection and the right-hand pane
 * down with it, and the reviewer loses their place in a list they were working through.
 *
 * A REPLY APPLIES ONLY WHILE IT IS THE NEWEST. Each attempt carries the generation it was
 * started under, and an older generation never overwrites a newer one — the reviewer who
 * pressed Retry twice must not end up with the first reply on screen.
 */
import { useEffect, useRef, useState } from 'react';
import { ApiError } from '@/api';
import { useLifetime } from '@/hooks/useLifetime';

/** What kind of refusal a read met. */
type AdminFailure =
  /** `403 forbidden` — the account is not an operator. This is an answer, not a fault. */
  | 'forbidden'
  /** `503 admin_path_unavailable` — this deployment configured no admin connection. */
  | 'unavailable'
  /** Anything else: a network failure, a 500, a proxy. A Try again is worth offering. */
  | 'error';

/** The heading of the refusal block. */
export const FORBIDDEN_TITLE = 'This screen serves operator accounts';

/** The body of the refusal block. It says what to do, and it names no other screen. */
export const FORBIDDEN_MESSAGE =
  'The service refused this request for your account. Ask an operator to run the review, '
  + 'or sign in with an operator account.';

/** The heading of the closed-write block (`503 admin_path_unavailable`). */
export const UNAVAILABLE_TITLE = 'The review writes are closed';

/** The fallback line of a read that failed for no reason the envelope named. */
const GENERIC_FAILURE_MESSAGE = 'Could not load this screen.';

/** Classify one thrown value. Only the envelope code decides a refusal. */
function classify(e: Error): AdminFailure {
  // On the CODE, never on the status alone. The CSRF layer answers `403
  // cross_origin_rejected`, which is a transport fault and not a statement about the
  // account; reading 403 as "not an operator" would tell an operator they are not one.
  if (e instanceof ApiError && e.code === 'forbidden') return 'forbidden';
  if (e instanceof ApiError && e.code === 'admin_path_unavailable') return 'unavailable';
  return 'error';
}

/** The message of one thrown value, or the generic line. */
function messageOf(e: Error): string {
  return e.message || GENERIC_FAILURE_MESSAGE;
}

/** The refusal of one attempt, and the message that came with it. */
export interface AdminFault {
  failure: AdminFailure;
  message: string;
}

export interface AdminLoad<T> {
  /** The newest payload that arrived, or null before the first one. */
  data: T | null;
  /** The refusal of the newest attempt, or null when it succeeded. */
  fault: AdminFault | null;
  /** True while an attempt is in flight, including the first. */
  loading: boolean;
  /** Start a new attempt. The payload on screen stays until the new one lands. */
  reload: () => void;
}

export interface AdminLoadDeps<T> {
  /**
   * The read. It MUST be stable — wrap it in `useCallback` — because it is the dependency
   * that starts an attempt: a fresh function per render reads in a loop.
   */
  load: () => Promise<T>;
  /** Demo mode. A 401 then keeps the reader on the screen, as everywhere else. */
  demo: boolean;
  /** The session-expired path. */
  onUnauthorized: () => void;
}

interface LoadState<T> {
  /** The attempt this state came from, or null while nothing has landed. */
  landed: object | null;
  data: T | null;
  fault: AdminFault | null;
}

/** The two deps a continuation reads after its await. */
interface LiveDeps {
  demo: boolean;
  onUnauthorized: () => void;
}

export function useAdminLoad<T>({ load, demo, onUnauthorized }: AdminLoadDeps<T>): AdminLoad<T> {
  const life = useLifetime();
  // One token per attempt. `reload` mints a new one, and the effect below starts on it.
  const [attempt, setAttempt] = useState<object>({});
  const [state, setState] = useState<LoadState<T>>({ landed: null, data: null, fault: null });

  // The deps go through a ref for the reason `useCall` states: a continuation that lands
  // after an await must not run against the values of the render that started it. The write
  // is in an effect, never during render, and that effect runs before the read below starts.
  const depsRef = useRef<LiveDeps | null>(null);
  useEffect(() => {
    depsRef.current = { demo, onUnauthorized };
  }, [demo, onUnauthorized]);

  // The newest attempt out. An older one that lands after it changes nothing.
  const latest = useRef<object | null>(null);

  useEffect(() => {
    latest.current = attempt;
    void (async () => {
      let data: T;
      try {
        data = await load();
      } catch (thrown) {
        // A `load` rejects with an `ApiError` or with a foreign `Error`; neither is void.
        const e = thrown as Error;
        const deps = depsRef.current!;
        if (e instanceof ApiError && e.sessionExpired && !deps.demo) {
          if (life.alive()) deps.onUnauthorized();
          return;
        }
        if (latest.current !== attempt) return;
        // A failure keeps the payload that is already on screen.
        setState((prev) => ({
          landed: attempt,
          data: prev.data,
          fault: { failure: classify(e), message: messageOf(e) },
        }));
        return;
      }
      if (latest.current !== attempt) return;
      setState({ landed: attempt, data, fault: null });
    })();
  }, [load, attempt, life]);

  const [reload] = useState(() => () => { setAttempt({}); });

  return {
    data: state.data,
    fault: state.fault,
    // Derived, not stored: an attempt is in flight exactly while no state of its token has
    // landed. One less flag to leave true on a path that forgot to clear it.
    loading: state.landed !== attempt,
    reload,
  };
}
