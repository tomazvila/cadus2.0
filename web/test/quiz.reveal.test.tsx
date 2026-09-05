/**
 * The timed quiz (S10), part 2: the reveal, the timeout, the gate, the stale Retry, and
 * accessibility.
 *
 * `quiz.test.tsx` carries the module note and the fixtures live in `test/helpers/quiz.tsx`.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, screen } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { QUIZ_SILENCE_NOTE, QUIZ_TIMEOUT_MESSAGE } from '@/views/Quiz';
import { RETRY_STALE_MESSAGE } from '@/hooks/useCall';
import { fireToastAction, TOAST_TIMEOUT_MS } from '@/app/toast';
import { AXE_IN_JSDOM } from './axe';
import { appendOnlyGrade, busy, flakyOnce, networkFailure } from './helpers/api';
import { tick } from './helpers/timers';
import {
  expectRefusalAmongPlainToasts, expectRetryArmed, pressRetryAfterUnmount,
} from './helpers/toasts';
import {
  QUIZ, Q, answerInput, completed, mount, posted, receipt, remaining, stubApi,
  pressSubmit, submitAnswer, submitButton, threeQuestions, toasts, typeAnswer,
} from './helpers/quiz';
import type { AnswerResponse, ApiClient, QuizReceiptResponse, TaskAnswerResponse } from '@/api/types';

/** Mount over a grade that closes the quiz, and answer once. */
async function finishQuiz(over: Partial<Parameters<typeof mount>[0]> = {}) {
  const view = await mount({ api: stubApi({ taskAnswer: async () => completed() }), ...over });
  await submitAnswer('7/12');
  return view;
}

/** A held grade: the test releases it by hand. */
function heldGrade() {
  let release!: (value: TaskAnswerResponse) => void;
  const held = new Promise<TaskAnswerResponse>((r) => { release = r; });
  const taskAnswer = vi.fn<ApiClient['taskAnswer']>(() => held);
  return { taskAnswer, release: (value: TaskAnswerResponse) => release(value) };
}

describe('QUIZ-reveal: silence until the last answer', () => {
  it('QUIZ-reveal: an accepted answer shows a count and nothing about correctness', async () => {
    // The reply carries `correct` — a service that grew the field, or a defect. Rendering
    // it defeats the batch reveal, so the screen renders none of it.
    const leaky: QuizReceiptResponse & Partial<AnswerResponse> = {
      ...receipt(),
      correct: true,
      solution: 'Divide by two.',
    };
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => leaky);
    await mount({ api: stubApi({ taskServe: threeQuestions(), taskAnswer }) });

    await submitAnswer('7/12');

    expect(remaining()!.textContent).toBe('2 remaining');
    expect(screen.getByText(QUIZ_SILENCE_NOTE)).toBeTruthy();
    expect(QUIZ_SILENCE_NOTE).toBe('No feedback until the end.');
    expect(document.querySelector('.feedback')).toBeNull();
    expect(document.querySelector('.solution')).toBeNull();
    expect(document.body.textContent).not.toContain('Divide by two.');
    expect(document.body.textContent!.toLowerCase()).not.toContain('correct');
  });

  it('QUIZ-reveal: the next question arrives with an empty field and a new node', async () => {
    await mount({
      api: stubApi({
        taskServe: vi.fn<ApiClient['taskServe']>()
          .mockResolvedValueOnce(Q(1, { text: 'First.' }))
          .mockResolvedValue(Q(2, { text: 'Second.' })),
      }),
    });

    const first = answerInput();
    await submitAnswer('7/12');

    expect(document.querySelector('.problem-text')!.textContent).toBe('Second.');
    // A DIFFERENT node, which is what `key={problem.problem_id}` guarantees: without it
    // React reuses the input and the previous answer pre-fills the next question.
    expect(answerInput()).not.toBe(first);
    expect(answerInput().value).toBe('');
    expect(document.querySelector('.progress-count')!.textContent).toBe('2 / 3');
  });

  it('QUIZ-reveal: the end screen states the count and invents no score', async () => {
    const { onDone } = await finishQuiz();

    expect(screen.getByText('Quiz complete')).toBeTruthy();
    expect(screen.getByText('All 3 answers are recorded.')).toBeTruthy();
    // M5 mounts no close route, so there is no score and no per-question breakdown to
    // show. A number invented here would be a lie about a recorded attempt.
    expect(document.querySelector('.score-big')).toBeNull();
    expect(document.body.textContent).not.toContain('%');
    fireEvent.click(screen.getByRole('button', { name: 'Back to dashboard' }));
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it('QUIZ-reveal: a grade reply that is not a receipt ends the quiz revealing nothing', async () => {
    // A verdict payload on a quiz route is a service defect. The honest response is to end
    // the quiz, never to paint a verdict this screen is not allowed to show.
    const verdict: Partial<AnswerResponse> = {
      correct: false,
      solution: 'Divide by two.',
      error_tags: ['sign-error'],
    };
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => verdict as TaskAnswerResponse);
    await mount({ api: stubApi({ taskAnswer }) });

    await submitAnswer('7/12');

    expect(screen.getByText('Quiz complete')).toBeTruthy();
    expect(document.body.textContent).not.toContain('Divide by two.');
    expect(document.body.textContent).not.toContain('sign-error');
  });

  it('names the way back to the session when the quiz was entered from one', async () => {
    await finishQuiz({ fromSession: true });

    expect(screen.getByRole('button', { name: 'Continue session' })).toBeTruthy();
  });
});

describe('QUIZ-timeout: the clock runs out', () => {
  it('QUIZ-timeout: blank-fills the questions that are left and ends the quiz', async () => {
    vi.useFakeTimers();
    const taskServe = threeQuestions();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>()
      .mockResolvedValueOnce(receipt({ remaining: 2 }))
      .mockResolvedValueOnce(receipt({ remaining: 1 }))
      .mockResolvedValue(completed());
    await mount({ task: { ...QUIZ, time_budget_secs: 5 }, api: stubApi({ taskServe, taskAnswer }) });

    await tick(5000);

    expect(posted(taskAnswer)).toEqual([['q1', ''], ['q2', ''], ['q3', '']]);
    expect(screen.getByText('Quiz complete')).toBeTruthy();
    expect(toasts().map((t) => t.message)).toContain(QUIZ_TIMEOUT_MESSAGE);
    expect(QUIZ_TIMEOUT_MESSAGE).toBe('Time’s up — grading your answers.');
  });

  it('QUIZ-timeout: never re-posts a problem_id already in flight', async () => {
    vi.useFakeTimers();
    const { taskAnswer, release } = heldGrade();
    await mount({ task: { ...QUIZ, time_budget_secs: 3 }, api: stubApi({ taskAnswer }) });

    typeAnswer('7');
    fireEvent.click(submitButton());
    expect(taskAnswer).toHaveBeenCalledTimes(1);

    // The clock expires while that answer is in flight. A second post of `q1` writes a
    // second attempt to an append-only log and 404s the loser.
    await tick(3000);
    expect(taskAnswer).toHaveBeenCalledTimes(1);
    expect(posted(taskAnswer)).toEqual([['q1', '7']]);

    await act(async () => { release(completed()); });
    expect(screen.getByText('Quiz complete')).toBeTruthy();
  });

  it('QUIZ-timeout: the skipped question resumes the blank fill when its answer lands', async () => {
    vi.useFakeTimers();
    let release!: (value: TaskAnswerResponse) => void;
    const held = new Promise<TaskAnswerResponse>((r) => { release = r; });
    const taskServe = threeQuestions();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>()
      .mockReturnValueOnce(held)
      .mockResolvedValueOnce(receipt({ remaining: 1 }))
      .mockResolvedValue(completed());
    await mount({ task: { ...QUIZ, time_budget_secs: 3 }, api: stubApi({ taskServe, taskAnswer }) });

    typeAnswer('7');
    fireEvent.click(submitButton());
    await tick(3000);
    expect(taskAnswer).toHaveBeenCalledTimes(1);

    // The in-flight continuation resumes the fill as soon as the next question is served,
    // so the quiz still closes with every question answered exactly once.
    await act(async () => { release(receipt({ remaining: 2 })); });

    expect(posted(taskAnswer)).toEqual([['q1', '7'], ['q2', ''], ['q3', '']]);
    expect(screen.getByText('Quiz complete')).toBeTruthy();
  });

  it('QUIZ-timeout: fires once, however long the clock runs past zero', async () => {
    vi.useFakeTimers();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => completed());
    await mount({ task: { ...QUIZ, time_budget_secs: 2 }, api: stubApi({ taskAnswer }) });

    await tick(60_000);

    // One post, not one per second past zero. The interval keeps running; `timedOutRef`
    // is what makes the timeout a single event.
    expect(taskAnswer).toHaveBeenCalledTimes(1);
  });
});

describe('the phase gate and the view lifetime', () => {
  it('F-37-1c: a second submit inside the grading window posts nothing', async () => {
    const { taskAnswer, release } = heldGrade();
    await mount({ api: stubApi({ taskAnswer }) });

    typeAnswer('7/12');
    fireEvent.keyDown(answerInput(), { key: 'Enter' });
    expect(taskAnswer).toHaveBeenCalledTimes(1);

    // The field stays enabled while the grade runs: the gate, not the attribute, is what
    // stops the second post. Two more Enters and a click land inside the window.
    fireEvent.keyDown(answerInput(), { key: 'Enter' });
    fireEvent.keyDown(answerInput(), { key: 'Enter' });
    fireEvent.click(submitButton());
    expect(taskAnswer).toHaveBeenCalledTimes(1);

    await act(async () => { release(completed()); });
    expect(taskAnswer).toHaveBeenCalledTimes(1);
  });

  it('F-37-1b: the clock dies with the view and blank-fills nothing after it', async () => {
    vi.useFakeTimers();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => receipt());
    const { unmount } = await mount({
      task: { ...QUIZ, time_budget_secs: 5 },
      api: stubApi({ taskAnswer }),
    });

    unmount();
    await tick(60_000);

    expect(taskAnswer).not.toHaveBeenCalled();
  });

  it('NO-2BILL: one mount serves exactly one question', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => Q(1));
    await mount({ api: stubApi({ taskServe }) });

    expect(taskServe).toHaveBeenCalledTimes(1);
    expect(taskServe).toHaveBeenCalledWith('t-quiz');
  });

  it('refuses a blank submit without posting, and returns the focus to the field', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => receipt());
    await mount({ api: stubApi({ taskAnswer }) });

    await pressSubmit();

    expect(taskAnswer).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(answerInput());
    expect(submitButton().hasAttribute('disabled')).toBe(false);
  });

  it('a failed grade returns the question to the learner instead of locking the card', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => { throw networkFailure(); });
    await mount({ api: stubApi({ taskAnswer }) });

    await submitAnswer('7/12');

    expect(submitButton().hasAttribute('disabled')).toBe(false);
    await submitAnswer('7/12');
    expect(taskAnswer).toHaveBeenCalledTimes(2);
  });
});

describe('the stale Retry', () => {
  it('F-37-1c: a stale quiz Retry posts nothing and consumes no question', async () => {
    // The defect this pins (V4). The Retry toast of a failed grade never expires, and it
    // re-entered `run()` with the SPENT `problem_id`. The stale continuation then ran a
    // second `taskServe`, which replaced the question on screen unanswered.
    vi.useFakeTimers();
    const taskServe = threeQuestions();
    /** A quiz whose FIRST grade fails and whose later grades are accepted. */
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(flakyOnce(() => receipt({ remaining: 2 })));
    await mount({ api: stubApi({ taskServe, taskAnswer }) });

    // The grade fails. The question comes back to the learner with a Retry armed.
    await submitAnswer('7/12');
    expectRetryArmed(submitButton);

    // The learner answers again instead, and THAT attempt is accepted. Question 2 arrives.
    await submitAnswer('7/12');
    expect(document.querySelector('.problem-text')!.textContent).toBe('Question 2.');
    expect(taskServe).toHaveBeenCalledTimes(2);

    // The Retry now names a consumed question, so the gate refuses it.
    const stale = toasts()[0];
    await act(async () => { fireToastAction(stale.id); });

    expect(posted(taskAnswer)).toEqual([['q1', '7/12'], ['q1', '7/12']]);
    // NO third serve: the question on screen is not consumed.
    expect(taskServe).toHaveBeenCalledTimes(2);
    expect(document.querySelector('.problem-text')!.textContent).toBe('Question 2.');
    expect(document.querySelector('.progress-count')!.textContent).toBe('2 / 3');
    expect(toasts().length).toBe(1);
    expect(toasts()[0].message).toBe(RETRY_STALE_MESSAGE);
    expect(RETRY_STALE_MESSAGE).toBe('That retry came too late. Continue from the screen.');
    expect(toasts()[0].kind).toBe('info');
  });

  it('F-36-1b: the refusal of a stale quiz Retry expires and arms no second Retry', async () => {
    // The defect this pins (V7). A stale Retry posted the spent `problem_id`, the service
    // answered `404 unknown_problem`, and that failure armed ANOTHER actionable toast —
    // which never expires either. The refusal carries no action, so it expires.
    vi.useFakeTimers();
    const taskServe = vi.fn<ApiClient['taskServe']>()
      .mockResolvedValueOnce(Q(1))
      .mockResolvedValue(Q(2));
    // The append-only log of the service: the first grade fails, the second is accepted,
    // and every later post of that same `problem_id` is `404 unknown_problem`.
    const grade = appendOnlyGrade(() => receipt({ remaining: 2 }));
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async (_task, body) => grade(body.problem_id));
    await mount({ api: stubApi({ taskServe, taskAnswer }) });

    await submitAnswer('7/12');
    const stale = toasts()[0];

    // The Retry of a real failure stays on screen for the life of the quiz.
    await tick(TOAST_TIMEOUT_MS + 1000);
    expect(toasts().map((t) => t.id)).toEqual([stale.id]);

    await submitAnswer('7/12');
    expect(taskAnswer).toHaveBeenCalledTimes(2);

    await act(async () => { fireToastAction(stale.id); });
    expect(toasts().length).toBe(1);
    expect(toasts()[0].label).toBeUndefined();
    expect(toasts()[0].onAction).toBeUndefined();

    // A plain toast expires, so the screen is left clean and no Retry survives.
    await tick(TOAST_TIMEOUT_MS + 1000);
    expect(toasts()).toEqual([]);
    expect(taskAnswer).toHaveBeenCalledTimes(2);
    expect(taskServe).toHaveBeenCalledTimes(2);
  });

  it('F-37-1b: a Retry pressed after the quiz view is gone posts nothing', async () => {
    // The toast store is module-scope, so an actionable toast OUTLIVES the view that raised
    // it. Without a gate the Retry of the blank fill posts an answer for a screen the
    // learner already left — a write to an append-only log with nobody on it.
    vi.useFakeTimers();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => { throw busy(); });
    const { unmount } = await mount({
      task: { ...QUIZ, time_budget_secs: 3 },
      api: stubApi({ taskAnswer }),
    });

    // The clock runs out and the blank fill fails, which arms the Retry.
    await tick(3000);
    expect(posted(taskAnswer)).toEqual([['q1', '']]);

    await pressRetryAfterUnmount(unmount);

    expect(posted(taskAnswer)).toEqual([['q1', '']]);
    expectRefusalAmongPlainToasts();
  });
});

describe('accessibility', () => {
  it('the quiz card reports no axe violation', async () => {
    const { container } = await mount();
    expect(await axe(container, AXE_IN_JSDOM)).toHaveNoViolations();
  });

  it('the answer field takes the focus on a fresh question', async () => {
    await mount();
    expect(document.activeElement).toBe(answerInput());
  });

  it('the end screen moves the focus to the way out', async () => {
    await finishQuiz();

    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Back to dashboard' }));
  });

  it('offers no hint control: a hint inside a quiz is 409 no_hints_in_quiz', async () => {
    await mount();
    expect(screen.queryByRole('button', { name: 'Hint' })).toBeNull();
  });
});
