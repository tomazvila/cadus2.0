/**
 * The out-of-React toast store and its host (S3).
 *
 * F-36-1b: an actionable toast never auto-dismisses. That toast IS the recovery affordance —
 * an auto-hide after 6 s deletes the only way back for a reader who looked away.
 *
 * The store lives outside React because `useCall` raises toasts from continuations that
 * resolve after a view unmounts, and React drops a `setState` on an unmounted component in
 * silence.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import { axe } from 'vitest-axe';
import {
  TOAST_TIMEOUT_MS,
  dismissToast,
  fireToastAction,
  resetToasts,
  toast,
  toastStore,
} from '@/app/toast';
import { ToastHost } from '@/app/ToastHost';
import { AXE_IN_JSDOM } from './axe';

const toasts = () => toastStore.getSnapshot();

beforeEach(() => {
  resetToasts();
});

describe('the toast store', () => {
  it('carries the default timeout of the 1.0 SPA', () => {
    expect(TOAST_TIMEOUT_MS).toBe(6000);
  });

  it('F-36-1b: an actionable toast never auto-dismisses', () => {
    vi.useFakeTimers();
    const onAction = vi.fn();
    toast('The service is busy.', { label: 'Retry', onAction });

    // No timer is armed at all — the strong form. A timer that fires and then checks would
    // still race a dismiss.
    expect(vi.getTimerCount()).toBe(0);

    vi.advanceTimersByTime(60_000);
    expect(toasts().length).toBe(1);
    expect(toasts()[0].label).toBe('Retry');
    expect(onAction).not.toHaveBeenCalled();
  });

  it('F-36-1b: an explicit timeout on an actionable toast is ignored', () => {
    vi.useFakeTimers();
    toast('Still busy.', { label: 'Retry', onAction: vi.fn(), timeout: 100 });
    vi.advanceTimersByTime(60_000);
    expect(toasts().length).toBe(1);
  });

  it('expires a plain toast after 6000 ms and not before', () => {
    vi.useFakeTimers();
    toast('Saved.', { kind: 'success' });

    vi.advanceTimersByTime(5_999);
    expect(toasts().length).toBe(1);

    vi.advanceTimersByTime(1);
    expect(toasts()).toEqual([]);
  });

  it('defaults to the error kind and takes an explicit one', () => {
    toast('Failed.');
    toast('Signed out.', { kind: 'info' });
    expect(toasts().map((t) => t.kind)).toEqual(['error', 'info']);
  });

  it('omits an absent label instead of writing undefined', () => {
    toast('Failed.');
    expect('label' in toasts()[0]).toBe(false);
    expect('onAction' in toasts()[0]).toBe(false);
  });

  it('the action dismisses first and fires after, so a retry does not stack', () => {
    const order: string[] = [];
    toast('Failed.', {
      label: 'Retry',
      onAction: () => { order.push(`fired with ${toasts().length} on screen`); },
    });

    fireToastAction(toasts()[0].id);
    expect(order).toEqual(['fired with 0 on screen']);
    expect(toasts()).toEqual([]);
  });

  it('dismisses by id and clears the pending timer with it', () => {
    vi.useFakeTimers();
    const dismiss = toast('Failed.');
    expect(vi.getTimerCount()).toBe(1);

    dismiss();
    expect(toasts()).toEqual([]);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('notifies its subscribers and releases them on unsubscribe', () => {
    let notices = 0;
    const off = toastStore.subscribe(() => { notices += 1; });

    toast('One.');
    expect(notices).toBe(1);

    off();
    toast('Two.');
    expect(notices).toBe(1);
  });

  it('is a no-op when a dismissed id is dismissed again', () => {
    let notices = 0;
    toast('One.');
    const off = toastStore.subscribe(() => { notices += 1; });
    const id = toasts()[0].id;

    dismissToast(id);
    dismissToast(id);
    expect(notices).toBe(1);
    off();
  });
});

describe('the toast host', () => {
  it('portals into the live region index.html ships, and never creates one', async () => {
    // `aria-live` announces a CHANGE of content. A region that mounts together with its first
    // toast is not a change, and a screen reader announces nothing.
    render(<ToastHost />);
    act(() => { toast('The service is busy.', { label: 'Retry', onAction: vi.fn() }); });

    const region = document.getElementById('toasts')!;
    expect(region.getAttribute('aria-live')).toBe('polite');
    expect(document.querySelectorAll('[aria-live="polite"]').length).toBe(1);
    expect(region.querySelectorAll('[role="status"]').length).toBe(1);
    expect(screen.getByRole('status').textContent).toContain('The service is busy.');
    expect(await axe(region, AXE_IN_JSDOM)).toHaveNoViolations();
  });

  it('F-36-1b: the Retry button is still on screen a minute later', () => {
    vi.useFakeTimers();
    const onAction = vi.fn();
    render(<ToastHost />);
    act(() => { toast('The service is busy.', { label: 'Retry', onAction }); });

    act(() => { vi.advanceTimersByTime(60_000); });
    const retry = screen.getByRole('button', { name: 'Retry' });

    act(() => { retry.click(); });
    expect(onAction).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('gives every toast a labelled dismiss control', () => {
    render(<ToastHost />);
    act(() => { toast('Failed.'); });

    act(() => { screen.getByRole('button', { name: 'Dismiss' }).click(); });
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('renders a toast raised after the view that asked for it is gone', () => {
    // The whole reason the store sits outside React: the 401 path navigates and THEN toasts.
    const view = render(<ToastHost />);
    act(() => { toast('Your session has expired — please sign in again.', { kind: 'info' }); });
    expect(screen.getByRole('status').textContent).toContain('Your session has expired');
    view.unmount();
    expect(document.getElementById('toasts')!.textContent).toBe('');
  });
});
