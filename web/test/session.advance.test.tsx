/**
 * The study loop (S8), part 3: the advance, one write per mount, the slow write, and the
 * exit paths.
 *
 * `session.test.tsx` carries the module note and the fixtures live in
 * `test/helpers/session.tsx`.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { createDemoApi } from '@/api';
import { DialogProvider } from '@/components/Modal';
import { Dashboard, type DashboardProps } from '@/views/Dashboard';
import { resetToasts } from '@/app/toast';
import { signed } from '@/lib/format';
import {
  LESSON, REVIEW, TEACHING, P, answerInput, clickNext, closed, graded, mount, planOf,
  press, progressCount, stubApi, submitAnswer, submitThenWait, workInput, typeAnswer,
} from './helpers/session';
import type {
  ApiClient, ServedProblem, SessionStartResponse, StatusResponse,
} from '@/api/types';

/** The dashboard's own fixture, for the one test that starts a session from that screen. */
const DASHBOARD_STATUS: StatusResponse = {
  course: { id: 'foundations', name: 'Foundations' },
  placed: true,
  courses: [{ id: 'foundations', name: 'Foundations', current: true }],
  test_prep: null,
  xp: { total: 340, today: 12, goal: 40, streak_days: 3 },
  velocity: { xp_per_day_28d: 21.5, topics_per_week_28d: 2.25, course_progress: 0.18, eta: null },
  quiz: { last_at: null, xp_since: 0, retake_pending: false },
  pending_remediation: [],
  quiz_due: false,
  drill_due: false,
  frontier: 4,
  due_reviews: 2,
  nearly_due: 1,
};

describe('the advance', () => {
  it('auto-advance fires only on correct-with-next', async () => {
    vi.useFakeTimers();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => graded());
    await mount({ api: stubApi({ taskAnswer }) });

    await submitAnswer('3/4');
    expect(progressCount()).toBe('1 / 3');

    // Nothing at 1399 ms; the next problem at 1400.
    await act(async () => { vi.advanceTimersByTime(1399); });
    expect(screen.getByText('Correct')).toBeTruthy();
    await act(async () => { vi.advanceTimersByTime(1); });

    expect(progressCount()).toBe('2 / 3');
    expect(screen.queryByText('Correct')).toBeNull();
    expect(answerInput().value).toBe('');
  });

  it('auto-advance never fires on a miss', async () => {
    vi.useFakeTimers();
    await mount({ api: stubApi({ taskAnswer: async () => graded({ correct: false }) }) });

    await submitThenWait('3/4', 5000);

    expect(screen.getByText('Not quite')).toBeTruthy();
    expect(progressCount()).toBe('1 / 3');
  });

  it('auto-advance never fires when the service could draw no next problem', async () => {
    vi.useFakeTimers();
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    await mount({
      api: stubApi({ taskServe, taskAnswer: async () => graded({ next: null, next_unavailable: true }) }),
    });

    await submitThenWait('3/4', 5000);

    // Still on the verdict, and the button says the task is not over (`next_unavailable`).
    expect(screen.getByRole('button', { name: 'Next problem →' })).toBeTruthy();
    expect(taskServe).toHaveBeenCalledTimes(1);
  });

  it('auto-advance cancels on click', async () => {
    vi.useFakeTimers();
    const replies = [graded(), graded({ next: P(3), attempt_id: 'a-2' })];
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => replies.shift() ?? graded({ next: null }));
    await mount({ api: stubApi({ taskAnswer }) });

    // The click lands inside the 1400 ms window and takes the one transition out of
    // `feedback`. The timer then finds the gate shut.
    await submitThenWait('3/4', 600);
    expect(await clickNext()).toBe('2 / 3');

    await act(async () => { vi.advanceTimersByTime(5000); });
    expect(progressCount()).toBe('2 / 3');
    expect(taskAnswer).toHaveBeenCalledTimes(1);
  });

  it('next_unavailable re-serves the same task rather than skipping what is owed', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(2));
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => graded({ next: null, next_unavailable: true }));
    await mount({ api: stubApi({ taskServe, taskAnswer }) });

    await submitAnswer('3/4');
    expect(await clickNext()).toBe('2 / 3');

    expect(taskServe).toHaveBeenCalledTimes(2);
    expect(taskServe.mock.calls[1]).toEqual(['t-review']);
  });

  it('key={problem_id} prevents cross-problem answer bleed', async () => {
    const replies = [graded(), graded({ next: null, attempt_id: 'a-2' })];
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => replies.shift()!);
    await mount({ api: stubApi({ taskAnswer }) });

    typeAnswer('3/4');
    fireEvent.change(workInput(), { target: { value: 'the working of problem one' } });
    await submitAnswer('3/4');
    // A fresh subtree: both fields are new nodes, and both are empty.
    expect(await clickNext()).toBe('2 / 3');
    expect(answerInput().value).toBe('');
    expect(workInput().value).toBe('');

    await submitAnswer('1');

    // The second post carries the second problem and NO working. Without the key the first
    // problem's working posts here, into an append-only log.
    expect(taskAnswer.mock.calls[1]).toEqual(['t-review', { problem_id: 'p2', answer: '1' }]);
  });
});

describe('NO-2BILL: one write per mount', () => {
  it('NO-2BILL: a lesson mount teaches and serves nothing until the learner asks', async () => {
    const taskTeach = vi.fn<ApiClient['taskTeach']>(async () => TEACHING);
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    await mount({ plan: planOf(LESSON), api: stubApi({ taskTeach, taskServe }) });

    expect(taskTeach).toHaveBeenCalledTimes(1);
    expect(taskServe).not.toHaveBeenCalled();
    expect(screen.getByText('Worked example')).toBeTruthy();
    expect(screen.getByText(TEACHING.concept)).toBeTruthy();
    // The example is a lesson card, not a problem card: nothing here takes an answer.
    expect(screen.queryByLabelText('Answer')).toBeNull();

    await press("I've got it — practice ▸");

    expect(taskServe).toHaveBeenCalledTimes(1);
    expect(taskTeach).toHaveBeenCalledTimes(1);
    expect(answerInput()).toBeTruthy();
  });

  it('NO-2BILL: a review mount serves once and teaches never', async () => {
    const taskTeach = vi.fn<ApiClient['taskTeach']>(async () => TEACHING);
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    await mount({ api: stubApi({ taskTeach, taskServe }) });

    expect(taskServe).toHaveBeenCalledTimes(1);
    expect(taskTeach).not.toHaveBeenCalled();
  });

  it('AUDIT-j: a failed teach shows the no-instruction card and serves NO practice', async () => {
    const taskTeach = vi.fn<ApiClient['taskTeach']>(async () => { throw new Error('the teach route is down'); });
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    await mount({ plan: planOf(LESSON, REVIEW), api: stubApi({ taskTeach, taskServe }) });

    await waitFor(() => expect(screen.getByText('No instruction yet for this lesson')).toBeTruthy());
    // The whole point of the card: the learner never practises an untaught skill.
    expect(taskServe).not.toHaveBeenCalled();
    expect(screen.queryByLabelText('Answer')).toBeNull();

    // One control leads on, and it takes the NEXT task.
    await press('Skip to the next task');
    await waitFor(() => expect(taskServe).toHaveBeenCalledTimes(1));
    expect(taskServe.mock.calls[0]).toEqual(['t-review']);
  });

  it('AUDIT-j: an empty plan with blocked topics says the content is not written', async () => {
    const plan = { ...planOf(), blocked: [
      { task_type: 'lesson' as const, topic: 'fractions', kp: 'kp1', blockers: ['teachable'] },
    ] };
    await mount({ plan, api: stubApi({}) });
    expect(screen.getByText('1 topic(s) wait on content that is not written yet.')).toBeTruthy();
  });
});

describe('F-F2-2: a slow write never moves the learner', () => {
  it('F-F2-2: a slow session/start never yanks the user out of another view', async () => {
    resetToasts();
    let release!: (value: SessionStartResponse) => void;
    const pending = new Promise<SessionStartResponse>((r) => { release = r; });
    const api: ApiClient = {
      ...createDemoApi(),
      // Immediate, so the card is painted: the demo answers on a 120 ms delay.
      getStatus: async () => DASHBOARD_STATUS,
      sessionStart: () => pending,
    };
    const props: DashboardProps = {
      api,
      demo: false,
      onUnauthorized: vi.fn(),
      onSession: vi.fn(),
      onQuiz: vi.fn(),
      onDiagnostic: vi.fn(),
      onMap: vi.fn(),
    };
    let view!: ReturnType<typeof render>;
    await act(async () => {
      view = render(
        <DialogProvider><Dashboard {...props} /></DialogProvider>,
        { container: document.getElementById('view')! },
      );
    });

    await press('Continue studying');
    // The learner leaves before the write answers.
    view.unmount();
    await act(async () => {
      release({
        session: 's-1',
        reopened: false,
        xp: { total: 340, today: 12, goal: 40, streak_days: 3 },
        frontier: 4,
        due_reviews: 2,
      });
    });

    expect(props.onSession).not.toHaveBeenCalled();
  });

  it('F-F2-2: a serve that lands after the view is gone paints nothing', async () => {
    let release!: (p: ServedProblem) => void;
    const pending = new Promise<ServedProblem>((r) => { release = r; });
    const view = await mount({ api: stubApi({ taskServe: () => pending }) });

    view.unmount();
    await act(async () => { release(P(1)); });

    expect(document.querySelector('.problem-text')).toBeNull();
  });
});

describe('the exit paths', () => {
  it('Exit leaves the session open and posts nothing', async () => {
    const sessionEnd = vi.fn<ApiClient['sessionEnd']>(async () => closed({ minutes: 4 }));
    const { onExit } = await mount({ api: stubApi({ sessionEnd }) });

    fireEvent.click(screen.getByRole('button', { name: 'Exit' }));

    expect(onExit).toHaveBeenCalledTimes(1);
    expect(sessionEnd).not.toHaveBeenCalled();
  });

  it('End session closes with no minutes argument and shows the summary', async () => {
    const sessionEnd = vi.fn<ApiClient['sessionEnd']>(async () => closed({
      xp_earned: 12,
      minutes: 8,
      xp: { total: 352, today: 24, goal: 40, streak_days: 3 },
    }));
    const { onExit } = await mount({
      api: stubApi({ sessionEnd, taskAnswer: async () => graded({ next: null }) }),
    });

    await submitAnswer('3/4');
    await press('End session');

    // NO ARGUMENTS: the service measures the session itself and that value prices the XP.
    expect(sessionEnd).toHaveBeenCalledWith();
    expect(screen.getByText('Session complete')).toBeTruthy();
    expect(screen.getByText(signed(12))).toBeTruthy();
    expect(screen.getByText('8')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Back to dashboard' }));
    expect(onExit).toHaveBeenCalledTimes(1);
  });

  it('the last answer of the last task closes the session', async () => {
    const sessionEnd = vi.fn<ApiClient['sessionEnd']>(async () => closed());
    await mount({
      api: stubApi({
        sessionEnd,
        taskAnswer: async () => graded({ next: null, task_status: 'task_passed' }),
      }),
    });

    await submitAnswer('3/4');
    await press('Continue →');

    expect(sessionEnd).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Session complete')).toBeTruthy();
  });

  it('a remediation asks the core for a fresh plan, and the core decides what is next', async () => {
    const getPlan = vi.fn<ApiClient['getPlan']>(async () => planOf({ ...REVIEW, task_id: 't-remedial' }));
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    await mount({
      api: stubApi({
        getPlan,
        taskServe,
        taskAnswer: async () => graded({
          next: null,
          task_status: 'task_passed',
          remediation: [{ kind: 'review', targets: ['fractions'] }],
        }),
      }),
    });

    await submitAnswer('3/4');
    expect(screen.getByText('review: fractions')).toBeTruthy();

    await press('Continue →');

    expect(getPlan).toHaveBeenCalledTimes(1);
    expect(taskServe).toHaveBeenCalledTimes(2);
    expect(taskServe.mock.calls[1]).toEqual(['t-remedial']);
  });
});
