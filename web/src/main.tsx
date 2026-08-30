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
 * Drop the spent token from the URL, so a refresh cannot replay it and no Referer carries
 * it to a third party.
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
  if (verifyToken || resetToken) stripBootTokens(pathname);
  if (!user) user = await currentUser(client);

  const root = createRoot(mount);
  root.render(
    <StrictMode>
      <Root
        api={client}
        initialUser={user}
        authMode={resetToken ? 'reset' : authModeFor(pathname)}
        resetToken={resetToken ?? ''}
      />
    </StrictMode>,
  );
  return root;
}

export function boot(): Promise<ReactRoot> {
  return bootWith(resolveApi(location.search), location.pathname, location.search);
}

// The guard keeps the module importable by the suite: a test drives `bootWith()` and
// `readBootParams()` directly, against its own DOM.
if (import.meta.env.MODE !== 'test') void boot();
