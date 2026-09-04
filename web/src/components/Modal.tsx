/**
 * The modal overlay and the dialog context.
 *
 * THE IMPERATIVE PROMISE IS KEPT ON PURPOSE. Every call site is one linear async flow:
 *
 *   const mods = await call(() => api.listModules());   if (!mods) return;
 *   const choice = await pickModule(mods.modules);      if (!choice) return;
 *   await call(() => api.enroll(choice));
 *
 * Written as `setPickerOpen(true)` plus an `onConfirm` callback, that one readable function
 * becomes three disconnected pieces, and the cancel path has to be rebuilt at each step.
 *
 * A PROMISE HANDED OUT MUST ALWAYS SETTLE — the whole risk of this design. Three paths have
 * to be closed for that to hold, and 1.0 found all three open:
 *
 *  1. A second `open()` while one is showing RESOLVES the incumbent `null` first. Before
 *     that, the incumbent was overwritten and its awaiting handler hung forever — reachable
 *     by a double click on one button.
 *  2. `currentRef` is written SYNCHRONOUSLY inside `open()`, never in an effect. Written in
 *     an effect, an `open()` that shares a batch with the unmount leaves the ref stale and
 *     the promise pending.
 *  3. The provider unmount settles whatever is open.
 *
 * Each one has a test that AWAITS the promise. A test that asserts the overlay left the DOM
 * proves nothing: React does that for free by unmounting the portal, so such a test passes
 * with all of the machinery above deleted.
 */
import {
  createContext, useCallback, useContext, useEffect, useMemo, useRef, useState,
} from 'react';
import { createPortal } from 'react-dom';

type Resolver<T> = (value: T | null) => void;

interface OpenDialog {
  id: number;
  render: (resolve: Resolver<never>) => React.ReactNode;
  resolve: Resolver<never>;
}

export interface Dialogs {
  /**
   * Show a dialog and await its result. It resolves `null` on cancel, on Esc, on a backdrop
   * click, on a superseding `open()`, and on provider unmount.
   */
  open<T>(render: (resolve: Resolver<T>) => React.ReactNode): Promise<T | null>;
}

const DialogContext = createContext<Dialogs | null>(null);

export function useDialogs(): Dialogs {
  const value = useContext(DialogContext);
  if (!value) throw new Error('useDialogs() outside a DialogProvider');
  return value;
}

export function DialogProvider({ children }: { children: React.ReactNode }) {
  const [current, setCurrent] = useState<OpenDialog | null>(null);
  const nextId = useRef(1);
  const currentRef = useRef<OpenDialog | null>(null);

  useEffect(() => () => { currentRef.current?.resolve(null); currentRef.current = null; }, []);

  const open = useCallback(<T,>(render: (resolve: Resolver<T>) => React.ReactNode) =>
    new Promise<T | null>((resolvePromise) => {
      const id = nextId.current++;
      let settled = false;

      const resolve = (value: T | null) => {
        // Idempotent: Cancel then Esc, or a resolve that races the unmount, must not settle
        // the same promise twice.
        if (settled) return;
        settled = true;
        if (currentRef.current?.id === id) currentRef.current = null;
        setCurrent((c) => (c?.id === id ? null : c));
        resolvePromise(value);
      };

      // Supersede: settle the incumbent instead of dropping it on the floor.
      currentRef.current?.resolve(null);

      const entry: OpenDialog = {
        id,
        render: render as OpenDialog['render'],
        resolve: resolve as Resolver<never>,
      };
      currentRef.current = entry;
      setCurrent(entry);
    }), []);

  const value = useMemo<Dialogs>(() => ({ open }), [open]);

  return (
    <DialogContext.Provider value={value}>
      {children}
      {current ? (
        <Modal key={current.id} onCancel={current.resolve}>
          {current.render(current.resolve)}
        </Modal>
      ) : null}
    </DialogContext.Provider>
  );
}

/**
 * The overlay: a portal to `document.body`, a backdrop dismiss, Esc, a real focus trap, and
 * focus restore.
 *
 * THE TRAP IS NOT DECORATION. `aria-modal="true"` tells a screen reader that the rest of the
 * page is inert, and a Tab that walks out to the page behind makes that a lie. A 1.0 version
 * claimed a trap in three comments and bound Escape alone.
 */
function Modal({ children, onCancel }: {
  children: React.ReactNode;
  onCancel: (value: null) => void;
}) {
  // Scoped to the OVERLAY, so the trap finds the dialog's controls wherever the caller puts
  // them.
  const ref = useRef<HTMLDivElement>(null);
  // `onCancel`'s identity changes with every provider render, so the listener must not
  // re-bind: a re-bind also re-runs the initial focus and steals the caret mid-interaction.
  const cancelRef = useRef(onCancel);
  useEffect(() => { cancelRef.current = onCancel; }, [onCancel]);

  useEffect(() => {
    const previouslyFocused = document.activeElement as HTMLElement | null;

    const focusable = () => Array.from(
      ref.current?.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]),'
        + ' textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ) ?? [],
    );

    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { e.preventDefault(); cancelRef.current(null); return; }
      if (e.key !== 'Tab') return;

      const items = focusable();
      if (!items.length) return;
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement;

      // Wrap at both ends, and pull focus back in when it is already outside.
      if (e.shiftKey && (active === first || !ref.current?.contains(active))) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && (active === last || !ref.current?.contains(active))) {
        e.preventDefault();
        first.focus();
      }
    };

    document.addEventListener('keydown', onKey);
    focusable()[0]?.focus();

    return () => {
      document.removeEventListener('keydown', onKey);
      // Focus belongs back on the control that opened the dialog.
      previouslyFocused?.focus?.();
    };
    // Mount and unmount only.
  }, []);

  const host = typeof document === 'undefined' ? null : document.body;
  if (!host) return null;

  return createPortal(
    <div
      ref={ref}
      className="modal-overlay"
      // Scenery, not a control: the dialog inside carries `role="dialog"`. The backdrop
      // click is a mouse convenience whose keyboard equivalent is Esc, bound above — a
      // keyboard listener HERE would fire only while the backdrop itself has focus, which it
      // never does. `role="presentation"` says exactly that.
      role="presentation"
      onClick={(e) => { if (e.target === e.currentTarget) onCancel(null); }}
    >
      {/*
        NO wrapper element. The child IS the dialog surface, and it carries `.modal` plus
        `role="dialog"` itself. `app.css` gives `.modal` `display: grid` and expects the
        dialog's own children to be its grid items; a classed div around it collapses that
        grid to one item and every internal gap disappears.
      */}
      {children}
    </div>,
    host,
  );
}
