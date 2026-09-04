/**
 * The timed quiz (S10), part 3: the view lifetime, the fill bound, the Retry that proceeds,
 * and the payload shapes.
 *
 * `quiz.test.tsx` carries the module note and the fixtures live in `test/helpers/quiz.tsx`.
 */
import { StrictMode } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, screen } from '@testing-library/react';
import { Quiz } from '@/views/Quiz';
import { RETRY_STALE_MESSAGE } from '@/hooks/useCall';
import { fireToastAction } from '@/app/toast';
import { busy, flakyOnce } from './helpers/api';
import { held } from './helpers/held';
import { renderInView } from './helpers/render';
import { tick } from './helpers/timers';
import { expectRefusalOnly } from './helpers/toasts';
import { allowConsoleError } from './setup';
import {
  QUIZ, Q, answerInput, completed, mount, posted, receipt, stubApi, submitAnswer, submitButton,
  threeQuestions, toasts, typeAnswer,
} from './helpers/quiz';
import type { ApiClient, ServedProblem, TaskAnswerResponse } from '@/api/types';

describe('the view lifetime', () => {
  it('NO-2BILL: a StrictMode mount serves exactly one question', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => Q(1));
    await renderInView(
      <StrictMode>
        <Quiz api={stubApi({ taskServe })} task={QUIZ} onUnauthorized={vi.fn()} onDone={vi.fn()} />
      </StrictMode>,
    );
    expect(taskServe).toHaveBeenCalledTimes(1);
    expect(answerInput()).toBeTruthy();
  });

  it('paints no question from a serve that lands after the view left', async () => {
    const serve = held<ServedProblem>();
    const view = await mount({ api: stubApi({ taskServe: () => serve.promise }) });
    view.unmount();
    await act(async () => { serve.release(Q(1)); });
    expect(document.querySelector('.problem-card')).toBeNull();
  });

  it('paints nothing from a grade, or the serve behind it, that lands after the view left', async () => {
    const grade = held<TaskAnswerResponse>();
    const serve = held<ServedProblem>();
    const taskServe = vi.fn<ApiClient['taskServe']>()
      .mockResolvedValueOnce(Q(1))
      .mockReturnValue(serve.promise);
    const view = await mount({ api: stubApi({ taskServe, taskAnswer: () => grade.promise }) });
    typeAnswer('7');
    fireEvent.click(submitButton());

    view.unmount();
    await act(async () => { grade.release(receipt({ remaining: 2 })); });
    // The grade landed on a dead view: no serve is asked for, and nothing is painted.
    expect(taskServe).toHaveBeenCalledTimes(1);
    expect(document.querySelector('.problem-card')).toBeNull();
  });

  it('paints nothing from a serve that lands after the view left mid-answer', async () => {
    const serve = held<ServedProblem>();
    const taskServe = vi.fn<ApiClient['taskServe']>()
      .mockResolvedValueOnce(Q(1))
      .mockReturnValue(serve.promise);
    const view = await mount({ api: stubApi({ taskServe }) });
    await submitAnswer('7');
    expect(taskServe).toHaveBeenCalledTimes(2);

    view.unmount();
    await act(async () => { serve.release(Q(2)); });
    expect(document.querySelector('.problem-card')).toBeNull();
  });

  it('QUIZ-timeout: a serve behind a blank fill that lands after the view left fills nothing more', async () => {
    vi.useFakeTimers();
    const serve = held<ServedProblem>();
    const taskServe = vi.fn<ApiClient['taskServe']>()
      .mockResolvedValueOnce(Q(1))
      .mockReturnValue(serve.promise);
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => receipt({ remaining: 2 }));
    const view = await mount({ task: { ...QUIZ, time_budget_secs: 2 }, api: stubApi({ taskServe, taskAnswer }) });
    await tick(2000);
    expect(posted(taskAnswer)).toEqual([['q1', '']]);
    expect(taskServe).toHaveBeenCalledTimes(2);

    view.unmount();
    await act(async () => { serve.release(Q(2)); });
    expect(posted(taskAnswer)).toEqual([['q1', '']]);
  });

  it('QUIZ-timeout: a blank fill that lands after the view left fills nothing more', async () => {
    vi.useFakeTimers();
    const grade = held<TaskAnswerResponse>();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(() => grade.promise);
    const view = await mount({ task: { ...QUIZ, time_budget_secs: 2 }, api: stubApi({ taskAnswer }) });
    await tick(2000);
    expect(posted(taskAnswer)).toEqual([['q1', '']]);

    view.unmount();
    await act(async () => { grade.release(receipt({ remaining: 2 })); });
    expect(posted(taskAnswer)).toEqual([['q1', '']]);
  });
});

describe('the gate under two events in one tick', () => {
  it('F-37-1c: two Enters in one task post once, past the attribute', async () => {
    // `fireEvent` commits the disabled attribute between two events, so the ATTRIBUTE stops
    // the second. Two raw events in one task reach the handler before the commit, and only
    // the gate can refuse the second.
    allowConsoleError(/not wrapped in act/);
    const grade = held<TaskAnswerResponse>();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(() => grade.promise);
    await mount({ api: stubApi({ taskAnswer }) });

    fireEvent.change(answerInput(), { target: { value: '7' } });
    const enter = () => new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true });
    answerInput().dispatchEvent(enter());
    answerInput().dispatchEvent(enter());
    expect(taskAnswer).toHaveBeenCalledTimes(1);

    await act(async () => { grade.release(completed()); });
    expect(taskAnswer).toHaveBeenCalledTimes(1);
  });
});

describe('the fill bound', () => {
  it('QUIZ-timeout: stops the blank fill after total + 2 posts when the service never closes', async () => {
    // A service that keeps serving past its own count is a defect. The fill is bounded, so
    // the defect costs a handful of posts and not a browser tab that never answers.
    vi.useFakeTimers();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => receipt({ remaining: 2 }));
    await mount({ task: { ...QUIZ, time_budget_secs: 1 }, api: stubApi({ taskAnswer }) });
    await tick(1000);
    await tick(60_000);
    expect(taskAnswer).toHaveBeenCalledTimes(5);
    expect(screen.queryByText('Quiz complete')).toBeNull();
  });
});

describe('the Retry that proceeds, and the one the timeout refuses', () => {
  it('re-posts the live question when the learner presses Retry before answering again', async () => {
    const taskServe = threeQuestions();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(flakyOnce(() => receipt({ remaining: 2 })));
    await mount({ api: stubApi({ taskServe, taskAnswer }) });
    await submitAnswer('7/12');
    expect(toasts()[0].label).toBe('Retry');

    await act(async () => { fireToastAction(toasts()[0].id); });
    expect(posted(taskAnswer)).toEqual([['q1', '7/12'], ['q1', '7/12']]);
    expect(document.querySelector('.problem-text')!.textContent).toBe('Question 2.');
    expect(toasts()).toEqual([]);
  });

  it('QUIZ-timeout: refuses the Retry of an answer once the clock ran out on that question', async () => {
    vi.useFakeTimers();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => { throw busy(); });
    await mount({ task: { ...QUIZ, time_budget_secs: 3 }, api: stubApi({ taskAnswer }) });

    // The answer fails, and its Retry is armed. Then the clock runs out on the SAME question.
    await submitAnswer('7');
    const answerRetry = toasts()[0];
    expect(answerRetry.label).toBe('Retry');
    await tick(3000);
    expect(posted(taskAnswer)).toEqual([['q1', '7'], ['q1', '']]);

    // Past the deadline the blank fill owns every post: the answer's Retry is refused. The
    // time-up line, the fill's own Retry and the refusal are what is left on screen.
    await act(async () => { fireToastAction(answerRetry.id); });
    expect(taskAnswer).toHaveBeenCalledTimes(2);
    expect(toasts().map((t) => [t.label, t.kind])).toEqual([
      [undefined, 'info'], ['Retry', 'error'], [undefined, 'info'],
    ]);
    expect(toasts()[2].message).toBe(RETRY_STALE_MESSAGE);
  });

  it('F-37-1c: refuses the Retry of a fill only once the view is gone', async () => {
    vi.useFakeTimers();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(flakyOnce(() => completed()));
    await mount({ task: { ...QUIZ, time_budget_secs: 1 }, api: stubApi({ taskAnswer }) });
    await tick(1000);
    expect(posted(taskAnswer)).toEqual([['q1', '']]);

    // The card is locked, so the Retry is the one way on, and it resumes the fill.
    await act(async () => { fireToastAction(toasts().find((t) => t.label === 'Retry')!.id); });
    await tick(0);
    expect(posted(taskAnswer)).toEqual([['q1', ''], ['q1', '']]);
    expect(screen.getByText('Quiz complete')).toBeTruthy();
  });

  it('F-36-1b: a fill Retry refused after unmount leaves one plain toast', async () => {
    vi.useFakeTimers();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => { throw busy(); });
    const { unmount } = await mount({ task: { ...QUIZ, time_budget_secs: 1 }, api: stubApi({ taskAnswer }) });
    await tick(1000);
    expect(taskAnswer).toHaveBeenCalledTimes(1);
    const stale = toasts().find((t) => t.label === 'Retry')!;
    unmount();
    // The time-up line expired with the fake clock; the refusal is the one toast left.
    await tick(6000);
    await act(async () => { fireToastAction(stale.id); });
    expectRefusalOnly();
    expect(taskAnswer).toHaveBeenCalledTimes(1);
  });
});

describe('the payload shapes', () => {
  it('counts against zero when the serve names no total, and says so at the end', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => Q(1, { total: null }));
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => completed());
    await mount({ api: stubApi({ taskServe, taskAnswer }) });
    expect(document.querySelector('.progress-count')!.textContent).toBe('1 / 0');
    expect(document.querySelector('.remaining')!.textContent).toBe('0 remaining');

    await submitAnswer('7');
    expect(screen.getByText('Your answers are recorded.')).toBeTruthy();
  });

  it('closes a quiz that never had a clock', async () => {
    await mount({
      task: { ...QUIZ, time_budget_secs: null },
      api: stubApi({
        taskServe: async () => Q(1, { time_budget_secs: null }),
        taskAnswer: async () => completed(),
      }),
    });
    await submitAnswer('7');
    expect(screen.getByText('Quiz complete')).toBeTruthy();
  });
});
