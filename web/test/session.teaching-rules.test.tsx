/**
 * The teaching rules of D-L5 in the study loop: the give-up control and the lesson page
 * after two misses in a row.
 */
import { describe, expect, it, vi } from 'vitest';
import { screen } from '@testing-library/react';
import {
  DRILL, P, REVIEW, graded, mount, planOf, press, stubApi, submitAnswer,
} from './helpers/session';
import type { ApiClient, TopicTeachResponse } from '@/api/types';

const GIVE_UP = "I don't know. Show the solution.";

const PAGE: TopicTeachResponse = {
  topic: 'fractions',
  name: 'Fractions',
  module: 'Arithmetic',
  course: 'c',
  course_name: 'Course',
  position: 1,
  count: 1,
  previous: null,
  next: null,
  learn_first: [],
  opens_next: [],
  points: [
    {
      kp: 'kp-simplify',
      name: 'Simplify',
      page: {
        concept: 'Divide both parts by the same factor.',
        worked_example: { problem: 'Simplify $\\frac{4}{6}$.', steps: ['Divide both parts by 2.'] },
      },
    },
    { kp: 'kp-other', name: 'Other point', page: null },
  ],
};

describe('the give-up control', () => {
  it('shows on a review problem', async () => {
    const view = await mount({ plan: planOf(REVIEW) });
    expect(screen.queryByRole('button', { name: GIVE_UP })).not.toBeNull();
    view.unmount();
  });

  it('does not show on a drill problem', async () => {
    const view = await mount({ plan: planOf(DRILL) });
    expect(screen.queryByRole('button', { name: GIVE_UP })).toBeNull();
    view.unmount();
  });

  it('sends no typed text, sends gave_up, and shows the solution', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () =>
      graded({ correct: false, error_tags: ['gave_up'], solution: 'Divide both parts by 2.' }));
    const view = await mount({ api: stubApi({ taskAnswer }), plan: planOf(REVIEW) });
    await press(GIVE_UP);
    expect(taskAnswer).toHaveBeenCalledWith('t-review', { problem_id: 'p1', answer: '', gave_up: true });
    expect(screen.getByText(/Divide both parts by 2\./)).toBeTruthy();
    view.unmount();
  });

  it('sends no gave_up flag with a typed answer', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => graded());
    const view = await mount({ api: stubApi({ taskAnswer }), plan: planOf(REVIEW) });
    await submitAnswer('3/4');
    expect(taskAnswer).toHaveBeenCalledWith('t-review', { problem_id: 'p1', answer: '3/4' });
    view.unmount();
  });
});

describe('the lesson page after two misses in a row', () => {
  const second = () => graded({
    correct: false,
    teach_first: { topic: 'fractions', kp: 'kp-simplify' },
    next: P(3),
    feedback_practice: true,
  });

  it('opens the page of the missed point before the next problem', async () => {
    const topicTeach = vi.fn<ApiClient['topicTeach']>(async () => PAGE);
    const api = stubApi({ taskAnswer: async () => second(), topicTeach });
    const view = await mount({ api, plan: planOf(REVIEW) });
    await submitAnswer('1');
    await press('Read the lesson page →');
    expect(topicTeach).toHaveBeenCalledWith('fractions');
    expect(await screen.findByText('Divide both parts by the same factor.')).toBeTruthy();
    expect(screen.getByText('Next: a new problem on this skill.')).toBeTruthy();
    // Only the missed point shows.
    expect(screen.queryByText('Other point')).toBeNull();
    view.unmount();
  });

  it('goes on to the practice problem after the learner presses Continue', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    const api = stubApi({
      taskAnswer: async () => second(),
      topicTeach: async () => PAGE,
      taskServe,
    });
    const view = await mount({ api, plan: planOf(REVIEW) });
    await submitAnswer('1');
    await press('Read the lesson page →');
    await screen.findByText('Divide both parts by the same factor.');
    taskServe.mockClear();
    await press('Continue');
    expect(taskServe).toHaveBeenCalledTimes(1);
    expect(screen.queryByText('Divide both parts by the same factor.')).toBeNull();
    view.unmount();
  });

  it('never blocks the learner when the page does not load', async () => {
    const api = stubApi({
      taskAnswer: async () => second(),
      topicTeach: async () => { throw new Error('no page'); },
    });
    const view = await mount({ api, plan: planOf(REVIEW) });
    await submitAnswer('1');
    await press('Read the lesson page →');
    expect(await screen.findByText('There is no lesson text for this skill yet.')).toBeTruthy();
    await press('Continue');
    expect(screen.queryByText('There is no lesson text for this skill yet.')).toBeNull();
    view.unmount();
  });

  it('says the next screen is a prerequisite question when the reply queues one', async () => {
    const api = stubApi({
      taskAnswer: async () => graded({
        correct: false,
        teach_first: { topic: 'fractions', kp: 'kp-simplify' },
        remediation: [{ kind: 'in_sitting_prereq', targets: ['division'] }],
      }),
      topicTeach: async () => PAGE,
    });
    const view = await mount({ api, plan: planOf(REVIEW) });
    await submitAnswer('1');
    await press('Read the lesson page →');
    expect(await screen.findByText('Next: one question on a skill that this one builds on.')).toBeTruthy();
    view.unmount();
  });

  it('shows no page after one miss', async () => {
    const topicTeach = vi.fn<ApiClient['topicTeach']>(async () => PAGE);
    const api = stubApi({ taskAnswer: async () => graded({ correct: false }), topicTeach });
    const view = await mount({ api, plan: planOf(REVIEW) });
    await submitAnswer('1');
    await press(/^(Next problem|Try a fresh problem|Continue)/);
    expect(topicTeach).not.toHaveBeenCalled();
    view.unmount();
  });
});
