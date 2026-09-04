/**
 * The Retry gate of `useCall` (FIX-M6-D1, F10).
 *
 * A Retry arrives long after the failure it belongs to, and `run` holds no view state: no
 * phase, no live problem, no test of validity. A Retry that re-sends a write the view has
 * since moved past posts a CONSUMED id — for the study loop, a `problem_id` the service
 * already graded into an append-only log.
 *
 * So a call site that holds a lock hands `useCall` two functions:
 *
 *   `retryGate`  Called synchronously when Retry is pressed. It re-takes the caller's lock
 *                and answers whether the retry is still valid. False refuses the retry with
 *                a PLAIN toast — a refusal toast is never actionable (F-36-1b), so it
 *                expires by itself instead of arming another refusal forever.
 *   `onFail`     Called on every failure, the first attempt and each retry alike, so the
 *                lock the gate took is released whatever the outcome.
 *
 * `test/call.test.tsx` owns the rest of the hook's contract.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { waitFor } from '@testing-library/react';
import { RETRY_STALE_MESSAGE } from '@/hooks/useCall';
import { TOAST_TIMEOUT_MS, fireToastAction, resetToasts } from '@/app/toast';
import { busy } from './helpers/api';
import { flakyAttempts, mountCall } from './helpers/call';
import { expectRefusalOnly, toasts } from './helpers/toasts';

beforeEach(() => {
  resetToasts();
});

describe('useCall: the Retry gate', () => {
  it('runs the request and its continuation again when the gate admits the Retry', async () => {
    const { result } = mountCall();
    const seen: string[] = [];
    const fn = flakyAttempts();
    const retryGate = vi.fn(() => true);

    await result.current(fn, (v: string) => { seen.push(v); }, { retryGate });

    expect(toasts()[0].label).toBe('Retry');
    fireToastAction(toasts()[0].id);
    expect(retryGate).toHaveBeenCalledTimes(1);

    // The request ran again AND the continuation ran with the retried value; the action
    // dismissed its own toast first.
    await waitFor(() => { expect([seen, fn.mock.calls.length, toasts()]).toEqual([['attempt-2'], 2, []]); });
  });

  it('sends nothing when the gate refuses the Retry, and says so once', async () => {
    const { result } = mountCall();
    const seen: string[] = [];
    const fn = vi.fn(async () => { throw busy(); });
    const retryGate = vi.fn(() => false);

    await result.current(fn, (v: string) => { seen.push(v); }, { retryGate });

    expect(toasts().length).toBe(1);
    fireToastAction(toasts()[0].id);

    // The request never ran again, and the continuation never ran at all.
    expect(retryGate).toHaveBeenCalledTimes(1);
    expect(fn).toHaveBeenCalledTimes(1);
    expect(seen).toEqual([]);
    // One toast: the refusal, and it carries no action of its own.
    expectRefusalOnly();
  });

  it('lets the refusal toast expire, because a refusal is not a way back', async () => {
    vi.useFakeTimers();
    const { result } = mountCall();
    const fn = vi.fn(async () => { throw busy(); });

    await result.current(fn, undefined, { retryGate: () => false });
    fireToastAction(toasts()[0].id);
    expect(toasts()[0].message).toBe(RETRY_STALE_MESSAGE);

    vi.advanceTimersByTime(TOAST_TIMEOUT_MS);

    expect(toasts()).toEqual([]);
    vi.useRealTimers();
  });

  it('calls onFail on the first failure and on every retried one, so a lock is released', async () => {
    const { result } = mountCall();
    const onFail = vi.fn();
    const fn = vi.fn(async () => { throw busy(); });

    await result.current(fn, undefined, { retryGate: () => true, onFail });
    expect(onFail).toHaveBeenCalledTimes(1);

    fireToastAction(toasts()[0].id);
    await waitFor(() => { expect(fn).toHaveBeenCalledTimes(2); });
    await waitFor(() => { expect(onFail).toHaveBeenCalledTimes(2); });
    // The retry failed too, so a fresh Retry stands, with the same gate behind it.
    expect(toasts()[0].label).toBe('Retry');
  });

  it('leaves a call with no gate exactly as it was, so no view has to opt in', async () => {
    const { result } = mountCall();
    let attempts = 0;
    const fn = vi.fn(async () => {
      attempts += 1;
      if (attempts === 1) throw busy();
      return 'ok';
    });

    await result.current(fn);
    fireToastAction(toasts()[0].id);

    await waitFor(() => { expect(fn).toHaveBeenCalledTimes(2); });
    expect(toasts()).toEqual([]);
  });
});
