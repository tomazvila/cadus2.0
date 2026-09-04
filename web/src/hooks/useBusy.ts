/**
 * The per-button re-entrancy guard — the port of 1.0's `busy()` (`static/ui.js:153`).
 *
 * The vanilla app wraps every async button handler in `busy(e.currentTarget, fn)`, which
 * disables the button SYNCHRONOUSLY, before the first await. Two things depend on that,
 * and the first one is not cosmetic:
 *
 *  1. RE-ENTRANCY. A double click on "Start Proofs ▸" without the guard posts `/api/enroll`
 *     twice, and each post appends an `enrolled` row to the append-only event log —
 *     a table `cadus_app` holds no DELETE on. The learner sees one button; the log keeps
 *     two facts.
 *  2. THE SPINNER. `app.css` paints `.btn.is-busy` with an in-button spinner. Without the
 *     class a call of several seconds shows a dimmed button and nothing else.
 *
 * THE GUARD IS A REF, read and written synchronously before the first await. React commits
 * `disabled` one render later, and that render is exactly the window a fast second click
 * lands in. `usePhase` guards the study loop for the same reason.
 *
 * This hook guards ONE button against ITSELF. It is not the phase gate: a view that owns a
 * study phase gates on `usePhase`, which admits one transition across the whole view.
 */
import { useState } from 'react';

export interface Busy {
  /** Run an async handler under `key`. A re-entrant call is dropped, never queued. */
  run: (key: string, fn: () => Promise<void> | void) => void;
  /** True while `key` runs. It drives `disabled`. */
  is: (key: string) => boolean;
  /** The `className` for a button, with `is-busy` appended while `key` runs. */
  cls: (key: string, base: string) => string;
}

export function useBusy(): Busy {
  // A fresh object is never the previous state, so every `force()` renders once more.
  const [, rerender] = useState({});

  // ONE object per mount. The set of running keys lives in its closure, and `rerender` is a
  // stable setter, so the three functions hold for the life of the view.
  const [busy] = useState<Busy>(() => {
    const running = new Set<string>();
    const force = () => { rerender({}); };

    return {
      run(key, fn) {
        // The synchronous check and set. A second click inside the same tick — or any time
        // before React commits the disabled attribute — finds the key present and drops.
        if (running.has(key)) return;
        running.add(key);
        force();

        // Called synchronously, not through a microtask: the handler's own first state
        // updates then land in the same batch as the click, and not one tick outside the
        // caller's `act()`.
        let result: Promise<void> | void;
        try {
          result = fn();
        } catch (e) {
          running.delete(key);
          force();
          throw e;
        }

        void Promise.resolve(result).finally(() => {
          running.delete(key);
          force();
        });
      },
      is: (key) => running.has(key),
      cls: (key, base) => (running.has(key) ? `${base} is-busy` : base),
    };
  });

  return busy;
}
