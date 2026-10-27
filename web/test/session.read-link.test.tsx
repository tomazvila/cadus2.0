/**
 * The link from a missed problem to the lesson text of its topic (D-TR1).
 */
import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { Feedback } from '@/views/session/Feedback';
import { REVIEW, graded, mount, planOf, press, stubApi, submitAnswer } from './helpers/session';
import type { PlanTask } from '@/api/types';

const TOPIC = { id: 'fractions', name: 'Fractions' };
const noop = vi.fn();

describe('the Read the lesson button', () => {
  it('shows after a miss and opens the lesson text by topic id', () => {
    const onRead = vi.fn();
    render(<Feedback res={graded({ correct: false })} hasNext onContinue={noop} onEnd={noop} lessonTopic={TOPIC} onRead={onRead} />);
    fireEvent.click(screen.getByRole('button', { name: 'Read the lesson: Fractions' }));
    expect(onRead).toHaveBeenCalledWith('fractions');
  });

  it('is absent after a correct answer', () => {
    render(<Feedback res={graded()} hasNext onContinue={noop} onEnd={noop} lessonTopic={TOPIC} onRead={vi.fn()} />);
    expect(screen.queryByRole('button', { name: /Read the lesson/ })).toBeNull();
  });

  it('is absent after an ungraded answer, which is not a miss', () => {
    render(<Feedback res={graded({ outcome: 'ungraded', correct: false })} hasNext onContinue={noop} onEnd={noop} lessonTopic={TOPIC} onRead={vi.fn()} />);
    expect(screen.queryByRole('button', { name: /Read the lesson/ })).toBeNull();
  });

  it('is absent when the task has no topic', () => {
    render(<Feedback res={graded({ correct: false })} hasNext onContinue={noop} onEnd={noop} lessonTopic={null} onRead={vi.fn()} />);
    expect(screen.queryByRole('button', { name: /Read the lesson/ })).toBeNull();
  });

  it('works inside the session with the topic of the planned task', async () => {
    const api = stubApi({ taskAnswer: async () => graded({ correct: false }) });
    const view = await mount({ api, plan: planOf(REVIEW) });
    await submitAnswer('1');
    await press('Read the lesson: Fractions');
    expect(view.onRead).toHaveBeenCalledWith('fractions');
    view.unmount();
  });

  it('is absent in a session whose task has no topic', async () => {
    const task: PlanTask = { ...REVIEW, topic: null };
    const api = stubApi({ taskAnswer: async () => graded({ correct: false }) });
    const view = await mount({ api, plan: planOf(task) });
    await submitAnswer('1');
    expect(screen.queryByRole('button', { name: /Read the lesson/ })).toBeNull();
    view.unmount();
  });
});
