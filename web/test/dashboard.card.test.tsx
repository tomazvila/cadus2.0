/**
 * The dashboard, control by control: the stats and their classes, the course arc, the busy
 * keys of the quiet menu, and the moves that land after the screen left.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, cleanup, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ApiError, createDemoApi } from '@/api';
import { etaDisplay, hasScheduledWork } from '@/views/Dashboard';
import { toastStore } from '@/app/toast';
import { held } from './helpers/held';
import { EMPTY_PLAN, ONE_COURSE, mount, status, stubApi } from './helpers/dashboard';
import type { EnrollResponse, SessionPlanResponse, SessionStartResponse } from '@/api/types';

const button = (name: string) => screen.getByRole('button', { name }) as HTMLButtonElement;
const stat = (label: string) =>
  Array.from(document.querySelectorAll('.stat')).find((s) => s.querySelector('.stat-label')?.textContent === label)!;

describe('hasScheduledWork', () => {
  it('counts each kind of work on its own', () => {
    const none = status(EMPTY_PLAN);
    expect(hasScheduledWork(none)).toBe(false);
    expect(hasScheduledWork({ ...none, due_reviews: 1 })).toBe(true);
    expect(hasScheduledWork({ ...none, nearly_due: 1 })).toBe(true);
    expect(hasScheduledWork({ ...none, frontier: 1 })).toBe(true);
    expect(hasScheduledWork({ ...none, quiz_due: true })).toBe(true);
    expect(hasScheduledWork({ ...none, drill_due: true })).toBe(true);
  });
});

describe('the status card', () => {
  it('warns on the due count only while something is due, and dashes an unknown ETA', async () => {
    const first = await mount();
    expect(stat('due now').className).toBe('stat warn');
    expect(stat('finish by').querySelector('.stat-value')!.textContent).toBe('Nov 4');
    first.unmount();
    cleanup();

    await mount({ api: stubApi({ getStatus: async () => status({ due_reviews: 0, velocity: { ...status().velocity, eta: null } }) }) });
    expect(stat('due now').className).toBe('stat');
    expect(stat('finish by').querySelector('.stat-value')!.textContent).toBe('—');
  });

  it('shows the not-marked tile only while an ungraded attempt waits, and explains it on hover', async () => {
    // D-F2: nothing waits, so the learner reads the same six tiles as before.
    const first = await mount();
    expect(stat('not marked')).toBeUndefined();
    first.unmount();
    cleanup();

    await mount({ api: stubApi({ getStatus: async () => status({ ungraded: 2 }) }) });
    const tile = stat('not marked');
    expect(tile.querySelector('.stat-value')!.textContent).toBe('2');
    expect(tile.className).toBe('stat warn');
    // H-2 (ISSUES.md), retold as a tooltip: every tile explains itself on hover, and
    // the not-marked tile carries the whole story — what happened, what it scored,
    // and the human way out. The prose paragraph under the grid is gone.
    const said = tile.getAttribute('title') ?? '';
    expect(said).toContain('could not read');
    expect(said).toContain('Nothing was scored');
    expect(said).toContain('person can still mark them by hand');
  });

  it('dashes an ETA the observed pace cannot honestly carry', async () => {
    // H-6 (ISSUES.md): the horizon is 730 days. A date inside it shows; a
    // multi-year extrapolation from a 28-day window does not.
    expect(etaDisplay({ ...status().velocity, eta: '2030-01-01' }, new Date('2026-09-15T00:00:00'))).toBe('—');
    expect(etaDisplay({ ...status().velocity, eta: '2027-09-01' }, new Date('2026-09-15T00:00:00'))).toBe('2027-09-01');
    expect(etaDisplay({ ...status().velocity, eta: null }, new Date('2026-09-15T00:00:00'))).toBe('—');
    // A past date is a stale extrapolation, not a missed promise.
    expect(etaDisplay({ ...status().velocity, eta: '2026-01-01' }, new Date('2026-09-15T00:00:00'))).toBe('—');
    // A date 700 days out sits inside the horizon.
    const inside = new Date('2026-09-15T00:00:00');
    inside.setDate(inside.getDate() + 700);
    const etaStr = inside.toISOString().slice(0, 10);
    expect(etaDisplay({ ...status().velocity, eta: etaStr }, new Date('2026-09-15T00:00:00'))).toBe(etaStr);
  });

  it('draws the course arc with the current course marked', async () => {
    await mount();
    const arc = Array.from(document.querySelectorAll('.arc-course'));
    expect(arc.map((c) => [c.textContent, c.className])).toEqual([
      ['Foundations', 'arc-course arc-current'],
      ['Proofs', 'arc-course'],
    ]);
  });

  it('routes a 401 on the status read to sign-in', async () => {
    const view = await mount({
      api: stubApi({ getStatus: async () => { throw new ApiError(401, 'unauthorized', 'No session.'); } }),
    });
    await waitFor(() => expect(view.onUnauthorized).toHaveBeenCalledTimes(1));
  });
});

describe('the primary action', () => {
  it('offers no next course after the last one, and none without a current one', async () => {
    const last = status().courses.map((c) => ({ ...c, current: c.id === 'proofs' }));
    const first = await mount({ api: stubApi({ getStatus: async () => status({ ...EMPTY_PLAN, courses: last }) }) });
    expect(screen.queryByRole('button', { name: /^Start (?!a timed)/ })).toBeNull();
    first.unmount();
    cleanup();

    const none = status().courses.map((c) => ({ ...c, current: false }));
    await mount({ api: stubApi({ getStatus: async () => status({ ...EMPTY_PLAN, courses: none }) }) });
    expect(screen.queryByRole('button', { name: /^Start (?!a timed)/ })).toBeNull();
  });

  it('marks the hero busy while the session starts, and while the enrolment posts', async () => {
    const start = held<SessionStartResponse>();
    const first = await mount({ api: stubApi({ sessionStart: () => start.promise }) });
    await userEvent.click(button('Continue studying'));
    expect(button('Continue studying').disabled).toBe(true);
    expect(button('Continue studying').className).toBe('btn btn-primary btn-hero is-busy');
    await act(async () => { start.release(await createDemoApi().sessionStart()); });
    first.unmount();
    cleanup();

    const enroll = held<EnrollResponse>();
    await mount({ api: stubApi({ getStatus: async () => status(EMPTY_PLAN), enroll: () => enroll.promise }) });
    await userEvent.click(button('Start Proofs'));
    expect(button('Start Proofs').disabled).toBe(true);
    expect(button('Start Proofs').className).toBe('btn btn-primary btn-hero is-busy');
    await act(async () => { enroll.release(await createDemoApi().enroll('proofs')); });
  });

  it('toasts the enrolment as a success, and reads the status again', async () => {
    const getStatus = vi.fn(async () => status(EMPTY_PLAN));
    await mount({ api: stubApi({ getStatus }) });
    await userEvent.click(button('Start Proofs'));
    await waitFor(() => expect(getStatus).toHaveBeenCalledTimes(2));
    expect(toastStore.getSnapshot()).toEqual([
      { id: 1, message: 'Enrolled in Proofs. Answer the starting questions to get started.', kind: 'success' },
    ]);
  });

  it('opens no session from a start that lands after the screen left', async () => {
    const start = held<SessionStartResponse>();
    const view = await mount({ api: stubApi({ sessionStart: () => start.promise }) });
    await userEvent.click(button('Continue studying'));
    view.unmount();
    await act(async () => { start.release(await createDemoApi().sessionStart()); });
    await act(async () => {});
    expect(view.onSession).not.toHaveBeenCalled();
  });

  it('toasts nothing from an enrolment that lands after the screen left', async () => {
    const enroll = held<EnrollResponse>();
    const view = await mount({ api: stubApi({ getStatus: async () => status(EMPTY_PLAN), enroll: () => enroll.promise }) });
    await userEvent.click(button('Start Proofs'));
    view.unmount();
    await act(async () => { enroll.release(await createDemoApi().enroll('proofs')); });
    expect(toastStore.getSnapshot()).toEqual([]);
  });
});

describe('the quiet menu', () => {
  async function openMenu() {
    await userEvent.click(screen.getByText(/^More/));
  }

  it('keys each control on its own, so one busy control leaves the others live', async () => {
    const start = held<SessionStartResponse>();
    await mount({ api: stubApi({ sessionStart: () => start.promise }) });
    await openMenu();
    await userEvent.click(button('Start a timed quiz'));
    expect(button('Start a timed quiz').disabled).toBe(true);
    expect(button('Start a timed quiz').className).toBe('btn is-busy');
    expect(button('Download my data').disabled).toBe(false);
    expect(button('Switch course').disabled).toBe(false);
    await act(async () => { start.release(await createDemoApi().sessionStart()); });
  });

  it('holds Switch course busy while the picker is open, and offers it only with a choice', async () => {
    const first = await mount();
    await openMenu();
    await userEvent.click(button('Switch course'));
    expect(button('Switch course').className).toBe('btn is-busy');
    expect(button('Switch course').disabled).toBe(true);
    expect(button('Download my data').disabled).toBe(false);
    // The picker names the current course as such, and offers it to nobody.
    const current = within(screen.getByRole('dialog')).getByRole('button', { name: 'Foundations · current' });
    expect(current.hasAttribute('disabled')).toBe(true);
    await userEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(button('Switch course').className).toBe('btn'));
    expect(toastStore.getSnapshot()).toEqual([]);
    first.unmount();
    cleanup();

    await mount({ api: stubApi({ getStatus: async () => status({ courses: ONE_COURSE }) }) });
    await openMenu();
    expect(button('Switch course').disabled).toBe(true);
  });

  it('holds Export busy while the file downloads', async () => {
    const download = held<void>();
    await mount({ api: stubApi({ downloadExport: () => download.promise }) });
    await openMenu();
    await userEvent.click(button('Download my data'));
    expect(button('Download my data').className).toBe('btn is-busy');
    expect(button('Download my data').disabled).toBe(true);
    expect(button('Start a timed quiz').disabled).toBe(false);
    await act(async () => { download.release(undefined); });
    expect(button('Download my data').className).toBe('btn');
  });

  it('toasts the no-quiz line as information, and a bare export refusal as the generic line', async () => {
    const first = await mount();
    await openMenu();
    await userEvent.click(button('Start a timed quiz'));
    await waitFor(() => expect(toastStore.getSnapshot()).toEqual([
      { id: 1, message: 'No quiz is due right now.', kind: 'info' },
    ]));
    first.unmount();
    cleanup();

    await mount({ api: stubApi({ downloadExport: async () => { throw null; } }) });
    await openMenu();
    await userEvent.click(button('Download my data'));
    await waitFor(() => expect(toastStore.getSnapshot().map((t) => [t.message, t.kind])).toEqual([
      ['Could not export your data. Try again in a minute.', 'error'],
    ]));
  });

  it('opens no quiz from a plan that lands after the screen left', async () => {
    const plan = held<SessionPlanResponse>();
    const view = await mount({ api: stubApi({ getPlan: () => plan.promise }) });
    await openMenu();
    await userEvent.click(button('Start a timed quiz'));
    view.unmount();
    await act(async () => { plan.release(await createDemoApi().getPlan()); });
    expect(view.onQuiz).not.toHaveBeenCalled();
  });
});
