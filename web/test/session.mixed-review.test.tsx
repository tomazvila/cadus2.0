/**
 * The study loop on reviews: no topic before the answer, and the mixed review block that
 * serves the questions of several reviews interleaved.
 *
 * The fixtures live in `test/helpers/session.tsx`.
 */
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import {
  LESSON, REVIEW, TEACHING, P, clickNext, graded, mount, planOf, press, progressCount,
  stubApi, submitAnswer,
} from './helpers/session';
import type { ApiClient, PlanTask } from '@/api/types';

const DECIMALS: PlanTask = {
  ...REVIEW,
  task_id: 't-review-2',
  topic: { id: 'decimals', name: 'Decimals', module: 'Arithmetic' },
};

const topicName = () => document.querySelector('.topic-name')!.textContent;

describe('a review hides its topic until the answer is graded', () => {
  it('names neither the topic nor the module on the problem, and names the topic in the feedback', async () => {
    await mount();

    expect(topicName()).toBe('Review');
    expect(screen.queryByText('Fractions')).toBeNull();
    expect(screen.queryByText('Arithmetic')).toBeNull();
    expect(screen.queryByText(/Topic:/)).toBeNull();

    await submitAnswer('3/4');

    expect(screen.getByText('Correct')).toBeTruthy();
    expect(screen.getByText('Topic: Fractions')).toBeTruthy();
  });

  it('a lesson still names its topic on the problem', async () => {
    await mount({
      plan: planOf(LESSON),
      api: stubApi({ taskTeach: async () => TEACHING }),
    });
    expect(topicName()).toBe('Fractions');
  });
});

describe('the mixed review block', () => {
  it('answers each problem on the task it belongs to and serves nothing twice', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>(async () =>
      P(1, { task_id: 't-review-2', mixed_review: { position: 1, total: 6 } }));
    const replies = [
      graded({ next: P(2, { task_id: 't-review', mixed_review: { position: 2, total: 6 } }) }),
      graded({ attempt_id: 'a-2', next: null }),
    ];
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => replies.shift()!);
    const taskTeach = vi.fn<ApiClient['taskTeach']>(async () => TEACHING);
    await mount({
      plan: planOf(REVIEW, DECIMALS, { ...LESSON, topic: { ...LESSON.topic!, name: 'Ratios' } }),
      api: stubApi({ taskServe, taskAnswer, taskTeach }),
    });

    // The serve of the first review handed back the second review's problem.
    expect(taskServe).toHaveBeenCalledTimes(1);
    expect(taskServe.mock.calls[0]).toEqual(['t-review']);
    expect(topicName()).toBe('Mixed review');
    expect(progressCount()).toBe('1 / 6 · 5 left');
    expect(screen.queryByText('Decimals')).toBeNull();

    await submitAnswer('1');
    expect(taskAnswer.mock.calls[0]![0]).toBe('t-review-2');
    expect(screen.getByText('Topic: Decimals')).toBeTruthy();

    // The grade handed on to the first review: shown at once, with no second serve.
    expect(await clickNext()).toBe('2 / 6 · 4 left');
    expect(taskServe).toHaveBeenCalledTimes(1);
    expect(screen.queryByText(/Topic:/)).toBeNull();

    await submitAnswer('2');
    expect(taskAnswer.mock.calls[1]![0]).toBe('t-review');
    expect(screen.getByText('Topic: Fractions')).toBeTruthy();

    // The block handed back no next problem: both reviews are finished, and the loop
    // moves to the lesson after them without serving either review again.
    await press('Continue →');
    await waitFor(() => expect(taskTeach).toHaveBeenCalledTimes(1));
    expect(taskTeach.mock.calls[0]).toEqual(['t-lesson']);
    expect(taskServe).toHaveBeenCalledTimes(1);
  });

  it('a lone review keeps its own count', async () => {
    await mount();
    expect(topicName()).toBe('Review');
    expect(progressCount()).toBe('1 / 3 · 2 left');
  });

  it('a review served with no total shows no count, so a one-question probe looks the same', async () => {
    // The service sends no per-task total for a review (D-F11).
    await mount({ api: stubApi({ taskServe: async () => P(1, { total: null }) }) });
    expect(topicName()).toBe('Review');
    expect(progressCount()).toBe('Review');
  });

  it('a drill served with no total still shows its question number', async () => {
    await mount({
      plan: planOf({ ...LESSON, task_type: 'drill' }),
      api: stubApi({ taskServe: async () => P(2, { total: null }) }),
    });
    expect(progressCount()).toBe('Question 2');
  });
});
