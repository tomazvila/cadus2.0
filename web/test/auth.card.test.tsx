/**
 * The auth cards, control by control: what each field says, what each button does while a
 * request is out, and what a toggle clears.
 */
import { describe, expect, it, vi } from 'vitest';
import { cleanup, screen, waitFor } from '@testing-library/react';
import { ApiError } from '@/api';
import { Auth } from '@/views/Auth';
import { resetToasts, toastStore } from '@/app/toast';
import { held } from './helpers/held';
import { alertText, mount, press, stub, type } from './helpers/auth';
import type { SessionResponse } from '@/api/types';

const USER = {
  id: 'u1',
  email: 'learner@example.com',
  email_verified: true,
  created_at: '2026-08-30T00:00:00Z',
};

const field = (label: string) => screen.getByLabelText(label) as HTMLInputElement;
const alertBox = () => document.querySelector('.field-error') as HTMLElement;

describe('the fields', () => {
  it('spells nothing for the learner, and names each password box for its card', () => {
    mount(<Auth api={stub()} onSignedIn={vi.fn()} />);
    expect(field('Email').getAttribute('spellcheck')).toBe('false');
    expect(field('Password').placeholder).toBe('Password');
    expect(field('Password').autocomplete).toBe('current-password');
    expect(screen.getByText('New here?')).toBeTruthy();

    press('Create an account');
    expect(field('Password').placeholder).toBe('Choose a password (8+ characters)');
    expect(field('Password').autocomplete).toBe('new-password');
    expect(screen.getByText('Already have an account?')).toBeTruthy();
  });

  it('posts a sign-in whatever the length of the password', async () => {
    const login = vi.fn(async () => ({ user: USER }));
    mount(<Auth api={stub({ login })} onSignedIn={vi.fn()} />);
    type('Email', 'a@b.test');
    type('Password', 'short');
    press('Sign in');
    await waitFor(() => expect(login).toHaveBeenCalledWith('a@b.test', 'short'));
  });

  it('takes exactly eight characters for a new password, on sign-up and on reset', async () => {
    const signup = vi.fn(async () => ({ status: 'verification_required' as const, message: 'Check.' }));
    const first = mount(<Auth api={stub({ signup })} mode="signup" onSignedIn={vi.fn()} />);
    type('Email', 'a@b.test');
    type('Password', '12345678');
    press('Create account');
    await waitFor(() => expect(signup).toHaveBeenCalledWith('a@b.test', '12345678'));
    first.unmount();
    cleanup();

    const resetPassword = vi.fn(async () => ({ ok: true as const }));
    mount(<Auth api={stub({ resetPassword })} mode="reset" token="t" onSignedIn={vi.fn()} />);
    type('New password', '12345678');
    press('Set new password');
    await waitFor(() => expect(resetPassword).toHaveBeenCalledWith('t', '12345678'));
  });
});

describe('the focus each line hands back', () => {
  it('moves to the field the line names, from wherever the focus was', async () => {
    const login = vi.fn(async () => { throw new ApiError(401, 'invalid_credentials', 'Wrong.'); });
    mount(<Auth api={stub({ login })} onSignedIn={vi.fn()} />);

    field('Email').blur();
    press('Sign in');
    expect(await alertText()).toBe('Enter your email.');
    expect(document.activeElement).toBe(field('Email'));

    type('Email', 'a@b.test');
    field('Email').blur();
    press('Sign in');
    expect(await alertText()).toBe('Enter your password.');
    expect(document.activeElement).toBe(field('Password'));

    type('Password', 'hunter2hunter2');
    field('Password').blur();
    press('Sign in');
    await waitFor(() => expect(login).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(alertBox().hidden).toBe(false));
    expect(document.activeElement).toBe(field('Password'));

    press('Create an account');
    type('Password', 'short');
    field('Password').blur();
    press('Create account');
    expect(await alertText()).toBe('Password must be at least 8 characters.');
    expect(document.activeElement).toBe(field('Password'));
  });

  it('moves to the new password on the reset card, for a short one and for a refused one', async () => {
    const resetPassword = vi.fn(async () => { throw new ApiError(400, 'invalid_token', 'Spent.'); });
    mount(<Auth api={stub({ resetPassword })} mode="reset" token="t" onSignedIn={vi.fn()} />);

    type('New password', 'short');
    field('New password').blur();
    press('Set new password');
    expect(await alertText()).toBe('Password must be at least 8 characters.');
    expect(document.activeElement).toBe(field('New password'));

    type('New password', 'hunter2hunter2');
    field('New password').blur();
    press('Set new password');
    await waitFor(() => expect(resetPassword).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(alertBox().textContent).toContain('invalid or has expired'));
    expect(document.activeElement).toBe(field('New password'));
  });
});

describe('the busy state', () => {
  it('disables the submit while the sign-in is out, and frees it after', async () => {
    const reply = held<SessionResponse>();
    mount(<Auth api={stub({ login: () => reply.promise })} onSignedIn={vi.fn()} />);
    type('Email', 'a@b.test');
    type('Password', 'hunter2hunter2');
    press('Sign in');
    await waitFor(() => expect(screen.getByRole('button', { name: 'Sign in' }).hasAttribute('disabled')).toBe(true));
    reply.release({ user: USER });
    await waitFor(() => expect(screen.getByRole('button', { name: 'Sign in' }).hasAttribute('disabled')).toBe(false));
  });
});

describe('what a move clears', () => {
  it('clears the line of the last submit on the next one', async () => {
    const login = vi.fn(async () => ({ user: USER }));
    mount(<Auth api={stub({ login })} onSignedIn={vi.fn()} />);
    type('Email', 'a@b.test');
    press('Sign in');
    expect(await alertText()).toBe('Enter your password.');
    expect(alertBox().hidden).toBe(false);

    type('Password', 'hunter2hunter2');
    press('Sign in');
    await waitFor(() => expect(login).toHaveBeenCalledTimes(1));
    expect(alertBox().hidden).toBe(true);
    expect(alertBox().textContent).toBe('');
  });

  it('clears the line when the learner moves to another card', async () => {
    mount(<Auth api={stub()} onSignedIn={vi.fn()} />);
    press('Sign in');
    expect(await alertText()).toBe('Enter your email.');

    press('Forgot password?');
    expect(alertBox().hidden).toBe(true);
    press('Email me a reset link');
    expect(await alertText()).toBe('Enter your email.');
    press('Back to sign in');
    expect(alertBox().hidden).toBe(true);

    press('Create an account');
    press('Create account');
    expect(await alertText()).toBe('Enter your email.');
    press('Sign in');
    expect(alertBox().hidden).toBe(true);
  });

  it('lands on a clean sign-in card after the reset, with the news in an info toast', async () => {
    resetToasts();
    mount(<Auth api={stub({ resetPassword: async () => ({ ok: true as const }) })} mode="reset" token="t" onSignedIn={vi.fn()} />);
    type('New password', 'hunter2hunter2');
    press('Set new password');
    expect(await screen.findByRole('button', { name: 'Sign in' })).toBeTruthy();
    expect(screen.getByText('sign in')).toBeTruthy();
    expect(field('Password').value).toBe('');
    expect(toastStore.getSnapshot()).toEqual([
      { id: 1, message: 'Password updated — sign in with your new password.', kind: 'info' },
    ]);
  });

  it('comes back to sign in from the sent card', async () => {
    mount(<Auth api={stub()} mode="forgot" onSignedIn={vi.fn()} />);
    type('Email', 'a@b.test');
    press('Email me a reset link');
    expect(await screen.findByText(/a password-reset link is on its way/)).toBeTruthy();
    press('Back to sign in');
    expect(screen.getByText('sign in')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Sign in' })).toBeTruthy();
  });
});
