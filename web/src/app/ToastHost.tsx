/**
 * The toast host: mounted for the life of the app, portalled into `#toasts`.
 *
 * The live region exists in the DOM BEFORE its first toast, or a screen reader announces
 * nothing: `aria-live` announces a CHANGE of content, and a region that mounts together with
 * its content is not a change. `index.html` ships the container with `aria-live="polite"` on
 * it, and this component portals into that container instead of creating one. That is what
 * keeps the guarantee (spec section 4.5).
 *
 * Each toast carries `role="status"`. An actionable toast keeps its action button, because
 * F-36-1b gives it no timer to remove it.
 */
import { useSyncExternalStore } from 'react';
import { createPortal } from 'react-dom';
import { dismissToast, fireToastAction, toastStore } from './toast';

export function ToastHost() {
  const toasts = useSyncExternalStore(
    toastStore.subscribe,
    toastStore.getSnapshot,
    toastStore.getSnapshot,
  );

  const host = document.getElementById('toasts');
  if (!host) return null;

  return createPortal(
    toasts.map((t) => (
      <div key={t.id} className={`toast toast-${t.kind}`} role="status">
        <span className="toast-msg">{t.message}</span>
        {t.onAction ? (
          <button type="button" className="toast-action" onClick={() => fireToastAction(t.id)}>
            {t.label || 'Retry'}
          </button>
        ) : null}
        <button
          type="button"
          className="toast-close"
          aria-label="Dismiss"
          onClick={() => dismissToast(t.id)}
        >
          ×
        </button>
      </div>
    )),
    host,
  );
}
