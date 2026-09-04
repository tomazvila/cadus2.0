/**
 * The view-lifetime registry (invariants D10 and F-37-1b).
 *
 * One implementation for every view. A view registers its timers here; teardown clears them
 * and marks the view dead, and every post-await continuation asks `alive()` or `current(g)`
 * before it touches the DOM.
 *
 * The surface drops the 1.0 `onEnd` callback: in React it duplicates effect cleanup, and two
 * teardown orders that agree is one order too many. Two behaviors go with it, and each later
 * unit replaces its own by hand:
 *
 *  1. 1.0 wraps every ender in `try { fn(); } catch {}`, so one throwing teardown does not
 *     strand the rest. A React cleanup that throws aborts the remaining cleanups of that
 *     unmount. S11 registers `cy.destroy()`, which is the call that throws, so the map wraps
 *     its own cleanup.
 *  2. 1.0 runs a late `onEnd(fn)` immediately when the view is already dead, so a late
 *     registration does not leak the resource it frees. React has no equivalent. A
 *     late-arriving resource goes free in its own effect cleanup.
 *
 * THE STRICTMODE TRAP, and the reason `revive()` exists.
 *
 * React 19 StrictMode simulates a remount in development: mount, effects, cleanups, effects
 * again. Refs survive that simulated remount. So the obvious implementation — create the
 * registry in a ref, tear it down in the effect cleanup — leaves `alive` false for the rest
 * of the life of the view:
 *
 *   1. the first render creates the registry in a ref; alive = true
 *   2. the effect runs; the cleanup runs, and alive = false
 *   3. the effect runs again, against THE SAME ref object, which is now dead forever
 *
 * Every post-await continuation then bails at its liveness check, and `setTimeout` refuses to
 * arm. The app works in production and is frozen in development, which is the worse failure
 * shape of the two. `revive()` makes the second pass a real mount again.
 */
import { useEffect, useState } from 'react';

export interface Lifetime {
  /** False after the teardown of the view. */
  alive: () => boolean;
  /** The current generation token. */
  gen: () => number;
  /** `alive() && g === gen()` — "this work still owns the view". */
  current: (g: number) => boolean;
  /** Start a new generation and clear every registered timer. Gives the new token. */
  bump: () => number;
  /** A registered timeout. After teardown it arms nothing and gives 0. */
  setTimeout: (fn: () => void, ms: number) => number;
  /** A registered interval. After teardown it arms nothing and gives 0. */
  setInterval: (fn: () => void, ms: number) => number;
  clearTimer: (id: number | undefined) => void;
  clearTimers: () => void;
}

/** The two operations only the hook calls. A view sees `Lifetime` and nothing more. */
export interface LifetimeInternal extends Lifetime {
  revive: () => void;
  end: () => void;
  /** How many timers the registry holds. A fired timeout leaves it; a cleared one too. */
  pending: () => number;
}

export function createLifetime(): LifetimeInternal {
  let alive = true;
  let gen = 0;
  let timeouts = new Set<number>();
  let intervals = new Set<number>();

  function clearTimers(): void {
    for (const id of timeouts) clearTimeout(id);
    for (const id of intervals) clearInterval(id);
    timeouts = new Set();
    intervals = new Set();
  }

  return {
    alive: () => alive,
    gen: () => gen,
    current: (g) => alive && g === gen,
    bump() { clearTimers(); return ++gen; },

    setTimeout(fn, ms) {
      // The refusal to ARM is F-37-1b. A timer that did arm is cleared by `end()`, so it never
      // fires after teardown; a guard on the fire would be a second copy of that promise.
      if (!alive) return 0;
      const id = window.setTimeout(() => {
        timeouts.delete(id);
        fn();
      }, ms);
      timeouts.add(id);
      return id;
    },

    setInterval(fn, ms) {
      if (!alive) return 0;
      const id = window.setInterval(fn, ms);
      intervals.add(id);
      return id;
    },

    clearTimer(id) {
      // 0 and undefined name no timer, and the browser ignores both.
      clearTimeout(id);
      clearInterval(id);   // the browser gives the two one id space
      timeouts.delete(id!);
      intervals.delete(id!);
    },

    clearTimers,

    pending: () => timeouts.size + intervals.size,

    /**
     * Undo a StrictMode development teardown. A true first mount sees no change.
     *
     * It does NOT bump the generation, and that is a correction, not an oversight. An earlier
     * design bumped here, to discard the work of the first pass. But `end()` bumps already,
     * so the extra bump bought nothing and cost correctness: `revive()` runs on EVERY mount,
     * so a token captured during render went stale before the effect finished.
     *
     * That breaks the shipped idiom outright. A view captures `const g = life.gen()` and gates
     * its slot fill on `life.current(g)`. With a bump here the guard rejects forever and the
     * view renders nothing — silently, on the first mount, in production.
     */
    revive() {
      alive = true;
    },

    end() {
      if (!alive) return;
      alive = false;
      gen += 1;
      clearTimers();
    },
  };
}

export function useLifetime(): Lifetime {
  // Created at the FIRST RENDER, not in the effect, so the registry exists before an effect
  // or a continuation needs it.
  //
  // The LAZY `useState` initializer, not `useRef(...) ??=`. The second form is a ref read
  // during render, which the react-hooks rule rejects.
  //
  // This is not "construct once": React calls a useState initializer TWICE in StrictMode
  // development and throws the second instance away. That is safe here ONLY because
  // `createLifetime` is a pure allocation that arms nothing. A factory with a side effect
  // leaks one instance per mount, invisibly. One test pins that property.
  const [life] = useState(createLifetime);

  useEffect(() => {
    life.revive();
    return () => life.end();
  }, [life]);

  return life;
}
