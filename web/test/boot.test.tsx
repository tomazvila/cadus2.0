/**
 * Boot and the single-use email tokens (S6).
 *
 * THE ACCEPTANCE CHECK OF THIS UNIT: a `?verify=` token is spent EXACTLY ONCE under
 * StrictMode. React 19 mounts, unmounts and remounts every component in development, so an
 * effect that posts the token posts it twice — the first POST verifies the account, the
 * second gets `invalid_token`, and the learner whose link just worked is told it expired.
 * `bootWith` spends it outside React, before `createRoot`, where there is no second mount.
 *
 * The double-invocation is asserted here rather than assumed: one test counts a mounted
 * effect's calls and reads 2. That number is the failure the boot path avoids.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, screen, waitFor } from '@testing-library/react';
import type { Root as ReactRoot } from 'react-dom/client';
import { ApiError, createDemoApi } from '@/api';
import type { ApiClient, User } from '@/api';
import { authModeFor, bootWith, readBootParams, stripBootTokens } from '@/main';
import { resetToasts, toastStore } from '@/app/toast';
import { setSearch } from './setup';

const USER: User = {
  id: 'u1',
  email: 'learner@example.com',
  email_verified: true,
  created_at: '2026-08-30T00:00:00Z',
};

/** Signed out: `/auth/me` answers 401, which is the normal signed-out reply. */
const SIGNED_OUT = () => {
  throw new ApiError(401, 'unauthorized', 'No session.');
};

function stub(overrides: Partial<ApiClient> = {}): ApiClient {
  return {
    ...createDemoApi(),
    demo: false,
    oauthProviders: async () => ({ providers: [] }),
    ...overrides,
  };
}

let root: ReactRoot | null = null;

/** Drive the real boot path and keep the root, so the tree is unmounted before the next test. */
async function boot(client: ApiClient, pathname: string, search: string): Promise<void> {
  await act(async () => {
    root = await bootWith(client, pathname, search);
  });
}

const messages = () => toastStore.getSnapshot().map((t) => t.message);

/** Type a new password into the reset card, press Set, and wait for the post. */
async function setNewPassword(resetPassword: ReturnType<typeof vi.fn>, token: string) {
  fireEvent.change(screen.getByLabelText('New password'), {
    target: { value: 'hunter2hunter2' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Set new password' }));

  await waitFor(() => {
    expect(resetPassword).toHaveBeenCalledWith(token, 'hunter2hunter2');
  });
}

beforeEach(() => {
  resetToasts();
  // `stripBootTokens` writes the real history, and the write outlives the test.
  history.replaceState({}, '', '/');
});

afterEach(async () => {
  const live = root;
  root = null;
  if (live) await act(async () => { live.unmount(); });
});

describe('the boot tokens', () => {
  it('a ?verify= token is spent exactly once under StrictMode', async () => {
    const verifyEmail = vi.fn(async () => ({ user: USER }));
    const me = vi.fn(async () => ({ user: USER }));

    await boot(stub({ verifyEmail, me }), '/', '?verify=tok-9');

    // ONE POST. Two is the bug this unit exists to prevent.
    expect(verifyEmail).toHaveBeenCalledTimes(1);
    expect(verifyEmail).toHaveBeenCalledWith('tok-9');
    // The reply carries the account and sets the cookie, so no `/auth/me` follows it — the
    // second read raced the verification write in 1.0 and reported the learner unverified.
    expect(me).not.toHaveBeenCalled();
    expect(messages()).toEqual(['Email verified — thanks!']);
    expect(toastStore.getSnapshot()[0].kind).toBe('info');
    // Verified and signed in: the shell shows the account, not the auth card.
    expect(screen.getByTitle('learner@example.com')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Sign in' })).toBeNull();
  });

  it('StrictMode invokes a mounted effect twice, which is why the token is not spent in one', async () => {
    const oauthProviders = vi.fn(async () => ({ providers: [] }));

    await boot(stub({ me: SIGNED_OUT, oauthProviders }), '/', '');

    // The auth card's provider probe runs in an effect, and StrictMode runs it twice. A
    // verify POST written the same way would spend the token twice.
    expect(oauthProviders).toHaveBeenCalledTimes(2);
  });

  it('strips the spent ?verify= token from the URL before the first render', async () => {
    // The browser is ON the email link, so the strip is observable rather than vacuous.
    history.replaceState({}, '', '/verify?token=tok-4');
    expect(window.location.search).toBe('?token=tok-4');
    const replace = vi.spyOn(history, 'replaceState');

    await boot(stub({ verifyEmail: async () => ({ user: USER }) }), '/verify', '?token=tok-4');

    // `/verify` has no screen of its own, so the spent link lands on the dashboard.
    expect(window.location.pathname).toBe('/');
    expect(window.location.search).toBe('');
    expect(replace).toHaveBeenCalledTimes(1);
    expect(replace).toHaveBeenCalledWith({}, '', '/');
  });

  it('toasts a generic line when the verify call fails for another reason', async () => {
    const verifyEmail = vi.fn(async () => { throw new Error('offline'); });

    await boot(stub({ verifyEmail, me: SIGNED_OUT }), '/', '?verify=tok-2');

    expect(messages()).toEqual(['Could not verify your email.']);
    expect(screen.getByRole('button', { name: 'Sign in' })).toBeTruthy();
  });

  it('calls a link expired on invalid_token alone, not on any service refusal', async () => {
    const verifyEmail = vi.fn(async () => {
      throw new ApiError(503, 'unavailable', 'Later.');
    });

    await boot(stub({ verifyEmail, me: SIGNED_OUT }), '/', '?verify=tok-3');

    expect(messages()).toEqual(['Could not verify your email.']);
    expect(toastStore.getSnapshot()[0].kind).toBe('error');
  });

  it('survives a blocked history write when it strips a token', () => {
    vi.spyOn(history, 'replaceState').mockImplementation(() => { throw new Error('blocked'); });
    expect(() => stripBootTokens('/verify')).not.toThrow();
  });

  it('toasts an expired ?verify= link and still boots to the sign-in card', async () => {
    const verifyEmail = vi.fn(async () => {
      throw new ApiError(400, 'invalid_token', 'Token spent.');
    });

    await boot(stub({ verifyEmail, me: SIGNED_OUT }), '/', '?verify=stale');

    expect(verifyEmail).toHaveBeenCalledTimes(1);
    expect(messages()).toEqual(['That verification link is invalid or has expired.']);
    expect(screen.getByRole('button', { name: 'Sign in' })).toBeTruthy();
  });

  it('opens the reset card with the token and leaves no token in the URL', async () => {
    const resetPassword = vi.fn(async () => ({ ok: true as const }));
    history.replaceState({}, '', '/reset?token=reset-3');

    await boot(stub({ me: SIGNED_OUT, resetPassword }), '/reset', '?token=reset-3');

    expect(screen.getByText('choose a new password')).toBeTruthy();
    // The path stays, because the reset card renders there. The token does not.
    expect(window.location.pathname).toBe('/reset');
    expect(window.location.search).toBe('');

    await setNewPassword(resetPassword, 'reset-3');
  });

  it('opens the reset card for a learner who still holds a session', async () => {
    // F22 (M6-review-1). Boot resolved the live session and `Root` rendered the signed-in
    // branch, so a learner who was still signed in on that browser reached the dashboard and
    // never saw the card the link is for. The token is read BEFORE the session, and it wins:
    // a reset link names one thing to do, and the account that holds the cookie is the same
    // account the link belongs to.
    const me = vi.fn(async () => ({ user: USER }));
    const resetPassword = vi.fn(async () => ({ ok: true as const }));
    history.replaceState({}, '', '/reset?token=reset-9');

    await boot(stub({ me, resetPassword }), '/reset', '?token=reset-9');

    expect(screen.getByText('choose a new password')).toBeTruthy();
    // The session is never read on this path, so nothing can outrank the card.
    expect(me).not.toHaveBeenCalled();

    await setNewPassword(resetPassword, 'reset-9');
    // The card took the token, so the URL no longer carries it.
    expect(window.location.pathname).toBe('/reset');
    expect(window.location.search).toBe('');
  });

  it('spends nothing when the URL carries no token', async () => {
    const verifyEmail = vi.fn(async () => ({ user: USER }));
    const resetPassword = vi.fn(async () => ({ ok: true as const }));
    const replace = vi.spyOn(history, 'replaceState');

    await boot(stub({ verifyEmail, resetPassword, me: SIGNED_OUT }), '/login', '');

    expect(verifyEmail).not.toHaveBeenCalled();
    expect(resetPassword).not.toHaveBeenCalled();
    // Nothing to strip, so the address bar is not touched.
    expect(replace).not.toHaveBeenCalled();
    expect(messages()).toEqual([]);
    expect(screen.getByText('sign in')).toBeTruthy();
  });
});

describe('the entry', () => {
  it('boots the live page once, against the client the URL names', async () => {
    // `?demo=1` hands the entry the demo client, so the boot needs no service: the demo
    // account is signed in and the dashboard is the first screen.
    setSearch('?demo=1');
    const { started } = await import('@/index');
    await act(async () => { root = await started; });

    await waitFor(() => expect(screen.getByText('Continue studying')).toBeTruthy());
    expect(screen.getByText('DEMO')).toBeTruthy();
  });
});

describe('reading the URL', () => {
  it('reads both spellings of a live email link', () => {
    // The 1.0 mailer writes `/?verify=`; the 2.0 route table writes `/verify?token=`.
    expect(readBootParams('/', '?verify=a')).toEqual({ verifyToken: 'a', resetToken: null });
    expect(readBootParams('/verify', '?token=b')).toEqual({ verifyToken: 'b', resetToken: null });
    expect(readBootParams('/', '?reset=c')).toEqual({ verifyToken: null, resetToken: 'c' });
    expect(readBootParams('/reset', '?token=d')).toEqual({ verifyToken: null, resetToken: 'd' });
  });

  it('reads a bare ?token= on no other route', () => {
    // A `token` outside `/verify` and `/reset` names nothing, and spending it as either
    // would burn the wrong single-use secret.
    expect(readBootParams('/login', '?token=e')).toEqual({ verifyToken: null, resetToken: null });
    expect(readBootParams('/', '')).toEqual({ verifyToken: null, resetToken: null });
  });

  it('maps a path to its auth card', () => {
    expect(authModeFor('/signup')).toBe('signup');
    expect(authModeFor('/forgot')).toBe('forgot');
    expect(authModeFor('/login')).toBe('login');
    expect(authModeFor('/anything-else')).toBe('login');
  });
});

describe('the signed-in switch', () => {
  it('swaps the auth card for the shell when the learner signs in', async () => {
    const login = vi.fn(async () => ({ user: USER }));

    await boot(stub({ me: SIGNED_OUT, login }), '/', '');

    expect(screen.queryByTitle('learner@example.com')).toBeNull();
    fireEvent.change(screen.getByLabelText('Email'), {
      target: { value: 'learner@example.com' },
    });
    fireEvent.change(screen.getByLabelText('Password'), {
      target: { value: 'hunter2hunter2' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));

    expect(await screen.findByTitle('learner@example.com')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Sign in' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Log out' })).toBeTruthy();
  });

  it('signs the learner out even when the logout call fails', async () => {
    const logout = vi.fn(async () => {
      throw new ApiError(0, 'network', 'offline');
    });

    await boot(stub({ me: async () => ({ user: USER }), logout }), '/', '');

    fireEvent.click(screen.getByRole('button', { name: 'Log out' }));

    expect(await screen.findByRole('button', { name: 'Sign in' })).toBeTruthy();
    expect(logout).toHaveBeenCalledTimes(1);
    expect(screen.queryByTitle('learner@example.com')).toBeNull();
  });
});
