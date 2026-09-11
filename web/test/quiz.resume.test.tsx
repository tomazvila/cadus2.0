/** A completed quiz resumes its reveal or independent practice without restarting time. */
import { act, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { ApiError } from '@/api';
import { fireToastAction } from '@/app/toast';
import type { ApiClient } from '@/api/types';
import { mount, Q, stubApi, toasts } from './helpers/quiz';
import { press } from './helpers/session';

it('offers the recorded reveal when the first serve reports task_complete', async () => {
  const taskAnswer = vi.fn<ApiClient['taskAnswer']>();
  const taskQuizResult = vi.fn<ApiClient['taskQuizResult']>().mockResolvedValue({
    inconclusive: false, score: 1, xp: 10, answers: [],
    practice_available: false, practice_pending: false,
  });
  await mount({ api: stubApi({ taskAnswer, taskQuizResult,
    taskServe: async () => { throw new ApiError(409, 'task_complete', 'The quiz is complete.'); },
  }) });
  expect(screen.getByText('Quiz complete')).toBeTruthy();
  expect(screen.getByText('Your answers are recorded.')).toBeTruthy();
  expect(document.querySelector('.timer')).toBeNull();
  expect(taskAnswer).not.toHaveBeenCalled();
  expect(taskQuizResult).not.toHaveBeenCalled();
  await press('Review results');
  expect(taskQuizResult).toHaveBeenCalledWith('t-quiz', false);
  expect(screen.getByText('100% of graded answers correct · 10 XP')).toBeTruthy();
});

it('resumes server-owned feedback practice without revealing the original answers', async () => {
  const taskServe = vi.fn<ApiClient['taskServe']>().mockResolvedValue(Q(4, {
    feedback_practice: true, text: 'Fresh practice question', quiz_elapsed_secs: 900,
  }));
  const taskAnswer = vi.fn<ApiClient['taskAnswer']>();
  await mount({ api: stubApi({ taskServe, taskAnswer }) });
  expect(screen.getByText('Independent practice')).toBeTruthy();
  expect(screen.queryByRole('button', { name: 'Review results' })).toBeNull();
  expect(document.querySelector('.timer')).toBeNull();
  expect(taskAnswer).not.toHaveBeenCalled();
  await press('Start fresh practice');
  expect(screen.getByText('Fresh practice question')).toBeTruthy();
  expect(taskServe.mock.calls).toEqual([['t-quiz'], ['t-quiz']]);
});

it.each([
  new ApiError(503, 'unavailable', 'The service is busy.'),
  new Error('Connection interrupted'),
])('retries an unavailable first serve without claiming quiz completion: $message', async (error) => {
  const taskServe = vi.fn<ApiClient['taskServe']>()
    .mockRejectedValueOnce(error).mockResolvedValue(Q(1));
  await mount({ api: stubApi({ taskServe }) });
  expect(screen.queryByText('Quiz complete')).toBeNull();
  expect(screen.queryByRole('textbox')).toBeNull();
  const retry = toasts().find((item) => item.label === 'Retry')!;
  expect(retry).toBeTruthy();
  await act(async () => { fireToastAction(retry.id); });
  expect(screen.getByText('Question 1.')).toBeTruthy();
  expect(taskServe.mock.calls).toEqual([['t-quiz'], ['t-quiz']]);
});
