/**
 * The dashboard (S7).
 *
 * Three claims, and each one is a shape rule the screen keeps in every state:
 *   W-C2  exactly one primary button;
 *   W-C3  an empty plan offers the diagnostic — no dead end;
 *   DEP-3 the JSONL export rides the session cookie and carries no token.
 *
 * The status fixture is the frozen `GET /api/status` contract of the web-service spec, so
 * every number below is a literal a reader checks by hand: 12 of 40 XP is 30 percent, and
 * a course progress of 0.18 is 18 percent.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axe } from 'vitest-axe';
import { ApiError, api as realApi, createDemoApi } from '@/api';
import { DialogProvider } from '@/components/Modal';
import { Dashboard, hasScheduledWork, type DashboardProps } from '@/views/Dashboard';
import { resetToasts, toastStore } from '@/app/toast';
import { AXE_IN_JSDOM } from './axe';
import { downloads, objectUrls } from './setup';
import { renderInView } from './helpers/render';
import type { ApiClient, StatusResponse } from '@/api/types';

const STATUS: StatusResponse = {
  course: { id: 'foundations', name: 'Foundations' },
  placed: true,
  courses: [
    { id: 'foundations', name: 'Foundations', current: true },
    { id: 'proofs', name: 'Proofs', current: false },
  ],
  test_prep: null,
  xp: { total: 340, today: 12, goal: 40, streak_days: 3 },
  velocity: {
    xp_per_day_28d: 21.5,
    topics_per_week_28d: 2.25,
    course_progress: 0.18,
    eta: '2026-11-04',
  },
  quiz: { last_at: null, xp_since: 0, retake_pending: false },
  pending_remediation: [],
  quiz_due: false,
  drill_due: false,
  frontier: 4,
  due_reviews: 2,
  nearly_due: 1,
};

const status = (over: Partial<StatusResponse> = {}): StatusResponse => ({ ...STATUS, ...over });

/** Nothing scheduled: no review, no nearly-due review, no frontier, no quiz, no drill. */
const EMPTY_PLAN: Partial<StatusResponse> = {
  due_reviews: 0,
  nearly_due: 0,
  frontier: 0,
  quiz_due: false,
  drill_due: false,
};

/** One course, and it is the current one — so no next course takes the primary slot. */
const ONE_COURSE = [{ id: 'foundations', name: 'Foundations', current: true }];

/**
 * A client built from the demo backend, so every method of `ApiClient` exists and a missing
 * override is a type error rather than a `not a function` inside a handler.
 */
function stubApi(over: Partial<ApiClient> = {}): ApiClient {
  return { ...createDemoApi(), getStatus: async () => status(), ...over };
}

const nav = () => ({
  onUnauthorized: vi.fn(),
  onSession: vi.fn(),
  onQuiz: vi.fn(),
  onDiagnostic: vi.fn(),
  onMap: vi.fn(),
});

/** Mount into the `<main>` the shell owns, and settle the status fetch. */
async function mount(over: Partial<DashboardProps> = {}) {
  resetToasts();
  const handlers = nav();
  const props: DashboardProps = { api: stubApi(), ...handlers, ...over };
  const view = await renderInView(
    <DialogProvider>
      <Dashboard {...props} />
    </DialogProvider>,
  );
  return { ...view, ...handlers };
}

const primaries = () => document.querySelectorAll('.view-dashboard .btn-primary');
const actionBlock = () =>
  document.querySelector<HTMLElement>('.primary-action, .onboard-card')!;

/**
 * Press the diagnostic control in the OPEN — in the action block, never inside the quiet
 * disclosure — and count the navigations it made.
 */
async function pressInTheOpen(name: string, onDiagnostic: ReturnType<typeof vi.fn>) {
  const cta = within(actionBlock()).getByRole('button', { name });
  const insideDetails = cta.closest('details') !== null;
  await userEvent.click(cta);
  return { insideDetails, calls: onDiagnostic.mock.calls.length };
}

/** Open the quiet menu and the course picker behind "Switch course". */
async function openPicker() {
  const user = userEvent.setup();
  const enroll = vi.fn(createDemoApi().enroll);
  await mount({ api: stubApi({ enroll }) });
  await user.click(screen.getByText('More'));
  await user.click(screen.getByRole('button', { name: 'Switch course' }));
  return { user, enroll };
}

describe('the dashboard', () => {
  it('waits with a labelled block, then paints the status card', async () => {
    let release!: (s: StatusResponse) => void;
    const pending = new Promise<StatusResponse>((r) => { release = r; });
    const view = mount({ api: stubApi({ getStatus: () => pending }) });

    expect(await screen.findByText('Loading your dashboard…')).toBeTruthy();
    await act(async () => { release(status()); });
    await view;

    expect(screen.getByRole('heading', { name: '12 / 40 XP today' })).toBeTruthy();
    expect(screen.getByText('Foundations · 18% complete')).toBeTruthy();
    expect(screen.queryByText('Loading your dashboard…')).toBeNull();
  });

  it('renders the counters of the frozen status contract, and nothing derived', async () => {
    await mount();
    const stats = document.querySelectorAll('.stat');
    const read = (label: string) =>
      Array.from(stats).find((s) => s.querySelector('.stat-label')!.textContent === label)!
        .querySelector('.stat-value')!.textContent;

    expect(read('day streak')).toBe('3');
    expect(read('due now')).toBe('2');
    expect(read('nearly due')).toBe('1');
    expect(read('frontier')).toBe('4');
    expect(read('course')).toBe('18%');
    expect(read('ETA')).toBe('2026-11-04');
    // 12 of 40 is 30 percent of the daily goal.
    expect(document.querySelector('.ring-label strong')!.textContent).toBe('30%');
  });

  it('names the scheduled work in one line, in the order the core reports it', async () => {
    await mount({ api: stubApi({ getStatus: async () => status({ quiz_due: true }) }) });
    expect(screen.getByText('Up next: 2 reviews · 4 new lessons · a quiz.')).toBeTruthy();
  });

  it('counts a nearly-due review as work, so the plan is not empty', () => {
    expect(hasScheduledWork(status({ ...EMPTY_PLAN, nearly_due: 1 }))).toBe(true);
    expect(hasScheduledWork(status({ ...EMPTY_PLAN, drill_due: true }))).toBe(true);
    expect(hasScheduledWork(status(EMPTY_PLAN))).toBe(false);
  });

  // --- W-C2 ---------------------------------------------------------------
  it('W-C2: exactly one primary button while work is scheduled', async () => {
    await mount();
    expect(primaries().length).toBe(1);
    expect(primaries()[0].textContent).toBe('▶ Continue studying');
  });

  it('W-C2: exactly one primary button on an unplaced account', async () => {
    await mount({ api: stubApi({ getStatus: async () => status({ placed: false }) }) });
    expect(primaries().length).toBe(1);
    expect(primaries()[0].textContent).toBe('Start placement ▸');
  });

  it('W-C2: exactly one primary button on an empty plan, with a next course', async () => {
    await mount({ api: stubApi({ getStatus: async () => status(EMPTY_PLAN) }) });
    expect(primaries().length).toBe(1);
    expect(primaries()[0].textContent).toBe('Start Proofs ▸');
  });

  it('W-C2: exactly one primary button on an empty plan and the last course', async () => {
    await mount({
      api: stubApi({ getStatus: async () => status({ ...EMPTY_PLAN, courses: ONE_COURSE }) }),
    });
    expect(primaries().length).toBe(1);
    expect(primaries()[0].textContent).toBe('Re-check where you are ▸');
  });

  it('W-C2: opening the quiet menu adds no second primary', async () => {
    await mount();
    await userEvent.click(screen.getByText('More'));
    expect(screen.getByRole('button', { name: 'Curriculum map' })).toBeTruthy();
    expect(primaries().length).toBe(1);
  });

  // --- W-C3 ---------------------------------------------------------------
  it('W-C3: an empty plan offers the diagnostic as the primary action', async () => {
    const view = await mount({
      api: stubApi({ getStatus: async () => status({ ...EMPTY_PLAN, courses: ONE_COURSE }) }),
    });
    expect(screen.getByText('You are all caught up — nice work.')).toBeTruthy();

    // In the open, not inside the quiet disclosure: the learner must not open a menu to
    // find the one thing left to do.
    expect(await pressInTheOpen('Re-check where you are', view.onDiagnostic))
      .toEqual({ insideDetails: false, calls: 1 });
  });

  it('W-C3: an empty plan with a next course still offers the diagnostic beside it', async () => {
    const view = await mount({ api: stubApi({ getStatus: async () => status(EMPTY_PLAN) }) });
    expect(await pressInTheOpen('Re-check where you are', view.onDiagnostic))
      .toEqual({ insideDetails: false, calls: 1 });
  });

  it('W-C3: an unplaced account gets the placement and no other action', async () => {
    const view = await mount({
      api: stubApi({ getStatus: async () => status({ placed: false }) }),
    });
    // One action only. A wall of buttons here asks the learner to plan the placement.
    expect(document.querySelectorAll('.view-dashboard button').length).toBe(1);
    expect(await pressInTheOpen('Start placement', view.onDiagnostic))
      .toEqual({ insideDetails: false, calls: 1 });
  });

  // --- DEP-3 --------------------------------------------------------------
  it('DEP-3: the export downloads through the cookie, not a token', async () => {
    const getItem = vi.spyOn(Storage.prototype, 'getItem');
    const setItem = vi.spyOn(Storage.prototype, 'setItem');
    const fetchMock = vi.fn<(url: string, init: RequestInit) => Promise<Response>>(
      async () =>
        new Response('{"event":"answered"}\n{"event":"session_end"}\n', {
          status: 200,
          headers: { 'Content-Disposition': 'attachment; filename="cadus-events.jsonl"' },
        }),
    );
    vi.stubGlobal('fetch', fetchMock);

    await mount({ api: stubApi({ downloadExport: realApi.downloadExport }) });
    await userEvent.click(screen.getByText('More'));
    await userEvent.click(screen.getByRole('button', { name: 'Export my data (JSONL)' }));

    await waitFor(() => expect(downloads.length).toBe(1));
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe('/api/export');
    // The whole point: the browser attaches the HttpOnly cookie, and the URL carries no
    // credential of any kind.
    expect(init.credentials).toBe('same-origin');
    expect(init.headers).toBeUndefined();
    expect(url).not.toContain('?');
    expect(url.toLowerCase()).not.toContain('token');
    // The service names the file; the client only falls back.
    expect(downloads[0].download).toBe('cadus-events.jsonl');
    expect(downloads[0].href).toContain(objectUrls[0]);
    // No credential is read from or written to storage.
    expect(getItem).not.toHaveBeenCalled();
    expect(setItem).not.toHaveBeenCalled();
  });

  it('DEP-3: a refused export toasts and writes no file', async () => {
    const failing = async () => {
      throw new ApiError(403, 'forbidden', 'The demo keeps no event log to export.');
    };
    await mount({ api: stubApi({ downloadExport: failing }) });
    await userEvent.click(screen.getByText('More'));
    await userEvent.click(screen.getByRole('button', { name: 'Export my data (JSONL)' }));

    await waitFor(() => expect(toastStore.getSnapshot().length).toBe(1));
    expect(toastStore.getSnapshot()[0].message).toBe('The demo keeps no event log to export.');
    expect(downloads.length).toBe(0);
  });

  // --- the ways out -------------------------------------------------------
  it('starts the session before it leaves the screen', async () => {
    const sessionStart = vi.fn(createDemoApi().sessionStart);
    const view = await mount({ api: stubApi({ sessionStart }) });
    await userEvent.click(screen.getByRole('button', { name: 'Continue studying' }));
    await waitFor(() => expect(view.onSession).toHaveBeenCalledTimes(1));
    expect(sessionStart).toHaveBeenCalledTimes(1);
  });

  it('stays on the dashboard when the session refuses to start', async () => {
    const sessionStart = async () => {
      throw new ApiError(500, 'server_error', 'The service failed.');
    };
    const view = await mount({ api: stubApi({ sessionStart }) });
    await userEvent.click(screen.getByRole('button', { name: 'Continue studying' }));
    await waitFor(() => expect(toastStore.getSnapshot().length).toBe(1));
    expect(view.onSession).not.toHaveBeenCalled();
  });

  it('posts one session start for a double click in one tick, never two', async () => {
    // Two `session_start` events for one press are two rows in an append-only log.
    //
    // BOTH CLICKS IN ONE TICK, deliberately. React commits `disabled` one render later, and
    // that render is the window a fast second click lands in. A test that awaits between the
    // two clicks proves only that the attribute works, and it stays green with the guard
    // deleted.
    let resolve!: () => void;
    const gate = new Promise<void>((r) => { resolve = () => r(); });
    const demo = createDemoApi();
    const sessionStart = vi.fn(async () => { await gate; return demo.sessionStart(); });
    await mount({ api: stubApi({ sessionStart }) });

    const cta = screen.getByRole('button', { name: 'Continue studying' });
    await act(async () => {
      cta.click();
      cta.click();
      resolve();
    });

    expect(sessionStart).toHaveBeenCalledTimes(1);
  });

  it('hands the quiz its plan task, not a bare id', async () => {
    const demo = createDemoApi();
    const quizTask = {
      task_id: 't-quiz-1',
      task_type: 'quiz' as const,
      topic: null,
      kp: null,
      start_at_kp: null,
      n_problems: 8,
      mix: null,
      component_topics: null,
      time_budget_secs: 600,
      difficulty_target: null,
      why: 'quiz due',
      progress: { answered: 0, done: false },
    };
    const getPlan = async () => {
      const plan = await demo.getPlan();
      return { ...plan, tasks: [...plan.tasks, quizTask] };
    };
    const view = await mount({ api: stubApi({ getPlan }) });
    await userEvent.click(screen.getByText('More'));
    await userEvent.click(screen.getByRole('button', { name: 'Quiz now' }));

    await waitFor(() => expect(view.onQuiz).toHaveBeenCalledTimes(1));
    // The whole task: the quiz clock reads `time_budget_secs` of the task.
    expect(view.onQuiz).toHaveBeenCalledWith(quizTask);
  });

  it('says so when no quiz is due, and goes nowhere', async () => {
    const view = await mount();
    await userEvent.click(screen.getByText('More'));
    await userEvent.click(screen.getByRole('button', { name: 'Quiz now' }));

    await waitFor(() => expect(toastStore.getSnapshot().length).toBe(1));
    expect(toastStore.getSnapshot()[0].message).toBe('No quiz is due right now.');
    expect(view.onQuiz).not.toHaveBeenCalled();
  });

  it('enrolls in the next course and re-reads the status', async () => {
    const getStatus = vi.fn(async () => status(EMPTY_PLAN));
    const enroll = vi.fn(createDemoApi().enroll);
    await mount({ api: stubApi({ getStatus, enroll }) });
    const before = getStatus.mock.calls.length;

    await userEvent.click(screen.getByRole('button', { name: 'Start Proofs' }));

    await waitFor(() => expect(getStatus.mock.calls.length).toBeGreaterThan(before));
    expect(enroll).toHaveBeenCalledWith('proofs');
    expect(toastStore.getSnapshot()[0].message)
      .toBe('Enrolled in Proofs. Take the placement to get started.');
  });

  it('offers Try again when the status never arrives, and recovers on it', async () => {
    let attempt = 0;
    const getStatus = async () => {
      attempt += 1;
      if (attempt === 1) throw new ApiError(500, 'server_error', 'The service failed.');
      return status();
    };
    await mount({ api: stubApi({ getStatus }) });

    expect(screen.getByText('Could not load your dashboard.')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));

    expect(await screen.findByRole('heading', { name: '12 / 40 XP today' })).toBeTruthy();
    expect(screen.queryByText('Could not load your dashboard.')).toBeNull();
  });

  it('keeps the secondaries closed until the learner asks', async () => {
    await mount();
    const more = document.querySelector('details.more-menu') as HTMLDetailsElement;
    expect(more.open).toBe(false);
    expect(more.querySelectorAll('button').length).toBe(5);
  });

  it('F9: the course picker is a dialog, on a modal surface, and Esc leaves it', async () => {
    const { user, enroll } = await openPicker();

    const picker = screen.getByRole('dialog');
    // `aria-modal` is what tells a screen reader the page behind is inert, and the focus
    // trap of `Modal` is what makes that true. Neither one works without the role.
    expect(picker.getAttribute('aria-modal')).toBe('true');
    expect(picker.getAttribute('aria-labelledby')).toBe('picker-h');
    expect(document.getElementById('picker-h')!.textContent).toBe('Switch course');
    // `.modal` is the one rule in app.css that paints a dialog surface: the background, the
    // border, the radius, the padding, the width and the grid the rows are laid out by.
    expect(picker.classList.contains('modal')).toBe(true);
    expect(picker.parentElement!.classList.contains('modal-overlay')).toBe(true);

    // The trap holds: focus starts inside, and Tab does not walk out to the page behind.
    expect(picker.contains(document.activeElement)).toBe(true);
    await user.tab();
    await user.tab();
    expect(picker.contains(document.activeElement)).toBe(true);

    await user.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(enroll).not.toHaveBeenCalled();
  });

  it('F9: the picker enrolls in the course the learner names', async () => {
    const { user, enroll } = await openPicker();

    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Proofs' }));

    await waitFor(() => expect(enroll).toHaveBeenCalledWith('proofs'));
  });

  it('F9: Cancel leaves the picker and enrolls in nothing', async () => {
    const { user, enroll } = await openPicker();
    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(enroll).not.toHaveBeenCalled();
  });

  it('names one review and one lesson in the singular, and a drill', async () => {
    await mount({
      api: stubApi({ getStatus: async () => status({ due_reviews: 1, frontier: 1, drill_due: true }) }),
    });
    expect(screen.getByText('Up next: 1 review · 1 new lesson · a drill.')).toBeTruthy();
  });

  it('says practice is ready when only a nearly-due review is scheduled', async () => {
    await mount({
      api: stubApi({ getStatus: async () => status({ ...EMPTY_PLAN, nearly_due: 1 }) }),
    });
    expect(screen.getByText('Practice is ready.')).toBeTruthy();
    expect(primaries()[0].textContent).toBe('▶ Continue studying');
  });

  it('draws an empty ring on a goal of zero, and names no course arc without courses', async () => {
    await mount({
      api: stubApi({
        getStatus: async () => status({
          xp: { total: 0, today: 5, goal: 0, streak_days: 0 },
          courses: [],
          course: { id: null, name: null },
        }),
      }),
    });
    expect(screen.getByRole('heading', { name: '5 / 0 XP today' })).toBeTruthy();
    expect(document.querySelector('.ring-label strong')!.textContent).toBe('0%');
    expect(document.querySelector('.course-arc')).toBeNull();
    expect(screen.getByText('your course · 18% complete')).toBeTruthy();
  });

  it('keeps the newer status when an older read lands last', async () => {
    // Two reads in flight: the learner pressed Try again while the first was still out.
    const replies: Array<(s: StatusResponse) => void> = [];
    const getStatus = vi.fn(() => new Promise<StatusResponse>((r) => { replies.push(r); }));
    await mount({ api: stubApi({ getStatus }) });
    expect(screen.getByText('Loading your dashboard…')).toBeTruthy();

    // The first read fails, which paints Try again; the press starts the second read.
    await act(async () => { replies[0]!(status({ due_reviews: 9 })); });
    expect(screen.getByRole('heading', { name: '12 / 40 XP today' })).toBeTruthy();
    expect(getStatus).toHaveBeenCalledTimes(1);
  });

  it('ignores a stale reply and a stale failure behind a newer generation', async () => {
    let attempt = 0;
    const held: Array<{ resolve: (s: StatusResponse) => void; reject: (e: Error) => void }> = [];
    const getStatus = vi.fn(() => {
      attempt += 1;
      return new Promise<StatusResponse>((resolve, reject) => { held.push({ resolve, reject }); });
    });
    await mount({ api: stubApi({ getStatus }) });

    // The first read fails: Try again is on screen, and a press starts read two.
    await act(async () => { held[0]!.reject(new ApiError(500, 'server_error', 'Down.')); });
    expect(screen.getByText('Could not load your dashboard.')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(attempt).toBe(2);
    // The Retry of the FIRST failure re-runs its request: that is read three, generation 0.
    await act(async () => { toastStore.getSnapshot()[0].onAction?.(); });
    expect(attempt).toBe(3);

    // Read two lands with 2 due; the stale read three lands with 9 and changes nothing.
    await act(async () => { held[1]!.resolve(status({ due_reviews: 2 })); });
    await act(async () => { held[2]!.resolve(status({ due_reviews: 9 })); });
    expect(screen.getByText('Up next: 2 reviews · 4 new lessons.')).toBeTruthy();
  });

  it('keeps the card when a stale read fails behind a newer failure', async () => {
    const held: Array<{ resolve: (s: StatusResponse) => void; reject: (e: Error) => void }> = [];
    const getStatus = vi.fn(() => new Promise<StatusResponse>((resolve, reject) => {
      held.push({ resolve, reject });
    }));
    await mount({ api: stubApi({ getStatus }) });
    await act(async () => { held[0]!.reject(new ApiError(500, 'server_error', 'Down.')); });
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await act(async () => { toastStore.getSnapshot()[0].onAction?.(); });

    // Generation 1 fails, then the stale generation 0 fails again: one failure card, and the
    // failed generation stays at 1.
    await act(async () => { held[1]!.reject(new ApiError(500, 'server_error', 'Down.')); });
    await act(async () => { held[2]!.reject(new ApiError(500, 'server_error', 'Down.')); });
    expect(screen.getByText('Could not load your dashboard.')).toBeTruthy();
    // The next successful read still paints, so the failed generation did not run ahead.
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await act(async () => { held[3]!.resolve(status()); });
    expect(screen.getByRole('heading', { name: '12 / 40 XP today' })).toBeTruthy();
  });

  it('F-F2-2: a session start or a quiz read that lands after the screen left moves nothing', async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => { release = r; });
    const demo = createDemoApi();
    const sessionStart = vi.fn(async () => { await gate; return demo.sessionStart(); });
    const view = await mount({ api: stubApi({ sessionStart }) });
    await userEvent.click(screen.getByText('More'));
    await userEvent.click(screen.getByRole('button', { name: 'Quiz now' }));
    await userEvent.click(screen.getByRole('button', { name: 'Continue studying' }));
    expect(sessionStart).toHaveBeenCalledTimes(2);

    view.unmount();
    await act(async () => { release(); await gate; });
    expect(view.onSession).not.toHaveBeenCalled();
    expect(view.onQuiz).not.toHaveBeenCalled();
  });

  it('F-F2-2: a plan that lands after the screen left opens no quiz', async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => { release = r; });
    const demo = createDemoApi();
    const getPlan = vi.fn(async () => { await gate; return demo.getPlan(); });
    const view = await mount({ api: stubApi({ getPlan }) });
    await userEvent.click(screen.getByText('More'));
    await userEvent.click(screen.getByRole('button', { name: 'Quiz now' }));
    await waitFor(() => expect(getPlan).toHaveBeenCalledTimes(1));

    view.unmount();
    await act(async () => { release(); await gate; });
    expect(view.onQuiz).not.toHaveBeenCalled();
    expect(toastStore.getSnapshot()).toEqual([]);
  });

  it('DEP-3: a refused export with no message toasts the generic line', async () => {
    await mount({ api: stubApi({ downloadExport: async () => { throw new Error(''); } }) });
    await userEvent.click(screen.getByText('More'));
    await userEvent.click(screen.getByRole('button', { name: 'Export my data (JSONL)' }));

    await waitFor(() => expect(toastStore.getSnapshot().length).toBe(1));
    expect(toastStore.getSnapshot()[0].message).toBe('Could not export your data.');
  });

  it('reports zero axe violations', async () => {
    const view = await mount();
    expect(await axe(view.container, AXE_IN_JSDOM)).toHaveNoViolations();
  });

  it('reports zero axe violations on an unplaced account', async () => {
    const view = await mount({
      api: stubApi({ getStatus: async () => status({ placed: false }) }),
    });
    expect(await axe(view.container, AXE_IN_JSDOM)).toHaveNoViolations();
  });
});
