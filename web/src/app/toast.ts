/**
 * The toast store. It lives OUTSIDE React, deliberately.
 *
 * `useCall` raises toasts from continuations that resolve after a view unmounts — the
 * session-expired path navigates and THEN toasts — and React drops a `setState` on an
 * unmounted component in silence. No warning, and one lost toast; for the session-expired
 * line that is the one message the learner needs.
 *
 * A module-level store, read through `useSyncExternalStore` by one host that stays mounted,
 * has no such window.
 *
 * F-36-1b: a toast that carries an action NEVER auto-dismisses. That toast IS the recovery
 * affordance. An auto-hide after 6 s deletes the only way back for a reader who looked away.
 * A plain toast still expires.
 */

type ToastKind = 'error' | 'info' | 'success';

export interface ToastOptions {
  label?: string;
  onAction?: () => void;
  kind?: ToastKind;
  /** Milliseconds. Ignored when `onAction` is set — see F-36-1b. */
  timeout?: number;
}

export interface Toast {
  id: number;
  message: string;
  kind: ToastKind;
  label?: string;
  onAction?: () => void;
}

/** The default life of a plain toast, in milliseconds. */
export const TOAST_TIMEOUT_MS = 6000;

let nextId = 1;
let toasts: readonly Toast[] = [];
const listeners = new Set<() => void>();
const timers = new Map<number, number>();

function emit(): void {
  for (const fn of [...listeners]) fn();
}

export const toastStore = {
  subscribe(fn: () => void): () => void {
    listeners.add(fn);
    return () => { listeners.delete(fn); };
  },
  getSnapshot(): readonly Toast[] {
    return toasts;
  },
};

export function dismissToast(id: number): void {
  const timer = timers.get(id);
  if (timer) { clearTimeout(timer); timers.delete(id); }
  if (!toasts.some((t) => t.id === id)) return;
  toasts = toasts.filter((t) => t.id !== id);
  emit();
}

/** Raise a toast. Gives back its dismiss function. */
export function toast(message: string, opts: ToastOptions = {}): () => void {
  const { label, onAction, kind = 'error', timeout = TOAST_TIMEOUT_MS } = opts;
  const id = nextId++;

  // `exactOptionalPropertyTypes` is on, so an absent field is omitted and never written as
  // `undefined`.
  const entry: Toast = {
    id,
    message,
    kind,
    ...(label !== undefined ? { label } : {}),
    ...(onAction ? { onAction } : {}),
  };
  toasts = [...toasts, entry];

  // F-36-1b lives on this one line: an actionable toast arms no timer at all.
  if (timeout && !onAction) {
    timers.set(id, window.setTimeout(() => dismissToast(id), timeout));
  }
  emit();
  return () => dismissToast(id);
}

/** The action dismisses FIRST and fires after, so a retry that toasts again does not stack. */
export function fireToastAction(id: number): void {
  const entry = toasts.find((t) => t.id === id);
  if (!entry) return;
  dismissToast(id);
  entry.onAction?.();
}

/**
 * A test seam, and nothing else.
 *
 * It clears the store and deliberately does NOT notify. It runs from `beforeEach`, where a
 * host is still mounted; a notify there re-renders that host outside `act()`, which React
 * reports as an unwrapped update and the suite then fails on. A host that mounts after this
 * reads the cleared snapshot anyway.
 */
export function resetToasts(): void {
  for (const timer of timers.values()) clearTimeout(timer);
  timers.clear();
  nextId = 1;
  toasts = [];
}
