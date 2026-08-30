/**
 * The view error boundary (S3).
 *
 * `useCall` puts the continuation outside its try, so a continuation throw escapes. In the
 * 1.0 SPA that reaches `window.onerror` and the view keeps its DOM. In React it blanks the
 * page, so the failure inverts from "stale view" to "white screen, no recovery, no toast".
 *
 * Every test here declares the console.error it expects: `test/setup.ts` fails a test on any
 * console.error, and the boundary keeps the 1.0 observability on purpose.
 */
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { ErrorBoundary } from '@/app/ErrorBoundary';
import { App } from '@/app/App';
import { AXE_IN_JSDOM } from './axe';
import { allowConsoleError } from './setup';

/** Every console.error a caught render throw produces: React's own, and the boundary's. */
function allowBoundaryLogs(): void {
  allowConsoleError(/view crashed/);
  allowConsoleError(/The above error occurred/);
  allowConsoleError(/An error occurred in/);
}

function Boom({ fail }: { fail: boolean }) {
  if (fail) throw new Error('the grade continuation threw');
  return <p>the view</p>;
}

describe('ErrorBoundary', () => {
  it('renders its children while nothing throws', () => {
    render(
      <ErrorBoundary onReset={vi.fn()}>
        <Boom fail={false} />
      </ErrorBoundary>,
    );
    expect(screen.getByText('the view')).toBeTruthy();
  });

  it('catches a view throw, names it, and offers a way out', async () => {
    allowBoundaryLogs();
    const { container } = render(
      <ErrorBoundary onReset={vi.fn()}>
        <Boom fail />
      </ErrorBoundary>,
    );

    expect(screen.getByRole('alert').textContent)
      .toContain('Something went wrong on this screen.');
    expect(screen.getByText('the grade continuation threw')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeTruthy();
    expect(await axe(container, AXE_IN_JSDOM)).toHaveNoViolations();
  });

  it('keeps the console observability the 1.0 SPA had', () => {
    allowBoundaryLogs();
    const spy = vi.spyOn(console, 'error');
    render(
      <ErrorBoundary onReset={vi.fn()}>
        <Boom fail />
      </ErrorBoundary>,
    );
    expect(spy.mock.calls.some((args) => String(args[0]).includes('view crashed'))).toBe(true);
  });

  it('clears its own error and asks the mount point for a way back', () => {
    allowBoundaryLogs();
    const onReset = vi.fn();
    render(
      <ErrorBoundary onReset={onReset}>
        <Boom fail />
      </ErrorBoundary>,
    );

    act(() => { screen.getByRole('button', { name: 'Try again' }).click(); });
    expect(onReset).toHaveBeenCalledTimes(1);
  });

  it('a new key builds a fresh boundary, so one broken screen does not poison the next', () => {
    allowBoundaryLogs();

    function Host() {
      const [route, setRoute] = useState('broken');
      return (
        <ErrorBoundary key={route} onReset={() => setRoute('dashboard')}>
          <Boom fail={route === 'broken'} />
        </ErrorBoundary>
      );
    }

    render(<Host />);
    expect(screen.getByRole('alert')).toBeTruthy();

    act(() => { screen.getByRole('button', { name: 'Try again' }).click(); });
    expect(screen.getByText('the view')).toBeTruthy();
    expect(screen.queryByRole('alert')).toBeNull();
  });
});

describe('the app root', () => {
  it('mounts the boundary and the toast host around the view', () => {
    render(<App />);
    expect(screen.getByRole('heading', { level: 1 }).textContent).toBe('Cadus');
    // The host portals into the live region of index.html, which stays empty until a toast.
    expect(document.getElementById('toasts')!.getAttribute('aria-live')).toBe('polite');
  });
});
