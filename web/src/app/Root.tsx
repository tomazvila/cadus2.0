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
 * THE OPERATOR ROUTE HAS ONE SOURCE, AND IT IS THE LIVE LOCATION (M6-review-1, F8). The
 * `pathname` prop seeds that location and is read no other time. Every control that leaves
 * an operator screen calls `navigate`, which writes the location with `history.pushState`
 * and the state in the same move, and a `popstate` reads the location back. Before this,
 * the route came from a prop boot fixed once and it beat the view name unconditionally: on
 * `/ops` and `/review` the topbar moved `view` and the operator screen stayed on screen, so
 * Home and Map did nothing at all.
 *
 * THE PUSH IS A REAL HISTORY ENTRY, not a replace. An operator who followed a link to
 * `/review` and then pressed Home still has one Back between them and the page they came
 * from; a replace would send that press out of the application.
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
import { useEffect, useState, type Dispatch, type SetStateAction } from 'react';
import { App } from './App';
import { adminRouteFor, type AdminRoute } from './routes';
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
const HOME: View = { name: 'dashboard' };

/** The path every learner screen renders on. The two operator paths are `app/routes.ts`. */
const HOME_PATH = '/';

export interface RootProps {
  api: ApiClient;
  /** The account boot resolved, or null when nobody is signed in. */
  initialUser: User | null;
  /** Which auth card the URL asked for. */
  authMode?: AuthMode;
  /**
   * The single-use reset token. It is still in the URL until the reset card takes it.
   * Absent, or undefined, on every boot that did not come from a reset link.
   */
  resetToken?: string | undefined;
  /**
   * The path the page loaded on.
   *
   * It SEEDS the location this router reads; after the first render the location itself is
   * the source, and `navigate` and `popstate` are the only two things that move it.
   */
  pathname?: string;
  /**
   * Called once, when the reset card takes the single-use token.
   *
   * Boot passes the strip of that token from the URL. The strip belongs to this moment and
   * not to boot: a token dropped before the card is on is a token no card ever receives,
   * and a refresh after this point cannot replay a link the learner already opened.
   */
  onResetTokenTaken?: () => void;
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
  resetToken,
  pathname = HOME_PATH,
  diag,
  initialView = HOME,
  onResetTokenTaken,
}: RootProps) {
  const [user, setUser] = useState<User | null>(initialUser);
  const [view, setView] = useState<View>(initialView);
  const [path, setPath] = useState<string>(pathname);
  // ONCE per mount. Both adapters hold state — the demo one counts the probes it asked —
  // so a port rebuilt on every render restarts the placement at probe 1 forever.
  const [placement] = useState<DiagnosticApi>(() => diag ?? resolveDiag(!!api.demo));

  /**
   * Back, Forward, and every other move the browser makes on its own.
   *
   * The location is the source, so a change this application did not make has to reach the
   * same state. Without this listener a Back out of the dashboard puts `/ops` in the address
   * bar and leaves the dashboard on screen.
   */
  useEffect(() => {
    const onPop = () => { setPath(window.location.pathname); };
    window.addEventListener('popstate', onPop);
    return () => { window.removeEventListener('popstate', onPop); };
  }, []);

  /**
   * The three moves, created ONCE per mount. Each reads state setters alone, and a setter is
   * stable, so the three hold for the life of the root and every dependency array they land
   * in holds with them.
   */
  const [{ navigate, goHome, onUnauthorized }] = useState(() => {
    /**
     * Move the location and the state together. Nothing else writes `path`.
     *
     * The push happens OUTSIDE the state updater on purpose: React invokes an updater twice
     * under StrictMode, and a `pushState` written there would add two history entries per
     * navigation, so one Back press would land on the page the operator just left.
     */
    const navigate = (next: string): void => {
      if (window.location.pathname !== next) {
        try {
          window.history.pushState({}, '', next);
        } catch {
          /* a non-browser host, or a blocked history write */
        }
      }
      setPath(next);
    };

    /**
     * The boot screen: the dashboard, on the dashboard's path.
     *
     * ONE move, and three callers share it — the topbar brand, sign-out, and the 401. Each
     * of the three has to drop the screen AND the operator path, or the screen the caller
     * left comes straight back.
     */
    const goHome = (): void => {
      navigate(HOME_PATH);
      setView(HOME);
    };

    /**
     * The session-expired path every screen shares.
     *
     * IT RETURNS HOME AS WELL AS DROPPING THE ACCOUNT (M6-review-1, F21). The quiz view
     * holds the whole `PlanTask`, not a task id. A 401 that dropped the account and kept the
     * screen left that task in state, so the next sign-in on the same browser re-mounted the
     * quiz of the account that just left and posted a serve against a task id the new
     * account does not own. `useCall` raises the toast that says the session expired, so
     * this must not raise a second one.
     */
    const onUnauthorized = (): void => {
      setUser(null);
      goHome();
    };

    return { navigate, goHome, onUnauthorized };
  });

  /**
   * Drop the session locally whatever the service says.
   *
   * A failed logout means the cookie is already gone, or the network is: in both cases the
   * learner asked to leave, and keeping them signed in on screen is the wrong answer. The
   * service revokes the row; the browser drops the `__Host-` cookie on its own reply.
   *
   * The screen returns home in the same move, for the reason `onUnauthorized` gives.
   */
  async function logout(): Promise<void> {
    try {
      await api.logout();
    } catch {
      /* already gone, or offline — leave anyway */
    }
    setUser(null);
    goHome();
  }

  const route = adminRouteFor(path);
  const screen = adminOr(route, view);

  // The reset card is on. Boot holds the token in the URL until this render, and drops it
  // now (M6-review-1, F22). A second call writes the same URL, so a StrictMode remount and
  // a re-render are both harmless.
  const resetCardOn = !user && authMode === 'reset' && resetToken !== undefined;
  useEffect(() => {
    if (resetCardOn) onResetTokenTaken?.();
  }, [resetCardOn, onResetTokenTaken]);

  return (
    <App
      user={user}
      demo={api.demo}
      routeKey={screen}
      onHome={goHome}
      // The map opens over whatever is on screen and gives that screen back on Done. It
      // LEAVES an operator path first: the two operator screens are not learner screens, so
      // the map opened from one of them gives the dashboard back.
      onMap={() => {
        navigate(HOME_PATH);
        setView((from) => (from.name === 'map' ? from : { name: 'map', back: from }));
      }}
      onLogout={() => void logout()}
    >
      {user ? (
        <Screen
          api={api}
          route={route}
          view={view}
          placement={placement}
          onUnauthorized={onUnauthorized}
          goHome={goHome}
          setView={setView}
        />
      ) : (
        <Auth api={api} mode={authMode} token={resetToken} onSignedIn={setUser} />
      )}
    </App>
  );
}

interface ScreenProps {
  api: ApiClient;
  /** The operator screen the location names, or null on every learner path. */
  route: AdminRoute | null;
  view: View;
  placement: DiagnosticApi;
  onUnauthorized: () => void;
  goHome: () => void;
  setView: Dispatch<SetStateAction<View>>;
}

/**
 * The signed-in branch: the operator paths first, then the view name.
 *
 * The map gives back the screen it was opened over, and the quiz gives back the session
 * it came from: the session is still open behind it and its remaining tasks are re-served,
 * while a quiz opened from the dashboard has none.
 */
function Screen({ api, route, view, placement, onUnauthorized, goHome, setView }: ScreenProps) {
  const common = { api, demo: api.demo, onUnauthorized };
  const openDiagnostic = () => { setView({ name: 'diagnostic' }); };

  if (route === 'ops') return <OperatorScreen {...common} />;
  if (route === 'review') return <ReviewScreen {...common} />;

  switch (view.name) {
    case 'session':
      return (
        <Session
          {...common}
          onExit={goHome}
          onQuiz={(task) => { setView({ name: 'quiz', task, fromSession: true }); }}
          onDiagnostic={openDiagnostic}
        />
      );
    case 'quiz':
      return (
        <Quiz
          {...common}
          task={view.task}
          fromSession={view.fromSession}
          onDone={() => { setView(view.fromSession ? { name: 'session' } : HOME); }}
        />
      );
    case 'diagnostic':
      return <Diagnostic diag={placement} demo={api.demo} onUnauthorized={onUnauthorized} onExit={goHome} />;
    case 'map':
      return <CurriculumMap {...common} onExit={() => { setView(view.back); }} />;
    default:
      return (
        <Dashboard
          {...common}
          onSession={() => { setView({ name: 'session' }); }}
          onQuiz={(task) => { setView({ name: 'quiz', task, fromSession: false }); }}
          onDiagnostic={openDiagnostic}
          onMap={() => { setView({ name: 'map', back: HOME }); }}
        />
      );
  }
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
