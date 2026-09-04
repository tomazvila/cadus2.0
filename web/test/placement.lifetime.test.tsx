/**
 * The placement diagnostic (S10), part 3: the view lifetime, the gate under a same-tick
 * double, the Retry that proceeds, and the payload shapes.
 *
 * `placement.test.tsx` carries the module note and the fixtures live in
 * `test/helpers/placement.tsx`.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, screen } from '@testing-library/react';
import { fireToastAction } from '@/app/toast';
import { flakyOnce } from './helpers/api';
import { allowConsoleError } from './setup';
import {
  START, answer, answerInput, begin, mount, probe, probeText, progressCount, stubDiag,
  submitButton, toasts,
} from './helpers/placement';
import type { DiagAnswerResponse, DiagStartResponse, DiagnosticApi } from '@/api/diag';

/** A held reply the test releases by hand. */
function held<T>() {
  let release!: (value: T) => void;
  const promise = new Promise<T>((r) => { release = r; });
  return { promise, release: (value: T) => release(value) };
}

describe('the view lifetime', () => {
  it('paints no probe from a start that lands after the view left', async () => {
    const start = held<DiagStartResponse>();
    const view = await mount({ diag: stubDiag({ diagStart: () => start.promise }) });
    await begin();
    expect(screen.getByText('Starting the placement…')).toBeTruthy();

    view.unmount();
    await act(async () => { start.release(START); });
    expect(document.querySelector('.problem-card')).toBeNull();
  });

  it('paints no verdict from an answer that lands after the view left', async () => {
    const reply = held<DiagAnswerResponse>();
    const view = await mount({ diag: stubDiag({ diagAnswer: () => reply.promise }) });
    await begin();
    await answer('5');

    view.unmount();
    await act(async () => { reply.release({ correct: true, next_probe: { done: true } }); });
    expect(document.querySelector('.feedback')).toBeNull();
  });
});

describe('the gate under two events in one tick', () => {
  it('F-37-1c: two Enters in one task post once, past the attribute', async () => {
    // `fireEvent` wraps every event in `act()` and commits the disabled attribute between
    // two of them, so the ATTRIBUTE stops the second. Two raw events in one task reach the
    // handler before the commit, and only the gate can refuse the second.
    allowConsoleError(/not wrapped in act/);
    const reply = held<DiagAnswerResponse>();
    const diagAnswer = vi.fn<DiagnosticApi['diagAnswer']>(() => reply.promise);
    await mount({ diag: stubDiag({ diagAnswer }) });
    await begin();

    fireEvent.change(answerInput(), { target: { value: '5' } });
    const enter = () => new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true });
    answerInput().dispatchEvent(enter());
    answerInput().dispatchEvent(enter());
    expect(diagAnswer).toHaveBeenCalledTimes(1);

    await act(async () => { reply.release({ correct: true, next_probe: { done: true } }); });
    expect(diagAnswer).toHaveBeenCalledTimes(1);
  });
});

describe('the Retry that proceeds', () => {
  it('re-posts the live probe when the learner presses Retry before answering again', async () => {
    const diagAnswer = vi.fn<DiagnosticApi['diagAnswer']>(
      flakyOnce(() => ({ correct: true, next_probe: { done: true } })),
    );
    await mount({ diag: stubDiag({ diagAnswer }) });
    await begin();
    await answer('5');
    expect(toasts()[0].label).toBe('Retry');

    // The probe on screen is the one the request named, and it is still unanswered: the
    // gate admits the Retry, and the retried reply paints the verdict.
    await act(async () => { fireToastAction(toasts()[0].id); });
    expect(diagAnswer).toHaveBeenCalledTimes(2);
    expect(screen.getByText('Correct')).toBeTruthy();
    expect(toasts()).toEqual([]);
  });
});

describe('the payload shapes', () => {
  it('reads a cap of zero as the default cap', async () => {
    await mount({ diag: stubDiag({ diagStart: async () => ({ ...START, cap: 0 }) }) });
    await begin();
    expect(progressCount()).toBe('Question 1 of up to 40');
  });

  it('names a topic record by its id when it carries no name, and no topic as Placement', async () => {
    await mount({
      diag: stubDiag({
        diagStart: async () => ({
          probe: probe({ topic: { id: 'integers', name: null, module: 'Arithmetic' } }),
        }),
      }),
    });
    await begin();
    expect(document.querySelector('.topic-name')!.textContent).toBe('integers');
    expect(probeText()).toContain('-7 + 12');
    expect(submitButton()).toBeTruthy();
  });

  it('names a probe with no topic at all as Placement', async () => {
    await mount({ diag: stubDiag({ diagStart: async () => ({ probe: probe({ topic: null }) }) }) });
    await begin();
    expect(document.querySelector('.topic-name')!.textContent).toBe('Placement');
  });

  it('renders no frontier block when the summary names no frontier', async () => {
    vi.useFakeTimers();
    await mount({
      diag: stubDiag({ diagFinish: async () => ({ placed: ['a'], conditional: [], frontier: [] }) }),
    });
    await begin();
    await answer('5');
    await act(async () => { await vi.advanceTimersByTimeAsync(750); });

    expect(screen.getByText('Placement complete')).toBeTruthy();
    expect(screen.queryByText('Start here')).toBeNull();
    expect(document.querySelector('.frontier-list')).toBeNull();
  });
});
