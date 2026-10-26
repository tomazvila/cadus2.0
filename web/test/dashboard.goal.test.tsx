/**
 * The daily goal control (D-XP1): five presets and one number field in the More menu.
 * A press sends one request, and the heading, the ring and the streak read the reply.
 */
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toastStore } from '@/app/toast';
import { mount, primaries, status, stubApi } from './helpers/dashboard';
import type { ApiClient } from '@/api/types';

function openMenu(): HTMLElement {
  const menu = document.querySelector<HTMLDetailsElement>('.more-menu')!;
  menu.open = true;
  return menu;
}

describe('the daily goal control', () => {
  it('holds the heading and five presets in the More menu', async () => {
    await mount();
    const menu = openMenu();
    expect(within(menu).getByRole('heading', { name: 'Your daily goal' })).toBeTruthy();
    expect(within(menu).getByText('Daily goal: 40 points.')).toBeTruthy();
    for (const value of ['10', '20', '40', '60', '100']) {
      expect(within(menu).getByRole('button', { name: `${value} points a day` })).toBeTruthy();
    }
    expect(within(menu).getByRole('button', { name: '40 points a day' }).getAttribute('aria-pressed')).toBe('true');
    expect(within(menu).getByRole('button', { name: '60 points a day' }).getAttribute('aria-pressed')).toBe('false');
    expect(screen.getByText('More: goal, map, proofs, quiz, data')).toBeTruthy();
    expect(primaries()).toHaveLength(1);
  });

  it('sends one request on a preset and the heading reads the new goal', async () => {
    const setXpGoal = vi.fn<ApiClient['setXpGoal']>(async (goal) => ({
      goal,
      xp: { total: 340, today: 12, goal, streak_days: 4 },
    }));
    await mount({ api: stubApi({ setXpGoal }) });
    const menu = openMenu();
    await userEvent.click(within(menu).getByRole('button', { name: '60 points a day' }));

    await waitFor(() =>
      expect(screen.getByRole('heading', { name: '12 / 60 points today' })).toBeTruthy());
    expect(setXpGoal).toHaveBeenCalledTimes(1);
    expect(setXpGoal).toHaveBeenCalledWith(60);
    expect(document.querySelector('.stat-value.accent, .accent .stat-value')?.textContent).toBe('4');
    expect(toastStore.getSnapshot().map((t) => t.message)).toContain(
      'Goal set to 60 points a day. Your streak counts the days that reach it.',
    );
    expect(primaries()).toHaveLength(1);
  });

  it('sends a typed goal from 10 to 100', async () => {
    const setXpGoal = vi.fn<ApiClient['setXpGoal']>(async (goal) => ({
      goal,
      xp: { total: 340, today: 12, goal, streak_days: 3 },
    }));
    await mount({ api: stubApi({ setXpGoal }) });
    const menu = openMenu();
    await userEvent.type(within(menu).getByLabelText('Or type a goal from 10 to 100'), '35');
    await userEvent.click(within(menu).getByRole('button', { name: 'Set goal' }));
    await waitFor(() => expect(setXpGoal).toHaveBeenCalledWith(35));
    await waitFor(() =>
      expect(screen.getByRole('heading', { name: '12 / 35 points today' })).toBeTruthy());
  });

  it('refuses a typed 7 before any request', async () => {
    const setXpGoal = vi.fn<ApiClient['setXpGoal']>();
    await mount({ api: stubApi({ setXpGoal, getStatus: async () => status() }) });
    const menu = openMenu();
    await userEvent.type(within(menu).getByLabelText('Or type a goal from 10 to 100'), '7');
    await userEvent.click(within(menu).getByRole('button', { name: 'Set goal' }));
    expect(within(menu).getByText('Type a whole number from 10 to 100.')).toBeTruthy();
    expect(setXpGoal).not.toHaveBeenCalled();
    expect(screen.getByRole('heading', { name: '12 / 40 points today' })).toBeTruthy();
    expect(primaries()).toHaveLength(1);
  });
});
