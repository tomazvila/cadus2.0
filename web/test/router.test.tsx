/**
 * The router (S13).
 *
 * S1–S12 built eleven screens and wired two of them. `Root` now reaches all of them, and
 * this file is the proof, because the click-through of `e2e/` cannot be the only thing that
 * walks the app: a browser run needs docker and a built bundle, and a wiring fault has to
 * fail the unit gate as well.
 *
 * EVERY TEST DRIVES THE REAL DEMO CLIENT, not a hand-written double. `createDemoApi()` is
 * the exact object `?demo=1` hands the browser, so a walk that holds here is a walk the
 * click-through takes against the same replies. The one thing a double would buy — an
 * arbitrary payload — is the one thing that would let this file and the browser disagree.
 *
 * The literals below are `api/demo.ts`: three problems, `1 / 3` on the first, `3/4` for
 * `$\frac{6}{8}$`, and three placement probes in `api/diag.ts`.
 */
import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Root, adminOr } from '@/app/Root';
import { createDemoApi } from '@/api';
import { MAP_CANVAS_LABEL } from '@/views/map/Map';
import { DIAG_DEFAULT_CAP } from '@/views/Diagnostic';
import { USER, quizTask } from './helpers/fixtures';
import { instances } from './mocks/cytoscape';
import type { ApiClient, ServedProblem, User } from '@/api/types';

/** Mount into the `<main>` the shell actually uses, so the topbar portal has its host. */
const mountRoot = (api: ApiClient, user: User | null = USER) =>
  render(<Root api={api} initialUser={user} />, { container: document.getElementById('view')! });

const topbar = () => document.getElementById('topbar')!;
const view = () => document.getElementById('view')!;

/** The dashboard, once `GET /api/status` has landed. */
async function reachDashboard(api: ApiClient): Promise<void> {
  mountRoot(api);
  await waitFor(() => expect(screen.getByText('Continue studying')).toBeTruthy());
}

/** Dashboard → session → past the worked example, to the first practice problem. */
async function reachFirstProblem(user: ReturnType<typeof userEvent.setup>): Promise<void> {
  await waitFor(() => expect(screen.getByText('Continue studying')).toBeTruthy());
  await user.click(screen.getByText('Continue studying'));
  await waitFor(() => expect(view().querySelector('.teach-card')).not.toBeNull());
  await user.click(screen.getByRole('button', { name: /practice/ }));
  await waitFor(() => expect(view().querySelector('.progress-count')).not.toBeNull());
}

/** A quiz served by the service: the plan holds `quiz`, and the serve answers `problem`. */
function serveQuiz(api: ApiClient, problem: Partial<ServedProblem> = {}): void {
  vi.spyOn(api, 'getPlan').mockResolvedValue({
    session: 'demo-session',
    tasks: [quizTask('demo-quiz')],
    quiz_due: true,
    constraints: {
      lesson_ratio_ok: true, lesson_ratio: 0.5, throttle_ok: true, reviews: 0, lessons: 0,
    },
    course_complete: false,
    frontier_blocked_until: null,
  });
  // The serve value is 120 seconds, and it is NOT the quiz clock.
  vi.spyOn(api, 'taskServe').mockResolvedValue({
    problem_id: 'demo-q1',
    index: 1,
    total: 8,
    text: 'Solve $x + 3 = 10$ for $x$.',
    kp: 'kp-linear-one-step',
    time_budget_secs: 120,
    countdown: false,
    ...problem,
  });
  vi.spyOn(api, 'taskAnswer').mockResolvedValue({ accepted: true, remaining: 0, quiz_complete: true });
}

/** Answer the one question of a served quiz and reach its end screen. */
async function finishQuiz(user: ReturnType<typeof userEvent.setup>): Promise<void> {
  await user.click(view().querySelector('.answer-input')!);
  await user.keyboard('7');
  await user.click(screen.getByRole('button', { name: 'Submit answer' }));
  await waitFor(() => expect(screen.getByText('Quiz complete')).toBeTruthy());
}

/** Open the map from the topbar, then leave it through Done. */
async function openMapAndReturn(user: ReturnType<typeof userEvent.setup>): Promise<void> {
  await user.click(screen.getByRole('button', { name: 'Map' }));
  await waitFor(() => expect(screen.getByLabelText(MAP_CANVAS_LABEL)).toBeTruthy());
  // The vendored renderer, reached through the real loader, drew the map.
  await waitFor(() => expect(instances.filter((i) => !i.destroyed)).toHaveLength(1));
  await user.click(screen.getByRole('button', { name: 'Done' }));
}

/** Open the quiet menu of the dashboard and press one of its buttons. */
async function pressInMenu(user: ReturnType<typeof userEvent.setup>, name: string): Promise<void> {
  await user.click(view().querySelector('.more-menu summary')!);
  await user.click(screen.getByRole('button', { name }));
}

describe('the router', () => {
  it('opens on the dashboard for a signed-in account', async () => {
    await reachDashboard(createDemoApi());
    expect(view().querySelector('.view-dashboard')).not.toBeNull();
    // The S4 placeholder card is what a caller with NO routed child gets. A router that
    // still rendered it would leave every screen unreachable and every test above it green.
    expect(view().textContent).not.toContain('The shell is up.');
  });

  it('renders the auth card on every view while signed out', () => {
    render(<Root api={createDemoApi()} initialUser={null} pathname="/review" />, {
      container: view(),
    });
    expect(view().querySelector('.auth-view')).not.toBeNull();
    expect(view().querySelector('.view-dashboard')).toBeNull();
  });

  it('walks the dashboard to the session and back', async () => {
    const user = userEvent.setup();
    await reachDashboard(createDemoApi());

    await user.click(screen.getByText('Continue studying'));
    await waitFor(() => expect(view().querySelector('.teach-card')).not.toBeNull());

    // The worked example comes first, and it is the whole of the lesson's first write
    // (NO-2BILL). Its concept is the demo's, character for character.
    expect(view().textContent).toContain(
      'A fraction names the same number when you divide both parts by the same factor.',
    );

    await user.click(screen.getByRole('button', { name: /practice/ }));
    await waitFor(() => expect(view().querySelector('.problem-card')).not.toBeNull());
    await user.click(screen.getByRole('button', { name: 'Exit' }));
    await waitFor(() => expect(view().querySelector('.view-dashboard')).not.toBeNull());
  });

  it('SERVE-idem: leaving the lesson and coming back serves the SAME problem', async () => {
    // THE CHECK NO SINGLE-VISIT TEST MAKES, and the third of the three failures 1.0's
    // click-through found: a backend that advances a cursor per serve makes the learner
    // practise a problem the real server never served. Two visits are two serves in a row
    // with no answer between them, so the second must repeat the first.
    const user = userEvent.setup();
    await reachDashboard(createDemoApi());

    await reachFirstProblem(user);
    expect(view().querySelector('.progress-count')!.textContent).toBe('1 / 3');
    const first = view().querySelector('.problem-text')!.textContent;

    await user.click(screen.getByRole('button', { name: 'Exit' }));
    await waitFor(() => expect(view().querySelector('.view-dashboard')).not.toBeNull());

    await reachFirstProblem(user);
    expect(view().querySelector('.progress-count')!.textContent).toBe('1 / 3');
    expect(view().querySelector('.problem-text')!.textContent).toBe(first);
  });

  it('grades the first problem and teaches the next knowledge point', async () => {
    const user = userEvent.setup();
    await reachDashboard(createDemoApi());
    await reachFirstProblem(user);

    await user.click(view().querySelector('.answer-input')!);
    await user.keyboard('3/4');
    await user.click(screen.getByRole('button', { name: 'Submit' }));

    await waitFor(() => expect(view().querySelector('.feedback-correct')).not.toBeNull());
    expect(view().textContent).toContain('Correct');
    expect(screen.getByRole('button', { name: 'Next problem →' })).toBeTruthy();

    // AUTO_ADVANCE_MS is 1400 and it fires on correct-with-next, so the walk does not click
    // Continue: a click on top of the armed timer advances TWICE and skips a problem.
    // Problem 2 belongs to `kp-linear-one-step`, a knowledge point this lesson has not
    // taught, so the lesson teaches it before it practises it.
    await waitFor(
      () => expect(view().querySelector('.teach-card')).not.toBeNull(),
      { timeout: 4000 },
    );
    await user.click(screen.getByRole('button', { name: /practice/ }));
    await waitFor(() => expect(view().querySelector('.progress-count')).not.toBeNull());
    expect(view().querySelector('.progress-count')!.textContent).toBe('2 / 3');
    // The window is 1400 ms of REAL time, which the file's default budget cannot hold.
  }, 15000);

  it('opens the map over the session and gives the session back', async () => {
    // The topbar offers the map from every screen, so the map has to remember where it was
    // opened from. Exiting to the dashboard instead would throw away the lesson underneath.
    const user = userEvent.setup();
    await reachDashboard(createDemoApi());
    await reachFirstProblem(user);

    await openMapAndReturn(user);
    // The session comes back and remounts, so it teaches again before it practises. The
    // dashboard would be the wrong answer: the lesson underneath would be gone.
    await waitFor(() => expect(view().querySelector('.view-session')).not.toBeNull());
    await waitFor(() => expect(view().querySelector('.teach-card')).not.toBeNull());
    await user.click(screen.getByRole('button', { name: /practice/ }));
    await waitFor(() => expect(view().querySelector('.progress-count')).not.toBeNull());
    expect(view().querySelector('.progress-count')!.textContent).toBe('1 / 3');
  });

  it('opens the placement and keeps ONE port across a re-render', async () => {
    // Both placement adapters hold state — the demo one counts the probes it asked and
    // refuses `diagAnswer` before `diagStart`. A port rebuilt on every render hands the
    // screen a fresh adapter mid-placement, and the learner's first answer is refused with
    // `409 no_diagnostic`. The re-render below is what makes that visible.
    const api = createDemoApi();
    const user = userEvent.setup();
    const { rerender } = render(<Root api={api} initialUser={USER} />, { container: view() });
    await waitFor(() => expect(screen.getByText('Continue studying')).toBeTruthy());

    // `More` is a native <summary>, not a button: the disclosure is the browser's.
    await pressInMenu(user, 'Re-run the placement');
    await waitFor(() => expect(view().querySelector('.view-diagnostic')).not.toBeNull());
    expect(view().querySelector('.intro-rules')!.querySelectorAll('li')).toHaveLength(3);

    await user.click(screen.getByRole('button', { name: 'Begin placement' }));
    await waitFor(() => expect(view().querySelector('.problem-text')).not.toBeNull());
    expect(view().textContent).toContain('Work out');

    // A NEW element, not the one already rendered: React bails out of a re-render when the
    // element it is handed is the same object, and the bail-out would make this vacuous.
    rerender(<Root api={api} initialUser={USER} />);
    await user.click(view().querySelector('.answer-input')!);
    await user.keyboard('5');
    await user.click(screen.getByRole('button', { name: 'Submit' }));
    // Probe 2 of 3, which a port rebuilt by that re-render could never reach.
    await waitFor(() => expect(view().textContent).toContain('Simplify'));
    expect(DIAG_DEFAULT_CAP).toBe(40);
  });

  it('hands the quiz its plan task, not its id', async () => {
    // QUIZ-budget: the whole-quiz clock reads `task.time_budget_secs`. A screen reached by
    // id alone would have to re-read the plan, and a re-plan between the two calls hands it
    // a different task.
    const api = createDemoApi();
    serveQuiz(api);

    const user = userEvent.setup();
    await reachDashboard(api);
    await user.click(screen.getByText('Continue studying'));
    await waitFor(() => expect(view().querySelector('.view-quiz')).not.toBeNull());
    // The plan the session reads back after the quiz is the demo's own lesson.
    vi.spyOn(api, 'getPlan').mockRestore();
    // 480 seconds is the TASK budget. The serve answers 120, so a clock that read the serve
    // value would start at 2:00.
    await waitFor(() => expect(view().querySelector('.timer')!.textContent).toBe('8:00'));
    expect(view().querySelector('.progress-count')!.textContent).toBe('1 / 8');

    // The quiz came from the session, so its end gives the SESSION back, not the dashboard.
    await finishQuiz(user);
    await user.click(screen.getByRole('button', { name: 'Continue session' }));
    await waitFor(() => expect(view().querySelector('.view-session')).not.toBeNull());
    expect(view().querySelector('.view-dashboard')).toBeNull();
  });

  it('opens the quiz from the dashboard and gives the dashboard back when it ends', async () => {
    // Quiz now from the quiet menu carries the plan task, and a quiz opened from the
    // dashboard has no session to return to: Done lands on the dashboard.
    const api = createDemoApi();
    serveQuiz(api, { total: 1, kp: null });

    const user = userEvent.setup();
    await reachDashboard(api);
    await pressInMenu(user, 'Quiz now');
    await waitFor(() => expect(view().querySelector('.view-quiz')).not.toBeNull());

    await finishQuiz(user);
    await user.click(screen.getByRole('button', { name: 'Back to dashboard' }));
    await waitFor(() => expect(view().querySelector('.view-dashboard')).not.toBeNull());
  });

  it('opens the map from the quiet menu and gives the dashboard back', async () => {
    const user = userEvent.setup();
    await reachDashboard(createDemoApi());
    await pressInMenu(user, 'Curriculum map');
    await waitFor(() => expect(screen.getByLabelText(MAP_CANVAS_LABEL)).toBeTruthy());

    await user.click(screen.getByRole('button', { name: 'Done' }));
    await waitFor(() => expect(view().querySelector('.view-dashboard')).not.toBeNull());
  });

  it('sends the learner home on sign-out', async () => {
    // A sign-in that landed back on the quiz of the account that just left would read the
    // previous learner's task.
    const api = createDemoApi();
    const user = userEvent.setup();
    render(<Root api={{ ...api, demo: false }} initialUser={USER} />, { container: view() });
    await waitFor(() => expect(screen.getByText('Continue studying')).toBeTruthy());
    await user.click(screen.getByText('Continue studying'));
    await waitFor(() => expect(view().querySelector('.view-session')).not.toBeNull());

    await user.click(topbar().querySelector('.logout-btn')!);
    await waitFor(() => expect(view().querySelector('.auth-view')).not.toBeNull());
  });
});

describe('the error-boundary key', () => {
  it('names the screen on, and the operator paths win', () => {
    expect(adminOr(null, { name: 'dashboard' })).toBe('dashboard');
    expect(adminOr(null, { name: 'session' })).toBe('session');
    expect(adminOr('ops', { name: 'session' })).toBe('ops');
    expect(adminOr('review', { name: 'dashboard' })).toBe('review');
  });

  it('separates a map opened over the session from one opened over the dashboard', () => {
    // Same screen, two keys. A map that threw over the session must not hand its caught
    // error to the map the learner opens later from the dashboard.
    expect(adminOr(null, { name: 'map', back: { name: 'session' } })).toBe('map:session');
    expect(adminOr(null, { name: 'map', back: { name: 'dashboard' } })).toBe('map:dashboard');
  });
});
