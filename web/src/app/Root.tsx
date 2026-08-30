/**
 * The signed-in / signed-out switch, and the router the learner screens are reached by.
 *
 * It holds the two pieces of state the shell cannot: WHO is signed in, and WHICH screen is
 * on. Boot resolves the account once, before the first render, and hands it here; a sign-in
 * on the auth card and a sign-out from the topbar both move that value, and the tree below
 * re-renders.
 *
 * TWO ROUTERS, AND THE SPLIT IS THE ONE `app/routes.ts` ARGUES FOR.
 *
 *  * `/ops` and `/review` come from the URL. Spec section 4.1 adds URL routing for exactly
 *    these two: they are pages an operator links to and reloads.
 *  * Every learner screen is a VIEW NAME, as it was in 1.0. Each one is reached from
 *    another screen — the dashboard starts the session, the session hands the quiz over —
 *    so none of them is ever typed, pasted, or bookmarked.
 *
 * The quiz carries its PLAN TASK, not a task id, and that is the reason the view name holds
 * a payload at all. The whole-quiz clock reads `task.time_budget_secs` (QUIZ-budget); a
 * screen reached by id alone would have to re-read the plan to find the task, and a re-plan
 * between the two calls hands it a different one.
 *
 * The map remembers WHERE IT WAS OPENED FROM. The topbar offers it from every screen, so a
 * map that always exited to the dashboard would throw away the quiz or the placement the
 * learner was in the middle of.
 *
 * SIGNED OUT, EVERY PATH IS THE AUTH CARD, `/ops` and `/review` included. The operator
 * screens sit inside the signed-in branch, so a signed-out visitor to either path is asked
 * to sign in and reaches no admin call at all.
 */
import { useCallback, useState } from 'react';
import { App } from './App';
import { adminRouteFor } from './routes';
import { Auth } from '@/views/Auth';
import { Dashboard } from '@/views/Dashboard';
import { Diagnostic } from '@/views/Diagnostic';
import { Quiz } from '@/views/Quiz';
import { Session } from '@/views/session/Session';
import { CurriculumMap } from '@/views/map/Map';
import { OperatorScreen } from '@/views/admin/Ops';
import { ReviewScreen } from '@/views/admin/Review';
import { resolveDiag } from '@/api/diag';
import type { DiagnosticApi } from '@/api/diag';
import type { AuthMode } from '@/views/Auth';
import type { ApiClient, PlanTask, User } from '@/api';

/**
 * The screen on, and everything that screen needs.
 *
 * A discriminated union, so the quiz cannot be shown without its task and the map cannot be
 * shown without somewhere to return to. Both facts are unrepresentable rather than checked.
 */
export type View =
  | { name: 'dashboard' }
  | { name: 'session' }
  | { name: 'quiz'; task: PlanTask; fromSession: boolean }
  | { name: 'diagnostic' }
  | { name: 'map'; back: View };

/** The dashboard, which is where every exit path ends. */
export const HOME: View = { name: 'dashboard' };

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
  /**
   * The placement port (`api/diag.ts`).
   *
   * Absent, it is resolved once from `api.demo`. A test passes its own.
   */
  diag?: DiagnosticApi;
  /** The screen the first render shows. A test starts on the one it is about. */
  initialView?: View;
}

export function Root({
  api,
  initialUser,
  authMode = 'login',
  resetToken = '',
  pathname = '/',
  diag,
  initialView = HOME,
}: RootProps) {
  const [user, setUser] = useState<User | null>(initialUser);
  const [view, setView] = useState<View>(initialView);
  // ONCE per mount. Both adapters hold state — the demo one counts the probes it asked —
  // so a port rebuilt on every render restarts the placement at probe 1 forever.
  const [placement] = useState<DiagnosticApi>(() => diag ?? resolveDiag(!!api.demo));

  const home = useCallback(() => { setView(HOME); }, []);

  /**
   * The session-expired path every screen shares.
   *
   * It drops the account and nothing else: the tree below re-renders into the auth card,
   * which is where a caller with no session belongs. `useCall` raises the toast that says
   * so, so this must not raise a second one.
   */
  const onUnauthorized = useCallback(() => { setUser(null); }, []);

  /**
   * Drop the session locally whatever the service says.
   *
   * A failed logout means the cookie is already gone, or the network is: in both cases the
   * learner asked to leave, and keeping them signed in on screen is the wrong answer. The
   * service revokes the row; the browser drops the `__Host-` cookie on its own reply.
   *
   * The view returns home in the same move. A sign-in that landed back on the quiz of the
   * account that just left would read the previous learner's task.
   */
  async function logout(): Promise<void> {
    try {
      await api.logout();
    } catch {
      /* already gone, or offline — leave anyway */
    }
    setUser(null);
    setView(HOME);
  }

  const route = adminRouteFor(pathname);
  const screen = adminOr(route, view);

  return (
    <App
      user={user}
      demo={api.demo}
      routeKey={screen}
      onHome={home}
      // The map opens over whatever is on screen and gives that screen back on Done.
      onMap={() => { setView((from) => (from.name === 'map' ? from : { name: 'map', back: from })); }}
      onLogout={() => void logout()}
    >
      {!user ? (
        <Auth api={api} mode={authMode} token={resetToken} onSignedIn={setUser} />
      ) : route === 'ops' ? (
        <OperatorScreen api={api} demo={api.demo} onUnauthorized={onUnauthorized} />
      ) : route === 'review' ? (
        <ReviewScreen api={api} demo={api.demo} onUnauthorized={onUnauthorized} />
      ) : view.name === 'session' ? (
        <Session
          api={api}
          demo={api.demo}
          onUnauthorized={onUnauthorized}
          onExit={home}
          onQuiz={(task) => { setView({ name: 'quiz', task, fromSession: true }); }}
          onDiagnostic={() => { setView({ name: 'diagnostic' }); }}
        />
      ) : view.name === 'quiz' ? (
        <Quiz
          api={api}
          task={view.task}
          demo={api.demo}
          fromSession={view.fromSession}
          onUnauthorized={onUnauthorized}
          // Back where the quiz came from. The session is still open behind it and its
          // remaining tasks are re-served; a quiz opened from the dashboard has none.
          onDone={() => { setView(view.fromSession ? { name: 'session' } : HOME); }}
        />
      ) : view.name === 'diagnostic' ? (
        <Diagnostic
          diag={placement}
          demo={api.demo}
          onUnauthorized={onUnauthorized}
          onExit={home}
        />
      ) : view.name === 'map' ? (
        <CurriculumMap
          api={api}
          demo={api.demo}
          onUnauthorized={onUnauthorized}
          onExit={() => { setView(view.back); }}
        />
      ) : (
        <Dashboard
          api={api}
          demo={api.demo}
          onUnauthorized={onUnauthorized}
          onSession={() => { setView({ name: 'session' }); }}
          onQuiz={(task) => { setView({ name: 'quiz', task, fromSession: false }); }}
          onDiagnostic={() => { setView({ name: 'diagnostic' }); }}
          onMap={() => { setView({ name: 'map', back: HOME }); }}
        />
      )}
    </App>
  );
}

/**
 * The one name of the screen on, for the error boundary's key.
 *
 * The operator paths win, because the signed-in branch checks them first. `map:session` and
 * `map:dashboard` are different keys on purpose: a map that threw over the session must not
 * hand its caught error to the map opened later from the dashboard.
 */
export function adminOr(route: 'ops' | 'review' | null, view: View): string {
  if (route) return route;
  return view.name === 'map' ? `map:${view.back.name}` : view.name;
}
