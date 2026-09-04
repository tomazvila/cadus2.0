/**
 * The stale Retry on the placement screen (finding V5 of docs/reviews/M6-review-2.md).
 *
 * Placement writes to an append-only log, so a second post of one `problem_id` is
 * permanent. `useCall` arms a Retry on every failure, and that toast carries an action, so
 * it never expires (F-36-1b). The Retry re-enters the request with the closure of the
 * render that failed, and that closure holds the probe the learner has since answered.
 *
 * The defect: `Diagnostic.send` handed `call` no `retryGate`, so a Retry pressed after the
 * probe was graded re-posted the spent `problem_id`, and its continuation ran the
 * unconditional `gate.enter('feedback')` on top of the LIVE probe. The placement then
 * advanced off the old reply's `next_probe` and skipped the probe on screen unanswered.
 *
 * The tests use fake timers, because the 750 ms beat (DIAG-750) is what carries the stale
 * continuation onto the next probe.
 */
import { describe, expect, it, vi, type Mock } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { ApiError } from '@/api';
import { Diagnostic, DIAG_BEAT_MS, type DiagnosticProps } from '@/views/Diagnostic';
import { RETRY_STALE_MESSAGE } from '@/hooks/useCall';
import { fireToastAction, resetToasts, toastStore, TOAST_TIMEOUT_MS } from '@/app/toast';
import type {
  DiagFinishResponse,
  DiagProbe,
  DiagStartResponse,
  DiagnosticApi,
} from '@/api/diag';

// ---------------------------------------------------------------------------
// Fixtures — the frozen payloads of the three `/api/diag/*` rows.
// ---------------------------------------------------------------------------

const D1: DiagProbe = { problem_id: 'd1', topic: 'Adding integers', text: 'Probe one.' };
const D2: DiagProbe = { problem_id: 'd2', topic: 'Fractions', text: 'Probe two.' };
const D3: DiagProbe = { problem_id: 'd3', topic: 'Ratios', text: 'Probe three.' };

const START: DiagStartResponse = { probe: D1, asked: 0, cap: 40 };
const SUMMARY: DiagFinishResponse = { placed: ['a'], conditional: [], frontier: ['c'] };

function stubDiag(over: Partial<DiagnosticApi> = {}): DiagnosticApi {
  return {
    diagStart: async () => START,
    diagAnswer: async () => ({ correct: true, next_probe: { done: true } }),
    diagFinish: async () => SUMMARY,
    ...over,
  };
}

async function mount(over: Partial<DiagnosticProps> = {}) {
  resetToasts();
  const handlers = { onUnauthorized: vi.fn(), onExit: vi.fn() };
  const props: DiagnosticProps = { diag: stubDiag(), ...handlers, ...over };
  let view!: ReturnType<typeof render>;
  await act(async () => {
    view = render(<Diagnostic {...props} />, { container: document.getElementById('view')! });
  });
  return { ...view, ...handlers };
}

const answerInput = () => screen.getByLabelText('Answer') as HTMLInputElement;
const submitButton = () => screen.getByRole('button', { name: 'Submit' });
const probeText = () => document.querySelector('.problem-text')!.textContent;
const progressCount = () => document.querySelector('.progress-count')!.textContent;
const toasts = () => toastStore.getSnapshot();

/** Move the clock and let every continuation the move released settle. */
async function tick(ms: number): Promise<void> {
  await act(async () => { await vi.advanceTimersByTimeAsync(ms); });
}

/** Press Begin and let the start settle. */
async function begin(): Promise<void> {
  await act(async () => {
    fireEvent.click(screen.getByRole('button', { name: 'Begin placement' }));
  });
}

/** Answer the probe on screen. */
async function answer(text: string): Promise<void> {
  fireEvent.change(answerInput(), { target: { value: text } });
  await act(async () => { fireEvent.click(submitButton()); });
}

/** The `problem_id` values the placement posted, in order. */
const postedIds = (fn: Mock<DiagnosticApi['diagAnswer']>): string[] =>
  fn.mock.calls.map(([body]) => body.problem_id);

// ---------------------------------------------------------------------------

describe('the stale Retry on placement', () => {
  it('F-37-1c: a stale placement Retry posts nothing and keeps the live probe', async () => {
    vi.useFakeTimers();
    let attempts = 0;
    // The third post is ACCEPTED by this stub, deliberately. A service that answers a spent
    // probe is the worse half of the defect: the stale continuation then advanced the
    // placement onto its own `next_probe` and skipped the probe on screen.
    const diagAnswer = vi.fn<DiagnosticApi['diagAnswer']>(async () => {
      attempts += 1;
      if (attempts === 1) throw new ApiError(503, 'unavailable', 'The service is busy.');
      if (attempts === 2) return { correct: true, next_probe: D2 };
      return { correct: false, next_probe: D3 };
    });
    await mount({ diag: stubDiag({ diagAnswer }) });
    await begin();
    expect(probeText()).toBe('Probe one.');

    // The answer fails. The probe comes back to the learner with a Retry armed.
    await answer('12');
    expect(toasts().length).toBe(1);
    expect(toasts()[0].label).toBe('Retry');
    expect(submitButton().hasAttribute('disabled')).toBe(false);

    // The learner answers again instead, and THAT attempt is graded. Probe 2 arrives after
    // the 750 ms beat.
    await answer('12');
    expect(screen.getByText('Correct')).toBeTruthy();
    await tick(DIAG_BEAT_MS);
    expect(probeText()).toBe('Probe two.');
    expect(progressCount()).toBe('Question 2 of up to 40');

    // The Retry now names a graded probe, so the gate refuses it.
    const stale = toasts()[0];
    await act(async () => { fireToastAction(stale.id); });
    await tick(DIAG_BEAT_MS + 1000);

    // The view stays on the LIVE probe, unanswered and answerable.
    expect(probeText()).toBe('Probe two.');
    expect(diagAnswer).toHaveBeenCalledTimes(2);
    expect(postedIds(diagAnswer)).toEqual(['d1', 'd1']);
    expect(progressCount()).toBe('Question 2 of up to 40');
    expect(document.querySelector('.feedback')).toBeNull();
    expect(submitButton().hasAttribute('disabled')).toBe(false);
  });

  it('F-36-1b: the refusal of a stale placement Retry expires and arms no second Retry', async () => {
    vi.useFakeTimers();
    // The append-only log of the service: a post of a spent probe is `404 unknown_problem`,
    // and that failure armed another actionable Retry, which never expires either.
    const spent = new Set<string>();
    let attempts = 0;
    const diagAnswer = vi.fn<DiagnosticApi['diagAnswer']>(async ({ problem_id }) => {
      attempts += 1;
      if (attempts === 1) throw new ApiError(503, 'unavailable', 'The service is busy.');
      if (spent.has(problem_id)) {
        throw new ApiError(404, 'unknown_problem', 'That problem is no longer open.');
      }
      spent.add(problem_id);
      return { correct: true, next_probe: D2 };
    });
    await mount({ diag: stubDiag({ diagAnswer }) });
    await begin();

    await answer('12');
    const stale = toasts()[0];
    expect(stale.label).toBe('Retry');

    // The Retry of a real failure stays on screen for the life of the placement.
    await tick(TOAST_TIMEOUT_MS + 1000);
    expect(toasts().map((t) => t.id)).toEqual([stale.id]);

    await answer('12');
    await tick(DIAG_BEAT_MS);
    expect(probeText()).toBe('Probe two.');

    await act(async () => { fireToastAction(stale.id); });

    expect(diagAnswer).toHaveBeenCalledTimes(2);
    expect(toasts().length).toBe(1);
    expect(toasts()[0].message).toBe(RETRY_STALE_MESSAGE);
    expect(toasts()[0].kind).toBe('info');
    expect(toasts()[0].label).toBeUndefined();
    expect(toasts()[0].onAction).toBeUndefined();

    // A plain toast expires, so no Retry survives on the screen.
    await tick(TOAST_TIMEOUT_MS + 1000);
    expect(toasts()).toEqual([]);
    expect(diagAnswer).toHaveBeenCalledTimes(2);
  });
});
