import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ErrorBoundary } from '../src/app/ErrorBoundary';
import { RELOAD_FLAG, installChunkRecovery, isChunkError, reloadOnce } from '../src/app/staleChunk';

function store() {
  const m = new Map<string, string>();
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => { m.set(k, v); } };
}

describe('stale chunk recovery', () => {
  it('reloads once, then never again', () => {
    const s = store();
    const reload = vi.fn();
    expect(reloadOnce(s, reload)).toBe(true);
    expect(s.getItem(RELOAD_FLAG)).toBe('1');
    expect(reloadOnce(s, reload)).toBe(false);
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('listens for vite:preloadError and removes the listener', () => {
    const reload = vi.fn(() => true);
    const off = installChunkRecovery(window, reload);
    const ev = new Event('vite:preloadError', { cancelable: true });
    window.dispatchEvent(ev);
    expect(reload).toHaveBeenCalledTimes(1);
    expect(ev.defaultPrevented).toBe(true);
    off();
    window.dispatchEvent(new Event('vite:preloadError'));
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('knows the MIME error', () => {
    expect(isChunkError(new TypeError("'text/html' is not a valid JavaScript MIME type for module script"))).toBe(true);
    expect(isChunkError(new Error('boom'))).toBe(false);
  });

  it('the boundary shows the reload text for a chunk error', () => {
    sessionStorage.setItem(RELOAD_FLAG, '1');
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    function Bad(): never { throw new TypeError('Failed to fetch dynamically imported module: x.js'); }
    render(<ErrorBoundary><Bad /></ErrorBoundary>);
    expect(screen.getByText('A new version was installed. Reload the page.')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Reload' })).toBeTruthy();
    spy.mockRestore();
    sessionStorage.removeItem(RELOAD_FLAG);
  });
});
