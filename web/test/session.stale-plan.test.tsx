/** A stale planned task cannot keep the previous task's question on screen. */
import { describe, expect, it, vi } from 'vitest';
import { act, screen } from '@testing-library/react';
import { ApiError } from '@/api';
import type { ApiClient, SessionPlanResponse } from '@/api/types';
import { held } from './helpers/held';
import { busy } from './helpers/api';
import { pressRetry, pressRetryAfterUnmount } from './helpers/toasts';
import {
  LESSON, P, REVIEW, graded, mount, planOf, press, stubApi, submitAnswer, toasts,
} from './helpers/session';

const stale = { ...REVIEW, task_id: 't-stale',
  topic: { id: 'distributive-property', name: 'The Distributive Property', module: 'Algebra' } };
const replacement = { ...REVIEW, task_id: 't-current' };
const previous = P(4, { total: 4, text: 'Correct the claim that $7-3x=19$ has solution $4$.' });
const passed = async () => graded({ next: null, task_status: 'task_passed' });

describe('stale session plan recovery', () => {
  it('removes the answered question before the next task loads, including a failed load', async () => {
    const pending = held<void>();
    const taskServe = vi.fn<ApiClient['taskServe']>()
      .mockResolvedValueOnce(previous)
      .mockImplementationOnce(async () => { await pending.promise; throw busy(); })
      .mockRejectedValue(busy());
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(passed);
    await mount({ plan: planOf(REVIEW, stale), api: stubApi({ taskServe, taskAnswer }) });
    await submitAnswer('-4');
    await press('Continue →');
    expect(screen.queryByLabelText('Answer')).toBeNull();
    expect(document.querySelector('.problem-text')).toBeNull();
    expect(screen.getByText('Getting your next problem…')).toBeTruthy();
    await act(async () => { pending.release(); });
    expect(screen.queryByLabelText('Answer')).toBeNull();
    await pressRetry();
    expect(screen.queryByLabelText('Answer')).toBeNull();
    expect(taskAnswer).toHaveBeenCalledTimes(1);
  });

  it.each([[404, 'unknown_task'], [409, 'task_complete'], [409, 'session_rolled_over']] as const)(
    'refreshes the plan on %s %s and serves the server-selected next task',
    async (status, code) => {
      const getPlan = vi.fn<ApiClient['getPlan']>(async () => planOf(replacement));
      const taskServe = vi.fn<ApiClient['taskServe']>()
        .mockResolvedValueOnce(previous)
        .mockRejectedValueOnce(new ApiError(status, code))
        .mockResolvedValue(P(1, { problem_id: 'fresh', text: 'Fresh question' }));
      const taskAnswer = vi.fn<ApiClient['taskAnswer']>(passed);
      await mount({ plan: planOf(REVIEW, stale), api: stubApi({ getPlan, taskServe, taskAnswer }) });
      await submitAnswer('-4');
      await press('Continue →');
      expect(getPlan).toHaveBeenCalledTimes(1);
      expect(taskServe.mock.calls.map(([id]) => id)).toEqual(['t-review', 't-stale', 't-current']);
      expect(screen.getByText('Fresh question')).toBeTruthy();
      expect((screen.getByLabelText('Answer') as HTMLInputElement).value).toBe('');
      expect(screen.queryByRole('button', { name: 'Report submitted question' })).toBeNull();
      expect(toasts()).toHaveLength(0);
      await submitAnswer('3');
      expect(taskAnswer.mock.calls[1]).toEqual(['t-current', { problem_id: 'fresh', answer: '3' }]);
    },
  );

  it('recovers a stale initial lesson before showing a missing-instruction card', async () => {
    const taskTeach = vi.fn<ApiClient['taskTeach']>().mockRejectedValue(new ApiError(404, 'unknown_task'));
    const taskServe = vi.fn<ApiClient['taskServe']>().mockResolvedValue(P(1));
    const getPlan = vi.fn<ApiClient['getPlan']>().mockResolvedValue(planOf(replacement));
    await mount({ plan: planOf(LESSON), api: stubApi({ taskTeach, taskServe, getPlan }) });
    expect(taskTeach).toHaveBeenCalledTimes(1);
    expect(taskServe).toHaveBeenCalledWith('t-current');
    expect(screen.getByLabelText('Answer')).toBeTruthy();
    expect(screen.queryByText('This lesson is not ready yet')).toBeNull();
  });

  it('keeps a failed refresh retryable and never ends the session on that failure', async () => {
    const getPlan = vi.fn<ApiClient['getPlan']>()
      .mockRejectedValueOnce(busy()).mockResolvedValue(planOf(replacement));
    const taskServe = vi.fn<ApiClient['taskServe']>()
      .mockRejectedValueOnce(new ApiError(404, 'unknown_task'))
      .mockRejectedValueOnce(new ApiError(404, 'unknown_task'))
      .mockResolvedValue(P(1));
    const sessionEnd = vi.fn<ApiClient['sessionEnd']>();
    await mount({ plan: planOf(stale), api: stubApi({ getPlan, taskServe, sessionEnd }) });
    expect(sessionEnd).not.toHaveBeenCalled();
    expect(screen.queryByLabelText('Answer')).toBeNull();
    await pressRetry();
    expect(getPlan).toHaveBeenCalledTimes(2);
    expect(taskServe).toHaveBeenLastCalledWith('t-current');
    expect(screen.getByLabelText('Answer')).toBeTruthy();
    expect(sessionEnd).not.toHaveBeenCalled();
  });

  it('shows a server-confirmed empty plan without closing the open session', async () => {
    const sessionEnd = vi.fn<ApiClient['sessionEnd']>();
    await mount({ plan: planOf(stale), api: stubApi({
      taskServe: async () => { throw new ApiError(404, 'unknown_task'); },
      getPlan: async () => planOf(), sessionEnd,
    }) });
    expect(screen.queryByText('Getting your next problem…')).toBeNull();
    expect(screen.queryByLabelText('Answer')).toBeNull();
    expect(sessionEnd).not.toHaveBeenCalled();
    expect(toasts()).toHaveLength(0);
  });

  it('bounds recovery when the server still lists the refused task as open', async () => {
    const getPlan = vi.fn<ApiClient['getPlan']>().mockResolvedValue(planOf(stale));
    const taskServe = vi.fn<ApiClient['taskServe']>().mockRejectedValue(new ApiError(404, 'unknown_task'));
    await mount({ plan: planOf(stale), api: stubApi({ getPlan, taskServe }) });
    expect(getPlan).toHaveBeenCalledTimes(1);
    expect(taskServe).toHaveBeenCalledTimes(1);
    expect(toasts()).toHaveLength(1);
    expect(screen.queryByLabelText('Answer')).toBeNull();
  });

  it('does not serve from a recovered plan after leaving the screen', async () => {
    const pending = held<SessionPlanResponse>();
    const taskServe = vi.fn<ApiClient['taskServe']>().mockRejectedValue(new ApiError(404, 'unknown_task'));
    const { unmount } = await mount({ plan: planOf(stale), api: stubApi({
      taskServe, getPlan: () => pending.promise,
    }) });
    unmount();
    await act(async () => { pending.release(planOf(replacement)); });
    expect(taskServe).toHaveBeenCalledTimes(1);
  });

  it('refuses a failed load retry after leaving the screen', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>().mockRejectedValue(busy());
    const { unmount } = await mount({ api: stubApi({ taskServe }) });
    await pressRetryAfterUnmount(unmount);
    expect(taskServe).toHaveBeenCalledTimes(1);
  });
});
