/**
 * The application shell.
 *
 * FOUR THINGS SIT OUTSIDE EVERY SCREEN, and this is where they compose:
 *
 *   `Topbar`        the one way out of any screen; portalled into `#topbar`.
 *   `DialogProvider` the modal host. It wraps the view, so `useDialogs().open()` reaches it
 *                    from any depth, and a view unmount settles the promise it is awaiting.
 *   `ErrorBoundary` a continuation throw would otherwise blank the page. It resets BY KEY.
 *   `ToastHost`     mounted for the life of the app, so a toast raised after a view leaves
 *                   still lands.
 *
 * THE ORDER OF THE THREE WRAPPERS IS LOAD-BEARING. The boundary is INSIDE the provider, not
 * outside it: a screen that throws while a dialog is open must still let that dialog's
 * promise settle, and a provider unmounted by the throw cannot settle anything. The toast
 * host is outside both, because the failure it reports is often the failure that removed the
 * view.
 *
 * WHAT S4 DOES NOT DECIDE. There is no router yet, so `children` defaults to the placeholder
 * card and the navigation callbacks default to no-ops. The unit that adds URL routing (spec
 * section 4.1) passes the routed view as `children`, `key={route}` on the boundary, and real
 * `onHome` / `onMap` / `onLogout`; S6 passes the signed-in `user`.
 */
import { useState, type ReactNode } from 'react';
import { ErrorBoundary } from './ErrorBoundary';
import { ToastHost } from './ToastHost';
import { Topbar } from './Topbar';
import { DialogProvider } from '@/components/Modal';
import type { User } from '@/api/types';

export interface AppProps {
  /** The signed-in account. `null` while signed out. */
  user?: User | null;
  /** Demo mode: the bar shows the DEMO badge and offers no sign-out. */
  demo?: boolean;
  onHome?: () => void;
  onMap?: () => void;
  onLogout?: () => void;
  /** The routed view. */
  children?: ReactNode;
}

const noop = () => {};

export function App({
  user = null,
  demo = false,
  onHome = noop,
  onMap = noop,
  onLogout = noop,
  children,
}: AppProps) {
  // The boundary resets BY KEY: a new key builds a new boundary whose error is null. The
  // router passes the route name here, so one broken screen does not poison the next.
  const [generation, setGeneration] = useState(0);

  return (
    <>
      <Topbar user={user} demo={demo} onHome={onHome} onMap={onMap} onLogout={onLogout} />
      <DialogProvider>
        <ErrorBoundary key={generation} onReset={() => setGeneration((n) => n + 1)}>
          {children ?? (
            <section className="card">
              <h1>Cadus</h1>
              <p className="muted">The shell is up. The screens arrive with the units after it.</p>
            </section>
          )}
        </ErrorBoundary>
      </DialogProvider>
      {/* Mounted for the life of the app: a toast raised after a view unmounts still lands. */}
      <ToastHost />
    </>
  );
}
