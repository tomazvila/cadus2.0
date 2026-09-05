/**
 * The auth screens (S6).
 *
 * Two invariants live here and they are the reason the file exists.
 *
 * AUTH-inline. `invalid_credentials` is a 401, byte for byte the status an expired session
 * carries. Routed through `useCall` it takes the session-expired path: a toast reading "Your
 * session has expired", and a navigation to the sign-in screen the learner is already
 * looking at. The test proves the same error object BOTH satisfies `sessionExpired` AND
 * lands as a line beside the password field, because that pair is the whole trap.
 *
 * AUTH-7. A provider the service does not advertise gets no button. The list is the only
 * input: an empty list renders no divider, no button, and no dead link to a start route that
 * answers a redirect to nowhere.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, screen, waitFor } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { ApiError } from '@/api';
import { Auth, messageFor, providerLabel } from '@/views/Auth';
import { SESSION_EXPIRED_MESSAGE } from '@/hooks/useCall';
import { resetToasts, toastStore } from '@/app/toast';
import { AXE_IN_JSDOM } from './axe';
import { alertText, mount, press, stub, type } from './helpers/auth';
import { USER } from './helpers/fixtures';
import { navigations } from './setup';

const oauthNames = () =>
  screen
    .queryAllByRole('button')
    .map((b) => b.textContent ?? '')
    .filter((t) => t.startsWith('Continue with'));

beforeEach(() => {
  resetToasts();
});

describe('the sign-in card', () => {
  it('opens on sign in, with the email field focused', async () => {
    mount(<Auth api={stub({ oauthProviders: async () => ({ providers: [] }) })} onSignedIn={vi.fn()} />);

    expect(screen.getByText('sign in')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Sign in' })).toBeTruthy();
    // Focus moves on every transition (spec section 4.5); on this screen that is the field
    // the learner still has to fill.
    expect(document.activeElement).toBe(screen.getByLabelText('Email'));
    // Nothing is announced before there is something to announce.
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('AUTH-inline: invalid_credentials renders beside the field and never routes to session-expired', async () => {
    const failure = new ApiError(401, 'invalid_credentials', 'Bad credentials.');
    // The trap in one line: this error IS a session-expired error to the central wrapper.
    expect(failure.sessionExpired).toBe(true);

    const onSignedIn = vi.fn();
    const login = vi.fn(async () => { throw failure; });
    mount(<Auth api={stub({ login, oauthProviders: async () => ({ providers: [] }) })} onSignedIn={onSignedIn} />);

    type('Email', 'learner@example.com');
    type('Password', 'wrong-password');
    press('Sign in');

    expect(await alertText()).toBe('Incorrect email or password.');
    // The learner is still on the sign-in card, and nothing navigated.
    expect(screen.getByRole('button', { name: 'Sign in' })).toBeTruthy();
    expect(navigations).toEqual([]);
    expect(onSignedIn).not.toHaveBeenCalled();
    // The session-expired path always toasts. No toast was raised at all.
    expect(toastStore.getSnapshot()).toEqual([]);
    expect(toastStore.getSnapshot().map((t) => t.message)).not.toContain(SESSION_EXPIRED_MESSAGE);
    expect(login).toHaveBeenCalledWith('learner@example.com', 'wrong-password');
  });

  it('AUTH-inline: a rate-limited attempt also stays on the card', async () => {
    const login = vi.fn(async () => { throw new ApiError(429, 'rate_limited', 'Slow down.'); });
    mount(<Auth api={stub({ login, oauthProviders: async () => ({ providers: [] }) })} onSignedIn={vi.fn()} />);

    type('Email', 'learner@example.com');
    type('Password', 'hunter2hunter2');
    press('Sign in');

    expect(await alertText()).toBe('Too many attempts. Please wait a minute, then try again.');
    expect(toastStore.getSnapshot()).toEqual([]);
  });

  it('hands the account to onSignedIn and posts the trimmed address', async () => {
    const onSignedIn = vi.fn();
    const login = vi.fn(async () => ({ user: USER }));
    mount(<Auth api={stub({ login, oauthProviders: async () => ({ providers: [] }) })} onSignedIn={onSignedIn} />);

    type('Email', '  learner@example.com  ');
    type('Password', 'hunter2hunter2');
    press('Sign in');

    await waitFor(() => { expect(onSignedIn).toHaveBeenCalledWith(USER); });
    expect(login).toHaveBeenCalledWith('learner@example.com', 'hunter2hunter2');
  });

  it('refuses an empty field without a request', async () => {
    const login = vi.fn(async () => ({ user: USER }));
    mount(<Auth api={stub({ login, oauthProviders: async () => ({ providers: [] }) })} onSignedIn={vi.fn()} />);

    press('Sign in');
    expect(await alertText()).toBe('Enter your email.');
    expect(login).not.toHaveBeenCalled();
  });

  it('reports zero axe violations on the sign-in card', async () => {
    mount(<Auth api={stub({ oauthProviders: async () => ({ providers: ['google'] }) })} onSignedIn={vi.fn()} />);
    await screen.findByRole('button', { name: 'Continue with Google' });
    expect(await axe(document.body, AXE_IN_JSDOM)).toHaveNoViolations();
  });
});

describe('the OAuth buttons', () => {
  it('AUTH-7: an unconfigured provider renders no button', async () => {
    const oauthStartUrl = vi.fn(() => '/api/auth/oauth/google/start');
    // Nothing is configured, so the service advertises nothing.
    mount(<Auth api={stub({ oauthProviders: async () => ({ providers: [] }), oauthStartUrl })} onSignedIn={vi.fn()} />);

    await screen.findByRole('button', { name: 'Sign in' });
    expect(oauthNames()).toEqual([]);
    expect(document.querySelector('.auth-divider')).toBeNull();
    // No URL is even built: a start route for an unconfigured provider is a dead link.
    expect(oauthStartUrl).not.toHaveBeenCalled();
  });

  it('AUTH-7: an advertised provider renders one button that navigates to its start route', async () => {
    const providers = vi.fn(async () => ({ providers: ['google'] }));
    mount(<Auth api={stub({ oauthProviders: providers })} onSignedIn={vi.fn()} />);

    const button = await screen.findByRole('button', { name: 'Continue with Google' });
    expect(oauthNames()).toEqual(['Continue with Google']);
    expect(document.querySelector('.auth-divider')?.textContent).toBe('or');

    fireEvent.click(button);
    // A whole-page navigation, because the provider answers a cross-origin redirect.
    expect(navigations).toEqual(['/api/auth/oauth/google/start']);
  });

  it('AUTH-7: a failed provider probe renders no button and keeps the password form', async () => {
    mount(<Auth api={stub({ oauthProviders: async () => { throw new ApiError(0, 'network', 'down'); } })} onSignedIn={vi.fn()} />);

    await screen.findByRole('button', { name: 'Sign in' });
    expect(oauthNames()).toEqual([]);
    expect(screen.getByLabelText('Password')).toBeTruthy();
  });
});

describe('sign-up', () => {
  it('lands on check-email and signs nobody in', async () => {
    const onSignedIn = vi.fn();
    const signup = vi.fn(async () => ({
      status: 'verification_required' as const,
      message: 'Check your email.',
    }));
    mount(<Auth api={stub({ signup, oauthProviders: async () => ({ providers: [] }) })} mode="signup" onSignedIn={onSignedIn} />);

    type('Email', 'new@example.com');
    type('Password', 'hunter2hunter2');
    press('Create account');

    expect(await screen.findByText('check your email')).toBeTruthy();
    expect(screen.getByText('new@example.com')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Resend email' })).toBeTruthy();
    // Non-enumerable sign-up opens no session: the emailed link does.
    expect(onSignedIn).not.toHaveBeenCalled();
    expect(signup).toHaveBeenCalledWith('new@example.com', 'hunter2hunter2');
  });

  it('refuses a password under eight characters without a request', async () => {
    const signup = vi.fn(async () => ({ status: 'verification_required' as const, message: '' }));
    mount(<Auth api={stub({ signup, oauthProviders: async () => ({ providers: [] }) })} mode="signup" onSignedIn={vi.fn()} />);

    type('Email', 'new@example.com');
    type('Password', 'short');
    press('Create account');

    expect(await alertText()).toBe('Password must be at least 8 characters.');
    expect(signup).not.toHaveBeenCalled();
  });

  it('renders weak_password from the service verbatim', async () => {
    const signup = vi.fn(async () => {
      throw new ApiError(400, 'weak_password', 'That password is in a breach list.');
    });
    mount(<Auth api={stub({ signup, oauthProviders: async () => ({ providers: [] }) })} mode="signup" onSignedIn={vi.fn()} />);

    type('Email', 'new@example.com');
    type('Password', 'password1234');
    press('Create account');

    expect(await alertText()).toBe('That password is in a breach list.');
  });
});

describe('the recovery flows', () => {
  it('answers the same confirmation for any address', async () => {
    const forgotPassword = vi.fn(async () => ({ ok: true as const }));
    mount(<Auth api={stub({ forgotPassword, oauthProviders: async () => ({ providers: [] }) })} mode="forgot" onSignedIn={vi.fn()} />);

    type('Email', 'stranger@example.com');
    press('Email me a reset link');

    expect(
      await screen.findByText(/If an account exists for stranger@example.com/),
    ).toBeTruthy();
    expect(forgotPassword).toHaveBeenCalledWith('stranger@example.com');
  });

  it('posts the reset token boot captured, then returns to sign in', async () => {
    const resetPassword = vi.fn(async () => ({ ok: true as const }));
    mount(<Auth api={stub({ resetPassword, oauthProviders: async () => ({ providers: [] }) })} mode="reset" token="reset-token-7" onSignedIn={vi.fn()} />);

    expect(document.activeElement).toBe(screen.getByLabelText('New password'));
    type('New password', 'hunter2hunter2');
    press('Set new password');

    expect(await screen.findByRole('button', { name: 'Sign in' })).toBeTruthy();
    expect(resetPassword).toHaveBeenCalledWith('reset-token-7', 'hunter2hunter2');
    expect(toastStore.getSnapshot().map((t) => t.message)).toEqual([
      'Password updated — sign in with your new password.',
    ]);
  });

  it('renders an expired reset link inline', async () => {
    const resetPassword = vi.fn(async () => {
      throw new ApiError(400, 'invalid_token', 'Token spent.');
    });
    mount(<Auth api={stub({ resetPassword, oauthProviders: async () => ({ providers: [] }) })} mode="reset" token="stale" onSignedIn={vi.fn()} />);

    type('New password', 'hunter2hunter2');
    press('Set new password');

    expect(await alertText()).toBe(
      'That link is invalid or has expired — request a new one below.',
    );
    expect(toastStore.getSnapshot()).toEqual([]);
  });
});

describe('the message map', () => {
  it.each([
    ['invalid_credentials', 'Incorrect email or password.'],
    ['invalid_token', 'That link is invalid or has expired — request a new one below.'],
    ['rate_limited', 'Too many attempts. Please wait a minute, then try again.'],
  ])('maps %s to its own line', (code, line) => {
    expect(messageFor(new ApiError(401, code, 'raw server text'))).toBe(line);
  });

  it('falls back to the service message for a code it does not know', () => {
    expect(messageFor(new ApiError(500, 'teapot', 'The service is a teapot.'))).toBe(
      'The service is a teapot.',
    );
    expect(messageFor(null)).toBe('Something went wrong. Please try again.');
  });

  it('titles a provider id for its button', () => {
    expect(providerLabel('google')).toBe('Google');
    expect(providerLabel('github')).toBe('Github');
  });
});
