/**
 * The answer buttons in the views: the session, the quiz, the placement, the fresh practice.
 *
 * Each view gets the same two checks. A payload with `choices` shows the buttons and no typed
 * field, and a tap posts the option text through the answer route. A payload with no
 * `choices` key shows the typed field as before.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { QuizResults } from '@/views/QuizResults';
import type { ApiClient, QuizResultResponse, TaskAnswerResponse } from '@/api/types';
import type { DiagAnswerResponse, DiagnosticApi } from '@/api/diag';
import { AXE_IN_JSDOM } from './axe';
import { held } from './helpers/held';
import * as quiz from './helpers/quiz';
import * as placement from './helpers/placement';
import * as session from './helpers/session';

const STEPS = ['Step 3', 'Step 1', 'Step 4', 'Step 2'];

const group = () => screen.getByRole('group', { name: 'Answer choices' });
const choiceButtons = () => within(group()).getAllByRole('button') as HTMLButtonElement[];
const labels = () => choiceButtons().map((b) => b.textContent);
const allDisabled = () => choiceButtons().every((b) => b.disabled);
const noneDisabled = () => choiceButtons().every((b) => !b.disabled);
const typedField = () => screen.queryByLabelText('Answer');
/** The text of each button that has `aria-pressed="true"`. */
const pressed = () => choiceButtons().filter((b) => b.getAttribute('aria-pressed') === 'true').map((b) => b.textContent);

/** Tap one option and let the continuations settle. */
async function tap(name: string): Promise<void> {
  await act(async () => { fireEvent.click(within(group()).getByRole('button', { name })); });
}

/** The choice mode: the buttons in the payload order, no typed field, no primary Submit. */
function expectButtonsOnly(submitName: string): void {
  expect(labels()).toEqual(['Step 3', 'Step 1', 'Step 4', 'Step 2']);
  expect(typedField()).toBeNull();
  expect(screen.queryByRole('button', { name: submitName })).toBeNull();
}

afterEach(() => { vi.useRealTimers(); });

describe('the session with a Label problem', () => {
  const labelProblem = () => session.P(1, { choices: STEPS });

  it('shows the buttons in the payload order, no typed field and no Submit button', async () => {
    await session.mount({ api: session.stubApi({ taskServe: async () => labelProblem() }) });
    expectButtonsOnly('Submit');
    // The work field and the hint control stay.
    expect(session.workInput()).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Hint' })).toBeTruthy();
  });

  it('posts the option text one time, locks the buttons in flight, and shows the verdict', async () => {
    const reply = held<TaskAnswerResponse>();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(() => reply.promise);
    await session.mount({ api: session.stubApi({ taskServe: async () => labelProblem(), taskAnswer }) });
    expect(noneDisabled()).toBe(true);

    await tap('Step 1');
    expect(taskAnswer.mock.calls).toEqual([['t-review', { problem_id: 'p1', answer: 'Step 1' }]]);
    expect(allDisabled()).toBe(true);
    // A second tap in flight posts nothing.
    await tap('Step 4');
    expect(taskAnswer).toHaveBeenCalledTimes(1);

    await act(async () => { reply.release(session.graded()); });
    expect(screen.getByText('Correct')).toBeTruthy();
    expect(allDisabled()).toBe(true);
  });

  it('posts the working with the option text', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => session.graded());
    await session.mount({ api: session.stubApi({ taskServe: async () => labelProblem(), taskAnswer }) });
    fireEvent.change(session.workInput(), { target: { value: 'Step 2 divides by zero' } });
    await tap('Step 2');
    expect(taskAnswer.mock.calls).toEqual([
      ['t-review', { problem_id: 'p1', answer: 'Step 2', work: 'Step 2 divides by zero' }],
    ]);
  });

  it('shows the buttons again for the re-solve of the same problem', async () => {
    const replies: TaskAnswerResponse[] = [
      session.REWORK, session.graded({ next: null, task_status: 'task_passed' }),
    ];
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => replies.shift()!);
    await session.mount({ api: session.stubApi({ taskServe: async () => labelProblem(), taskAnswer }) });

    await tap('Step 1');
    expect(screen.getByText(session.REWORK.re_solve)).toBeTruthy();
    expect(labels()).toEqual(['Step 3', 'Step 1', 'Step 4', 'Step 2']);
    expect(noneDisabled()).toBe(true);
    expect(typedField()).toBeNull();

    await tap('Step 3');
    expect(taskAnswer.mock.calls.map(([, body]) => [body.problem_id, body.answer])).toEqual([
      ['p1', 'Step 1'], ['p1', 'Step 3'],
    ]);
    expect(screen.getByText('Correct')).toBeTruthy();
  });

  it('shows in the verdict view which option the learner pressed', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => session.graded({ correct: false }));
    await session.mount({ api: session.stubApi({ taskServe: async () => labelProblem(), taskAnswer }) });
    expect(pressed()).toEqual([]);
    await tap('Step 4');
    expect(screen.getByText('Not quite')).toBeTruthy();
    expect(pressed()).toEqual(['Step 4']);
    expect(choiceButtons().map((b) => b.className)).toEqual([
      'btn choice-button', 'btn choice-button', 'btn choice-button is-selected', 'btn choice-button',
    ]);
  });

  it('removes the mark for the re-solve, and marks the option of the second tap', async () => {
    const second = held<TaskAnswerResponse>();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>()
      .mockResolvedValueOnce(session.REWORK)
      .mockReturnValueOnce(second.promise);
    await session.mount({ api: session.stubApi({ taskServe: async () => labelProblem(), taskAnswer }) });
    await tap('Step 1');
    // The re-solve starts with no answer: the buttons are enabled and no button has the mark.
    expect(noneDisabled()).toBe(true);
    expect(pressed()).toEqual([]);
    expect(document.querySelector('.is-selected')).toBeNull();
    await tap('Step 3');
    expect(pressed()).toEqual(['Step 3']);
    await act(async () => { second.release(session.graded({ next: null, task_status: 'task_passed' })); });
    expect(screen.getByText('Correct')).toBeTruthy();
    expect(pressed()).toEqual(['Step 3']);
  });

  it('moves to a typed problem after Continue, and that problem has the typed field', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => session.graded({ next: session.P(2) }));
    await session.mount({ api: session.stubApi({ taskServe: async () => labelProblem(), taskAnswer }) });
    await tap('Step 1');
    await session.press('Next problem →');
    expect(session.progressCount()).toBe('2 / 3 · 1 left');
    expect(session.answerInput()).toBeTruthy();
    expect(screen.queryByRole('group', { name: 'Answer choices' })).toBeNull();
    expect(session.submitButton()).toBeTruthy();
  });

  it('puts the focus on the choice container when the problem is ready', async () => {
    await session.mount({ api: session.stubApi({ taskServe: async () => labelProblem() }) });
    expect((document.activeElement as HTMLElement).className).toBe('choice-input');
  });

  it('posts a blank answer when the drill clock of a Label problem ends', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => session.graded({ correct: false }));
    vi.useFakeTimers();
    await session.mount({
      plan: session.planOf(session.DRILL),
      api: session.stubApi({
        taskAnswer,
        taskServe: async () => session.P(1, { countdown: true, time_budget_secs: 3, choices: STEPS }),
      }),
    });
    await act(async () => { vi.advanceTimersByTime(3000); });
    expect(taskAnswer.mock.calls).toEqual([['t-drill', { problem_id: 'p1', answer: '' }]]);
  });

  it('passes axe with the buttons on screen', async () => {
    const view = await session.mount({ api: session.stubApi({ taskServe: async () => labelProblem() }) });
    expect(await axe(view.container, AXE_IN_JSDOM)).toHaveNoViolations();
  });

  it.each([
    ['no choices key', session.P(1)],
    ['an empty choices list', session.P(1, { choices: [] })],
  ])('shows the typed field and the Submit button for %s', async (_label, problem) => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => session.graded());
    await session.mount({ api: session.stubApi({ taskServe: async () => problem, taskAnswer }) });
    expect(screen.queryByRole('group', { name: 'Answer choices' })).toBeNull();
    expect(document.activeElement).toBe(session.answerInput());
    await session.submitAnswer('3/4');
    expect(taskAnswer.mock.calls).toEqual([['t-review', { problem_id: 'p1', answer: '3/4' }]]);
  });
});

describe('the quiz with a Label question', () => {
  it('shows the buttons, no typed field and no Submit button', async () => {
    await quiz.mount({ api: quiz.stubApi({ taskServe: async () => quiz.Q(1, { choices: STEPS }) }) });
    expectButtonsOnly('Submit');
    // The promise of the quiz stays on the card.
    expect(screen.getByText('No feedback until the end.')).toBeTruthy();
  });

  it('posts the option text and moves to the next question with a receipt only', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>()
      .mockResolvedValueOnce(quiz.Q(1, { choices: STEPS }))
      .mockResolvedValue(quiz.Q(2));
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => quiz.receipt());
    await quiz.mount({ api: quiz.stubApi({ taskServe, taskAnswer }) });

    await tap('Step 4');
    expect(quiz.posted(taskAnswer)).toEqual([['q1', 'Step 4']]);
    // The next question is a typed question: the field and the Submit button are back.
    expect(screen.getByText('Question 2.')).toBeTruthy();
    expect(quiz.answerInput()).toBeTruthy();
    expect(quiz.submitButton()).toBeTruthy();
    expect(screen.queryByRole('group', { name: 'Answer choices' })).toBeNull();
    // QUIZ-reveal: no verdict for the tap.
    expect(screen.queryByText('Correct')).toBeNull();
    expect(quiz.remaining()!.textContent).toBe('2 remaining');
  });

  it('locks the buttons while the answer is in flight, so a second tap posts nothing', async () => {
    const reply = held<TaskAnswerResponse>();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(() => reply.promise);
    await quiz.mount({ api: quiz.stubApi({ taskServe: async () => quiz.Q(1, { choices: STEPS }), taskAnswer }) });
    await tap('Step 3');
    expect(allDisabled()).toBe(true);
    await tap('Step 1');
    expect(quiz.posted(taskAnswer)).toEqual([['q1', 'Step 3']]);
    await act(async () => { reply.release(quiz.completed()); });
    expect(screen.getByText('Quiz complete')).toBeTruthy();
  });

  it('shows the typed field for a question with no choices key', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => quiz.receipt());
    await quiz.mount({ api: quiz.stubApi({ taskAnswer }) });
    expect(screen.queryByRole('group', { name: 'Answer choices' })).toBeNull();
    await quiz.submitAnswer('17');
    expect(quiz.posted(taskAnswer)).toEqual([['q1', '17']]);
  });
});

describe('the placement with a Label probe', () => {
  const labelStart = { probe: placement.probe({ choices: STEPS }), asked: 0, cap: 40 };

  it('shows the buttons, no typed field and no Submit; Skip and Save & exit stay', async () => {
    await placement.mount({ diag: placement.stubDiag({ diagStart: async () => labelStart }) });
    await placement.begin();
    expectButtonsOnly('Submit');
    expect(placement.skipButton()).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Save & exit' })).toBeTruthy();
  });

  it('posts the option text of the probe and locks the buttons', async () => {
    const reply = held<DiagAnswerResponse>();
    const diagAnswer = vi.fn<DiagnosticApi['diagAnswer']>(() => reply.promise);
    await placement.mount({ diag: placement.stubDiag({ diagStart: async () => labelStart, diagAnswer }) });
    await placement.begin();
    await tap('Step 2');
    expect(diagAnswer.mock.calls).toEqual([[{ problem_id: 'd1', answer: 'Step 2' }]]);
    expect(allDisabled()).toBe(true);
    expect(document.querySelector('.feedback-title')).toBeNull();
    await act(async () => { reply.release({ correct: true, next_probe: { done: true } }); });
    expect(allDisabled()).toBe(true);
    // The feedback of the placement shows as for a typed answer.
    expect(document.querySelector('.feedback-title')).not.toBeNull();
  });

  it('posts a blank answer for Skip on a Label probe', async () => {
    const diagAnswer = vi.fn<DiagnosticApi['diagAnswer']>(async () => ({ correct: false, next_probe: { done: true } }));
    await placement.mount({ diag: placement.stubDiag({ diagStart: async () => labelStart, diagAnswer }) });
    await placement.begin();
    await act(async () => { fireEvent.click(placement.skipButton()); });
    expect(diagAnswer.mock.calls).toEqual([[{ problem_id: 'd1', answer: '' }]]);
  });

  it('shows the typed field for a probe with no choices key', async () => {
    const diagAnswer = vi.fn<DiagnosticApi['diagAnswer']>(async () => ({ correct: true, next_probe: { done: true } }));
    await placement.mount({ diag: placement.stubDiag({ diagAnswer }) });
    await placement.begin();
    expect(screen.queryByRole('group', { name: 'Answer choices' })).toBeNull();
    await placement.answer('5');
    expect(diagAnswer.mock.calls).toEqual([[{ problem_id: 'd1', answer: '5' }]]);
  });
});

describe('the fresh practice after a quiz with a Label problem', () => {
  const pending: QuizResultResponse = {
    inconclusive: false, score: 0, xp: 0, practice_available: false, practice_pending: true, answers: [],
  };

  function show(api: ApiClient): void {
    render(<QuizResults api={api} taskId="recorded-quiz" onUnauthorized={vi.fn()} resumePractice />);
  }

  it('shows the buttons and posts the option text', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => session.graded({ next: null }));
    show(quiz.stubApi({
      taskQuizResult: async () => pending,
      taskServe: async () => quiz.Q(1, { choices: STEPS }),
      taskAnswer,
    }));
    await session.press('Start fresh practice');
    expect(labels()).toEqual(['Step 3', 'Step 1', 'Step 4', 'Step 2']);
    expect(screen.queryByRole('textbox')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Submit' })).toBeNull();
    await tap('Step 1');
    expect(quiz.posted(taskAnswer)).toEqual([['q1', 'Step 1']]);
    expect(screen.getByText(/Practice done/)).toBeTruthy();
  });

  it('shows the typed field for a problem with no choices key', async () => {
    show(quiz.stubApi({ taskQuizResult: async () => pending, taskServe: async () => quiz.Q(1) }));
    await session.press('Start fresh practice');
    expect(screen.getByRole('textbox')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Submit' })).toBeTruthy();
    expect(screen.queryByRole('group', { name: 'Answer choices' })).toBeNull();
  });
});
