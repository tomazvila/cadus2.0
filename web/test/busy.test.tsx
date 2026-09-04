/**
 * The per-button re-entrancy guard (S3).
 *
 * The guard is a ref, read and written synchronously before the first await: a second
 * click inside the same tick finds the key present and drops. The dashboard tests prove
 * that against a real button; this file pins the handler contract itself.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, render, renderHook, screen } from '@testing-library/react';
import { useBusy } from '@/hooks/useBusy';

describe('useBusy', () => {
  it('runs one handler per key at a time and drops the re-entrant call', async () => {
    const { result } = renderHook(() => useBusy());
    let release!: () => void;
    const gate = new Promise<void>((r) => { release = r; });
    const fn = vi.fn(() => gate);

    act(() => {
      result.current.run('save', fn);
      result.current.run('save', fn);
    });
    expect(fn).toHaveBeenCalledTimes(1);
    expect(result.current.is('save')).toBe(true);
    expect(result.current.cls('save', 'btn')).toBe('btn is-busy');
    expect(result.current.is('other')).toBe(false);
    expect(result.current.cls('other', 'btn')).toBe('btn');

    await act(async () => { release(); await gate; });
    expect(result.current.is('save')).toBe(false);
    expect(result.current.cls('save', 'btn')).toBe('btn');
  });

  it('releases the key and rethrows when the handler throws before its first await', async () => {
    const { result } = renderHook(() => useBusy());
    const boom = () => { throw new Error('sync throw'); };

    act(() => { expect(() => result.current.run('save', boom)).toThrow('sync throw'); });
    expect(result.current.is('save')).toBe(false);
    // The key is free again, so the next press runs.
    const fn = vi.fn(() => undefined);
    await act(async () => { result.current.run('save', fn); });
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('accepts a handler that returns nothing', async () => {
    const { result } = renderHook(() => useBusy());
    const fn = vi.fn(() => undefined);
    await act(async () => { result.current.run('go', fn); });
    expect(fn).toHaveBeenCalledTimes(1);
    expect(result.current.is('go')).toBe(false);
  });
});

describe('useBusy on screen', () => {
  function Button({ handler }: { handler: () => Promise<void> | void }) {
    const busy = useBusy();
    return (
      <button
        type="button"
        disabled={busy.is('k')}
        className={busy.cls('k', 'btn')}
        // `run` rethrows a synchronous throw; the hook test above pins that. Here the screen
        // is the subject, so the throw stops at the button.
        onClick={() => { try { busy.run('k', handler); } catch { /* pinned elsewhere */ } }}
      >
        go
      </button>
    );
  }

  it('disables the control while the handler runs, and frees it after', async () => {
    let finish!: () => void;
    render(<Button handler={() => new Promise<void>((resolve) => { finish = resolve; })} />);
    const button = screen.getByRole<HTMLButtonElement>('button');

    await act(async () => { button.click(); });
    expect(button.disabled).toBe(true);
    expect(button.className).toBe('btn is-busy');

    await act(async () => { finish(); });
    expect(button.disabled).toBe(false);
    expect(button.className).toBe('btn');
  });

  it('frees the control when the handler throws before its first await', async () => {
    render(<Button handler={() => { throw new Error('threw before its first await'); }} />);
    const button = screen.getByRole<HTMLButtonElement>('button');

    await act(async () => { button.click(); });
    expect(button.disabled).toBe(false);
  });
});
