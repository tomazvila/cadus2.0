/**
 * The lifecycle discriminant (invariants F-37-1 and F-37-1c).
 *
 * One phase per study view, set SYNCHRONOUSLY before the first await. This is the single
 * highest-risk detail of the rewrite, because the defect it stops already shipped once in
 * 1.0: a second submit path posted the same `problem_id` twice.
 *
 * THE SIZE OF THE WINDOW, measured and not assumed. React flushes at the microtask
 * checkpoint of the current task, so two submit paths that read render state are both stale
 * only WITHIN ONE TASK. Two real keypresses arrive in separate tasks and do not collide. The
 * design survives that correction, because three reasons hold:
 *
 *  * PARITY. The 1.0 code reads a live variable, so its window is zero. Any React design
 *    with a window above zero is a behavior change toward a defect.
 *  * TWO PATHS IN ONE TASK is reachable. The Submit button and the Enter key are separate
 *    handlers, and a disabled button stops only the first of the two. A drill countdown
 *    auto-submit and a post-await continuation both land on a DefaultLane, which the
 *    Scheduler stretches under main-thread load.
 *  * THE COST OF AN ERROR. Two posts of one `problem_id` write two attempts to an
 *    append-only event log, and the loser gets `404 unknown_problem`.
 *
 * WHY AN EXTERNAL STORE, and not a ref mirrored into state:
 *
 *  * A ref/state mirror forces one `is()` to serve two consumers with opposite needs: render
 *    stays tear-free, and a handler stays synchronous. Whichever source it reads, the other
 *    consumer is wrong.
 *  * A ref read during render is a real tearing hazard. A concurrent render stops and
 *    resumes, and a ref mutated between the two halves gives inconsistent output inside one
 *    tree. `useSyncExternalStore` is the supported answer.
 *  * `tryEnter` takes a guard SET, not one `from` value. The quiz guards negatively
 *    (`if (phase !== 'done')`), and a single-value compare cannot say that.
 *
 * The hook gives back a TUPLE. An object that carries `phase` gets a new identity on every
 * render and poisons every dependency array it lands in. `gate` is stable; `phase` is a value.
 *
 * `useSyncExternalStore` puts the update on SyncLane, so a phase change is never deferred
 * into a transition. The store still moves BEFORE the DOM: `enter()` is synchronous, and the
 * commit follows at the microtask checkpoint. One test asserts exactly that gap.
 *
 * It is generic over the union of each view. The diagnostic has an `intro` phase the others
 * do not, so there is no one shared `Phase` type.
 */
import { useState, useSyncExternalStore } from 'react';

/** One phase, a set of them, or a predicate. */
type Guard<P extends string> = P | readonly P[] | ((p: P) => boolean);

function match<P extends string>(value: P, guard: Guard<P>): boolean {
  if (typeof guard === 'function') return guard(value);
  if (Array.isArray(guard)) return (guard as readonly P[]).includes(value);
  return value === (guard as P);
}

class PhaseStore<P extends string> {
  private value: P;
  private readonly listeners = new Set<() => void>();

  constructor(initial: P) { this.value = initial; }

  readonly get = (): P => this.value;

  readonly subscribe = (fn: () => void): (() => void) => {
    this.listeners.add(fn);
    return () => { this.listeners.delete(fn); };
  };

  /**
   * An unconditional set. Synchronous. A set to the phase already on notifies as well;
   * `useSyncExternalStore` compares the snapshot and renders nothing for it.
   */
  readonly enter = (next: P): void => {
    this.value = next;
    for (const fn of [...this.listeners]) fn();
  };

  /**
   * Compare and set, atomic against the JavaScript event loop.
   *
   * THE gate. Every submit path starts with
   * `if (!gate.tryEnter('ready', 'submitting')) return;`. The read and the write happen in
   * one synchronous step, so two events inside the grading window cannot both win.
   */
  readonly tryEnter = (from: Guard<P>, to: P): boolean => {
    if (!match(this.value, from)) return false;
    this.enter(to);
    return true;
  };
}

export interface Gate<P extends string> {
  /** The synchronous read, for HANDLERS. Do not call it during render — read `phase`. */
  peek: () => P;
  is: (guard: Guard<P>) => boolean;
  enter: (to: P) => void;
  tryEnter: (from: Guard<P>, to: P) => boolean;
}

export function usePhase<P extends string>(initial: P): readonly [P, Gate<P>] {
  // The lazy `useState` initializer, not `useRef(...) ??=`, which the react-hooks rule
  // rejects as a ref read during render.
  //
  // This is not "construct once": React calls the initializer TWICE in StrictMode development
  // and discards the second result. That is safe here only because the constructor is a pure
  // allocation. Keep it pure — a factory with a side effect runs it on every mount and leaks
  // the discarded half.
  const [store] = useState(() => new PhaseStore<P>(initial));

  // Tear-free: React re-reads a torn render and discards it.
  const phase = useSyncExternalStore(store.subscribe, store.get, store.get);

  // One gate per store, and one store per mount.
  const [gate] = useState<Gate<P>>(() => ({
    peek: store.get,
    is: (guard) => match(store.get(), guard),
    enter: store.enter,
    tryEnter: store.tryEnter,
  }));

  return [phase, gate] as const;
}
