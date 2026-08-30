/**
 * The central request wrapper (S3).
 *
 * F-36-1: a Retry re-runs the request AND its continuation. Without the continuation riding
 * along, the awaiting caller already resolved `undefined` and bailed by the time Retry fires,
 * the retried response goes in the bin, and the view sits on its spinner forever.
 *
 * The 401 branch and the demo branch are here too, because both decide whether a learner
 * keeps the screen they are on.
 */
import { StrictMode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { ApiError, NETWORK_MESSAGE } from '@/api';
import {
  GENERIC_FAILURE_MESSAGE,
  SESSION_EXPIRED_MESSAGE,
  useCall,
  type CallDeps,
} from '@/hooks/useCall';
import { fireToastAction, resetToasts, toastStore } from '@/app/toast';

function mountCall(deps: Partial<CallDeps> = {}) {
  const onUnauthorized = vi.fn();
  const view = renderHook(
    (props: CallDeps) => useCall(props),
    {
      wrapper: StrictMode,
      initialProps: { demo: false, onUnauthorized, ...deps },
    },
  );
  return { ...view, onUnauthorized };
}

const toasts = () => toastStore.getSnapshot();

beforeEach(() => {
  // The store is a module-level singleton by design, so nothing may survive a test.
  resetToasts();
});

describe('useCall', () => {
  it('resolves the value and runs the continuation on success', async () => {
    const { result } = mountCall();
    const seen: string[] = [];

    const res = await result.current(async () => 'ok', (v) => { seen.push(v); });

    expect(res).toBe('ok');
    expect(seen).toEqual(['ok']);
    expect(toasts()).toEqual([]);
  });

  it('resolves undefined on a failure, which is what a view branches on', async () => {
    const { result } = mountCall();
    const seen: string[] = [];

    const res = await result.current(
      async () => { throw new ApiError(500, 'server_error', 'The service failed.'); },
      (v) => { seen.push(v); },
    );

    // Every view writes `if (!res)`. The continuation does NOT run on a failure.
    expect(res).toBeUndefined();
    expect(seen).toEqual([]);
  });

  it('F-36-1: a Retry re-runs the request AND its continuation', async () => {
    const { result } = mountCall();
    const seen: string[] = [];
    let attempts = 0;

    const fn = vi.fn(async () => {
      attempts += 1;
      if (attempts === 1) throw new ApiError(503, 'unavailable', 'The service is busy.');
      return `attempt-${attempts}`;
    });

    const first = await result.current(fn, (v: string) => { seen.push(v); });
    expect(first).toBeUndefined();
    expect(seen).toEqual([]);

    // One toast, and it carries the action.
    expect(toasts().length).toBe(1);
    expect(toasts()[0].message).toBe('The service is busy.');
    expect(toasts()[0].label).toBe('Retry');

    fireToastAction(toasts()[0].id);

    // The request ran again AND the continuation ran with the retried value.
    await waitFor(() => { expect(seen).toEqual(['attempt-2']); });
    expect(fn).toHaveBeenCalledTimes(2);
    // The action dismissed its own toast first, so a second failure does not stack.
    expect(toasts()).toEqual([]);
  });

  it('F-36-1: a Retry that fails again offers another Retry', async () => {
    const { result } = mountCall();
    const fn = vi.fn(async () => { throw new ApiError(503, 'unavailable', 'Still busy.'); });

    await result.current(fn);
    expect(toasts().length).toBe(1);

    fireToastAction(toasts()[0].id);
    await waitFor(() => { expect(fn).toHaveBeenCalledTimes(2); });
    expect(toasts().length).toBe(1);
    expect(toasts()[0].label).toBe('Retry');
  });

  it('routes a 401 to sign-in with no Retry, because there is nothing to retry', async () => {
    const { result, onUnauthorized } = mountCall();
    const seen: string[] = [];

    const res = await result.current(
      async () => { throw new ApiError(401, 'unauthorized', 'Unauthorized'); },
      (v) => { seen.push(v); },
    );

    expect(res).toBeUndefined();
    expect(seen).toEqual([]);
    expect(onUnauthorized).toHaveBeenCalledTimes(1);
    expect(toasts().length).toBe(1);
    expect(toasts()[0].message).toBe(SESSION_EXPIRED_MESSAGE);
    expect(toasts()[0].kind).toBe('info');
    expect(toasts()[0].onAction).toBeUndefined();
  });

  it('takes the Retry path on a 403, which is a different failure from an expired session', async () => {
    // `sessionExpired` is `status === 401` and nothing else. The CSRF layer answers
    // `403 cross_origin_rejected`, and that must not throw the learner out of the screen.
    const { result, onUnauthorized } = mountCall();

    await result.current(async () => {
      throw new ApiError(403, 'cross_origin_rejected', 'Refused.');
    });

    expect(onUnauthorized).not.toHaveBeenCalled();
    expect(toasts()[0].label).toBe('Retry');
  });

  it('never routes to sign-in in demo mode', async () => {
    const { result, onUnauthorized } = mountCall({ demo: true });

    await result.current(async () => { throw new ApiError(401, 'unauthorized', 'Unauthorized'); });

    expect(onUnauthorized).not.toHaveBeenCalled();
    expect(toasts()[0].label).toBe('Retry');
  });

  it('reads its deps through a ref, so a Retry long after the render is still correct', async () => {
    // The rule of this hook applies to the hook: a value captured at the render that created
    // the call goes stale. Here the demo flag flips between the call and its rejection.
    const { result, rerender, onUnauthorized } = mountCall({ demo: true });

    let reject: ((e: unknown) => void) | undefined;
    const pending = result.current(
      () => new Promise<string>((_, rej) => { reject = rej; }),
    );

    rerender({ demo: false, onUnauthorized });
    reject!(new ApiError(401, 'unauthorized', 'Unauthorized'));
    await pending;

    expect(onUnauthorized).toHaveBeenCalledTimes(1);
    expect(toasts()[0].message).toBe(SESSION_EXPIRED_MESSAGE);
  });

  it('lets a throw from the continuation propagate and raises no Retry', async () => {
    // A continuation throw is a VIEW defect, not a transport failure. A retry would re-send a
    // request the service already accepted, and a consumed serve answers 404 unknown_problem.
    const { result } = mountCall();
    const fn = vi.fn(async () => 'ok');

    await expect(
      result.current(fn, () => { throw new Error('the view is broken'); }),
    ).rejects.toThrow('the view is broken');

    expect(fn).toHaveBeenCalledTimes(1);
    expect(toasts()).toEqual([]);
  });

  it('shows the network message when fetch itself never reached the service', async () => {
    const { result } = mountCall();
    await result.current(async () => { throw new ApiError(0, 'network', NETWORK_MESSAGE); });
    expect(toasts()[0].message).toBe(NETWORK_MESSAGE);
  });

  it('falls back to one generic line when a foreign throw carries no message', async () => {
    const { result } = mountCall();
    await result.current(async () => { throw new Error(''); });
    expect(toasts()[0].message).toBe(GENERIC_FAILURE_MESSAGE);
  });

  it('keeps one call identity across renders, so a dependency array holds', () => {
    const { result, rerender, onUnauthorized } = mountCall();
    const first = result.current;
    rerender({ demo: true, onUnauthorized });
    expect(result.current).toBe(first);
  });
});
