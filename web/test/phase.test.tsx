/**
 * The phase gate (S3).
 *
 * F-37-1: one phase discriminant per view, moved SYNCHRONOUSLY before the first await. The
 * defect it stops shipped once in 1.0 — the Submit button and the Enter key are separate
 * handlers, and a disabled button stops only the first of the two, so one problem posted
 * twice.
 *
 * The two-path test dispatches both events in ONE task, outside `act()`, deliberately. An
 * `act()` around each event flushes a render between them and hides the exact race the gate
 * exists to lose.
 */
import { useRef } from 'react';
import { describe, expect, it } from 'vitest';
import { act, render, renderHook, screen } from '@testing-library/react';
import { usePhase, type Gate } from '@/hooks/usePhase';
import { allowConsoleError } from './setup';

type Phase = 'loading' | 'ready' | 'submitting' | 'feedback' | 'done';

describe('usePhase', () => {
  it('starts at its initial phase and reports it to render', () => {
    const { result } = renderHook(() => usePhase<Phase>('loading'));
    const [phase, gate] = result.current;
    expect(phase).toBe('loading');
    expect(gate.peek()).toBe('loading');
  });

  it('returns a tuple whose gate identity is stable across renders', () => {
    // An object that carries `phase` gets a new identity every render and poisons every
    // dependency array it lands in. The gate is stable; the phase is a value.
    const { result, rerender } = renderHook(() => usePhase<Phase>('ready'));
    const first = result.current[1];
    rerender();
    expect(result.current[1]).toBe(first);
    expect(result.current[0]).toBe('ready');
  });

  it('tryEnter takes a guard SET, not one from-value', () => {
    const { result } = renderHook(() => usePhase<Phase>('feedback'));
    const gate = result.current[1];

    const refused = gate.tryEnter('ready', 'submitting');
    let accepted = false;
    act(() => { accepted = gate.tryEnter(['ready', 'feedback'], 'submitting'); });

    expect(refused).toBe(false);
    expect(accepted).toBe(true);
    expect(gate.peek()).toBe('submitting');
  });

  it('tryEnter takes a predicate, which is how the quiz guards negatively', () => {
    // The quiz says `if (phase !== 'done')`, and a single-value compare cannot say that.
    const { result } = renderHook(() => usePhase<Phase>('submitting'));
    const gate = result.current[1];

    let moved = false;
    act(() => { moved = gate.tryEnter((p) => p !== 'done', 'ready'); });
    expect(moved).toBe(true);
    expect(gate.peek()).toBe('ready');

    let again = true;
    act(() => {
      gate.enter('done');
      again = gate.tryEnter((p) => p !== 'done', 'ready');
    });
    expect(again).toBe(false);
    expect(gate.peek()).toBe('done');
  });

  it('is() reads the live value, so a handler never sees the previous render', () => {
    const { result } = renderHook(() => usePhase<Phase>('ready'));
    const gate = result.current[1];
    act(() => { gate.enter('submitting'); });
    expect(gate.is('submitting')).toBe(true);
    expect(gate.is(['ready', 'feedback'])).toBe(false);
  });

  it('F-37-1: the store moves synchronously and the DOM follows at the commit', async () => {
    // Written down because the two are easy to conflate. `enter()` moves the store in the
    // same step; React commits at the microtask checkpoint that follows.
    allowConsoleError(/not wrapped in act/);

    const gateOut: Array<Gate<Phase>> = [];
    function View() {
      const [phase, gate] = usePhase<Phase>('ready');
      gateOut.push(gate);
      return <p data-testid="phase">{phase}</p>;
    }

    render(<View />);
    const gate = gateOut[0];

    gate.enter('submitting');
    expect(gate.peek()).toBe('submitting');
    expect(screen.getByTestId('phase').textContent).toBe('ready');

    await act(async () => {});
    expect(screen.getByTestId('phase').textContent).toBe('submitting');
  });
});

describe('the phase gate against two submit paths', () => {
  /**
   * One view with the two real paths: the Submit button and Enter in the answer field. Both
   * call the same guarded submit, and `busy()` in 1.0 disables the button alone.
   */
  function Submitter({ posts }: { posts: string[] }) {
    const [phase, gate] = usePhase<Phase>('ready');
    const field = useRef<HTMLInputElement>(null);

    // The gate moves BEFORE the first await. Everything after the await is a continuation.
    async function submit(path: string): Promise<void> {
      if (!gate.tryEnter('ready', 'submitting')) return;
      posts.push(path);
      await Promise.resolve();
      gate.enter('feedback');
    }

    return (
      <form onSubmit={(e) => { e.preventDefault(); void submit('enter'); }}>
        <label htmlFor="answer">Answer</label>
        <input id="answer" ref={field} />
        <button type="button" onClick={() => void submit('click')}>Submit</button>
        <output data-testid="phase">{phase}</output>
      </form>
    );
  }

  it('F-37-1: two submit paths in ONE task post exactly once', () => {
    allowConsoleError(/not wrapped in act/);
    const posts: string[] = [];
    render(<Submitter posts={posts} />);

    // Both events in the same task, with no act() between them — the window React's
    // microtask checkpoint leaves open, and the one the gate closes.
    screen.getByRole('button', { name: 'Submit' }).click();
    screen.getByRole('textbox', { name: 'Answer' }).closest('form')!
      .dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));

    expect(posts).toEqual(['click']);
  });

  it('F-37-1: the gate reopens only when the view puts it back', async () => {
    allowConsoleError(/not wrapped in act/);
    const posts: string[] = [];
    render(<Submitter posts={posts} />);
    const button = screen.getByRole('button', { name: 'Submit' });

    button.click();
    button.click();
    expect(posts).toEqual(['click']);

    await act(async () => {});
    expect(screen.getByTestId('phase').textContent).toBe('feedback');

    // `feedback` is not `ready`, so the guard still refuses. See trap T8: the view returns
    // itself to `ready` for the re-solve.
    button.click();
    expect(posts).toEqual(['click']);
  });
});
