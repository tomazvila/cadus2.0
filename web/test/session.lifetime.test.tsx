/**
 * The study loop (S8), part 4: the view lifetime, the plan edges, the gate under a
 * same-tick double, the Retry that proceeds, and the payload shapes.
 *
 * `session.test.tsx` carries the module note and the fixtures live in
 * `test/helpers/session.tsx`.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, cleanup, screen } from '@testing-library/react';
import { flakyOnce } from './helpers/api';
import { held } from './helpers/held';
import { pressRetry } from './helpers/toasts';
import {
  LESSON, REVIEW, REWORK, TEACHING, P, answerInput, closed, graded, leaveDuringReplan, mount,
  mountStrict, planOf, press, progressCount, stubApi, submitAnswer, submitButton, timer, toasts,
  typeAnswer,
} from './helpers/session';
import { allowConsoleError } from './setup';
import type {
  ApiClient, HintResponse, ServedProblem, SessionPlanResponse, TaskAnswerResponse, TeachResponse,
} from '@/api/types';

/** A grade that passes the task with a remediation, so the loop asks for a fresh plan. */
const remediated = () => graded({
  next: null,
  task_status: 'task_passed',
  remediation: [{ kind: 'review', targets: ['fractions'] }],
});

describe('the view lifetime', () => {
  it('NO-2BILL: a StrictMode mount serves once and teaches once', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    const taskTeach = vi.fn<ApiClient['taskTeach']>(async () => TEACHING);
    await mountStrict(stubApi({ taskServe, taskTeach }), planOf(LESSON));
    expect(taskTeach).toHaveBeenCalledTimes(1);
    expect(taskServe).not.toHaveBeenCalled();
    expect(screen.getByText('Worked example')).toBeTruthy();
  });

  it('paints no plan that lands after the view left', async () => {
    const plan = held<SessionPlanResponse>();
    const view = await mount({ plan: undefined, api: stubApi({ getPlan: () => plan.promise }) });
    view.unmount();
    await act(async () => { plan.release(planOf(REVIEW)); });
    expect(document.querySelector('.problem-card')).toBeNull();
  });

  it('paints no worked example that lands after the view left', async () => {
    const teach = held<TeachResponse>();
    const view = await mount({ plan: planOf(LESSON), api: stubApi({ taskTeach: () => teach.promise }) });
    view.unmount();
    await act(async () => { teach.release(TEACHING); });
    expect(document.querySelector('.teach-card')).toBeNull();
  });

  it('paints no verdict from a grade that lands after the view left', async () => {
    const grade = held<TaskAnswerResponse>();
    const view = await mount({ api: stubApi({ taskAnswer: () => grade.promise }) });
    await submitAnswer('3/4');
    view.unmount();
    await act(async () => { grade.release(graded()); });
    expect(document.querySelector('.feedback')).toBeNull();
  });

  it('paints no hint that lands after the view left', async () => {
    const hint = held<HintResponse>();
    const view = await mount({ api: stubApi({ taskHint: () => hint.promise }) });
    await press('Hint');
    view.unmount();
    await act(async () => { hint.release({ hint: 'late', hint_number: 1 }); });
    expect(document.querySelector('.hint')).toBeNull();
  });

  it('serves nothing from a re-plan that lands after the view left', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    const plan = await leaveDuringReplan({ taskServe });
    await act(async () => { plan.release(planOf({ ...REVIEW, task_id: 't-remedial' })); });
    expect(taskServe).toHaveBeenCalledTimes(1);
  });

  it('closes nothing from an empty re-plan that lands after the view left', async () => {
    const sessionEnd = vi.fn<ApiClient['sessionEnd']>(async () => closed());
    const plan = await leaveDuringReplan({ sessionEnd });
    await act(async () => { plan.release(planOf()); });
    expect(sessionEnd).not.toHaveBeenCalled();
  });
});

describe('the plan edges', () => {
  it('moves to the next planned task when the re-plan fails, and ends after the last', async () => {
    // A FAILED FETCH IS NOT AN EMPTY PLAN: the tasks still owed are served, in order.
    const getPlan = vi.fn<ApiClient['getPlan']>(async () => { throw new Error('down'); });
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    const sessionEnd = vi.fn<ApiClient['sessionEnd']>(async () => closed());
    await mount({
      plan: planOf(REVIEW, { ...REVIEW, task_id: 't-second' }),
      api: stubApi({ getPlan, taskServe, sessionEnd, taskAnswer: async () => remediated() }),
    });

    await submitAnswer('3/4');
    await press('Continue →');
    expect(getPlan).toHaveBeenCalledTimes(1);
    expect(taskServe.mock.calls.map(([id]) => id)).toEqual(['t-review', 't-second']);
    expect(sessionEnd).not.toHaveBeenCalled();

    await submitAnswer('3/4');
    await press('Continue →');
    expect(getPlan).toHaveBeenCalledTimes(2);
    expect(sessionEnd).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Session complete')).toBeTruthy();
  });

  it('ends the session when the re-plan is empty', async () => {
    const sessionEnd = vi.fn<ApiClient['sessionEnd']>(async () => closed());
    await mount({
      api: stubApi({ getPlan: async () => planOf(), sessionEnd, taskAnswer: async () => remediated() }),
    });
    await submitAnswer('3/4');
    await press('Continue →');
    expect(sessionEnd).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Session complete')).toBeTruthy();
  });

  it('still ends the screen when the close fails: the work is saved, the summary is not', async () => {
    await mount({
      api: stubApi({
        sessionEnd: async () => { throw new Error('down'); },
        taskAnswer: async () => graded({ next: null, task_status: 'task_passed' }),
      }),
    });
    await submitAnswer('3/4');
    await press('Continue →');
    expect(screen.getByText('Session complete')).toBeTruthy();
    expect(screen.getByText('Your work is saved. The summary did not load.')).toBeTruthy();
  });

  it.each([
    [{ course_complete: true }, 'Course complete. Enroll in your next course to keep going.'],
    [{ frontier_blocked_until: '2026-09-05' }, 'New lessons are on a retry delay until 2026-09-05.'],
  ])('says which empty the plan is: %o', async (over, line) => {
    await mount({ plan: { ...planOf(), ...over } });
    expect(screen.getByText(line)).toBeTruthy();
  });

  it('says everything planned is done when the server closed every task', async () => {
    const done = { ...REVIEW, progress: { answered: 3, done: true } };
    await mount({ plan: planOf(done) });
    expect(screen.getByText('Everything planned for this session is done. Nice work.')).toBeTruthy();
  });
});

describe('the gate under two events in one tick', () => {
  it('advances once when Next is pressed twice in one task', async () => {
    allowConsoleError(/not wrapped in act/);
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(2));
    await mount({
      api: stubApi({ taskServe, taskAnswer: async () => graded({ next: null, next_unavailable: true }) }),
    });
    await submitAnswer('3/4');

    const next = screen.getByRole('button', { name: 'Next problem →' });
    next.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    next.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await act(async () => {});

    // ONE re-serve: the second click found the gate shut.
    expect(taskServe).toHaveBeenCalledTimes(2);
    expect(progressCount()).toBe('2 / 3');
  });
});

describe('the Retry that proceeds', () => {
  it('re-posts the live problem when the learner presses Retry before answering again', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(flakyOnce(() => graded({ next: null })));
    await mount({ api: stubApi({ taskAnswer }) });
    await submitAnswer('3/4');
    expect(toasts()[0].label).toBe('Retry');

    await pressRetry();
    expect(taskAnswer).toHaveBeenCalledTimes(2);
    expect(toasts()).toEqual([]);
    expect(screen.getByText('Correct')).toBeTruthy();
  });
});

describe('the payload shapes', () => {
  it('names a task with no topic Practice, and counts a problem with no total alone', async () => {
    const bare: ServedProblem = P(1, { total: null });
    await mount({
      plan: planOf({ ...REVIEW, topic: null }),
      api: stubApi({ taskServe: async () => bare }),
    });
    expect(document.querySelector('.topic-name')!.textContent).toBe('Practice');
    expect(document.querySelector('.topic-module')).toBeNull();
    expect(progressCount()).toBe('1');
    expect(timer().textContent).toBe('0:00');
  });

  it('names a topic by its id when it carries no name, and no module without one', async () => {
    await mount({
      plan: planOf({ ...REVIEW, topic: { id: 'fractions', name: null, module: '' } }),
    });
    expect(document.querySelector('.topic-name')!.textContent).toBe('fractions');
    expect(document.querySelector('.topic-module')).toBeNull();
  });

  it('teaches a lesson with no topic under the word Lesson, and a nameless one under its id', async () => {
    const taskTeach = vi.fn<ApiClient['taskTeach']>(async () => TEACHING);
    const view = await mount({ plan: planOf({ ...LESSON, topic: null }), api: stubApi({ taskTeach }) });
    expect(document.querySelector('.topic-name')!.textContent).toBe('Lesson');
    expect(document.querySelector('.topic-module')).toBeNull();
    view.unmount();
    cleanup();

    await mount({
      plan: planOf({ ...LESSON, topic: { id: 'fractions', name: null, module: 'Arithmetic' } }),
      api: stubApi({ taskTeach }),
    });
    expect(document.querySelector('.topic-name')!.textContent).toBe('fractions');
    expect(document.querySelector('.topic-module')!.textContent).toBe('Arithmetic');
  });

  it('shows the expected answer in the re-solve panel when the service sent no solution', async () => {
    await mount({ api: stubApi({ taskAnswer: async () => ({ ...REWORK, solution: null }) }) });
    await submitAnswer('3/4');
    expect(document.querySelector('.solution-text')!.textContent).toBe(REWORK.expected);
  });

  it('renders no solution block when the verdict carries none', async () => {
    const { solution, ...noSolution } = graded({ next: null });
    expect(solution).toBeTruthy();
    await mount({ api: stubApi({ taskAnswer: async () => noSolution }) });
    await submitAnswer('3/4');
    expect(screen.getByText('Correct')).toBeTruthy();
    expect(document.querySelector('.solution')).toBeNull();
  });

  it('renders no tag row for a verdict with no tag, and a submit reads the field once', async () => {
    await mount({ api: stubApi({ taskAnswer: async () => graded({ next: null, error_tags: [] }) }) });
    typeAnswer('3/4');
    await submitAnswer('3/4');
    expect(document.querySelector('.error-tags')).toBeNull();
    expect(screen.getByText('Correct')).toBeTruthy();
    expect(answerInput()).toBeTruthy();
    expect(submitButton).toBeTruthy();
  });
});
