/** Recorded evidence and recovery through the fresh-practice route. */
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { QuizResults } from '@/views/QuizResults';
import type { ApiClient, QuizResultResponse, ServedProblem, TaskAnswerResponse } from '@/api/types';
import { busy } from './helpers/api';
import { held } from './helpers/held';
import { Q, receipt, stubApi } from './helpers/quiz';
import { graded, press, REWORK, ungraded } from './helpers/session';

const emptyResult: QuizResultResponse = {
  inconclusive: false, score: 1, xp: 10,
  practice_available: false, practice_pending: false, answers: [],
};

function show(api: ApiClient, resumePractice = false): void {
  render(<QuizResults api={api} taskId="recorded-quiz" onUnauthorized={vi.fn()} resumePractice={resumePractice} />);
}

/** Two events in one React batch exercise the synchronous request gate. */
function doubleClick(name: string): void {
  const button = screen.getByRole('button', { name });
  act(() => { button.click(); button.click(); });
}

async function answerPractice(value = '17'): Promise<void> {
  fireEvent.change(screen.getByRole('textbox'), { target: { value } });
  await press('Submit practice answer');
}

it('renders ungraded answers separately, including blank answers and absent reasons', async () => {
  const result: QuizResultResponse = {
    ...emptyResult, inconclusive: true,
    answers: [
      { problem_id: 'a', text: 'Uncertain first', given_answer: '', correct: false, outcome: 'ungraded', reason: 'Ambiguous notation', solution_sketch: 'Hidden solution' },
      { problem_id: 'b', text: 'Uncertain second', given_answer: 'x', correct: false, outcome: 'ungraded' },
      { problem_id: 'c', text: 'Confirmed answer', given_answer: '17', outcome: 'correct', correct: true },
    ],
  };
  show(stubApi({ taskQuizResult: async () => result }));
  await press('Review results');
  expect(screen.getByText('Quiz verdict pending: some answers need review. No progress or XP awarded.')).toBeTruthy();
  expect(screen.getByText('Your answer: (blank)')).toBeTruthy();
  expect(screen.getByText('Needs review: Ambiguous notation')).toBeTruthy();
  expect(screen.getByText('Needs review: No verdict')).toBeTruthy();
  expect(within(screen.getByText('Confirmed answer').closest('article')!).getByText('Correct')).toBeTruthy();
  expect(screen.queryByText('Hidden solution')).toBeNull();
  expect(screen.queryByRole('button', { name: /Practice missed skills/ })).toBeNull();
});

it('reopens pending practice only when the server confirms it remains pending', async () => {
  const taskQuizResult = vi.fn<ApiClient['taskQuizResult']>()
    .mockResolvedValueOnce({ ...emptyResult, practice_pending: true })
    .mockResolvedValue(emptyResult);
  show(stubApi({ taskQuizResult }));
  await press('Review results');
  await press('Done studying · Practice missed skills');
  expect(taskQuizResult.mock.calls).toEqual([['recorded-quiz', false], ['recorded-quiz', true]]);
  expect(screen.getByText('100% of graded answers correct · 10 XP')).toBeTruthy();
  expect(screen.queryByRole('button', { name: 'Start fresh practice' })).toBeNull();
});

it('unlocks a failed reveal and accepts only one request while its retry is pending', async () => {
  const pending = held<QuizResultResponse>();
  const taskQuizResult = vi.fn<ApiClient['taskQuizResult']>()
    .mockRejectedValueOnce(busy()).mockReturnValue(pending.promise);
  show(stubApi({ taskQuizResult }));
  await press('Review results');
  expect(screen.getByRole('button', { name: 'Review results' }).hasAttribute('disabled')).toBe(false);
  doubleClick('Review results');
  expect(taskQuizResult).toHaveBeenCalledTimes(2);
  expect(screen.getByRole('button', { name: 'Review results' }).hasAttribute('disabled')).toBe(true);
  await act(async () => { pending.release(emptyResult); });
  expect(screen.getByText('Recorded quiz result')).toBeTruthy();
});

it('unlocks a failed fresh serve and gates repeated clicks until its problem arrives', async () => {
  const pending = held<ServedProblem>();
  const taskServe = vi.fn<ApiClient['taskServe']>()
    .mockRejectedValueOnce(busy()).mockReturnValue(pending.promise);
  show(stubApi({ taskServe }), true);
  await press('Start fresh practice');
  expect(screen.getByRole('button', { name: 'Start fresh practice' }).hasAttribute('disabled')).toBe(false);
  doubleClick('Start fresh practice');
  expect(taskServe.mock.calls).toEqual([['recorded-quiz'], ['recorded-quiz']]);
  expect(screen.getByRole('button', { name: 'Start fresh practice' }).hasAttribute('disabled')).toBe(true);
  await act(async () => { pending.release(Q(1)); });
  expect(screen.getByText('Question 1.')).toBeTruthy();
});

it('keeps a failed answer editable, ignores whitespace, and gates a repeated submission', async () => {
  const pending = held<TaskAnswerResponse>();
  const taskAnswer = vi.fn<ApiClient['taskAnswer']>()
    .mockRejectedValueOnce(busy()).mockReturnValue(pending.promise);
  show(stubApi({ taskAnswer }), true);
  await press('Start fresh practice');
  await answerPractice('   ');
  expect(taskAnswer).not.toHaveBeenCalled();
  await answerPractice('17');
  expect(screen.getByRole('textbox').hasAttribute('disabled')).toBe(false);
  expect((screen.getByRole('textbox') as HTMLInputElement).value).toBe('17');
  doubleClick('Submit practice answer');
  expect(taskAnswer.mock.calls).toEqual([
    ['recorded-quiz', { problem_id: 'q1', answer: '17' }],
    ['recorded-quiz', { problem_id: 'q1', answer: '17' }],
  ]);
  await act(async () => { pending.release(graded({ next: null })); });
  expect(screen.getByText(/Independent practice complete/)).toBeTruthy();
});

it.each([
  ['incorrect', graded({ correct: false }), 'Study the solution, then try a fresh problem.'],
  ['ungraded', ungraded(), 'This answer needs review.'],
  ['correct with another item', graded(), 'Correct'],
  ['correct awaiting refill', graded({ next: null, next_unavailable: true }), 'Correct'],
])('continues fresh practice after %s feedback', async (_label, reply, message) => {
  const taskServe = vi.fn<ApiClient['taskServe']>()
    .mockResolvedValueOnce(Q(1)).mockResolvedValue(Q(2));
  show(stubApi({ taskServe, taskAnswer: async () => reply }), true);
  await press('Start fresh practice');
  await answerPractice();
  expect(screen.getByText(message)).toBeTruthy();
  expect(screen.queryByRole('textbox')).toBeNull();
  expect(screen.queryByText('Divide both parts by 2.') !== null).toBe(reply.outcome !== 'ungraded');
  await press('Done studying · Next fresh problem');
  expect(screen.queryByText(message)).toBeNull();
  expect(screen.queryByText('Divide both parts by 2.')).toBeNull();
  expect(screen.getByText('Question 2.')).toBeTruthy();
  expect((screen.getByRole('textbox') as HTMLInputElement).value).toBe('');
});

it.each([['quiz receipt', receipt()], ['rework', REWORK]])(
  'keeps fresh practice free of a verdict from an unexpected %s response',
  async (_label, reply) => {
    show(stubApi({ taskAnswer: async () => reply }), true);
    await press('Start fresh practice');
    await answerPractice();
    expect(screen.getByRole('button', { name: 'Start fresh practice' })).toBeTruthy();
    expect(screen.queryByText('Divide both parts by 2.')).toBeNull();
    expect(screen.queryByText(/Independent practice complete/)).toBeNull();
  },
);
