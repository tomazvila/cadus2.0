/**
 * The signed-in / signed-out switch.
 *
 * It holds the one piece of state the shell cannot: WHO is signed in. Boot resolves the
 * account once, before the first render, and hands it here; a sign-in on the auth card and
 * a sign-out from the topbar both move that value, and the tree below re-renders.
 *
 * THE TWO OPERATOR PATHS ARE ROUTED HERE, and nothing else is. Spec section 4.1 adds URL
 * routing for one stated reason — `/ops` and `/review` are pages an operator links to and
 * reloads — and `app/routes.ts` carries that reasoning. Every learner-facing screen is
 * still reached from another screen, so the signed-in branch renders the shell's own
 * placeholder card as before. The unit that routes those screens replaces that branch and
 * passes `key={route}` to the error boundary; the two lines below do not move.
 *
 * SIGNED OUT, EVERY PATH IS THE AUTH CARD, `/ops` and `/review` included. The operator
 * screens sit inside the signed-in branch, so a signed-out visitor to either path is asked
 * to sign in and reaches no admin call at all.
 */
import { useCallback, useState } from 'react';
import { App } from './App';
import { adminRouteFor } from './routes';
import { Auth } from '@/views/Auth';
import { OperatorScreen } from '@/views/admin/Ops';
import { ReviewScreen } from '@/views/admin/Review';
import type { AuthMode } from '@/views/Auth';
import type { ApiClient, User } from '@/api';

export interface RootProps {
  api: ApiClient;
  /** The account boot resolved, or null when nobody is signed in. */
  initialUser: User | null;
  /** Which auth card the URL asked for. */
  authMode?: AuthMode;
  /** The single-use reset token, already stripped from the URL by boot. */
  resetToken?: string;
  /** The path the page loaded on. Only the two operator routes read it. */
  pathname?: string;
}

export function Root({
  api,
  initialUser,
  authMode = 'login',
  resetToken = '',
  pathname = '/',
}: RootProps) {
  const [user, setUser] = useState<User | null>(initialUser);

  /**
   * The session-expired path every screen shares.
   *
   * It drops the account and nothing else: the tree below re-renders into the auth card,
   * which is where a caller with no session belongs. `useCall` raises the toast that says
   * so, so this must not raise a second one.
   */
  const onUnauthorized = useCallback(() => setUser(null), []);

  /**
   * Drop the session locally whatever the service says.
   *
   * A failed logout means the cookie is already gone, or the network is: in both cases the
   * learner asked to leave, and keeping them signed in on screen is the wrong answer. The
   * service revokes the row; the browser drops the `__Host-` cookie on its own reply.
   */
  async function logout(): Promise<void> {
    try {
      await api.logout();
    } catch {
      /* already gone, or offline — leave anyway */
    }
    setUser(null);
  }

  const route = adminRouteFor(pathname);

  return (
    <App user={user} demo={api.demo} onLogout={() => void logout()}>
      {!user ? (
        <Auth api={api} mode={authMode} token={resetToken} onSignedIn={setUser} />
      ) : route === 'ops' ? (
        <OperatorScreen api={api} demo={api.demo} onUnauthorized={onUnauthorized} />
      ) : route === 'review' ? (
        <ReviewScreen api={api} demo={api.demo} onUnauthorized={onUnauthorized} />
      ) : undefined}
    </App>
  );
}
