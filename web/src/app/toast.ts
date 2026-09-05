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

/** One live toast and the expiry timer it armed, if it armed one. */
interface Entry {
  toast: Toast;
  timer: number | undefined;
}

let nextId = 1;
let entries: readonly Entry[] = [];
/** The snapshot the host reads: the toasts of `entries`, in order. */
let toasts: readonly Toast[] = [];
const listeners = new Set<() => void>();

/** Replace the live entries, rebuild the snapshot, and tell the host. */
function commit(next: readonly Entry[]): void {
  entries = next;
  toasts = entries.map((entry) => entry.toast);
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
  const entry = entries.find((e) => e.toast.id === id);
  if (!entry) return;
  clearTimeout(entry.timer);
  commit(entries.filter((e) => e !== entry));
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

  // F-36-1b lives on this one line: an actionable toast arms no timer at all. A timeout of
  // 0 arms none either.
  const timer = onAction || !timeout ? undefined : window.setTimeout(() => dismissToast(id), timeout);
  commit([...entries, { toast: entry, timer }]);
  return () => dismissToast(id);
}

/** The action dismisses FIRST and fires after, so a retry that toasts again does not stack. */
export function fireToastAction(id: number): void {
  const entry = entries.find((e) => e.toast.id === id);
  if (!entry) return;
  dismissToast(id);
  entry.toast.onAction?.();
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
  for (const entry of entries) clearTimeout(entry.timer);
  entries = [];
  toasts = [];
  nextId = 1;
}
