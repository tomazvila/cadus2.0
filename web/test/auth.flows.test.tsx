/**
 * The auth screens (S6), part 2: the toggles, the recovery paths and the field checks.
 *
 * `auth.test.tsx` carries the module note.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import { ApiError, createDemoApi } from '@/api';
import type { ApiClient } from '@/api';
import { Auth, messageFor } from '@/views/Auth';
import { resetToasts, toastStore } from '@/app/toast';
import { alertText, mount, press, stub, type } from './helpers/auth';

const messages = () => toastStore.getSnapshot().map((t) => t.message);

beforeEach(() => {
  resetToasts();
});

describe('the toggles', () => {
  it('moves between sign in and create account, and keeps the typed email', () => {
    mount(<Auth api={stub()} />);
    type('Email', 'learner@example.com');
    type('Password', 'hunter2');

    press('Create an account');
    expect(screen.getByText('create account')).toBeTruthy();
    expect((screen.getByLabelText('Email') as HTMLInputElement).value).toBe('learner@example.com');
    // The password field resets, and it takes the focus: the email is already filled.
    expect((screen.getByLabelText('Password') as HTMLInputElement).value).toBe('');
    expect(document.activeElement).toBe(screen.getByLabelText('Password'));

    press('Sign in');
    expect(screen.getByText('sign in')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Sign in' })).toBeTruthy();
  });

  it('opens the forgot card from sign in, and comes back to sign in', () => {
    mount(<Auth api={stub()} />);
    type('Password', 'hunter2');
    press('Forgot password?');

    expect(screen.getByText('reset your password')).toBeTruthy();
    expect(document.activeElement).toBe(screen.getByLabelText('Email'));

    press('Back to sign in');
    expect(screen.getByRole('button', { name: 'Sign in' })).toBeTruthy();
    // A live credential is not left in the form the learner walked away from.
    expect((screen.getByLabelText('Password') as HTMLInputElement).value).toBe('');
  });
});

describe('the field checks', () => {
  it('refuses an empty password without a request', async () => {
    const login = vi.fn(createDemoApi().login);
    mount(<Auth api={stub({ login })} />);
    type('Email', 'learner@example.com');
    press('Sign in');

    expect(await alertText()).toBe('Enter your password.');
    expect(document.activeElement).toBe(screen.getByLabelText('Password'));
    expect(login).not.toHaveBeenCalled();
  });

  it('refuses an empty email on the forgot card without a request', async () => {
    const forgotPassword = vi.fn(async () => ({ ok: true as const }));
    mount(<Auth api={stub({ forgotPassword })} mode="forgot" />);
    press('Email me a reset link');

    expect(await alertText()).toBe('Enter your email.');
    expect(forgotPassword).not.toHaveBeenCalled();
  });

  it('renders a failed forgot request inline', async () => {
    const forgotPassword = vi.fn(async () => { throw new ApiError(429, 'rate_limited', 'Slow down.'); });
    mount(<Auth api={stub({ forgotPassword })} mode="forgot" />);
    type('Email', 'learner@example.com');
    press('Email me a reset link');

    expect(await alertText()).toBe('Too many attempts. Please wait a minute, then try again.');
    expect(screen.getByRole('button', { name: 'Email me a reset link' })).toBeTruthy();
  });

  it('refuses a short new password on the reset card without a request', async () => {
    const resetPassword = vi.fn(async () => ({ ok: true as const }));
    mount(<Auth api={stub({ resetPassword })} mode="reset" token="t" />);
    type('New password', 'short');
    press('Set new password');

    expect(await alertText()).toBe('Password must be at least 8 characters.');
    expect(document.activeElement).toBe(screen.getByLabelText('New password'));
    expect(resetPassword).not.toHaveBeenCalled();
  });
});

describe('the check-email card', () => {
  /** Sign up and land on the check-email card. */
  function signUp(overrides: Partial<ApiClient> = {}) {
    mount(<Auth api={stub({
      signup: async () => ({ status: 'verification_required' as const, message: 'Check.' }),
      ...overrides,
    })} mode="signup" />);
    type('Email', 'new@example.com');
    type('Password', 'hunter2hunter2');
    press('Create account');
  }

  it('re-sends the verification email to the address it shows', async () => {
    const resendVerification = vi.fn(async () => ({ ok: true as const }));
    signUp({ resendVerification });
    await screen.findByText('check your email');

    press('Resend email');
    await waitFor(() => expect(resendVerification).toHaveBeenCalledWith('new@example.com'));
    await waitFor(() => expect(messages()).toEqual(['Verification email re-sent.']));
    expect(toastStore.getSnapshot()[0].kind).toBe('success');
  });

  it('says so, quietly, when the re-send fails', async () => {
    const resendVerification = vi.fn(async () => { throw new ApiError(429, 'rate_limited', 'Slow.'); });
    signUp({ resendVerification });
    await screen.findByText('check your email');

    press('Resend email');
    await waitFor(() => expect(messages()).toEqual([
      'Could not resend right now — please try again shortly.',
    ]));
    expect(toastStore.getSnapshot()[0].kind).toBe('info');
  });

  it('swallows a submit of the card, and goes back to sign in on request', async () => {
    signUp();
    const form = (await screen.findByText('check your email')).closest('form')!;
    const submit = new Event('submit', { bubbles: true, cancelable: true });
    form.dispatchEvent(submit);
    expect(submit.defaultPrevented).toBe(true);

    press('Back to sign in');
    expect(screen.getByRole('button', { name: 'Sign in' })).toBeTruthy();
  });
});

describe('the sent card', () => {
  it('swallows a submit of the card', async () => {
    mount(<Auth api={stub({ forgotPassword: async () => ({ ok: true as const }) })} mode="forgot" />);
    type('Email', 'learner@example.com');
    press('Email me a reset link');
    const form = (await screen.findByText(/a password-reset link is on its way/)).closest('form')!;
    const submit = new Event('submit', { bubbles: true, cancelable: true });
    form.dispatchEvent(submit);
    expect(submit.defaultPrevented).toBe(true);
  });
});

describe('the message map, the rest of it', () => {
  it('reads the service line for weak_password and network, and falls back when it is absent', () => {
    expect(messageFor(new ApiError(400, 'weak_password', 'Too common.'))).toBe('Too common.');
    // An `ApiError` always carries a message; a foreign throw with the code alone does not.
    expect(messageFor({ code: 'weak_password' })).toBe(
      'Choose a stronger password (at least 8 characters).',
    );
    expect(messageFor(new ApiError(0, 'network', 'Offline.'))).toBe('Offline.');
    expect(messageFor({ code: 'network' })).toBe(
      'Network error — check your connection and try again.',
    );
  });
});
