/** The explicit batch reveal hides studied material before independent practice. */
import { act, fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { QuizResults } from '@/views/QuizResults';
import type { ApiClient, QuizResultResponse } from '@/api/types';
import { stubApi, Q } from './helpers/quiz';
import { graded } from './helpers/session';

const result: QuizResultResponse = {
  inconclusive: false, score: 0.5, xp: 5, practice_available: true, practice_pending: false,
  answers: [{ problem_id: 'old', text: 'Original question', given_answer: '9', correct: false, outcome: 'incorrect', solution_sketch: 'Original worked solution' }],
};

it('reveals only on request and hides originals for fresh practice while preserving score', async () => {
  const taskQuizResult = vi.fn<ApiClient['taskQuizResult']>().mockResolvedValueOnce(result)
    .mockResolvedValue({ ...result, practice_pending: true, practice_available: false });
  const api = stubApi({ taskQuizResult,
    taskServe: async () => Q(4, { text: 'Fresh independent question', feedback_practice: true }),
    taskAnswer: async () => graded({ correct: true, next: null }),
  });
  render(<QuizResults api={api} taskId="quiz" onUnauthorized={vi.fn()} />);
  expect(taskQuizResult).not.toHaveBeenCalled();
  await act(async () => { fireEvent.click(screen.getByText('Review results')); });
  expect(screen.getByText('Original worked solution')).toBeTruthy();
  expect(screen.getByText('50% of graded answers correct · 5 XP')).toBeTruthy();
  await act(async () => { fireEvent.click(screen.getByText('Done studying · Practice missed skills')); });
  expect(screen.queryByText('Original worked solution')).toBeNull();
  await act(async () => { fireEvent.click(screen.getByText('Start fresh practice')); });
  expect(screen.getByText('Fresh independent question')).toBeTruthy();
  fireEvent.change(screen.getByRole('textbox'), { target: { value: '47' } });
  await act(async () => { fireEvent.click(screen.getByText('Submit practice answer')); });
  expect(screen.getByText('Independent practice complete. The recorded quiz result is unchanged.')).toBeTruthy();
});

describe('practice recovery', () => {
  it('resumes without revealing the old solution or arming a quiz clock', () => {
    render(<QuizResults api={stubApi()} taskId="quiz" onUnauthorized={vi.fn()} resumePractice />);
    expect(screen.getByText('Start fresh practice')).toBeTruthy();
    expect(screen.queryByText('Review results')).toBeNull();
    expect(document.querySelector('.timer')).toBeNull();
  });
});
