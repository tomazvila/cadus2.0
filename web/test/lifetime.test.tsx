/**
 * The view-lifetime registry (S3).
 *
 * Two invariants live here. D10 is that ONE implementation serves every view, and that it
 * survives the simulated remount of React 19 StrictMode. F-37-1b is that a timer dies with
 * the view and refuses to arm after it.
 *
 * The StrictMode half is the reason the hook exists at all: a registry that dies on the first
 * simulated cleanup works in production and is frozen in development, and no test that mounts
 * without StrictMode can see the difference.
 */
import { StrictMode, useEffect } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { render, renderHook } from '@testing-library/react';
import { createLifetime, useLifetime, type Lifetime } from '@/hooks/useLifetime';

describe('createLifetime', () => {
  it('starts alive at generation 0', () => {
    const life = createLifetime();
    expect(life.alive()).toBe(true);
    expect(life.gen()).toBe(0);
    expect(life.current(0)).toBe(true);
  });

  it('revive() does not bump the generation', () => {
    // THE correction the 1.0 review found. `end()` bumps already, and `revive()` runs on
    // every mount, so a second bump here makes a token captured during render stale before
    // the effect finishes — and the guard `life.current(g)` then rejects forever.
    const life = createLifetime();
    life.revive();
    expect(life.gen()).toBe(0);

    life.end();
    expect(life.gen()).toBe(1);

    life.revive();
    expect(life.gen()).toBe(1);
    expect(life.alive()).toBe(true);
    expect(life.current(1)).toBe(true);
  });

  it('end() is idempotent, so a second teardown does not skip a generation', () => {
    const life = createLifetime();
    life.end();
    life.end();
    expect(life.gen()).toBe(1);
    expect(life.alive()).toBe(false);
  });

  it('bump() clears the registered timers and gives the new token', () => {
    vi.useFakeTimers();
    const life = createLifetime();
    const fired: string[] = [];
    life.setTimeout(() => fired.push('old'), 10);
    life.setInterval(() => fired.push('tick'), 10);

    expect(life.bump()).toBe(1);
    vi.advanceTimersByTime(100);
    expect(fired).toEqual([]);
    expect(life.current(0)).toBe(false);
    expect(life.current(1)).toBe(true);
  });

  it('F-37-1b: after teardown a timer arms nothing and gives 0', () => {
    vi.useFakeTimers();
    const life = createLifetime();
    const fired: string[] = [];

    life.end();
    const t = life.setTimeout(() => fired.push('late'), 10);
    const i = life.setInterval(() => fired.push('ticker'), 10);

    // The REFUSAL TO ARM is the strong half. A refusal only to fire still lets a continuation
    // that lands after teardown start a 1 Hz ticker against detached nodes.
    expect(t).toBe(0);
    expect(i).toBe(0);
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(100);
    expect(fired).toEqual([]);
  });

  it('F-37-1b: a timer armed before teardown does not fire after it', () => {
    vi.useFakeTimers();
    const life = createLifetime();
    const fired: string[] = [];
    life.setTimeout(() => fired.push('t'), 10);

    life.end();
    vi.advanceTimersByTime(100);
    expect(fired).toEqual([]);
  });

  it('clearTimer() ignores 0, which is what a refused arm gives back', () => {
    vi.useFakeTimers();
    const life = createLifetime();
    const fired: string[] = [];
    const id = life.setTimeout(() => fired.push('t'), 10);

    life.clearTimer(0);
    life.clearTimer(undefined);
    vi.advanceTimersByTime(100);
    expect(fired).toEqual(['t']);
    expect(id).not.toBe(0);
  });

  it('clearTimer() stops a timeout and an interval alike', () => {
    vi.useFakeTimers();
    const life = createLifetime();
    const fired: string[] = [];
    const t = life.setTimeout(() => fired.push('t'), 10);
    const i = life.setInterval(() => fired.push('i'), 10);
    expect(life.pending()).toBe(2);

    life.clearTimer(t);
    life.clearTimer(i);
    expect(life.pending()).toBe(0);
    vi.advanceTimersByTime(100);
    expect(fired).toEqual([]);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('forgets a timeout once it fired, and every timer once bumped or ended', () => {
    vi.useFakeTimers();
    const life = createLifetime();
    life.setTimeout(() => {}, 10);
    vi.advanceTimersByTime(10);
    expect(life.pending()).toBe(0);

    life.setTimeout(() => {}, 10);
    life.setInterval(() => {}, 10);
    life.bump();
    expect(life.pending()).toBe(0);
    expect(vi.getTimerCount()).toBe(0);

    life.setTimeout(() => {}, 10);
    life.setInterval(() => {}, 10);
    life.end();
    expect(life.pending()).toBe(0);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('is a pure allocation, so the discarded StrictMode instance leaks no timer', () => {
    // React calls a useState initializer TWICE in StrictMode development and throws the
    // second instance away. That is safe only while the factory arms nothing.
    vi.useFakeTimers();
    createLifetime();
    createLifetime();
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe('useLifetime', () => {
  it('D10: the StrictMode remount leaves alive === true', () => {
    const { result } = renderHook(() => useLifetime(), { wrapper: StrictMode });

    // The failing design — a registry created in a ref and ended by the first cleanup — reads
    // false here for the rest of the life of the view, and every continuation bails.
    expect(result.current.alive()).toBe(true);

    // The simulated unmount ran and bumped once; `revive()` did not bump again.
    expect(result.current.gen()).toBe(1);
    expect(result.current.current(1)).toBe(true);
  });

  it('D10: one instance serves every render of the view', () => {
    const { result, rerender } = renderHook(() => useLifetime(), { wrapper: StrictMode });
    const first = result.current;
    rerender();
    expect(result.current).toBe(first);
  });

  it('D10: the effect of the view really does run twice under StrictMode', () => {
    // Pins the premise of the test above. Without the double invoke, the assertion that
    // `alive` survives a remount proves nothing at all.
    const runs: string[] = [];

    function Probe({ onLife }: { onLife: (l: Lifetime) => void }) {
      const life = useLifetime();
      useEffect(() => {
        onLife(life);
        runs.push('mount');
        return () => { runs.push('cleanup'); };
      }, [life, onLife]);
      return <p>probe</p>;
    }

    const seen: Lifetime[] = [];
    render(
      <StrictMode>
        <Probe onLife={(l) => { seen.push(l); }} />
      </StrictMode>,
    );

    expect(runs).toEqual(['mount', 'cleanup', 'mount']);
    // One registry, reported by both passes of the effect.
    expect(seen.length).toBe(2);
    expect(seen[0]).toBe(seen[1]);
    expect(seen[0].alive()).toBe(true);
  });

  it('F-37-1b: the real unmount ends the lifetime and refuses a later timer', () => {
    vi.useFakeTimers();
    const { result, unmount } = renderHook(() => useLifetime(), { wrapper: StrictMode });
    const life = result.current;
    const fired: string[] = [];

    expect(life.setTimeout(() => fired.push('armed'), 10)).not.toBe(0);
    unmount();

    expect(life.alive()).toBe(false);
    expect(life.setTimeout(() => fired.push('late'), 10)).toBe(0);
    vi.advanceTimersByTime(100);
    expect(fired).toEqual([]);
  });
});
