/**
 * Recover from a route chunk that a deploy removed.
 *
 * A tab that stays open across a deploy runs the old bundle. Its lazy chunk is gone, so the
 * dynamic import fails. Vite reports that as `vite:preloadError` on `window`. One reload
 * fetches the new index and chunks. A sessionStorage flag limits this to one reload, so a
 * real outage cannot loop.
 */
export const RELOAD_FLAG = 'cadus:chunk-reload';

/** True when the error is a failed dynamic import of a module chunk. */
export function isChunkError(error: Error | string): boolean {
  const msg = typeof error === 'string' ? error : error.message;
  return /not a valid JavaScript MIME type|Failed to fetch dynamically imported module|error loading dynamically imported module|Importing a module script failed/i.test(msg);
}

/** Reload once. Returns true if it started a reload, false if it already did. */
export function reloadOnce(
  storage: Pick<Storage, 'getItem' | 'setItem'> | null = safeStorage(),
  reload: () => void = () => { location.reload(); },
): boolean {
  try {
    if (storage?.getItem(RELOAD_FLAG)) return false;
    storage?.setItem(RELOAD_FLAG, '1');
  } catch {
    return false;
  }
  reload();
  return true;
}

function safeStorage(): Storage | null {
  try { return window.sessionStorage; } catch { return null; }
}

/** Listen for `vite:preloadError`. Returns the remover. */
export function installChunkRecovery(
  target: Pick<Window, 'addEventListener' | 'removeEventListener'> = window,
  reload: () => boolean = () => reloadOnce(),
): () => void {
  const handler = (event: Event) => {
    if (reload()) event.preventDefault();
  };
  target.addEventListener('vite:preloadError', handler);
  return () => { target.removeEventListener('vite:preloadError', handler); };
}
