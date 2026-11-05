/**
 * D-VBE3: a course with exam weights offers the skills below it as a third scope and one
 * start button for each exam set. The result of a paper reads "<earned> of <total> points".
 */
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { createDemoApi } from '@/api';
import { QuizResults } from '@/views/QuizResults';
import { paperScoreLine, roundPlace, setStateLine, startSetLabel } from '@/views/roundLabel';
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

const TWO_PARTS = status({
  courses: [
    {
      id: 'vbe-math', name: 'Lithuanian Matura Math', current: true,
      exam_sets: [
        { id: 'vbe-set1-2025-main', name: 'Exam set 1: 2025 main paper', part: 2, form: 'vbe-set1-2025-main', max_points: 60, target_points: 57, points: 41 },
        { id: 'vbe-set1-2025-main-i', name: 'Exam set 1: 2025 main paper', part: 1, form: 'vbe-set1-2025-main', max_points: 40, target_points: 38, points: null },
      ],
    },
  ],
});

describe('the two parts of an exam form', () => {
  it('shows part I before part II, each with its state', async () => {
    await mount({ api: stubApi({ getStatus: async () => TWO_PARTS }) });
    await userEvent.click(screen.getByText(/^More/));
    const one = screen.getByRole('button', { name: 'Start Exam set 1: 2025 main paper, part I' });
    const two = screen.getByRole('button', { name: 'Start Exam set 1: 2025 main paper, part II' });
    expect(one.compareDocumentPosition(two) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByText('Not started yet.')).toBeTruthy();
    expect(screen.getByText('Last paper: 41 of 60 points. Aim for 57 or more.')).toBeTruthy();
  });

  it('starts the paper of part I by its set id', async () => {
    const reply: QuizRoundResponse = await createDemoApi().startQuizRound(10, 'all');
    const startQuizRound = vi.fn<ApiClient['startQuizRound']>().mockResolvedValue(reply);
    const view = await mount({ api: stubApi({ getStatus: async () => TWO_PARTS, startQuizRound }) });
    await userEvent.click(screen.getByText(/^More/));
    await userEvent.click(screen.getByRole('button', { name: 'Start Exam set 1: 2025 main paper, part I' }));
    await waitFor(() => expect(view.onQuiz).toHaveBeenCalledTimes(1));
    expect(startQuizRound).toHaveBeenCalledWith(0, 'paper', 'vbe-set1-2025-main-i');
  });

  it('does not repeat a part that the set name already holds', () => {
    expect(startSetLabel('Mock paper, part II', 'x', 2)).toBe('Start exam set Mock paper, part II');
  });

  it('names a state a stranger can predict', () => {
    expect(setStateLine(null, 40, 38)).toBe('Not started yet.');
    expect(setStateLine(38, 40, 38)).toBe('Last paper: 38 of 40 points. Done.');
    expect(setStateLine(30, 40, 38)).toBe('Last paper: 30 of 40 points. Aim for 38 or more.');
  });
});

describe('the exam line of a result', () => {
  const result = (exam: NonNullable<Awaited<ReturnType<ApiClient['taskQuizResult']>>['exam']>) => stubApi({
    taskQuizResult: async () => ({
      round: { length: 20, n: 20, scope: 'paper', course_name: 'Maths', kind: 'paper', set: 'vbe-set1-2025-main', form: 'Exam set 1' },
      inconclusive: false, score: 0.9, xp: 4, points_earned: 54, points_total: 60, percent: 90, exam,
      practice_pending: false, practice_available: false, answers: [],
    }),
  });

  it('shows the exam score above the points of the paper', async () => {
    const line = 'Exam points 73 of 100. Score 78.';
    render(<QuizResults api={result({ part: 2, part_points: 40, part_of: 60, exam_points: 73, score: 78, passed: true, line })} taskId="t" onUnauthorized={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: 'Review results' }));
    const exam = await screen.findByText(line);
    const points = screen.getByText('54 of 60 points (90%)');
    expect(exam.compareDocumentPosition(points) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('names the open part when one part is done', async () => {
    const line = 'Part I: 38 of 40 points. Part II is still open.';
    render(<QuizResults api={result({ part: 1, part_points: 38, part_of: 40, open_part: 2, line })} taskId="t" onUnauthorized={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: 'Review results' }));
    expect(await screen.findByText(line)).toBeTruthy();
  });

  it('shows no exam line for a paper with no other part', async () => {
    const api = stubApi({
      taskQuizResult: async () => ({
        round: { length: 20, n: 20, scope: 'paper', course_name: 'Maths', kind: 'paper', set: 'set2', form: 'Mock paper' },
        inconclusive: false, score: 0.5, xp: 4, points_earned: 30, points_total: 60, percent: 50, exam: null,
        practice_pending: false, practice_available: false, answers: [],
      }),
    });
    render(<QuizResults api={api} taskId="t" onUnauthorized={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: 'Review results' }));
    await screen.findByText('30 of 60 points (50%)');
    expect(document.querySelector('.exam-line')).toBeNull();
  });
});
