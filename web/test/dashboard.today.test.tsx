/**
 * The Today card of the dashboard (D-DP1): the title, the date in words, the set, the read
 * button, the week line, and the single primary action (W-C2).
 */
import { describe, expect, it } from 'vitest';
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { mount, primaries, status, stubApi } from './helpers/dashboard';

const withStatus = (over: Parameters<typeof status>[0]) =>
  stubApi({ getStatus: async () => status(over) });

describe('the Today card', () => {
  it('shows the title, the date, the set, the read and the week', async () => {
    await mount();
    expect(screen.getByRole('heading', { name: 'Today · Study day 12' })).toBeTruthy();
    expect(screen.getByText('Sunday, August 30, 2026')).toBeTruthy();
    expect(screen.getByText('Lesson · Fractions · about 15 minutes')).toBeTruthy();
    expect(screen.getByText('Review · Whole numbers · about 5 minutes')).toBeTruthy();
    expect(screen.getByText('Read: Fractions')).toBeTruthy();
    expect(screen.getByText('This week: 4 study days, 3 topics passed.')).toBeTruthy();
  });

  it('marks a closed line Done and an open line with its answer count', async () => {
    const base = status();
    const today = base.today!;
    await mount({
      api: withStatus({
        today: {
          ...today,
          set: [
            { ...today.set[0]!, progress: { answered: 2, done: false } },
            { ...today.set[1]!, progress: { answered: 0, done: true } },
          ],
        },
      }),
    });
    expect(screen.getByText('Lesson · Fractions · about 15 minutes · 2 answered')).toBeTruthy();
    expect(screen.getByText('Review · Whole numbers · about 5 minutes · Done')).toBeTruthy();
  });

  it('says nothing is due and shows no read button for an empty set', async () => {
    const today = status().today!;
    await mount({ api: withStatus({ today: { ...today, set: [], read: null } }) });
    expect(screen.getByText('Nothing is due today.')).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Read the lesson text:/ })).toBeNull();
  });

  it('keeps the read button plain, so the dashboard has one primary action', async () => {
    await mount();
    const read = screen.getByRole('button', { name: 'Read the lesson text: Fractions' });
    expect(read.classList.contains('btn')).toBe(true);
    expect(read.classList.contains('btn-primary')).toBe(false);
    expect(primaries().length).toBe(1);
  });

  it('opens the read view of the topic when the read button is pressed', async () => {
    const { onRead, onSession } = await mount();
    await userEvent.click(screen.getByRole('button', { name: 'Read the lesson text: Fractions' }));
    expect(onRead).toHaveBeenCalledWith('fractions', 'Fractions');
    expect(onSession).not.toHaveBeenCalled();
  });

  it('shows no card when the service sends no today', async () => {
    await mount({ api: withStatus({ today: null }) });
    expect(screen.queryByText(/Study day/)).toBeNull();
  });

  it('shows the read button for a review read', async () => {
    const today = status().today!;
    await mount({
      api: withStatus({
        today: {
          ...today,
          read: { topic: { id: 'whole-numbers', name: 'Whole numbers' }, kp: null, source: 'review' },
        },
      }),
    });
    expect(screen.getByText('Read: Whole numbers')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Read the lesson text: Whole numbers' })).toBeTruthy();
  });
});
