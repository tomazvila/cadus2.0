/**
 * The study loop (S8), part 2: the stale Retry, the drill countdown, and accessibility.
 *
 * `session.test.tsx` carries the module note and the fixtures live in
 * `test/helpers/session.tsx`.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, screen } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { isDrill } from '@/views/session/Session';
import { RETRY_STALE_MESSAGE } from '@/hooks/useCall';
import { fireToastAction } from '@/app/toast';
import { fmtClock } from '@/lib/format';
import { AXE_IN_JSDOM } from './axe';
import { busy, flakyOnce } from './helpers/api';
import {
  expectRefusalAmongPlainToasts, expectRetryArmed, pressRetryAfterUnmount,
} from './helpers/toasts';
import {
  DRILL, REVIEW, REWORK, P, answerInput, graded, mount, mountDrill, stubApi, submitAnswer,
  submitButton, submitThenWait, timer, toasts, typeAnswer,
} from './helpers/session';
import type { ApiClient, TaskAnswerResponse } from '@/api/types';

/** Mount over a grade that fails once, submit, and read the Retry the failure armed. */
async function submitOverFlakyGrade(reply: TaskAnswerResponse) {
  const taskAnswer = vi.fn<ApiClient['taskAnswer']>(flakyOnce(() => reply));
  await mount({ api: stubApi({ taskAnswer }) });

  // The first grade fails. The problem comes back to the learner with a Retry armed.
  await submitAnswer('3/4');
  expectRetryArmed(submitButton);
  return taskAnswer;
}

/** Press the stale Retry, and read the one refusal toast it leaves. */
async function pressStaleRetry(taskAnswer: ReturnType<typeof vi.fn>): Promise<void> {
  const stale = toasts()[0];
  await act(async () => { fireToastAction(stale.id); });

  expect(taskAnswer).toHaveBeenCalledTimes(2);
  // One toast, and it is the refusal. A refusal toast carries no action, so it expires.
  expect(toasts().length).toBe(1);
  expect(toasts()[0].message).toBe(RETRY_STALE_MESSAGE);
  expect(toasts()[0].onAction).toBeUndefined();
}

describe('the stale Retry', () => {
  it('F-37-1c: a Retry pressed after the same problem was graded posts nothing', async () => {
    // The defect this pins (F10): the Retry re-entered the request function direct, past the
    // phase gate, and posted a consumed `problem_id`. The service answers `404
    // unknown_problem`, and that failure armed another Retry that never expired.
    const taskAnswer = await submitOverFlakyGrade(graded({ next: null, task_status: 'task_passed' }));

    // The learner submits again instead, and THAT attempt is graded.
    await submitAnswer('3/4');
    expect(screen.getByText('Correct')).toBeTruthy();
    expect(taskAnswer).toHaveBeenCalledTimes(2);

    // The Retry now names a consumed problem, so the gate refuses it.
    await pressStaleRetry(taskAnswer);
    expect(toasts()[0].label).toBeUndefined();
    expect(toasts()[0].kind).toBe('info');
  });

  it('DD-3/P1: a Retry is refused once the service answered, even with the problem still live', async () => {
    // The re-solve keeps the SAME problem live, so problem identity alone does not say the
    // request is still good. The assisted attempt earned its reply and is spent: a Retry
    // would post that attempt's answer as the unaided re-solve.
    const taskAnswer = await submitOverFlakyGrade(REWORK);

    await submitAnswer('3/4');
    expect(screen.getByText('Make it stick')).toBeTruthy();

    await pressStaleRetry(taskAnswer);
    // The problem is still live, and the learner's own re-solve is still the way on.
    expect(screen.getByText('Make it stick')).toBeTruthy();
    expect(submitButton().hasAttribute('disabled')).toBe(false);
  });

  it('F-37-1b: a Retry pressed after the session view is gone posts nothing', async () => {
    // The residual of FIX2-M6-C. The quiz and the placement got the liveness term; the
    // session grade did not. The toast store is module-scope, so the Retry OUTLIVES the
    // view, and the refs of an unmounted view still name the last problem: every other term
    // of the gate holds, and the press posts a grade for a screen the learner already left.
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => { throw busy(); });
    const { unmount } = await mount({ api: stubApi({ taskAnswer }) });

    await submitAnswer('3/4');
    expect(taskAnswer).toHaveBeenCalledTimes(1);

    await pressRetryAfterUnmount(unmount);

    // ONE post, the one the learner made while the screen was up.
    expect(taskAnswer).toHaveBeenCalledTimes(1);
    expectRefusalAmongPlainToasts();
  });
});

describe('the drill countdown', () => {
  it('counts down only for a drill with a budget', () => {
    expect(isDrill(DRILL, P(1, { countdown: true, time_budget_secs: 30 }))).toBe(true);
    expect(isDrill(DRILL, P(1, { countdown: false, time_budget_secs: 30 }))).toBe(false);
    expect(isDrill(DRILL, P(1, { countdown: true, time_budget_secs: 0 }))).toBe(false);
    expect(isDrill(REVIEW, P(1, { countdown: true, time_budget_secs: 30 }))).toBe(false);
  });

  it('auto-submits once at zero, through the same gate', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => graded({ next: null, correct: false }));
    await mountDrill(taskAnswer);

    expect(timer().textContent).toBe(fmtClock(3));
    // Three seconds, one tick each. The clock turns urgent at three and posts at zero.
    expect(timer().className).toContain('urgent');
    await act(async () => { vi.advanceTimersByTime(3000); });

    expect(timer().textContent).toBe('0:00');
    expect(taskAnswer).toHaveBeenCalledTimes(1);
    // A blank answer is an honest miss, not a skipped problem.
    expect(taskAnswer.mock.calls[0][1]).toEqual({ problem_id: 'p1', answer: '' });

    await act(async () => { vi.advanceTimersByTime(5000); });
    expect(taskAnswer).toHaveBeenCalledTimes(1);
  });

  it('DD-3/P1: a drill re-solve stops the countdown, so no blank second post lands', async () => {
    // The defect this pins (F7): the rework branch returned the view to `ready` and left the
    // leftover seconds to run out. The auto-submit then posted a BLANK answer for the same
    // `problem_id`, and the service rewrote the stashed assisted pass into a permanent miss
    // in an append-only log.
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => REWORK);
    await mountDrill(taskAnswer);

    expect(timer().textContent).toBe('0:03');
    await submitAnswer('3/4');

    // The assisted pass is stashed, and the problem waits for the unaided re-solve.
    expect(screen.getByText('Make it stick')).toBeTruthy();

    // Five seconds — more than the three the countdown had left.
    await act(async () => { vi.advanceTimersByTime(5000); });

    expect(taskAnswer).toHaveBeenCalledTimes(1);
    expect(taskAnswer.mock.calls[0][1]).toEqual({ problem_id: 'p1', answer: '3/4' });
    expect(screen.getByText('Make it stick')).toBeTruthy();
    // The re-solve is untimed: the countdown was stopped and cleared, so the clock counts
    // the re-solve up from zero and never turns urgent again.
    expect(timer().textContent).toBe('0:05');
    expect(timer().className).not.toContain('urgent');
  });

  it('ends the countdown on ANY reply that hands the problem back', async () => {
    // The rule is not "the rework branch". The service holds an attempt for this
    // `problem_id` whatever the reply says, so every branch that returns the view to `ready`
    // latches the countdown out. The receipt branch drives it here because it is the one
    // such branch that leaves `rework` null.
    const receipt = { accepted: true as const, remaining: 2, quiz_complete: false };
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => receipt);
    await mountDrill(taskAnswer);

    await submitThenWait('3/4', 5000);

    expect(taskAnswer).toHaveBeenCalledTimes(1);
    expect(taskAnswer.mock.calls[0][1]).toEqual({ problem_id: 'p1', answer: '3/4' });
  });
});

describe('accessibility', () => {
  it('the problem card reports no axe violation', async () => {
    const { container } = await mount();
    expect(await axe(container, AXE_IN_JSDOM)).toHaveNoViolations();
  });

  it('the answer field takes the focus on a fresh problem', async () => {
    await mount();
    expect(document.activeElement).toBe(answerInput());
  });

  it('the Continue button takes the focus on the verdict', async () => {
    await mount({ api: stubApi({ taskAnswer: async () => graded({ next: null }) }) });
    typeAnswer('3/4');
    await act(async () => { fireEvent.click(submitButton()); });
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Continue →' }));
  });
});
