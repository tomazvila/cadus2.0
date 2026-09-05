/**
 * The study loop (S8).
 *
 * Six invariants are claimed here, and every one of them is a NEGATIVE — the reason the
 * invariant gate exists beside a coverage floor:
 *
 *   F-37-1c   Enter during grading posts nothing.
 *   F-F2-2    a slow `session/start` never yanks the learner out of another view.
 *   DD-3/P1   an assisted pass returns to `ready`; the unaided re-solve locks it in.
 *   W-A4      the reference lesson is shown once.
 *   W-C5      Submit is dominant; the rest stays quiet.
 *   NO-2BILL  no view mount issues two writes.
 *
 * Every fixture is the frozen contract of `docs/reference/web-service-1.0-spec.md`
 * (`test/helpers/session.tsx`), so each assertion is a literal a reader checks by hand:
 * three problems in the task, the progress count `1 / 3`, the clock `0:00`, the
 * auto-advance at 1400 ms. This part holds the loop, the gate, the re-solve and the hints;
 * `session.retry.test.tsx` and `session.advance.test.tsx` hold the rest.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, screen } from '@testing-library/react';
import { networkFailure } from './helpers/api';
import {
  REVIEW, REWORK, P, answerInput, graded, mount, planOf, press, progressCount, stubApi,
  submitAnswer, submitButton, timer, typeAnswer, workInput,
} from './helpers/session';
import type { AnswerResponse, ApiClient, PlanTask } from '@/api/types';

describe('the study loop', () => {
  it('serves the first task and paints the problem, the count and the clock', async () => {
    await mount();

    expect(screen.getByText('review')).toBeTruthy();
    expect(screen.getByText('Fractions')).toBeTruthy();
    expect(screen.getByText('due for review')).toBeTruthy();
    expect(progressCount()).toBe('1 / 3');
    expect(timer().textContent).toBe('0:00');
    expect(document.querySelector('.problem-text')!.textContent).toContain('Simplify');
  });

  it('reads the plan itself when the caller hands none over', async () => {
    const getPlan = vi.fn<ApiClient['getPlan']>(async () => planOf(REVIEW));
    await mount({ plan: undefined, api: stubApi({ getPlan }) });

    expect(getPlan).toHaveBeenCalledTimes(1);
    expect(document.querySelector('.problem-text')).toBeTruthy();
  });

  it('offers the placement when the plan is empty, so the screen is no dead end', async () => {
    const { onDiagnostic, onExit } = await mount({ plan: planOf() });

    expect(screen.getByText('Nothing is due right now — enjoy the break.')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Take the placement diagnostic' }));
    expect(onDiagnostic).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole('button', { name: 'Back to dashboard' }));
    expect(onExit).toHaveBeenCalledTimes(1);
  });

  it('hands a quiz task to the quiz screen and serves nothing', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    const quiz: PlanTask = { ...REVIEW, task_id: 't-quiz', task_type: 'quiz', topic: null };
    const { onQuiz } = await mount({ plan: planOf(quiz), api: stubApi({ taskServe }) });

    expect(onQuiz).toHaveBeenCalledTimes(1);
    expect(onQuiz.mock.calls[0][0]).toEqual(quiz);
    // WITH the task, never the id alone: the quiz clock reads the task budget.
    expect(taskServe).not.toHaveBeenCalled();
  });

  it('posts the answer and the working, then paints the verdict and the solution', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => graded({ correct: false, next: null, error_tags: ['sign-error'], work_quality: 'passable' }));
    await mount({ api: stubApi({ taskAnswer }) });

    typeAnswer('3/4');
    fireEvent.change(workInput(), { target: { value: 'divide by two' } });
    await act(async () => { fireEvent.click(submitButton()); });

    expect(taskAnswer).toHaveBeenCalledTimes(1);
    expect(taskAnswer.mock.calls[0]).toEqual([
      't-review',
      { problem_id: 'p1', answer: '3/4', work: 'divide by two' },
    ]);
    expect(screen.getByText('Not quite')).toBeTruthy();
    expect(screen.getByText('sign-error')).toBeTruthy();
    expect(screen.getByText('Divide both parts by 2.')).toBeTruthy();
    // Hard Rule 2: the structural verdict and the work quality are both shown, and neither
    // is derived from the other.
    expect(screen.getByText('passable')).toBeTruthy();
  });

  it('an empty answer posts nothing and returns the focus to the field', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => graded());
    await mount({ api: stubApi({ taskAnswer }) });
    answerInput().blur();
    expect(document.activeElement).not.toBe(answerInput());

    await act(async () => { fireEvent.click(submitButton()); });

    expect(taskAnswer).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(answerInput());
  });

  it('a failed grade returns the problem to the learner instead of locking the card', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => { throw networkFailure(); });
    await mount({ api: stubApi({ taskAnswer }) });

    await submitAnswer('3/4');

    expect(taskAnswer).toHaveBeenCalledTimes(1);
    expect(submitButton().hasAttribute('disabled')).toBe(false);
    await submitAnswer('3/4');
    expect(taskAnswer).toHaveBeenCalledTimes(2);
  });
});

describe('F-37-1c: the phase gate', () => {
  it('F-37-1c: Enter during grading posts nothing', async () => {
    let release!: (value: AnswerResponse) => void;
    const pending = new Promise<AnswerResponse>((r) => { release = r; });
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(() => pending);
    await mount({ api: stubApi({ taskAnswer }) });

    typeAnswer('3/4');
    fireEvent.keyDown(answerInput(), { key: 'Enter' });
    expect(taskAnswer).toHaveBeenCalledTimes(1);

    // The field stays enabled while the grade runs — the gate, not the attribute, is what
    // stops the second post. Three more Enters, and a click, land inside the window.
    expect(answerInput().disabled).toBe(false);
    fireEvent.keyDown(answerInput(), { key: 'Enter' });
    fireEvent.keyDown(answerInput(), { key: 'Enter' });
    fireEvent.keyDown(workInput(), { key: 'Enter' });
    fireEvent.click(submitButton());
    expect(taskAnswer).toHaveBeenCalledTimes(1);

    await act(async () => { release(graded()); });
    expect(taskAnswer).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Correct')).toBeTruthy();
  });

  it('F-37-1c: Enter after the verdict posts nothing more', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => graded({ next: null }));
    await mount({ api: stubApi({ taskAnswer }) });

    typeAnswer('3/4');
    await act(async () => { fireEvent.keyDown(answerInput(), { key: 'Enter' }); });
    expect(screen.getByText('Correct')).toBeTruthy();

    fireEvent.keyDown(answerInput(), { key: 'Enter' });
    expect(taskAnswer).toHaveBeenCalledTimes(1);
  });

  it('F-37-1c: a hint asked for during grading is dropped', async () => {
    let release!: (value: AnswerResponse) => void;
    const pending = new Promise<AnswerResponse>((r) => { release = r; });
    const taskHint = vi.fn<ApiClient['taskHint']>(async () => ({ hint: 'Look at the factors.', hint_number: 1 }));
    await mount({ api: stubApi({ taskAnswer: () => pending, taskHint }) });

    typeAnswer('3/4');
    fireEvent.keyDown(answerInput(), { key: 'Enter' });
    // The `H` key reaches the field while the buttons are disabled.
    fireEvent.change(answerInput(), { target: { value: '' } });
    fireEvent.keyDown(answerInput(), { key: 'h' });

    expect(taskHint).not.toHaveBeenCalled();
    await act(async () => { release(graded()); });
  });
});

describe('DD-3/P1: the re-solve', () => {
  it('DD-3/P1: an assisted pass returns to ready and the same submit sends the re-solve', async () => {
    const replies = [REWORK, graded({ next: null, task_status: 'task_passed' })];
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => replies.shift()!);
    await mount({ api: stubApi({ taskAnswer }) });

    await submitAnswer('3/4');

    // Not terminal: the problem is still on screen, the field is cleared, and Submit is live.
    expect(screen.getByText('Make it stick')).toBeTruthy();
    expect(screen.getByText(REWORK.re_solve)).toBeTruthy();
    expect(screen.getByText('Divide both parts by 2.')).toBeTruthy();
    expect(answerInput().value).toBe('');
    expect(submitButton().hasAttribute('disabled')).toBe(false);
    expect(screen.queryByRole('button', { name: 'Next problem →' })).toBeNull();

    // The SAME submit, and the SAME problem id.
    await submitAnswer('3/4');

    expect(taskAnswer).toHaveBeenCalledTimes(2);
    expect(taskAnswer.mock.calls[1][1].problem_id).toBe('p1');
    expect(taskAnswer.mock.calls[1][1]).not.toHaveProperty('assisted');
    expect(screen.getByText('Correct')).toBeTruthy();
    expect(screen.queryByText('Make it stick')).toBeNull();
  });

  it('DD-3/P1: the re-solve panel arms no auto-advance', async () => {
    vi.useFakeTimers();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => REWORK);
    await mount({ api: stubApi({ taskAnswer }) });

    await submitAnswer('3/4');
    await act(async () => { vi.advanceTimersByTime(5000); });

    expect(taskAnswer).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Make it stick')).toBeTruthy();
  });
});

describe('the hint ladder', () => {
  it('a hint body never contains expected', async () => {
    const taskHint = vi.fn<ApiClient['taskHint']>(async () => ({ hint: 'Find the common factor.', hint_number: 1 }));
    await mount({ api: stubApi({ taskHint }) });

    await press('Hint');

    expect(taskHint.mock.calls[0]).toEqual(['t-review', 'p1']);
    expect(screen.getByText('Hint 1:')).toBeTruthy();
    expect(screen.getByText('Find the common factor.')).toBeTruthy();
    // The expected answer of this problem reaches the screen only from a grade reply.
    expect(document.body.textContent).not.toContain(REWORK.expected);
    expect(document.body.textContent).not.toContain('Divide both parts by 2.');
  });

  it('W-A4: the reference lesson is shown once', async () => {
    const reference = { topic: 'fractions', name: 'Simplifying fractions' };
    let n = 0;
    const taskHint = vi.fn<ApiClient['taskHint']>(async () => {
      n += 1;
      return { hint: `Hint body ${n}`, hint_number: n, reference_lesson: reference };
    });
    await mount({ api: stubApi({ taskHint }) });

    await press('Hint');
    await press('Hint');
    await press('Hint');

    expect(taskHint).toHaveBeenCalledTimes(3);
    expect(document.querySelectorAll('.hint').length).toBe(3);
    expect(document.querySelectorAll('.reference-lesson').length).toBe(1);
    expect(document.querySelector('.reference-lesson')!.textContent).toContain(
      'Simplifying fractions',
    );
  });

  it('W-C5: Submit is dominant; Hint and Exit stay quiet', async () => {
    await mount();

    const primaries = document.querySelectorAll('.view-session .btn-primary');
    expect(primaries.length).toBe(1);
    expect(primaries[0].textContent).toBe('Submit');
    expect(screen.getByRole('button', { name: 'Hint' }).className).toContain('btn-ghost');
    expect(screen.getByRole('button', { name: 'Exit' }).className).toContain('btn-ghost');
  });
});
