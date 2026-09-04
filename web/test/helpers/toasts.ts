/**
 * The toast assertions every stale-Retry test makes.
 *
 * A refused Retry leaves ONE plain toast: the refusal line, with no action, so it expires
 * (F-36-1b). A Retry pressed after the view is gone leaves the same refusal and re-arms
 * nothing (F-37-1b).
 */
import { expect } from 'vitest';
import { act } from '@testing-library/react';
import { RETRY_STALE_MESSAGE } from '@/hooks/useCall';
import { fireToastAction, toastStore } from '@/app/toast';

export const toasts = () => toastStore.getSnapshot();

/** The failure armed ONE Retry, and the submit control is live again behind it. */
export function expectRetryArmed(submitButton: () => HTMLElement): void {
  expect(toasts().length).toBe(1);
  expect(toasts()[0].label).toBe('Retry');
  expect(submitButton().hasAttribute('disabled')).toBe(false);
}

/** One toast on screen: the refusal, plain, of the info kind. */
export function expectRefusalOnly(): void {
  expect(toasts().length).toBe(1);
  expect(toasts()[0].message).toBe(RETRY_STALE_MESSAGE);
  expect(toasts()[0].kind).toBe('info');
  expect(toasts()[0].label).toBeUndefined();
  expect(toasts()[0].onAction).toBeUndefined();
}

/** Press the Retry the last failure armed, and let the retried request settle. */
export async function pressRetry(): Promise<void> {
  await act(async () => { fireToastAction(toasts().find((t) => t.label === 'Retry')!.id); });
}

/** Unmount the view, then press the Retry its last failure armed. */
export async function pressRetryAfterUnmount(unmount: () => void): Promise<void> {
  const stale = toasts().find((t) => t.label === 'Retry')!;
  expect(stale).toBeTruthy();
  unmount();
  await act(async () => { fireToastAction(stale.id); });
}

/** The refusal is on screen, and no toast on screen carries an action any more. */
export function expectRefusalAmongPlainToasts(): void {
  expect(toasts().some((t) => t.message === RETRY_STALE_MESSAGE)).toBe(true);
  expect(toasts().every((t) => t.onAction === undefined)).toBe(true);
}
