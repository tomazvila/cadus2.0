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
import { act, screen } from '@testing-library/react';
import { DIAG_BEAT_MS } from '@/views/Diagnostic';
import { fireToastAction, TOAST_TIMEOUT_MS } from '@/app/toast';
import { appendOnlyGrade, busy } from './helpers/api';
import { tick } from './helpers/timers';
import { expectRefusalOnly, expectRetryArmed } from './helpers/toasts';
import {
  answer, begin, mount, probe, probeText, progressCount, stubDiag, submitButton, toasts,
} from './helpers/placement';
import type { DiagnosticApi } from '@/api/diag';

// ---------------------------------------------------------------------------
// Fixtures — the second and third probes of the three `/api/diag/*` rows.
// ---------------------------------------------------------------------------

const D2 = probe({ problem_id: 'd2', topic: 'Fractions', text: 'Probe two.' });
const D3 = probe({ problem_id: 'd3', topic: 'Ratios', text: 'Probe three.' });

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
      if (attempts === 1) throw busy();
      if (attempts === 2) return { correct: true, next_probe: D2 };
      return { correct: false, next_probe: D3 };
    });
    await mount({ diag: stubDiag({ diagAnswer }) });
    await begin();
    expect(probeText()).toBe('Work out $-7 + 12$.');

    // The answer fails. The probe comes back to the learner with a Retry armed.
    await answer('12');
    expectRetryArmed(submitButton);

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
    const grade = appendOnlyGrade(() => ({ correct: true, next_probe: D2 }));
    const diagAnswer = vi.fn<DiagnosticApi['diagAnswer']>(async ({ problem_id }) => grade(problem_id));
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
    expectRefusalOnly();

    // A plain toast expires, so no Retry survives on the screen.
    await tick(TOAST_TIMEOUT_MS + 1000);
    expect([toasts(), diagAnswer.mock.calls.length]).toEqual([[], 2]);
  });
});
