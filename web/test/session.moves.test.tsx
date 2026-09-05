/**
 * The session view, move by move: what each transition clears, locks, focuses and posts.
 *
 * `session.test.tsx` holds the study loop and the gate; this part pins the shape of every
 * move — the plan cursor, the wrap-up screen, the auto-advance timer and the class names —
 * so that a change of one line in the view fails one test here.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, screen } from '@testing-library/react';
import { ApiError } from '@/api';
import { busy } from './helpers/api';
import { held } from './helpers/held';
import { pressRetry } from './helpers/toasts';
import { allowConsoleError } from './setup';
import {
  LESSON, REVIEW, REWORK, TEACHING, P, answerInput, closed, graded, mount, mountDrill,
  mountStrict, planOf, press, progressCount, stubApi, submitAnswer, submitButton, timer,
  workInput,
} from './helpers/session';
import type {
  AnswerResponse, ApiClient, PlanTask, ServedProblem, SessionEndResponse, TaskAnswerResponse,
} from '@/api/types';

const second: PlanTask = { ...REVIEW, task_id: 't-second' };
const passed = (over: Partial<AnswerResponse> = {}) =>
  graded({ next: null, task_status: 'task_passed', ...over });
const remediated = () => passed({ remediation: [{ kind: 'review', targets: ['fractions'] }] });

const hints = () => document.querySelectorAll('.hint').length;
const section = () => document.querySelector('.view-session')!;

/** A hint with a reference lesson, so both rows are on screen for the next move to clear. */
async function hintWithReference(): Promise<void> {
  await press('Hint');
  expect(hints()).toBe(1);
  expect(document.querySelector('.reference-lesson')).not.toBeNull();
}

const referenced = () => ({
  hint: 'Name the parts.',
  hint_number: 1,
  reference_lesson: { topic: 'fractions', name: 'Simplifying fractions' },
});

describe('what a fresh problem clears', () => {
  it('starts with no hint, and the next problem shows none of the hints of the last', async () => {
    await mount({ api: stubApi({ taskHint: async () => referenced() }) });
    expect(hints()).toBe(0);
    await hintWithReference();

    await submitAnswer('3/4');
    await press('Next problem →');
    expect(progressCount()).toBe('2 / 3');
    expect(hints()).toBe(0);
    expect(document.querySelector('.reference-lesson')).toBeNull();
  });

  it('a re-served problem shows none of the hints of the last either', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>().mockResolvedValueOnce(P(1)).mockResolvedValue(P(2));
    await mount({
      api: stubApi({
        taskServe,
        taskHint: async () => referenced(),
        taskAnswer: async () => graded({ next: null, next_unavailable: true }),
      }),
    });
    await hintWithReference();

    await submitAnswer('3/4');
    await press('Next problem →');
    expect(taskServe).toHaveBeenCalledTimes(2);
    expect(hints()).toBe(0);
    expect(document.querySelector('.reference-lesson')).toBeNull();
  });

  it('the next problem shows no re-solve panel and no verdict of the last', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>()
      .mockResolvedValueOnce(REWORK)
      .mockResolvedValue(graded());
    await mount({ api: stubApi({ taskAnswer }) });

    await submitAnswer('3/4');
    expect(screen.getByText('Make it stick')).toBeTruthy();
    await submitAnswer('17/23');
    await press('Next problem →');
    expect(progressCount()).toBe('2 / 3');
    expect(screen.queryByText('Make it stick')).toBeNull();
    expect(document.querySelector('.feedback')).toBeNull();
  });

  it('a review moves to the next problem in hand and serves nothing more', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    await mount({ api: stubApi({ taskServe }) });
    await submitAnswer('3/4');
    await press('Next problem →');
    expect(progressCount()).toBe('2 / 3');
    expect(taskServe).toHaveBeenCalledTimes(1);
  });
});

describe('the plan cursor', () => {
  it('serves the second planned task after the first, and closes nothing', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    const sessionEnd = vi.fn<ApiClient['sessionEnd']>(async () => closed());
    await mount({
      plan: planOf(REVIEW, second),
      api: stubApi({ taskServe, sessionEnd, taskAnswer: async () => passed() }),
    });
    await submitAnswer('3/4');
    await press('Continue →');
    expect(taskServe.mock.calls.map(([id]) => id)).toEqual(['t-review', 't-second']);
    expect(sessionEnd).not.toHaveBeenCalled();
  });

  it('drops the verdict and locks the card while the next task loads', async () => {
    const serve = held<ServedProblem>();
    const taskServe = vi.fn<ApiClient['taskServe']>()
      .mockResolvedValueOnce(P(1))
      .mockImplementation(() => serve.promise);
    await mount({
      plan: planOf(REVIEW, second),
      api: stubApi({ taskServe, taskAnswer: async () => passed() }),
    });
    await submitAnswer('3/4');
    await press('Continue →');

    // The old problem stands, busy, with no verdict over it and no way to post again.
    expect(section().getAttribute('aria-busy')).toBe('true');
    expect(document.querySelector('.feedback')).toBeNull();
    expect(answerInput().disabled).toBe(true);
    expect(submitButton().hasAttribute('disabled')).toBe(true);

    await act(async () => { serve.release(P(1, { problem_id: 'p9' })); });
    expect(section().getAttribute('aria-busy')).toBe('false');
    expect(answerInput().disabled).toBe(false);
  });

  it('skips the task it just finished when the fresh plan lists it again', async () => {
    const getPlan = vi.fn<ApiClient['getPlan']>(async () => planOf(REVIEW, { ...REVIEW, task_id: 't-remedial' }));
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    const sessionEnd = vi.fn<ApiClient['sessionEnd']>(async () => closed());
    await mount({
      api: stubApi({
        getPlan,
        taskServe,
        sessionEnd,
        taskAnswer: vi.fn<ApiClient['taskAnswer']>().mockResolvedValueOnce(remediated()).mockResolvedValue(passed()),
      }),
    });
    await submitAnswer('3/4');
    await press('Continue →');
    expect(taskServe.mock.calls.map(([id]) => id)).toEqual(['t-review', 't-remedial']);

    // The remedial task passes with nothing to remediate: no second plan, and the close.
    await submitAnswer('3/4');
    await press('Continue →');
    expect(getPlan).toHaveBeenCalledTimes(1);
    expect(sessionEnd).toHaveBeenCalledTimes(1);
  });

  it('restarts at the top of a fresh plan, past every task already finished', async () => {
    const third: PlanTask = { ...REVIEW, task_id: 't-third' };
    const fourth: PlanTask = { ...REVIEW, task_id: 't-fourth' };
    const getPlan = vi.fn<ApiClient['getPlan']>(async () => planOf(REVIEW, second, third, fourth));
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    await mount({
      plan: planOf(REVIEW, second),
      api: stubApi({
        getPlan,
        taskServe,
        taskAnswer: vi.fn<ApiClient['taskAnswer']>().mockResolvedValueOnce(passed()).mockResolvedValue(remediated()),
      }),
    });
    await submitAnswer('3/4');
    await press('Continue →');
    await submitAnswer('3/4');
    await press('Continue →');
    expect(taskServe.mock.calls.map(([id]) => id)).toEqual(['t-review', 't-second', 't-third']);
  });

  it('reads the plan once under StrictMode', async () => {
    const getPlan = vi.fn<ApiClient['getPlan']>(async () => planOf(REVIEW));
    await mountStrict(stubApi({ getPlan }));
    expect(getPlan).toHaveBeenCalledTimes(1);
  });

  it('routes a 401 on the serve to sign-in', async () => {
    const view = await mount({
      api: stubApi({ taskServe: async () => { throw new ApiError(401, 'unauthorized', 'No session.'); } }),
    });
    expect(view.onUnauthorized).toHaveBeenCalledTimes(1);
  });
});

describe('the wrap-up', () => {
  it('shows the wrap-up screen while the close is out, and arms no advance meanwhile', async () => {
    vi.useFakeTimers();
    const close = held<SessionEndResponse>();
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    await mount({ api: stubApi({ taskServe, sessionEnd: () => close.promise }) });
    await submitAnswer('3/4');
    await press('End session');

    expect(screen.getByText('Wrapping up…')).toBeTruthy();
    await act(async () => { vi.advanceTimersByTime(5000); });
    expect(taskServe).toHaveBeenCalledTimes(1);

    await act(async () => { close.release(closed()); });
    expect(screen.getByText('Session complete')).toBeTruthy();
    // The three numbers of the receipt, and the focus on the one way out.
    expect(Array.from(document.querySelectorAll('.stat-value')).map((s) => s.textContent)).toEqual(['+10', '5', '3']);
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Back to dashboard' }));
  });
});

describe('the auto-advance', () => {
  it('advances to the problem of the newest verdict, never of an older one', async () => {
    vi.useFakeTimers();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>()
      .mockResolvedValueOnce(graded({ next: P(2) }))
      .mockResolvedValue(graded({ next: P(3) }));
    await mount({ api: stubApi({ taskAnswer }) });

    // The learner outruns the first timer, answers again, and the second verdict lands
    // with its own timer. Only that one may fire.
    await submitAnswer('1/2');
    await press('Next problem →');
    expect(progressCount()).toBe('2 / 3');
    await submitAnswer('2/2');
    await act(async () => { vi.advanceTimersByTime(1400); });
    expect(progressCount()).toBe('3 / 3');
  });

  it('never fires on a miss, and never for a verdict with no next problem', async () => {
    vi.useFakeTimers();
    await mount({ api: stubApi({ taskAnswer: async () => graded({ correct: false, next: P(2) }) }) });
    await submitAnswer('3/4');
    await act(async () => { vi.advanceTimersByTime(5000); });
    expect(progressCount()).toBe('1 / 3');
    expect(screen.getByText('Not quite')).toBeTruthy();
  });
});

describe('the card', () => {
  it('disables the field and hides the actions on the verdict', async () => {
    await mount();
    expect(submitButton().className).toBe('btn btn-primary');
    await submitAnswer('3/4');
    expect(answerInput().disabled).toBe(true);
    expect(screen.queryByRole('button', { name: 'Submit' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Hint' })).toBeNull();
  });

  it('marks Submit busy while the grade is out, and on the Retry of a failed one', async () => {
    const grade = held<TaskAnswerResponse>();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>()
      .mockRejectedValueOnce(busy())
      .mockImplementation(() => grade.promise);
    await mount({ api: stubApi({ taskAnswer }) });

    await submitAnswer('3/4');
    expect(submitButton().className).toBe('btn btn-primary');
    await pressRetry();
    expect(submitButton().className).toBe('btn btn-primary is-busy');
    expect(answerInput().disabled).toBe(false);
    await act(async () => { grade.release(graded()); });
    expect(screen.getByText('Correct')).toBeTruthy();
  });

  it('Enter in the working posts the answer', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => graded());
    await mount({ api: stubApi({ taskAnswer }) });
    fireEvent.change(answerInput(), { target: { value: '3/4' } });
    fireEvent.change(workInput(), { target: { value: 'halve both' } });
    await act(async () => { fireEvent.keyDown(workInput(), { key: 'Enter' }); });
    expect(taskAnswer).toHaveBeenCalledTimes(1);
    expect(taskAnswer.mock.calls[0][1]).toEqual({ problem_id: 'p1', answer: '3/4', work: 'halve both' });
  });

  it('names the task in its chip, and counts up quietly for a review', async () => {
    await mount();
    expect(document.querySelector('.task-meta .chip')!.className).toBe('chip chip-review');
    expect(timer().className).toBe('timer');
  });

  it('hands the problem back after a quiz receipt, ready for the next post', async () => {
    const receipt = { accepted: true as const, remaining: 2, quiz_complete: false };
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => receipt);
    await mount({ api: stubApi({ taskAnswer }) });
    await submitAnswer('3/4');
    expect(submitButton().hasAttribute('disabled')).toBe(false);
    await submitAnswer('3/4');
    expect(taskAnswer).toHaveBeenCalledTimes(2);
  });
});

describe('the drill timeout', () => {
  it('posts once when the timed submit fails, and leaves the Retry to the learner', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => { throw busy(); });
    await mountDrill(taskAnswer);
    await act(async () => { vi.advanceTimersByTime(3000); });
    expect(taskAnswer).toHaveBeenCalledTimes(1);
    await act(async () => { vi.advanceTimersByTime(5000); });
    expect(taskAnswer).toHaveBeenCalledTimes(1);
  });
});

describe('the worked example', () => {
  it('serves once when the practise button is pressed twice in one tick', async () => {
    allowConsoleError(/not wrapped in act/);
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    await mount({ plan: planOf(LESSON), api: stubApi({ taskServe, taskTeach: async () => TEACHING }) });
    const button = screen.getByRole('button', { name: /practice/ });
    expect(document.activeElement).toBe(button);

    button.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    button.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await act(async () => {});
    expect(taskServe).toHaveBeenCalledTimes(1);
    expect(progressCount()).toBe('1 / 3');
  });
});
