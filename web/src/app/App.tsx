/**
 * The application root.
 *
 * S1 delivered the scaffold. S3 adds the two pieces every later screen depends on and that
 * belong to no screen: the error boundary, which keeps a continuation throw from blanking the
 * page, and the toast host, which is the one reader of the out-of-React toast store.
 *
 * S4 replaces the card with the app shell and the topbar; the router of a later unit gives
 * the boundary `key={route}` and an `onReset` that returns to the dashboard.
 */
import { useState } from 'react';
import { ErrorBoundary } from './ErrorBoundary';
import { ToastHost } from './ToastHost';

export function App() {
  // The boundary resets BY KEY: a new key builds a new boundary whose error is null. The
  // router passes the route name here, so one broken screen does not poison the next.
  const [generation, setGeneration] = useState(0);

  return (
    <>
      <ErrorBoundary key={generation} onReset={() => setGeneration((n) => n + 1)}>
        <section className="card">
          <h1>Cadus</h1>
          <p className="muted">The scaffold is up. The screens arrive with the units after it.</p>
        </section>
      </ErrorBoundary>
      {/* Mounted for the life of the app: a toast raised after a view unmounts still lands. */}
      <ToastHost />
    </>
  );
}
