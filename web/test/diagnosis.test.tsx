/**
 * The async diagnosis panel (S9).
 *
 * Three invariants are claimed here, and all three are NEGATIVES — the reason the invariant
 * gate exists beside a coverage floor:
 *
 *   DIAG-async  the verdict paints before any diagnosis arrives.
 *   DIAG-poll   an SSE drop falls back to the 2 s poll and the prose still lands.
 *   DIAG-30s    a job pending past 30 s reads as failed, and the verdict stands.
 *
 * Every literal below is the frozen contract of `docs/reference/web-service-1.0-spec.md`
 * section 2.1: the `event: diagnosis` frame name, the stream path `/api/diagnosis/stream`,
 * the 2000 ms poll interval and the 30000 ms deadline. A reader checks each one by hand.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { createDemoApi } from '@/api';
import { Session, type SessionProps } from '@/views/session/Session';
import { DIAGNOSIS_DEADLINE_MS, DIAGNOSIS_POLL_MS } from '@/views/session/useDiagnosis';
import { DIAGNOSIS_FAILED, DIAGNOSIS_WAIT } from '@/views/session/Diagnosis';
import { resetToasts, toastStore } from '@/app/toast';
import { AXE_IN_JSDOM } from './axe';
import { eventSources, lastEventSource } from './setup';
import type {
  AnswerResponse,
  ApiClient,
  DiagnosisJob,
  PlanTask,
  ServedProblem,
  SessionPlanResponse,
} from '@/api/types';

// ---------------------------------------------------------------------------
// Fixtures — a one-task plan whose single problem is answered wrong, because a miss is the
// only shape that owes a diagnosis at all.
// ---------------------------------------------------------------------------

const REVIEW: PlanTask = {
  task_id: 't-review',
  task_type: 'review',
  topic: { id: 'fractions', name: 'Fractions', module: 'Arithmetic' },
  kp: 'kp-simplify',
  start_at_kp: null,
  n_problems: 2,
  mix: null,
  component_topics: null,
  time_budget_secs: 600,
  difficulty_target: 0.6,
  why: 'due for review',
  progress: { answered: 0, done: false },
};

const P = (n: number): ServedProblem => ({
  problem_id: `p${n}`,
  index: n,
  total: 2,
  text: `Simplify $\\frac{${n}}{2}$.`,
  kp: 'kp-simplify',
  time_budget_secs: 60,
  countdown: false,
});

const PLAN: SessionPlanResponse = {
  session: 's-1',
  tasks: [REVIEW],
  quiz_due: false,
  constraints: { lesson_ratio_ok: true, lesson_ratio: 0.5, throttle_ok: true, reviews: 1, lessons: 0 },
  course_complete: false,
  frontier_blocked_until: null,
};

const JOB_ID = '9f1d6f0e-0000-4000-8000-000000000001';

const SOLUTION = 'Divide both parts by 2.';

/** A miss. `next: null` keeps the feedback panel on screen: no auto-advance can steal it. */
const missed = (over: Partial<AnswerResponse> = {}): AnswerResponse => ({
  attempt_id: 'a-1',
  correct: false,
  work_quality: 'passable',
  error_tags: ['notation'],
  secs: 20,
  task_status: 'continue',
  remediation: [],
  next: null,
  next_unavailable: true,
  diagnosis: { id: JOB_ID, status: 'pending' },
  solution: SOLUTION,
  re_solve: 'Study the solution, then solve it again with no help.',
  ...over,
});

const READY_JOB: DiagnosisJob = {
  id: JOB_ID,
  status: 'ready',
  error_tags: ['sign-error'],
  prose: 'You subtracted the denominators instead of finding a common one.',
  model_id: 'test-model',
};

const PENDING_JOB: DiagnosisJob = { id: JOB_ID, status: 'pending', error_tags: [] };

function stubApi(over: Partial<ApiClient> = {}): ApiClient {
  return {
    ...createDemoApi(),
    taskServe: async () => P(1),
    taskAnswer: async () => missed(),
    ...over,
  };
}

async function mount(over: Partial<SessionProps> = {}) {
  resetToasts();
  const handlers = {
    onUnauthorized: vi.fn(),
    onExit: vi.fn(),
    onQuiz: vi.fn(),
    onDiagnostic: vi.fn(),
  };
  const props: SessionProps = { api: stubApi(), plan: PLAN, ...handlers, ...over };
  let view!: ReturnType<typeof render>;
  await act(async () => {
    view = render(<Session {...props} />, { container: document.getElementById('view')! });
  });
  return { ...view, ...handlers };
}

/** Answer the served problem wrong and land on the feedback panel. */
async function answerWrong(): Promise<void> {
  fireEvent.change(screen.getByLabelText('Answer'), { target: { value: '3/4' } });
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Submit' })); });
}

const panel = () => document.querySelector('.diagnosis');
const noteText = () => document.querySelector('.diagnosis-note')?.textContent ?? '';
const proseText = () => document.querySelector('.diagnosis-prose')?.textContent ?? '';

/** Move the clock and let every continuation that the move released settle. */
async function tick(ms: number): Promise<void> {
  await act(async () => { await vi.advanceTimersByTimeAsync(ms); });
}

// ---------------------------------------------------------------------------

describe('the async diagnosis panel', () => {
  it('DIAG-async: the verdict paints before any diagnosis arrives', async () => {
    const getDiagnosis = vi.fn<ApiClient['getDiagnosis']>(async () => PENDING_JOB);
    await mount({ api: stubApi({ getDiagnosis }) });
    await answerWrong();

    // The whole verdict is on screen, from local CPU, with the job still pending.
    expect(screen.getByText('Not quite')).toBeTruthy();
    expect(screen.getByText('notation')).toBeTruthy();
    expect(document.querySelector('.solution-text')!.textContent).toBe(SOLUTION);
    expect(screen.getByRole('button', { name: 'Next problem →' })).toBeTruthy();

    // And the panel is there, saying so, having asked the service for nothing yet.
    expect(panel()!.getAttribute('data-status')).toBe('pending');
    expect(noteText()).toBe(DIAGNOSIS_WAIT);
    expect(getDiagnosis).not.toHaveBeenCalled();
  });

  it('DIAG-async: nothing in the panel disables the way forward', async () => {
    const { onExit } = await mount();
    await answerWrong();

    expect(panel()!.getAttribute('data-status')).toBe('pending');
    for (const name of ['Next problem →', 'End session', 'Exit']) {
      expect((screen.getByRole('button', { name }) as HTMLButtonElement).disabled).toBe(false);
    }
    fireEvent.click(screen.getByRole('button', { name: 'Exit' }));
    expect(onExit).toHaveBeenCalledTimes(1);
  });

  it('opens one connection for the whole session, at /api/diagnosis/stream', async () => {
    await mount();
    expect(eventSources.length).toBe(1);
    expect(lastEventSource().url).toBe('/api/diagnosis/stream');

    // A second problem, and still one connection: per session, never per problem.
    await answerWrong();
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Next problem →' }));
    });
    expect(eventSources.length).toBe(1);
  });

  it('lands the pushed frame and never polls at all', async () => {
    vi.useFakeTimers();
    const getDiagnosis = vi.fn<ApiClient['getDiagnosis']>(async () => PENDING_JOB);
    await mount({ api: stubApi({ getDiagnosis }) });
    await answerWrong();

    // Inside the service's 250 ms notify-to-flush budget, so the 2 s poll never ticks.
    await tick(200);
    await act(async () => { lastEventSource().emit(READY_JOB); });

    expect(panel()!.getAttribute('data-status')).toBe('ready');
    expect(proseText()).toBe(READY_JOB.prose);
    expect(screen.getByText('sign-error')).toBeTruthy();

    await tick(DIAGNOSIS_POLL_MS * 3);
    expect(getDiagnosis).not.toHaveBeenCalled();
  });

  it('DIAG-poll: an SSE drop falls back to the 2 s poll and the prose still lands', async () => {
    vi.useFakeTimers();
    const getDiagnosis = vi.fn<ApiClient['getDiagnosis']>(async () => READY_JOB);
    await mount({ api: stubApi({ getDiagnosis }) });
    await answerWrong();

    // The connection dies and writes nothing ever again.
    act(() => { lastEventSource().drop(); });
    expect(getDiagnosis).not.toHaveBeenCalled();
    expect(panel()!.getAttribute('data-status')).toBe('pending');

    await tick(DIAGNOSIS_POLL_MS);

    expect(getDiagnosis).toHaveBeenCalledTimes(1);
    expect(getDiagnosis.mock.calls[0]).toEqual([JOB_ID]);
    expect(panel()!.getAttribute('data-status')).toBe('ready');
    expect(proseText()).toBe(READY_JOB.prose);

    // The job is done, so the fallback stops. A finished job is not polled forever.
    await tick(DIAGNOSIS_POLL_MS * 4);
    expect(getDiagnosis).toHaveBeenCalledTimes(1);
  });

  it('DIAG-poll: a failed poll raises no toast and the next tick tries again', async () => {
    vi.useFakeTimers();
    const getDiagnosis = vi.fn<ApiClient['getDiagnosis']>()
      .mockRejectedValueOnce(new Error('gateway is down'))
      .mockResolvedValue(READY_JOB);
    await mount({ api: stubApi({ getDiagnosis }) });
    await answerWrong();

    await tick(DIAGNOSIS_POLL_MS);
    expect(getDiagnosis).toHaveBeenCalledTimes(1);
    // A background read the learner did not ask for never interrupts him. The store is read
    // directly: this view renders no `ToastHost`, so a DOM query would pass vacuously.
    expect(toastStore.getSnapshot()).toEqual([]);
    expect(panel()!.getAttribute('data-status')).toBe('pending');

    await tick(DIAGNOSIS_POLL_MS);
    expect(getDiagnosis).toHaveBeenCalledTimes(2);
    expect(proseText()).toBe(READY_JOB.prose);
  });

  it('DIAG-30s: a job pending past 30 s reads as failed and the verdict stands', async () => {
    vi.useFakeTimers();
    const getDiagnosis = vi.fn<ApiClient['getDiagnosis']>(async () => PENDING_JOB);
    await mount({ api: stubApi({ getDiagnosis }) });
    await answerWrong();

    await tick(DIAGNOSIS_DEADLINE_MS - DIAGNOSIS_POLL_MS);
    expect(panel()!.getAttribute('data-status')).toBe('pending');
    expect(getDiagnosis).toHaveBeenCalledTimes(14);

    await tick(DIAGNOSIS_POLL_MS);

    expect(panel()!.getAttribute('data-status')).toBe('failed');
    expect(noteText()).toBe(DIAGNOSIS_FAILED);
    // THE VERDICT STANDS. Nothing the learner already had is taken away.
    expect(screen.getByText('Not quite')).toBeTruthy();
    expect(screen.getByText('notation')).toBeTruthy();
    expect(document.querySelector('.solution-text')!.textContent).toBe(SOLUTION);

    // And the polling stops with it: a dead job is never read again.
    const calls = getDiagnosis.mock.calls.length;
    await tick(DIAGNOSIS_POLL_MS * 5);
    expect(getDiagnosis.mock.calls.length).toBe(calls);
  });

  it('DIAG-30s: a frame that lands after the deadline never overwrites the failure', async () => {
    vi.useFakeTimers();
    await mount({ api: stubApi({ getDiagnosis: async () => PENDING_JOB }) });
    await answerWrong();

    await tick(DIAGNOSIS_DEADLINE_MS);
    expect(panel()!.getAttribute('data-status')).toBe('failed');

    await act(async () => { lastEventSource().emit(READY_JOB); });
    expect(panel()!.getAttribute('data-status')).toBe('failed');
    expect(document.querySelector('.diagnosis-prose')).toBeNull();
  });

  it('paints a pre-authored ready diagnosis at once, with no job to watch', async () => {
    const getDiagnosis = vi.fn<ApiClient['getDiagnosis']>(async () => PENDING_JOB);
    const api = stubApi({
      getDiagnosis,
      taskAnswer: async () => missed({
        diagnosis: { status: 'ready', error_tags: ['off-by-one'], prose: 'You dropped the carry.' },
      }),
    });
    await mount({ api });
    await answerWrong();

    expect(panel()!.getAttribute('data-status')).toBe('ready');
    expect(proseText()).toBe('You dropped the carry.');
    expect(screen.getByText('off-by-one')).toBeTruthy();
    expect(getDiagnosis).not.toHaveBeenCalled();
  });

  it('renders no panel at all for not_offered', async () => {
    await mount({ api: stubApi({ taskAnswer: async () => missed({ diagnosis: { status: 'not_offered' } }) }) });
    await answerWrong();

    expect(screen.getByText('Not quite')).toBeTruthy();
    expect(panel()).toBeNull();
  });

  it('reads a capped job as failed, because a cap the learner cannot lift is a failure', async () => {
    vi.useFakeTimers();
    await mount({ api: stubApi({ getDiagnosis: async () => PENDING_JOB }) });
    await answerWrong();

    await act(async () => {
      lastEventSource().emit({ id: JOB_ID, status: 'capped', error_tags: [] });
    });

    expect(panel()!.getAttribute('data-status')).toBe('failed');
    expect(noteText()).toBe(DIAGNOSIS_FAILED);
  });

  it('drops a frame that is not JSON and lets the poll behind it land the prose', async () => {
    vi.useFakeTimers();
    const getDiagnosis = vi.fn<ApiClient['getDiagnosis']>(async () => READY_JOB);
    await mount({ api: stubApi({ getDiagnosis }) });
    await answerWrong();

    await act(async () => { lastEventSource().emitRaw('{"id":"9f1d6f0e-0000-40'); });
    expect(panel()!.getAttribute('data-status')).toBe('pending');

    await tick(DIAGNOSIS_POLL_MS);
    expect(proseText()).toBe(READY_JOB.prose);
  });

  it('closes the subscription on unmount and arms nothing after it', async () => {
    vi.useFakeTimers();
    const getDiagnosis = vi.fn<ApiClient['getDiagnosis']>(async () => PENDING_JOB);
    const view = await mount({ api: stubApi({ getDiagnosis }) });
    await answerWrong();

    const source = lastEventSource();
    expect(source.closed).toBe(false);

    await act(async () => { view.unmount(); });

    expect(source.closed).toBe(true);
    // The poll and the 30 s deadline died with the view (F-37-1b), so a clock that runs on
    // past the deadline reaches no continuation at all.
    const calls = getDiagnosis.mock.calls.length;
    await tick(DIAGNOSIS_DEADLINE_MS * 2);
    expect(getDiagnosis.mock.calls.length).toBe(calls);
  });

  it('announces the panel politely and reports no axe violation', async () => {
    await mount();
    await answerWrong();

    expect(panel()!.getAttribute('aria-live')).toBe('polite');
    const results = await axe(document.getElementById('view')!, AXE_IN_JSDOM);
    expect(results).toHaveNoViolations();
  });
});
