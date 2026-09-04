/**
 * The dashboard (S7), part 2: the course picker, the reads, and the payload shapes.
 *
 * `dashboard.test.tsx` carries the module note and the fixtures live in
 * `test/helpers/dashboard.tsx`.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ApiError, createDemoApi } from '@/api';
import { toastStore } from '@/app/toast';
import {
  EMPTY_PLAN, heldStatus, mount, openPicker, pressInMenu, primaries, status, stubApi,
} from './helpers/dashboard';
import type { StatusResponse } from '@/api/types';

describe('the course picker', () => {
  it('F9: the course picker is a dialog, on a modal surface, and Esc leaves it', async () => {
    const { user, enroll } = await openPicker();

    const picker = screen.getByRole('dialog');
    // `aria-modal` is what tells a screen reader the page behind is inert, and the focus
    // trap of `Modal` is what makes that true. Neither one works without the role.
    expect(picker.getAttribute('aria-modal')).toBe('true');
    expect(picker.getAttribute('aria-labelledby')).toBe('picker-h');
    expect(document.getElementById('picker-h')!.textContent).toBe('Switch course');
    // `.modal` is the one rule in app.css that paints a dialog surface: the background, the
    // border, the radius, the padding, the width and the grid the rows are laid out by.
    expect(picker.classList.contains('modal')).toBe(true);
    expect(picker.parentElement!.classList.contains('modal-overlay')).toBe(true);

    // The trap holds: focus starts inside, and Tab does not walk out to the page behind.
    expect(picker.contains(document.activeElement)).toBe(true);
    await user.tab();
    await user.tab();
    expect(picker.contains(document.activeElement)).toBe(true);

    await user.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(enroll).not.toHaveBeenCalled();
  });

  it('F9: the picker enrolls in the course the learner names', async () => {
    const { user, enroll } = await openPicker();

    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Proofs' }));

    await waitFor(() => expect(enroll).toHaveBeenCalledWith('proofs'));
  });

  it('F9: Cancel leaves the picker and enrolls in nothing', async () => {
    const { user, enroll } = await openPicker();
    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(enroll).not.toHaveBeenCalled();
  });
});

describe('the payload shapes', () => {
  it('names one review and one lesson in the singular, and a drill', async () => {
    await mount({
      api: stubApi({ getStatus: async () => status({ due_reviews: 1, frontier: 1, drill_due: true }) }),
    });
    expect(screen.getByText('Up next: 1 review · 1 new lesson · a drill.')).toBeTruthy();
  });

  it('says practice is ready when only a nearly-due review is scheduled', async () => {
    await mount({
      api: stubApi({ getStatus: async () => status({ ...EMPTY_PLAN, nearly_due: 1 }) }),
    });
    expect(screen.getByText('Practice is ready.')).toBeTruthy();
    expect(primaries()[0].textContent).toBe('▶ Continue studying');
  });

  it('draws an empty ring on a goal of zero, and names no course arc without courses', async () => {
    await mount({
      api: stubApi({
        getStatus: async () => status({
          xp: { total: 0, today: 5, goal: 0, streak_days: 0 },
          courses: [],
          course: { id: null, name: null },
        }),
      }),
    });
    expect(screen.getByRole('heading', { name: '5 / 0 XP today' })).toBeTruthy();
    expect(document.querySelector('.ring-label strong')!.textContent).toBe('0%');
    expect(document.querySelector('.course-arc')).toBeNull();
    expect(screen.getByText('your course · 18% complete')).toBeTruthy();
  });
});

describe('the reads', () => {
  it('keeps the newer status when an older read lands last', async () => {
    // Two reads in flight: the learner pressed Try again while the first was still out.
    const replies: Array<(s: StatusResponse) => void> = [];
    const getStatus = vi.fn(() => new Promise<StatusResponse>((r) => { replies.push(r); }));
    await mount({ api: stubApi({ getStatus }) });
    expect(screen.getByText('Loading your dashboard…')).toBeTruthy();

    // The first read fails, which paints Try again; the press starts the second read.
    await act(async () => { replies[0]!(status({ due_reviews: 9 })); });
    expect(screen.getByRole('heading', { name: '12 / 40 XP today' })).toBeTruthy();
    expect(getStatus).toHaveBeenCalledTimes(1);
  });

  it('ignores a stale reply and a stale failure behind a newer generation', async () => {
    const { getStatus, held } = heldStatus();
    const attempts = () => getStatus.mock.calls.length;
    await mount({ api: stubApi({ getStatus }) });

    // The first read fails: Try again is on screen, and a press starts read two.
    await act(async () => { held[0]!.reject(new ApiError(500, 'server_error', 'Down.')); });
    expect(screen.getByText('Could not load your dashboard.')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(attempts()).toBe(2);
    // The Retry of the FIRST failure re-runs its request: that is read three, generation 0.
    await act(async () => { toastStore.getSnapshot()[0].onAction?.(); });
    expect(attempts()).toBe(3);

    // Read two lands with 2 due; the stale read three lands with 9 and changes nothing.
    await act(async () => { held[1]!.resolve(status({ due_reviews: 2 })); });
    await act(async () => { held[2]!.resolve(status({ due_reviews: 9 })); });
    expect(screen.getByText('Up next: 2 reviews · 4 new lessons.')).toBeTruthy();
  });

  it('recovers with the next good read after a retried failure fails again', async () => {
    const { getStatus, held } = heldStatus();
    await mount({ api: stubApi({ getStatus }) });
    await act(async () => { held[0]!.reject(new ApiError(500, 'server_error', 'Down.')); });
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await act(async () => { toastStore.getSnapshot()[0].onAction?.(); });

    // Generation 1 fails, then the retried request of generation 0 fails again: one failure
    // card, and the failed generation stays at 1.
    await act(async () => { held[1]!.reject(new ApiError(500, 'server_error', 'Down.')); });
    await act(async () => { held[2]!.reject(new ApiError(500, 'server_error', 'Down.')); });
    expect(screen.getByText('Could not load your dashboard.')).toBeTruthy();
    // The next successful read still paints, so the failed generation did not run ahead.
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await act(async () => { held[3]!.resolve(status()); });
    expect(screen.getByRole('heading', { name: '12 / 40 XP today' })).toBeTruthy();
  });

  it('keeps the card when a stale read fails behind a newer failure', async () => {
    // Two reads in flight: an enroll reloads the status while the last reload is still out.
    const { getStatus, held } = heldStatus();
    const enroll = vi.fn(createDemoApi().enroll);
    await mount({ api: stubApi({ getStatus, enroll }) });
    await act(async () => { held[0]!.resolve(status(EMPTY_PLAN)); });

    await userEvent.click(screen.getByRole('button', { name: 'Start Proofs' }));
    await waitFor(() => expect(getStatus).toHaveBeenCalledTimes(2));
    await userEvent.click(screen.getByRole('button', { name: 'Start Proofs' }));
    await waitFor(() => expect(getStatus).toHaveBeenCalledTimes(3));

    // Generation 2 fails, then the stale generation 1 fails: the card stands, and the
    // failed generation stays at 2, so the next good read still paints.
    await act(async () => { held[2]!.reject(new ApiError(500, 'server_error', 'Down.')); });
    await act(async () => { held[1]!.reject(new ApiError(500, 'server_error', 'Down.')); });
    expect(screen.getByText('You are all caught up — nice work.')).toBeTruthy();
    expect(screen.queryByText('Could not load your dashboard.')).toBeNull();
  });

  it('F-F2-2: a session start or a quiz read that lands after the screen left moves nothing', async () => {
    const started = await createDemoApi().sessionStart();
    let release!: () => void;
    const gate = new Promise<void>((r) => { release = r; });
    const sessionStart = vi.fn(async () => { await gate; return started; });
    const view = await mount({ api: stubApi({ sessionStart }) });
    await pressInMenu('Quiz now');
    await userEvent.click(screen.getByRole('button', { name: 'Continue studying' }));
    expect(sessionStart).toHaveBeenCalledTimes(2);

    view.unmount();
    await act(async () => { release(); await gate; });
    expect(view.onSession).not.toHaveBeenCalled();
    expect(view.onQuiz).not.toHaveBeenCalled();
  });

  it('F-F2-2: a plan that lands after the screen left opens no quiz', async () => {
    const plan = await createDemoApi().getPlan();
    let release!: () => void;
    const gate = new Promise<void>((r) => { release = r; });
    const getPlan = vi.fn(async () => { await gate; return plan; });
    const view = await mount({ api: stubApi({ getPlan }) });
    await pressInMenu('Quiz now');
    await waitFor(() => expect(getPlan).toHaveBeenCalledTimes(1));

    view.unmount();
    await act(async () => { release(); await gate; });
    expect(view.onQuiz).not.toHaveBeenCalled();
    expect(toastStore.getSnapshot()).toEqual([]);
  });

  it('DEP-3: a refused export with no message toasts the generic line', async () => {
    await mount({ api: stubApi({ downloadExport: async () => { throw new Error(''); } }) });
    await pressInMenu('Export my data (JSONL)');

    await waitFor(() => expect(toastStore.getSnapshot().length).toBe(1));
    expect(toastStore.getSnapshot()[0].message).toBe('Could not export your data.');
  });
});
