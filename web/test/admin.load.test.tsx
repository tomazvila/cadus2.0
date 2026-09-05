/**
 * The read path of the two operator screens (S12): `useAdminLoad`.
 *
 * A `403 forbidden` is an answer and is rendered; a 401 is the session-expired path,
 * outside demo mode; everything else is a fault with a Try again. A reply applies only
 * while it is the newest, and never after the view left.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, cleanup, renderHook, screen } from '@testing-library/react';
import { ApiError } from '@/api';
import { useAdminLoad } from '@/views/admin/adminLoad';
import { forbidden, mountReview, rowButtons, stubApi, QUEUE } from './helpers/admin';
import type { ReviewListResponse } from '@/api/types';

const unauthorized = () => {
  throw new ApiError(401, 'unauthorized', 'No session.');
};

/** A list read the test settles by hand, one attempt at a time. */
function heldList() {
  const held: Array<{ resolve: (q: ReviewListResponse) => void; reject: (e: Error) => void }> = [];
  const listContent = vi.fn(() => new Promise<ReviewListResponse>((resolve, reject) => {
    held.push({ resolve, reject });
  }));
  return { listContent, held };
}

describe('useAdminLoad', () => {
  it('routes a 401 to sign-in outside demo mode, and renders no refusal', async () => {
    const { onUnauthorized } = await mountReview(stubApi({ listContent: unauthorized }));
    expect(onUnauthorized).toHaveBeenCalledTimes(1);
    expect(screen.queryByText('No session.')).toBeNull();
    expect(screen.getByText('Loading the review queue…')).toBeTruthy();
  });

  it('keeps a 401 on the screen in demo mode, as a fault with a Try again', async () => {
    const { onUnauthorized } = await mountReview(stubApi({ listContent: unauthorized }), true);
    expect(onUnauthorized).not.toHaveBeenCalled();
    expect(screen.getByText('No session.')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeTruthy();
  });

  it('falls back to the generic line when the fault carries no message', async () => {
    await mountReview(stubApi({ listContent: async () => { throw new Error(''); } }));
    expect(screen.getByText('Could not load this screen.')).toBeTruthy();
  });

  it('routes no 401 that lands after the view left', async () => {
    const { listContent, held } = heldList();
    const view = await mountReview(stubApi({ listContent }));
    view.unmount();
    await act(async () => { held[0]!.reject(new ApiError(401, 'unauthorized', 'No session.')); });
    expect(view.onUnauthorized).not.toHaveBeenCalled();
  });

  it('renders no fault and no payload that lands after the view left', async () => {
    const { listContent, held } = heldList();
    const view = await mountReview(stubApi({ listContent }));
    view.unmount();
    await act(async () => { held[0]!.reject(new ApiError(500, 'server_error', 'Down.')); });
    expect(document.querySelector('.view-review')).toBeNull();

    cleanup();
    const again = await mountReview(stubApi({ listContent }));
    again.unmount();
    await act(async () => { held[1]!.resolve(QUEUE); });
    expect(document.querySelector('.review-row')).toBeNull();
  });

  it('lets only the newest attempt land, whether it fails or succeeds', async () => {
    const { listContent, held } = heldList();
    await mountReview(stubApi({ listContent }));
    // Refresh is disabled while the first read is out, so the second read comes from a
    // failure's Try again: fail the first, press it, and the two attempts are 0 and 1.
    await act(async () => { held[0]!.reject(new ApiError(500, 'server_error', 'Down.')); });
    await act(async () => { screen.getByRole('button', { name: 'Try again' }).click(); });
    // The Retry of the toast re-runs attempt 0's request: attempt 2, under generation 0.
    expect(listContent).toHaveBeenCalledTimes(2);

    // Attempt 1 lands with the queue; a stale failure and a stale success change nothing.
    await act(async () => { held[1]!.resolve(QUEUE); });
    expect(rowButtons()).toHaveLength(4);
  });

  it('keeps the payload on screen while a later read fails, and shows the failure', async () => {
    const { listContent, held } = heldList();
    await mountReview(stubApi({ listContent }));
    await act(async () => { held[0]!.resolve(QUEUE); });
    expect(rowButtons()).toHaveLength(4);

    await act(async () => { screen.getByRole('button', { name: 'Refresh' }).click(); });
    await act(async () => { held[1]!.reject(new ApiError(500, 'server_error', 'Down.')); });
    // The refusal block owns the screen, and the payload waits in the hook behind it.
    expect(screen.getByText('Down.')).toBeTruthy();
    await act(async () => { screen.getByRole('button', { name: 'Try again' }).click(); });
    await act(async () => { held[2]!.resolve({ ...QUEUE, items: QUEUE.items.slice(0, 1) }); });
    expect(rowButtons()).toHaveLength(1);
  });

  it('renders the forbidden block for the demo client too', async () => {
    await mountReview(stubApi({ listContent: forbidden }), true);
    expect(screen.getByText('This screen serves operator accounts')).toBeTruthy();
  });
});

describe('useAdminLoad, driven as a hook', () => {
  it('lets a stale reply and a stale fault pass behind a newer generation', async () => {
    const held: Array<{ resolve: (v: string) => void; reject: (e: Error) => void }> = [];
    const load = vi.fn(() => new Promise<string>((resolve, reject) => { held.push({ resolve, reject }); }));
    const { result } = renderHook(() => useAdminLoad({ load, demo: false, onUnauthorized: vi.fn() }));
    expect(result.current.loading).toBe(true);

    // Two more attempts while the first is still out: generations 1 and 2.
    act(() => { result.current.reload(); });
    act(() => { result.current.reload(); });
    expect(load).toHaveBeenCalledTimes(3);

    await act(async () => { held[2]!.resolve('newest'); });
    expect(result.current.data).toBe('newest');
    expect(result.current.loading).toBe(false);
    // The stale reply and the stale fault of the older generations change nothing.
    await act(async () => { held[1]!.resolve('older'); });
    await act(async () => { held[0]!.reject(new ApiError(500, 'server_error', 'Down.')); });
    expect(result.current.data).toBe('newest');
    expect(result.current.fault).toBeNull();
  });
});
