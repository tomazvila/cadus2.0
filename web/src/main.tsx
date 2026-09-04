/**
 * Boot.
 *
 * KaTeX is NOT imported from npm. It stays a `<link>` and two `<script>` tags to
 * `/vendor/katex/*`, injected by the build (see `vendor-tags.ts`). The npm package ships
 * woff2 AND woff AND ttf — a measured 1.5 MB of fonts where the vendored tree needs 600 KB
 * of woff2 — and the vendored tree is the CSP-audited 0.17.0 artifact. The `katex` package
 * stays a dev dependency for TESTS only: S5 runs the real auto-render against the render
 * idiom. Nothing in `src/` imports it.
 *
 * THE SINGLE-USE TOKENS ARE SPENT HERE, BEFORE `createRoot`, AND THIS IS THE POINT OF THE
 * UNIT. `?verify=` and `?reset=` are one-shot. React 19 StrictMode mounts, unmounts and
 * remounts every component in development, so an effect that posts the token posts it
 * TWICE: the first POST succeeds, the second gets `invalid_token`, and a learner whose link
 * just worked is told it expired. Outside React there is no second mount, so the token is
 * spent once by construction — in development, in production, and under the suite.
 */
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import type { Root as ReactRoot } from 'react-dom/client';
import { Root } from './app/Root';
import { toast } from './app/toast';
import { ApiError, resolveApi } from './api';
import type { ApiClient, User } from './api';
import type { AuthMode } from './views/Auth';
// The tokens come FIRST. `app.css` reads them and declares no color of its own, so a
// stylesheet loaded the other way round paints one frame of unstyled text.
import './styles/tokens.css';
import './styles/app.css';

/** The ids index.html provides. Each absence is a silent failure on its own. */
export const SHELL_IDS = ['view', 'topbar', 'toasts'] as const;

/**
 * Resolve the mount point, or say which part of the document is missing.
 *
 * Two of the three are worse than a missing view: without `#topbar` there is no way out of
 * a screen, and without `#toasts` every error the app reports — a failed grade, a lost
 * session — disappears with nothing on screen. A host that returns null for each of them
 * degrades three times over, so the precondition lives in one place.
 */
export function resolveMount(doc: Document): HTMLElement {
  const missing = SHELL_IDS.filter((id) => !doc.getElementById(id));
  if (missing.length) {
    throw new Error(`index.html is missing ${missing.map((id) => `#${id}`).join(', ')}`);
  }
  return doc.getElementById('view')!;
}

export interface BootParams {
  /** `?verify=<token>`, or `?token=` on the `/verify` route. Single-use. */
  verifyToken: string | null;
  /** `?reset=<token>`, or `?token=` on the `/reset` route. Single-use. */
  resetToken: string | null;
}

/**
 * Read the one-shot tokens out of the URL.
 *
 * TWO SPELLINGS, ON PURPOSE. The mailer of 1.0 links to `/?verify=<t>` and `/?reset=<t>`,
 * and the 2.0 route table (spec section 4.1) writes them as `/verify?token=` and
 * `/reset?token=`. Both forms are live mail in a learner's inbox, so both are read; `token`
 * counts only on the route that names it, or one link would spend the other's token.
 */
export function readBootParams(pathname: string, search: string): BootParams {
  const params = new URLSearchParams(search);
  const read = (name: string, route: string): string | null =>
    params.get(name) ?? (pathname === route ? params.get('token') : null);
  return { verifyToken: read('verify', '/verify'), resetToken: read('reset', '/reset') };
}

/** Which auth card a path asks for. An unknown path asks for sign-in. */
export function authModeFor(pathname: string): AuthMode {
  if (pathname === '/signup') return 'signup';
  if (pathname === '/forgot') return 'forgot';
  return 'login';
}

/**
 * Drop the token from the URL, so a refresh cannot replay it and no Referer carries it to a
 * third party.
 *
 * WHEN EACH OF THE TWO IS DROPPED. A `?verify=` token is spent by `bootWith` itself, so it
 * goes as soon as that POST returns. A `?reset=` token is spent by the reset CARD, so it
 * stays in the URL until `Root` hands it over and calls back (M6-review-1, F22); a token
 * dropped before the card is on is a token no card ever receives.
 *
 * `/verify` has no screen of its own — the account is verified and the session is open — so
 * it lands on the dashboard. `/reset` keeps its path, because the reset card renders there.
 */
export function stripBootTokens(pathname: string): void {
  try {
    history.replaceState({}, '', pathname === '/verify' ? '/' : pathname);
  } catch {
    /* a non-browser host, or a blocked history write */
  }
}

/**
 * Spend a `?verify=` token and say what happened.
 *
 * The reply carries the account AND sets the session cookie, so a success needs no
 * `/auth/me` after it: that second call is a round trip, and 1.0 recorded it racing the
 * verification write and reporting the just-verified learner as unverified.
 */
export async function spendVerifyToken(client: ApiClient, token: string): Promise<User | null> {
  try {
    const res = await client.verifyEmail(token);
    toast('Email verified — thanks!', { kind: 'info' });
    return res.user;
  } catch (e) {
    const expired = e instanceof ApiError && e.code === 'invalid_token';
    toast(
      expired
        ? 'That verification link is invalid or has expired.'
        : 'Could not verify your email.',
      { kind: 'error' },
    );
    return null;
  }
}

/** The signed-in account, or null. A 401 here is the normal signed-out answer. */
async function currentUser(client: ApiClient): Promise<User | null> {
  try {
    return (await client.me()).user;
  } catch {
    return null;
  }
}

/**
 * Boot against an explicit client and URL.
 *
 * Every input is an argument, so the suite drives the real boot path instead of a copy of
 * it. It gives back the React root, which a test unmounts.
 */
export async function bootWith(
  client: ApiClient,
  pathname: string,
  search: string,
): Promise<ReactRoot> {
  const mount = resolveMount(document);
  const { verifyToken, resetToken } = readBootParams(pathname, search);

  let user: User | null = null;
  if (verifyToken) user = await spendVerifyToken(client, verifyToken);
  // The verify token is spent by the call above, so it goes now. The reset token does not.
  if (verifyToken) stripBootTokens(pathname);

  // A RESET LINK OUTRANKS A LIVE SESSION, AND THE ORDER HERE IS WHAT MAKES IT SO
  // (M6-review-1, F22). The token is read before the session, and a URL that carries one
  // reads no session at all: `Root` shows the auth card while `user` is null, so a learner
  // who was still signed in on that browser reached the dashboard and never saw the card
  // the emailed link is for. The link names one thing to do, and the account holding the
  // cookie is the account the link belongs to.
  if (!user && !resetToken) user = await currentUser(client);

  const root = createRoot(mount);
  root.render(
    <StrictMode>
      <Root
        api={client}
        initialUser={user}
        authMode={resetToken ? 'reset' : authModeFor(pathname)}
        resetToken={resetToken ?? ''}
        // The seed of the location the router reads (`app/Root.tsx`). Only the two operator
        // routes name a screen (`app/routes.ts`); every other path, `/verify` included,
        // renders the same signed-in branch it rendered before.
        pathname={pathname}
        // The card has the token now, so the URL no longer needs it.
        onResetTokenTaken={() => { stripBootTokens(pathname); }}
      />
    </StrictMode>,
  );
  return root;
}

/** Boot against the live client and the URL of the page. `index.tsx` calls it once. */
export function boot(): Promise<ReactRoot> {
  return bootWith(resolveApi(location.search), location.pathname, location.search);
}
