/**
 * D-QZ1: the learner picks a length and a scope, sees the ready card, and starts the clock
 * with one press. No serve call happens before that press.
 */
import { act, fireEvent, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { createDemoApi } from '@/api';
import { Quiz } from '@/views/Quiz';
import { renderInView } from './helpers/render';
import { mount, primaries, stubApi as dashboardApi } from './helpers/dashboard';
import { Q, QUIZ, stubApi } from './helpers/quiz';
import type { ApiClient, QuizRound, QuizRoundResponse } from '@/api/types';

const pressed = (name: string): string | null =>
  screen.getByRole('button', { name }).getAttribute('aria-pressed');

describe('the quiz control of the More menu', () => {
  it('holds the heading, the lengths and the scopes, with 10 and the course pressed', async () => {
    await mount();
    await userEvent.click(screen.getByText(/^More/));
    expect(screen.getByText('Timed quiz')).toBeTruthy();
    expect(screen.getByText('How many questions')).toBeTruthy();
    expect(screen.getByText('Take questions from')).toBeTruthy();
    expect(pressed('5')).toBe('false');
    expect(pressed('10')).toBe('true');
    expect(pressed('20')).toBe('false');
    expect(pressed('Foundations')).toBe('true');
    expect(pressed('All my courses')).toBe('false');
    expect(primaries().length).toBeLessThanOrEqual(1);
  });

  it('asks for the picked round once, then hands it over', async () => {
    const demo = createDemoApi();
    const reply = await demo.startQuizRound(20, 'all');
    const startQuizRound = vi.fn<ApiClient['startQuizRound']>().mockResolvedValue(reply);
    const view = await mount({ api: dashboardApi({ startQuizRound }) });
    await userEvent.click(screen.getByText(/^More/));
    await userEvent.click(screen.getByRole('button', { name: '20' }));
    await userEvent.click(screen.getByRole('button', { name: 'All my courses' }));
    await userEvent.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(view.onQuiz).toHaveBeenCalledTimes(1));
    expect(startQuizRound).toHaveBeenCalledTimes(1);
    expect(startQuizRound).toHaveBeenCalledWith(20, 'all');
    expect(view.onQuiz).toHaveBeenCalledWith(reply.task, reply.round);
  });
});

describe('the ready card of a round', () => {
  const round: QuizRound = { length: 10, n: 5, scope: 'all', course_name: null, target_secs: 450 };
  const task: QuizRoundResponse['task'] = { ...QUIZ, task_id: 't-round', n_problems: 5, time_budget_secs: 450 };

  it('shows the count and the target time and serves nothing before the press', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>().mockResolvedValue(Q(1, { total: 5 }));
    await renderInView(
      <Quiz api={stubApi({ taskServe })} task={task} round={round} demo={false} onUnauthorized={vi.fn()} onDone={vi.fn()} />,
    );
    expect(screen.getByText('5 questions, all my courses.')).toBeTruthy();
    expect(screen.getByText('Target time: 8 minutes.')).toBeTruthy();
    expect(screen.getByText('The clock starts when you press Start the quiz. You see no feedback until the end.')).toBeTruthy();
    expect(taskServe).not.toHaveBeenCalled();
    expect(document.querySelectorAll('.btn-primary').length).toBe(1);

    await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Start the quiz' })); });
    expect(taskServe).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Question 1.')).toBeTruthy();
    expect(document.querySelector('.timer')).toBeTruthy();
  });

  it('names the course of a course round', async () => {
    const named: QuizRound = { ...round, scope: 'course', course_name: 'Foundations', n: 12 };
    await renderInView(
      <Quiz api={stubApi()} task={task} round={named} demo={false} onUnauthorized={vi.fn()} onDone={vi.fn()} />,
    );
    expect(screen.getByText('12 questions, Foundations.')).toBeTruthy();
  });

  it('says "1 minute" for a one-minute target and "my course" for an unnamed course', async () => {
    const short: QuizRound = { ...round, scope: 'course', course_name: null, target_secs: 40 };
    await renderInView(
      <Quiz api={stubApi()} task={task} round={short} demo={false} onUnauthorized={vi.fn()} onDone={vi.fn()} />,
    );
    expect(screen.getByText('5 questions, my course.')).toBeTruthy();
    expect(screen.getByText('Target time: 1 minute.')).toBeTruthy();
  });

  it('skips the ready card for a resumed round and serves the question at once', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>().mockResolvedValue(Q(2, { total: 5 }));
    await renderInView(
      <Quiz api={stubApi({ taskServe })} task={task} round={round} resumed demo={false} onUnauthorized={vi.fn()} onDone={vi.fn()} />,
    );
    await waitFor(() => expect(screen.getByText('Question 2.')).toBeTruthy());
    expect(screen.queryByRole('button', { name: 'Start the quiz' })).toBeNull();
    expect(taskServe).toHaveBeenCalledTimes(1);
  });
});
