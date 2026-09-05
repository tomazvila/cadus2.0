/**
 * The quiz card, control by control: the busy submit, the locked field, the count that
 * falls back to the first serve, and the clock that lets go when the quiz is over.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, screen } from '@testing-library/react';
import { failThenHold } from './helpers/api';
import { blurThenSubmitEmpty } from './helpers/field';
import { held } from './helpers/held';
import { pressRetry } from './helpers/toasts';
import { tick } from './helpers/timers';
import {
  Q, QUIZ, answerInput, completed, mount, receipt, stubApi, submitAnswer, submitButton,
  threeQuestions, timer, typeAnswer,
} from './helpers/quiz';
import type { ApiClient, QuizReceiptResponse } from '@/api/types';

describe('the quiz card', () => {
  it('waits with a labelled block until the first question', async () => {
    const serve = held<ReturnType<typeof Q>>();
    await mount({ api: stubApi({ taskServe: () => serve.promise }) });
    expect(screen.getByText('Loading the quiz…')).toBeTruthy();
    await act(async () => { serve.release(Q(1)); });
    expect(screen.getByText('Question 1.')).toBeTruthy();
  });

  it('locks the field and marks Submit busy while a grade is out', async () => {
    const grade = held<QuizReceiptResponse>();
    await mount({ api: stubApi({ taskAnswer: () => grade.promise }) });
    expect(submitButton().className).toBe('btn btn-primary');

    await submitAnswer('7');
    expect(submitButton().className).toBe('btn btn-primary is-busy');
    expect(submitButton().hasAttribute('disabled')).toBe(true);
    expect(answerInput().disabled).toBe(true);

    await act(async () => { grade.release(receipt()); });
    expect(submitButton().className).toBe('btn btn-primary');
    expect(answerInput().disabled).toBe(false);
  });

  it('marks Submit busy again on the Retry of a failed grade', async () => {
    const grade = failThenHold<QuizReceiptResponse>();
    await mount({ api: stubApi({ taskAnswer: grade.fn }) });
    await submitAnswer('7');
    expect(submitButton().className).toBe('btn btn-primary');
    await pressRetry();
    expect(submitButton().className).toBe('btn btn-primary is-busy');
    await act(async () => { grade.release(receipt()); });
  });

  it('marks Submit busy while the blank fill runs after the clock ran out', async () => {
    vi.useFakeTimers();
    const fill = held<QuizReceiptResponse>();
    await mount({ task: { ...QUIZ, time_budget_secs: 2 }, api: stubApi({ taskAnswer: () => fill.promise }) });
    await tick(2000);
    expect(submitButton().className).toBe('btn btn-primary is-busy');
    await act(async () => { fill.release(completed()); });
    expect(screen.getByText('Quiz complete')).toBeTruthy();
  });

  it('returns the focus to an empty field on Submit', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => receipt());
    await mount({ api: stubApi({ taskAnswer }) });
    await blurThenSubmitEmpty(answerInput(), submitButton());
    expect(taskAnswer).not.toHaveBeenCalled();
  });

  it('counts against the total of the first serve when a later one names none', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>()
      .mockResolvedValueOnce(Q(1))
      .mockResolvedValue(Q(2, { total: null }));
    await mount({ api: stubApi({ taskServe }) });
    await submitAnswer('7');
    expect(document.querySelector('.progress-count')!.textContent).toBe('2 / 3');
  });

  it('lets go of the clock when the quiz is over', async () => {
    vi.useFakeTimers();
    await mount({ task: { ...QUIZ, time_budget_secs: 60 }, api: stubApi({ taskAnswer: async () => completed() }) });
    expect(timer()).not.toBeNull();
    await submitAnswer('7');
    expect(screen.getByText('Quiz complete')).toBeTruthy();
    // Every timeout of the render runs out; an interval left ticking never would.
    await act(async () => { vi.advanceTimersByTime(20_000); });
    expect(vi.getTimerCount()).toBe(0);
  });

  it('serves the next question with an empty field, whatever was typed', async () => {
    const taskServe = threeQuestions();
    await mount({ api: stubApi({ taskServe }) });
    typeAnswer('7');
    await submitAnswer('7');
    expect(answerInput().value).toBe('');
  });
});
