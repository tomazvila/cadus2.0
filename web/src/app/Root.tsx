/**
 * The signed-in / signed-out switch.
 *
 * It holds the one piece of state the shell cannot: WHO is signed in. Boot resolves the
 * account once, before the first render, and hands it here; a sign-in on the auth card and
 * a sign-out from the topbar both move that value, and the tree below re-renders.
 *
 * There is still no URL router (spec section 4.1), so the signed-in branch renders the
 * shell's own placeholder card. The unit that adds routing replaces that branch with the
 * routed view and passes `key={route}` to the error boundary; nothing else here moves.
 */
import { useState } from 'react';
import { App } from './App';
import { Auth } from '@/views/Auth';
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
}

export function Root({ api, initialUser, authMode = 'login', resetToken = '' }: RootProps) {
  const [user, setUser] = useState<User | null>(initialUser);

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

  return (
    <App user={user} demo={api.demo} onLogout={() => void logout()}>
      {user ? undefined : (
        <Auth api={api} mode={authMode} token={resetToken} onSignedIn={setUser} />
      )}
    </App>
  );
}
