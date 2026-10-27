/**
 * The read page: the lesson text of one topic, opened outside a task (D-TR1).
 */
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, screen } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { ApiError } from '@/api';
import { TopicRead, READ_NOT_FOUND, READ_NO_PAGE } from '@/views/read/TopicRead';
import { AXE_IN_JSDOM } from './axe';
import { renderInView } from './helpers/render';
import { stubApi } from './helpers/session';
import type { ApiClient, TopicTeachResponse } from '@/api/types';

const PAGE: TopicTeachResponse = {
  topic: 'ratios',
  name: 'Ratios',
  module: 'Arithmetic',
  course: 'c1',
  course_name: 'Foundations',
  position: 2,
  count: 3,
  previous: { id: 'fractions', name: 'Fractions' },
  next: { id: 'rates', name: 'Rates' },
  learn_first: [{ id: 'fractions', name: 'Fractions' }],
  opens_next: [{ id: 'rates', name: 'Rates' }, { id: 'scaling', name: null }],
  points: [
    {
      kp: 'kp1',
      name: 'Compare two amounts',
      page: {
        concept: 'A ratio compares two amounts.',
        worked_example: { problem: 'Write 2 to 4 as a ratio.', steps: ['2:4 = 1:2.'] },
      },
    },
    { kp: 'kp2', name: 'Equal ratios', page: null },
  ],
};

async function mount(topicTeach: ApiClient['topicTeach'], over: { onOpen?: () => void; onExit?: () => void } = {}) {
  const handlers = { onOpen: vi.fn(), onExit: vi.fn(), onUnauthorized: vi.fn(), ...over };
  const view = await renderInView(
    <TopicRead api={stubApi({ topicTeach })} demo={false} topic="ratios" backLabel="Back to the map" {...handlers} />,
  );
  await act(async () => { await Promise.resolve(); });
  return { ...view, ...handlers };
}

describe('the read page', () => {
  it('shows the position, the neighbor buttons and both link lists', async () => {
    const view = await mount(async () => PAGE);
    expect(screen.getByText('Topic 2 of 3 in Foundations')).toBeTruthy();
    expect(screen.getByRole('button', { name: '← Previous: Fractions' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Next: Rates →' })).toBeTruthy();
    expect(screen.getByText('Learn first')).toBeTruthy();
    expect(screen.getByText('Opens next')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'scaling' })).toBeTruthy();
    expect(screen.getByText('A ratio compares two amounts.')).toBeTruthy();
    expect(screen.queryByLabelText('Answer')).toBeNull();
    view.unmount();
  });

  it('opens a neighbor by its id, and leaves with Back', async () => {
    const view = await mount(async () => PAGE);
    fireEvent.click(screen.getByRole('button', { name: 'Next: Rates →' }));
    expect(view.onOpen).toHaveBeenCalledWith('rates');
    fireEvent.click(screen.getByRole('button', { name: 'Back to the map' }));
    expect(view.onExit).toHaveBeenCalledTimes(1);
    view.unmount();
  });

  it('hides a missing neighbor and an empty list', async () => {
    const view = await mount(async () => ({ ...PAGE, previous: null, next: null, learn_first: [] }));
    expect(screen.queryByRole('button', { name: /Previous/ })).toBeNull();
    expect(screen.queryByRole('button', { name: /^Next/ })).toBeNull();
    expect(screen.queryByText('Learn first')).toBeNull();
    view.unmount();
  });

  it('says so for a point with no lesson text', async () => {
    const view = await mount(async () => PAGE);
    expect(screen.getByText(READ_NO_PAGE)).toBeTruthy();
    expect(READ_NO_PAGE).toBe('This step has no lesson text yet.');
    view.unmount();
  });

  it('answers a 404 with one plain line', async () => {
    const view = await mount(async () => { throw new ApiError(404, 'not_found', 'No topic "x" is in the curriculum.'); });
    expect(screen.getByText(READ_NOT_FOUND)).toBeTruthy();
    expect(READ_NOT_FOUND).toBe('This topic does not exist. Go back and pick another.');
    view.unmount();
  });

  it('passes the axe check', async () => {
    const view = await mount(async () => PAGE);
    expect(await axe(document.getElementById('view')!, AXE_IN_JSDOM)).toHaveNoViolations();
    view.unmount();
  });
});

describe('the optional parts of a read page', () => {
  it('prints the picture, the limit, the second example and the words in the order of the teach page', async () => {
    const full: TopicTeachResponse = {
      ...PAGE,
      points: [{
        kp: 'kp1',
        name: 'Compare two amounts',
        page: {
          concept: 'A ratio compares two amounts.',
          worked_example: { problem: 'Write 2 to 4 as a ratio.', steps: ['2:4 = 1:2.'] },
          picture: 'Two cups of juice to four cups of water.',
          watch_out: 'The order of the two amounts matters.',
          second_example: { problem: 'Read the ratio 3:5.', steps: ['3 parts to 5 parts.'] },
          terms: [{ term: 'Ratio', meaning: 'A comparison of two amounts.' }],
        },
      }],
    };
    await mount(async () => full);
    const labels = ['An everyday picture', 'A ratio compares two amounts.', 'Where the method fails', 'A second example: what a result means', 'Words used here'];
    const nodes = labels.map((text) => screen.getByText(text));
    for (let i = 1; i < nodes.length; i += 1) {
      expect(Boolean(nodes[i - 1]!.compareDocumentPosition(nodes[i]!) & Node.DOCUMENT_POSITION_FOLLOWING)).toBe(true);
    }
    expect(screen.getByText('Ratio')).toBeTruthy();
  });

  it('prints none of them for a page without the parts', async () => {
    await mount(async () => PAGE);
    expect(screen.queryByText('Words used here')).toBeNull();
    expect(screen.queryByText('An everyday picture')).toBeNull();
  });
});
