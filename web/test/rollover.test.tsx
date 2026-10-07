/**
 * The day rollover on every write the study screens send (`409 session_rolled_over`).
 *
 * The service closed the session the request named and opened a new one, so a retry of the
 * same request can only fail again. Every call site shows one plain notice, offers NO Retry,
 * and hands the screen to the new session.
 */
import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '@/api';
import type { ApiClient, IntegratedApi, QuizResultResponse } from '@/api/types';
import { resetToasts, toastStore } from '@/app/toast';
import { ROLLED_OVER_MESSAGE } from '@/hooks/useCall';
import { Integrated } from '@/views/session/Integrated';
import { QuizResults } from '@/views/QuizResults';
import { mountCall } from './helpers/call';
import { PROBLEM } from './helpers/integrated';
import { mount as mountQuiz, stubApi as quizApi, submitAnswer as submitQuiz } from './helpers/quiz';
import {
  P, REVIEW, mount as mountSession, planOf, press, stubApi as sessionApi, submitAnswer,
} from './helpers/session';

const rolled = () => new ApiError(409, 'session_rolled_over', 'A new day started.');
const toasts = () => toastStore.getSnapshot();

/** One notice, plain, with no Retry. */
function expectOneNotice(): void {
  expect(toasts()).toHaveLength(1);
  expect(toasts()[0].message).toBe(ROLLED_OVER_MESSAGE);
  expect(toasts()[0].kind).toBe('info');
  expect(toasts()[0].onAction).toBeUndefined();
}

beforeEach(() => { resetToasts(); });

describe('the request wrapper', () => {
  it('fails once, offers no Retry, and calls the view handler', async () => {
    const onRolledOver = vi.fn();
    const onFail = vi.fn();
    const { result } = mountCall({ onRolledOver });
    const fn = vi.fn(async () => { throw rolled(); });
    const res = await result.current(fn, undefined, { onFail });
    expect(res).toBeUndefined();
    expect(fn).toHaveBeenCalledTimes(1);
    expect(onFail).toHaveBeenCalledTimes(1);
    expect(onRolledOver).toHaveBeenCalledTimes(1);
    expectOneNotice();
  });
});

describe('the study loop', () => {
  const today = { ...REVIEW, task_id: 't-today' };

  it('an answer moves to the new session plan', async () => {
    const getPlan = vi.fn<ApiClient['getPlan']>(async () => planOf(today));
    const taskServe = vi.fn<ApiClient['taskServe']>()
      .mockResolvedValueOnce(P(1))
      .mockResolvedValue(P(1, { problem_id: 'fresh', text: 'Fresh question' }));
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => { throw rolled(); });
    await mountSession({ plan: planOf(REVIEW), api: sessionApi({ getPlan, taskServe, taskAnswer }) });
    await submitAnswer('-4');
    expect(taskAnswer).toHaveBeenCalledTimes(1);
    expect(getPlan).toHaveBeenCalledTimes(1);
    expect(taskServe).toHaveBeenLastCalledWith('t-today');
    expect(screen.getByText('Fresh question')).toBeTruthy();
    expectOneNotice();
  });

  it('a hint moves to the new session plan', async () => {
    const getPlan = vi.fn<ApiClient['getPlan']>(async () => planOf(today));
    const taskServe = vi.fn<ApiClient['taskServe']>()
      .mockResolvedValueOnce(P(1))
      .mockResolvedValue(P(1, { problem_id: 'fresh', text: 'Fresh question' }));
    const taskHint = vi.fn<ApiClient['taskHint']>(async () => { throw rolled(); });
    await mountSession({ plan: planOf(REVIEW), api: sessionApi({ getPlan, taskServe, taskHint }) });
    await press('Hint');
    expect(taskHint).toHaveBeenCalledTimes(1);
    expect(getPlan).toHaveBeenCalledTimes(1);
    expect(taskServe).toHaveBeenLastCalledWith('t-today');
    expect(screen.getByText('Fresh question')).toBeTruthy();
    expectOneNotice();
  });

  it('a serve moves to the new session plan without a notice', async () => {
    const getPlan = vi.fn<ApiClient['getPlan']>(async () => planOf(today));
    const taskServe = vi.fn<ApiClient['taskServe']>()
      .mockRejectedValueOnce(rolled())
      .mockResolvedValue(P(1, { problem_id: 'fresh', text: 'Fresh question' }));
    await mountSession({ plan: planOf(REVIEW), api: sessionApi({ getPlan, taskServe }) });
    expect(taskServe.mock.calls.map(([id]) => id)).toEqual(['t-review', 't-today']);
    expect(screen.getByText('Fresh question')).toBeTruthy();
    expect(toasts()).toHaveLength(0);
  });
});

describe('the integrated item', () => {
  function setup() {
    const api: IntegratedApi = {
      taskIntegratedAnswer: vi.fn(async () => { throw rolled(); }),
      taskIntegratedHint: vi.fn(async () => { throw rolled(); }),
    };
    const onRolledOver = vi.fn();
    const onGraded = vi.fn();
    render(<Integrated api={api} taskId="integrated" problem={PROBLEM}
      onRolledOver={onRolledOver} onGraded={onGraded} />);
    return { api, onRolledOver, onGraded };
  }
  const click = async (name: string) => {
    await act(async () => { fireEvent.click(screen.getByRole('button', { name })); });
  };

  it('a hint hands the screen back once and shows no failure to retry', async () => {
    const { api, onRolledOver } = setup();
    await click('Hint (0/2)');
    expect(api.taskIntegratedHint).toHaveBeenCalledTimes(1);
    expect(onRolledOver).toHaveBeenCalledTimes(1);
    expect(screen.queryByText(/Try again/)).toBeNull();
    expectOneNotice();
  });

  it('a submission hands the screen back once and grades nothing', async () => {
    const { api, onRolledOver, onGraded } = setup();
    await click('Submit all my answers');
    expect(api.taskIntegratedAnswer).toHaveBeenCalledTimes(1);
    expect(onRolledOver).toHaveBeenCalledTimes(1);
    expect(onGraded).not.toHaveBeenCalled();
    expect(screen.queryByText(/Try again/)).toBeNull();
    expectOneNotice();
  });
});

describe('the quiz', () => {
  it('an answer leaves the quiz once and posts nothing more', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => { throw rolled(); });
    const view = await mountQuiz({ api: quizApi({ taskAnswer }) });
    await submitQuiz('5');
    expect(taskAnswer).toHaveBeenCalledTimes(1);
    expect(view.onDone).toHaveBeenCalledTimes(1);
    expectOneNotice();
  });

  it('the recorded result shows a notice with no Retry', async () => {
    const taskQuizResult = vi.fn<ApiClient['taskQuizResult']>(async (): Promise<QuizResultResponse> => {
      throw rolled();
    });
    render(<QuizResults api={quizApi({ taskQuizResult })} taskId="q" onUnauthorized={vi.fn()} />);
    await press('Review results');
    expect(taskQuizResult).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('button', { name: 'Review results' })).toBeTruthy();
    expectOneNotice();
  });
});
