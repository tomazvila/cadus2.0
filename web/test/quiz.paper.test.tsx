/**
 * D-VBE3: a course with exam weights offers the skills below it as a third scope and one
 * start button for each exam set. The result of a paper reads "<earned> of <total> points".
 */
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { createDemoApi } from '@/api';
import { QuizResults } from '@/views/QuizResults';
import { paperScoreLine, roundPlace, startSetLabel } from '@/views/roundLabel';
import { mount, status, stubApi } from './helpers/dashboard';
import type { ApiClient, QuizRoundResponse } from '@/api/types';

const pressed = (name: string): string | null =>
  screen.getByRole('button', { name }).getAttribute('aria-pressed');

const WEIGHTED = status({
  courses: [
    {
      id: 'vbe-math', name: 'Lithuanian Matura Math', current: true,
      exam_sets: [{ id: 'set1', name: 'Exam set 1: 2025 main paper' }, { id: 'set2', name: 'Mock paper' }],
    },
    { id: 'foundations', name: 'Foundations', current: false },
  ],
});

describe('the quiz control of a course with exam weights', () => {
  it('shows the third scope and one start button for each exam set', async () => {
    await mount({ api: stubApi({ getStatus: async () => WEIGHTED }) });
    await userEvent.click(screen.getByText(/^More/));
    expect(pressed('This course and the skills it builds on')).toBe('false');
    expect(screen.getByRole('button', { name: 'Start Exam set 1: 2025 main paper' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Start exam set Mock paper' })).toBeTruthy();
  });

  it('shows neither for a course without exam weights', async () => {
    await mount();
    await userEvent.click(screen.getByText(/^More/));
    expect(screen.queryByRole('button', { name: 'This course and the skills it builds on' })).toBeNull();
    expect(screen.queryByText('Or take a full exam paper')).toBeNull();
  });

  it('asks for the ancestor scope once', async () => {
    const reply = await createDemoApi().startQuizRound(10, 'all');
    const startQuizRound = vi.fn<ApiClient['startQuizRound']>().mockResolvedValue(reply);
    const view = await mount({ api: stubApi({ getStatus: async () => WEIGHTED, startQuizRound }) });
    await userEvent.click(screen.getByText(/^More/));
    await userEvent.click(screen.getByRole('button', { name: 'This course and the skills it builds on' }));
    await userEvent.click(screen.getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(view.onQuiz).toHaveBeenCalledTimes(1));
    expect(startQuizRound).toHaveBeenCalledWith(10, 'course_and_prereqs');
  });

  it('starts the paper of the set that the learner presses', async () => {
    const reply: QuizRoundResponse = await createDemoApi().startQuizRound(10, 'all');
    const startQuizRound = vi.fn<ApiClient['startQuizRound']>().mockResolvedValue(reply);
    const view = await mount({ api: stubApi({ getStatus: async () => WEIGHTED, startQuizRound }) });
    await userEvent.click(screen.getByText(/^More/));
    await userEvent.click(screen.getByRole('button', { name: 'Start exam set Mock paper' }));
    await waitFor(() => expect(view.onQuiz).toHaveBeenCalledTimes(1));
    expect(startQuizRound).toHaveBeenCalledWith(0, 'paper', 'set2');
  });
});

describe('the words of a round', () => {
  it('names the place and the score of each kind', () => {
    expect(roundPlace({ scope: 'all', course_name: null })).toBe('all my courses');
    expect(roundPlace({ scope: 'course', course_name: null })).toBe('my course');
    expect(roundPlace({ scope: 'course_and_prereqs', course_name: 'Maths' })).toBe('Maths and the skills it builds on');
    expect(roundPlace({ scope: 'paper', course_name: 'Maths', set_name: 'Mock paper' })).toBe('Mock paper');
    expect(roundPlace({ scope: 'paper', course_name: 'Maths', form: 'Set 3' })).toBe('Set 3');
    expect(paperScoreLine(31.67, 59, 53.67)).toBe('31.67 of 59 points (53.67%)');
    expect(startSetLabel(null, 'set9')).toBe('Start exam set set9');
  });
});

describe('the result screen of a paper', () => {
  it('shows the points earned out of the points asked', async () => {
    const api = stubApi({
      taskQuizResult: async () => ({
        round: { length: 20, n: 20, scope: 'paper', course_name: 'Maths', kind: 'paper', set: 'set2', form: 'Mock paper' },
        inconclusive: false, score: 0.5367, xp: 4, points_earned: 31.67, points_total: 59, percent: 53.67,
        practice_pending: false, practice_available: false, answers: [],
      }),
    });
    render(<QuizResults api={api} taskId="t" onUnauthorized={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: 'Review results' }));
    expect(await screen.findByText('31.67 of 59 points (53.67%)')).toBeTruthy();
    expect(screen.getByText('Your quiz result: 20 questions, Mock paper')).toBeTruthy();
    expect(screen.queryByText(/of marked answers right/)).toBeNull();
  });
});
