/**
 * The view error boundary.
 *
 * `useCall` puts the continuation OUTSIDE its try on purpose (rule 3 there), so a throw from
 * a continuation escapes. In the 1.0 SPA that throw reaches `window.onerror` and THE VIEW
 * KEEPS ITS DOM: the learner sees a stale but intact screen and still navigates away.
 *
 * In React an uncaught throw during render unmounts the whole tree to a blank page. The
 * failure inverts from "stale view" to "white screen, no recovery, no toast", which is
 * strictly worse. This boundary restores a way out.
 *
 * The mount point gives it `key={route}`, so one broken screen does not poison the next: a
 * new key builds a new boundary with `error: null`. Try again clears the error in place, and
 * React mounts the screen once more under the same boundary.
 */
import { Component, type ErrorInfo, type ReactNode } from 'react';

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  override state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    // Keep the 1.0 observability: the throw still reaches the console, where an uncaught
    // error goes today.
    console.error('view crashed', error, info.componentStack);
  }

  override render(): ReactNode {
    const { error } = this.state;
    if (error === null) return this.props.children;

    return (
      <section className="empty" role="alert">
        <p>Something went wrong on this screen.</p>
        <p className="muted small">{error.message}</p>
        <button
          type="button"
          className="btn btn-primary"
          onClick={() => { this.setState({ error: null }); }}
        >
          Try again
        </button>
      </section>
    );
  }
}
