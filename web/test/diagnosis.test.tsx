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
 *
 * The tests of the first block move the clock in units of the IMPORTED constants, which
 * reads well and pins the RATIO only: a mutant that doubles both numbers keeps every one of
 * them green (M6-review-2, finding V12). The second block is the absolute oracle. It writes
 * 2000 and 30000 as numbers, and it reads the service's own `PENDING_DEADLINE_SECS` and
 * `POLL_INTERVAL_SECS` out of `crates/web/src/diagnosis.rs`, because the client rule and the
 * service rule are ONE rule and a client that gives up first reports a failure the service
 * is still working on.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, screen } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { createDemoApi } from '@/api';
import { DIAGNOSIS_DEADLINE_MS, DIAGNOSIS_POLL_MS } from '@/views/session/useDiagnosis';
import { DIAGNOSIS_FAILED, DIAGNOSIS_WAIT } from '@/views/session/Diagnosis';
import { toastStore } from '@/app/toast';
import { AXE_IN_JSDOM } from './axe';
import { eventSources, lastEventSource } from './setup';
import { REVIEW, mount as mountSession, planOf } from './helpers/session';
import { tick } from './helpers/timers';
import type { AnswerResponse, ApiClient, DiagnosisJob, ServedProblem } from '@/api/types';
import type { SessionProps } from '@/views/session/Session';

// ---------------------------------------------------------------------------
// Fixtures — a one-task plan whose single problem is answered wrong, because a miss is the
// only shape that owes a diagnosis at all.
// ---------------------------------------------------------------------------

const P = (n: number): ServedProblem => ({
  problem_id: `p${n}`,
  index: n,
  total: 2,
  text: `Simplify $\\frac{${n}}{2}$.`,
  kp: 'kp-simplify',
  time_budget_secs: 60,
  countdown: false,
});

const PLAN = planOf({ ...REVIEW, n_problems: 2 });

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

const mount = (over: Partial<SessionProps> = {}) =>
  mountSession({ api: stubApi(), plan: PLAN, ...over });

/** Answer the served problem wrong and land on the feedback panel. */
async function answerWrong(): Promise<void> {
  fireEvent.change(screen.getByLabelText('Answer'), { target: { value: '3/4' } });
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Submit' })); });
}

/**
 * On a fake clock, mount over a poll route that answers `job`, answer wrong, and hand back
 * the poll route so the test counts its reads.
 */
async function answerWrongOver(job: DiagnosisJob) {
  vi.useFakeTimers();
  const getDiagnosis = vi.fn<ApiClient['getDiagnosis']>(async () => job);
  await mount({ api: stubApi({ getDiagnosis }) });
  await answerWrong();
  return getDiagnosis;
}

const panel = () => document.querySelector('.diagnosis');
const noteText = () => document.querySelector('.diagnosis-note')?.textContent ?? '';
const proseText = () => document.querySelector('.diagnosis-prose')?.textContent ?? '';

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
    const getDiagnosis = await answerWrongOver(PENDING_JOB);

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
    const getDiagnosis = await answerWrongOver(READY_JOB);

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
    const getDiagnosis = await answerWrongOver(PENDING_JOB);

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

  it('renders no tag row for a ready diagnosis with no tag', async () => {
    await mount();
    await answerWrong();
    await act(async () => { lastEventSource().emit({ ...READY_JOB, error_tags: [] }); });
    expect(panel()!.getAttribute('data-status')).toBe('ready');
    expect(proseText()).toBe(READY_JOB.prose);
    expect(panel()!.querySelector('.error-tags')).toBeNull();
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
    await answerWrongOver(READY_JOB);

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

// ---------------------------------------------------------------------------
// The two timing literals of spec section 2.1, pinned in absolute terms.
// ---------------------------------------------------------------------------

/** The service file that owns the same two rules. ONE rule, two implementations. */
const DIAGNOSIS_RS = join(
  dirname(fileURLToPath(import.meta.url)),
  '../../crates/web/src/diagnosis.rs',
);

/**
 * Read one `pub const NAME: i64 = N;` out of the service source.
 *
 * The source IS the shared constant: nothing generates a fixture for these two numbers, and
 * a TypeScript copy of them would be one more thing to drift. An absent constant throws, so
 * a rename in the service fails this file instead of passing it vacuously.
 */
function serverSecs(name: string): number {
  const source = readFileSync(DIAGNOSIS_RS, 'utf8');
  const found = new RegExp(`pub const ${name}: i64 = (\\d+);`).exec(source);
  if (!found) throw new Error(`${name} is absent from crates/web/src/diagnosis.rs`);
  return Number(found[1]);
}

describe('the diagnosis timing literals', () => {
  it('polls every 2000 ms and gives up at 30000 ms', () => {
    expect(DIAGNOSIS_POLL_MS).toBe(2000);
    expect(DIAGNOSIS_DEADLINE_MS).toBe(30000);
  });

  it('gives up at the second the service gives up at', () => {
    expect(serverSecs('PENDING_DEADLINE_SECS')).toBe(30);
    expect(serverSecs('POLL_INTERVAL_SECS')).toBe(2);
    expect(DIAGNOSIS_DEADLINE_MS).toBe(serverSecs('PENDING_DEADLINE_SECS') * 1000);
    expect(DIAGNOSIS_POLL_MS).toBe(serverSecs('POLL_INTERVAL_SECS') * 1000);
  });

  it('reads nothing at 1999 ms, reads once at 2000 ms, and reads again at 4000 ms', async () => {
    const getDiagnosis = await answerWrongOver(PENDING_JOB);

    await tick(1999);
    expect(getDiagnosis).toHaveBeenCalledTimes(0);
    await tick(1);
    expect(getDiagnosis).toHaveBeenCalledTimes(1);

    await tick(1999);
    expect(getDiagnosis).toHaveBeenCalledTimes(1);
    await tick(1);
    expect(getDiagnosis).toHaveBeenCalledTimes(2);
  });

  it('is still pending at 29999 ms and reads failed at 30000 ms', async () => {
    vi.useFakeTimers();
    await mount({ api: stubApi({ getDiagnosis: async () => PENDING_JOB }) });
    await answerWrong();

    await tick(29999);
    expect(panel()!.getAttribute('data-status')).toBe('pending');

    await tick(1);
    expect(panel()!.getAttribute('data-status')).toBe('failed');
    expect(noteText()).toBe(DIAGNOSIS_FAILED);
  });
});
