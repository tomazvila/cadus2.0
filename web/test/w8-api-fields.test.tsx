/**
 * W8: the four dashboard and map notes that read new service fields.
 *
 *   764  "About N minutes" under the primary button;
 *   856  "Unfinished: <topic>." when a lesson was left open;
 *   919  "Last studied: <topic>, N days ago.";
 *   883  "Review this topic soon" on the map, for a topic the learner practiced.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createDemoApi } from '@/api';
import { aboutMinutes, lastStudied } from '@/views/dashboard/PrimaryAction';
import { toastStore } from '@/app/toast';
import { mount, status, stubApi } from './helpers/dashboard';
import * as mapHelpers from './helpers/map';

const NOW = new Date(2026, 9, 10, 15, 0, 0);

describe('the minutes line', () => {
  it('rounds the summed budget up, and says minute once', () => {
    const preview = (budget_secs: number | null) =>
      status({ plan_preview: { budget_secs, first_topic: null } });
    expect(aboutMinutes(preview(1500))).toBe('About 25 minutes');
    expect(aboutMinutes(preview(61))).toBe('About 2 minutes');
    expect(aboutMinutes(preview(30))).toBe('About 1 minute');
    expect(aboutMinutes(preview(null))).toBe('');
    expect(aboutMinutes(status({ plan_preview: null }))).toBe('');
  });
});

describe('the last-studied line', () => {
  const at = (days: number) => {
    const d = new Date(NOW);
    d.setDate(d.getDate() - days);
    return status({
      last_topic: { id: 'fractions', name: 'Fractions' },
      last_active_at: d.toISOString(),
    });
  };
  it('names today, yesterday and the day count', () => {
    expect(lastStudied(at(0), NOW)).toBe('Last studied: Fractions, today.');
    expect(lastStudied(at(1), NOW)).toBe('Last studied: Fractions, yesterday.');
    expect(lastStudied(at(5), NOW)).toBe('Last studied: Fractions, 5 days ago.');
  });
  it('says nothing before the first answer', () => {
    expect(lastStudied(status({ last_topic: null, last_active_at: null }), NOW)).toBe('');
  });
});

describe('the dashboard sub-line', () => {
  it('starts with the unfinished topic and ends with the minutes line', async () => {
    await mount({
      api: stubApi({
        getStatus: async () => status({
          due_reviews: 2,
          frontier: 0,
          session_open: true,
          plan_preview: { budget_secs: 1200, first_topic: { id: 'fractions', name: 'Fractions' } },
          last_topic: null,
          last_active_at: null,
        }),
      }),
    });
    expect(await screen.findByText('Unfinished: Fractions. Up next: 2 reviews.')).toBeTruthy();
    expect(screen.getByText('About 20 minutes')).toBeTruthy();
  });
});

describe('the map button', () => {
  it('asks for the review and confirms with a toast', async () => {
    const reviewSoon = vi.fn(async (topic: string) => ({ topic, review_soon: true }));
    const view = await mapHelpers.mountLoaded({ api: mapHelpers.stubApi({ reviewSoon }) });
    const cy = mapHelpers.last();
    await act(async () => { cy.emit('tap', cy.getElementById('decimals')); });
    await userEvent.click(screen.getByRole('button', { name: 'Review this topic soon' }));
    expect(reviewSoon).toHaveBeenCalledWith('decimals');
    expect(toastStore.getSnapshot().map((t) => t.message)).toContain('Added to your next reviews.');
    view.unmount();
  });

  it('is absent for a topic the learner has not practiced', async () => {
    const view = await mapHelpers.mountLoaded();
    const cy = mapHelpers.last();
    await act(async () => { cy.emit('tap', cy.getElementById('fractions')); });
    expect(screen.queryByRole('button', { name: 'Review this topic soon' })).toBeNull();
    view.unmount();
  });

  it('is answered by the demo backend', async () => {
    expect(await createDemoApi().reviewSoon('decimals')).toEqual({
      topic: 'decimals', review_soon: true,
    });
  });
});
