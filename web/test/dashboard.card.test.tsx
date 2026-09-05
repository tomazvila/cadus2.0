/**
 * The dashboard, control by control: the stats and their classes, the course arc, the busy
 * keys of the quiet menu, and the moves that land after the screen left.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, cleanup, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ApiError, createDemoApi } from '@/api';
import { hasScheduledWork } from '@/views/Dashboard';
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
    expect(stat('ETA').querySelector('.stat-value')!.textContent).toBe('2026-11-04');
    first.unmount();
    cleanup();

    await mount({ api: stubApi({ getStatus: async () => status({ due_reviews: 0, velocity: { ...status().velocity, eta: null } }) }) });
    expect(stat('due now').className).toBe('stat');
    expect(stat('ETA').querySelector('.stat-value')!.textContent).toBe('—');
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
    const last = ONE_COURSE;
    const first = await mount({ api: stubApi({ getStatus: async () => status({ ...EMPTY_PLAN, courses: last }) }) });
    expect(screen.queryByRole('button', { name: /^Start / })).toBeNull();
    first.unmount();
    cleanup();

    const none = status().courses.map((c) => ({ ...c, current: false }));
    await mount({ api: stubApi({ getStatus: async () => status({ ...EMPTY_PLAN, courses: none }) }) });
    expect(screen.queryByRole('button', { name: /^Start / })).toBeNull();
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
      { id: 1, message: 'Enrolled in Proofs. Take the placement to get started.', kind: 'success' },
    ]);
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
    await userEvent.click(screen.getByText('More'));
  }

  it('keys each control on its own, so one busy control leaves the others live', async () => {
    const start = held<SessionStartResponse>();
    await mount({ api: stubApi({ sessionStart: () => start.promise }) });
    await openMenu();
    await userEvent.click(button('Quiz now'));
    expect(button('Quiz now').disabled).toBe(true);
    expect(button('Quiz now').className).toBe('btn is-busy');
    expect(button('Export my data (JSONL)').disabled).toBe(false);
    expect(button('Switch course').disabled).toBe(false);
    await act(async () => { start.release(await createDemoApi().sessionStart()); });
  });

  it('holds Switch course busy while the picker is open, and offers it only with a choice', async () => {
    const first = await mount();
    await openMenu();
    await userEvent.click(button('Switch course'));
    expect(button('Switch course').className).toBe('btn is-busy');
    expect(button('Export my data (JSONL)').disabled).toBe(false);
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
    await userEvent.click(button('Export my data (JSONL)'));
    expect(button('Export my data (JSONL)').className).toBe('btn is-busy');
    expect(button('Quiz now').disabled).toBe(false);
    await act(async () => { download.release(undefined); });
    expect(button('Export my data (JSONL)').className).toBe('btn');
  });

  it('toasts the no-quiz line as information, and a bare export refusal as the generic line', async () => {
    const first = await mount();
    await openMenu();
    await userEvent.click(button('Quiz now'));
    await waitFor(() => expect(toastStore.getSnapshot()).toEqual([
      { id: 1, message: 'No quiz is due right now.', kind: 'info' },
    ]));
    first.unmount();
    cleanup();

    await mount({ api: stubApi({ downloadExport: async () => { throw null; } }) });
    await openMenu();
    await userEvent.click(button('Export my data (JSONL)'));
    await waitFor(() => expect(toastStore.getSnapshot().map((t) => [t.message, t.kind])).toEqual([
      ['Could not export your data.', 'error'],
    ]));
  });

  it('opens no quiz from a plan that lands after the screen left', async () => {
    const plan = held<SessionPlanResponse>();
    const view = await mount({ api: stubApi({ getPlan: () => plan.promise }) });
    await openMenu();
    await userEvent.click(button('Quiz now'));
    view.unmount();
    await act(async () => { plan.release(await createDemoApi().getPlan()); });
    expect(view.onQuiz).not.toHaveBeenCalled();
  });
});
