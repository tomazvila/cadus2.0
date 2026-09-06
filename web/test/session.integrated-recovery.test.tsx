/** Integrated retry lifetime and instruction fallback behavior through Session. */
import { act, fireEvent, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { ApiError } from '@/api';
import type { IntegratedProblem, PlanTask } from '@/api/types';
import { RETRY_STALE_MESSAGE } from '@/hooks/useCall';
import { held } from './helpers/held';
import { PROBLEM } from './helpers/integrated';
import { pressRetry } from './helpers/toasts';
import { LESSON, REVIEW, mount, planOf, press, stubApi, toasts } from './helpers/session';

const MULTI: PlanTask = { ...REVIEW, task_id: 'retry-integrated', task_type: 'multi-step' };

it('retries a failed integrated serve while the same task is waiting', async () => {
  const taskIntegrated = vi.fn(async () => PROBLEM);
  taskIntegrated.mockRejectedValueOnce(new Error('temporarily unavailable'));
  const api = stubApi({ taskIntegrated });
  await mount({ api, plan: planOf(MULTI) });
  expect(toasts()[0].label).toBe('Retry');
  await pressRetry();
  expect(taskIntegrated).toHaveBeenCalledTimes(2);
  expect(screen.getByText(PROBLEM.title)).toBeTruthy();
});

it('refuses a saved integrated retry after the view has unmounted', async () => {
  const taskIntegrated = vi.fn(async () => { throw new Error('offline'); });
  const view = await mount({ api: stubApi({ taskIntegrated }), plan: planOf(MULTI) });
  view.unmount();
  await pressRetry();
  expect(taskIntegrated).toHaveBeenCalledTimes(1);
  expect(toasts().some((toast) => toast.message === RETRY_STALE_MESSAGE)).toBe(true);
});

it('discards an integrated serve that arrives after leaving the screen', async () => {
  const response = held<IntegratedProblem>();
  const taskServe = vi.fn();
  const view = await mount({
    api: stubApi({ taskIntegrated: () => response.promise, taskServe }), plan: planOf(MULTI),
  });
  view.unmount();
  await act(async () => { response.release(PROBLEM); });
  expect(screen.queryByText(PROBLEM.title)).toBeNull();
  expect(taskServe).not.toHaveBeenCalled();
});

it('keeps expired integrated submissions on the demo screen', async () => {
  const api = stubApi({
    taskIntegrated: async () => PROBLEM,
    taskIntegratedAnswer: async () => { throw new ApiError(401, 'unauthorized', 'Expired'); },
  });
  const nav = await mount({ api, plan: planOf(MULTI), demo: true });
  await press('Submit the whole task');
  expect(nav.onUnauthorized).not.toHaveBeenCalled();
  expect(screen.getByText('The submission did not reach the service. Try again.')).toBeTruthy();
});

it.each([
  [{ id: 'fraction-id', name: null, module: '' }, 'fraction-id'],
  [null, 'this topic'],
] as const)('labels unavailable instruction using the available topic identity', async (topic, label) => {
  await mount({
    plan: planOf({ ...LESSON, topic }),
    api: stubApi({ taskTeach: async () => { throw new ApiError(409, 'no_instruction', 'Unavailable'); } }),
  });
  expect(screen.getByText(new RegExp(`The worked example for ${label} is not written yet`))).toBeTruthy();
});

it('skips unavailable instruction once under two clicks in the same tick', async () => {
  const taskServe = vi.fn(async () => { throw new Error('hold next screen'); });
  await mount({
    plan: planOf(LESSON, REVIEW),
    api: stubApi({ taskServe, taskTeach: async () => { throw new Error('no approved content'); } }),
  });
  const skip = screen.getByRole('button', { name: 'Skip to the next task' });
  await act(async () => { fireEvent.click(skip); fireEvent.click(skip); });
  expect(taskServe).toHaveBeenCalledExactlyOnceWith(REVIEW.task_id);
});
