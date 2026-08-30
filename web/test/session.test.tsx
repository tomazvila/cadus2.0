/**
 * The study loop (S8).
 *
 * Six invariants are claimed here, and every one of them is a NEGATIVE — the reason the
 * invariant gate exists beside a coverage floor:
 *
 *   F-37-1c   Enter during grading posts nothing.
 *   F-F2-2    a slow `session/start` never yanks the learner out of another view.
 *   DD-3/P1   an assisted pass returns to `ready`; the unaided re-solve locks it in.
 *   W-A4      the reference lesson is shown once.
 *   W-C5      Submit is dominant; the rest stays quiet.
 *   NO-2BILL  no view mount issues two writes.
 *
 * Every fixture below is the frozen contract of `docs/reference/web-service-1.0-spec.md`,
 * so each assertion is a literal a reader checks by hand: three problems in the task, the
 * progress count `1 / 3`, the clock `0:00`, the auto-advance at 1400 ms.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { createDemoApi } from '@/api';
import { DialogProvider } from '@/components/Modal';
import { Dashboard, type DashboardProps } from '@/views/Dashboard';
import { Session, isDrill, type SessionProps } from '@/views/session/Session';
import { resetToasts } from '@/app/toast';
import { fmtClock, signed } from '@/lib/format';
import { AXE_IN_JSDOM } from './axe';
import type {
  AnswerResponse,
  ApiClient,
  PlanTask,
  ReworkResponse,
  ServedProblem,
  SessionPlanResponse,
  SessionStartResponse,
  StatusResponse,
  TeachResponse,
} from '@/api/types';

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const TOPIC = { id: 'fractions', name: 'Fractions', module: 'Arithmetic' };

const REVIEW: PlanTask = {
  task_id: 't-review',
  task_type: 'review',
  topic: TOPIC,
  kp: 'kp-simplify',
  start_at_kp: null,
  n_problems: 3,
  mix: null,
  component_topics: null,
  time_budget_secs: 600,
  difficulty_target: 0.6,
  why: 'due for review',
  progress: { answered: 0, done: false },
};

const LESSON: PlanTask = { ...REVIEW, task_id: 't-lesson', task_type: 'lesson', why: null };

const DRILL: PlanTask = { ...REVIEW, task_id: 't-drill', task_type: 'drill', why: null };

/** The `n`-th problem of a three-problem task. */
const P = (n: number, over: Partial<ServedProblem> = {}): ServedProblem => ({
  problem_id: `p${n}`,
  index: n,
  total: 3,
  text: `Simplify $\\frac{${n}}{2}$.`,
  kp: 'kp-simplify',
  time_budget_secs: 60,
  countdown: false,
  ...over,
});

const planOf = (...tasks: PlanTask[]): SessionPlanResponse => ({
  session: 's-1',
  tasks,
  quiz_due: false,
  constraints: { lesson_ratio_ok: true, lesson_ratio: 0.5, throttle_ok: true, reviews: 1, lessons: 0 },
  course_complete: false,
  frontier_blocked_until: null,
});

const graded = (over: Partial<AnswerResponse> = {}): AnswerResponse => ({
  attempt_id: 'a-1',
  correct: true,
  work_quality: 'perfect',
  error_tags: [],
  secs: 20,
  task_status: 'continue',
  remediation: [],
  next: P(2),
  diagnosis: { status: 'not_offered' },
  solution: 'Divide both parts by 2.',
  xp: 10,
  ...over,
});

/** The H3 first branch. `expected` is deliberately unlike any hint text below. */
const REWORK: ReworkResponse = {
  rework_required: true,
  problem_id: 'p1',
  solution: 'Divide both parts by 2.',
  expected: '17/23',
  re_solve: 'Study the solution, then solve it again with no help.',
};

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

const TEACHING: TeachResponse = {
  kp: 'kp-simplify',
  concept: 'A fraction names the same number when both parts divide by the same factor.',
  worked_example: { problem: 'Simplify $\\frac{4}{6}$.', steps: 'Both parts divide by 2.' },
};

/**
 * A client built on the demo backend, so every method exists and a missing override is a
 * type error rather than a `not a function` inside a handler (F-F6-1).
 */
function stubApi(over: Partial<ApiClient> = {}): ApiClient {
  return {
    ...createDemoApi(),
    taskServe: async () => P(1),
    taskAnswer: async () => graded(),
    ...over,
  };
}

const nav = () => ({
  onUnauthorized: vi.fn(),
  onExit: vi.fn(),
  onQuiz: vi.fn(),
  onDiagnostic: vi.fn(),
});

/** `plan: undefined` MEANS "read the plan yourself", so the key is honored, not defaulted. */
type MountOver =
  Partial<Omit<SessionProps, 'plan'>> & { plan?: SessionPlanResponse | undefined };

async function mount(over: MountOver = {}) {
  resetToasts();
  const handlers = nav();
  const props: SessionProps = {
    api: stubApi(),
    ...handlers,
    ...(over as Partial<Omit<SessionProps, 'plan'>>),
  };
  const plan = 'plan' in over ? over.plan : planOf(REVIEW);
  if (plan) props.plan = plan;
  let view!: ReturnType<typeof render>;
  await act(async () => {
    view = render(<Session {...props} />, { container: document.getElementById('view')! });
  });
  return { ...view, ...handlers };
}

const answerInput = () => screen.getByLabelText('Answer') as HTMLInputElement;
const workInput = () => screen.getByLabelText('Working') as HTMLTextAreaElement;
const submitButton = () => screen.getByRole('button', { name: 'Submit' });

/** Type into the uncontrolled field the way a keyboard does: value first, then the event. */
function typeAnswer(text: string): void {
  fireEvent.change(answerInput(), { target: { value: text } });
}

// ---------------------------------------------------------------------------

describe('the study loop', () => {
  it('serves the first task and paints the problem, the count and the clock', async () => {
    await mount();

    expect(screen.getByText('review')).toBeTruthy();
    expect(screen.getByText('Fractions')).toBeTruthy();
    expect(screen.getByText('due for review')).toBeTruthy();
    expect(document.querySelector('.progress-count')!.textContent).toBe('1 / 3');
    expect(document.querySelector('.timer')!.textContent).toBe('0:00');
    expect(document.querySelector('.problem-text')!.textContent).toContain('Simplify');
  });

  it('reads the plan itself when the caller hands none over', async () => {
    const getPlan = vi.fn<ApiClient['getPlan']>(async () => planOf(REVIEW));
    await mount({ plan: undefined, api: stubApi({ getPlan }) });

    expect(getPlan).toHaveBeenCalledTimes(1);
    expect(document.querySelector('.problem-text')).toBeTruthy();
  });

  it('offers the placement when the plan is empty, so the screen is no dead end', async () => {
    const { onDiagnostic, onExit } = await mount({ plan: planOf() });

    expect(screen.getByText('Nothing is due right now — enjoy the break.')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Take the placement diagnostic' }));
    expect(onDiagnostic).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole('button', { name: 'Back to dashboard' }));
    expect(onExit).toHaveBeenCalledTimes(1);
  });

  it('hands a quiz task to the quiz screen and serves nothing', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    const quiz: PlanTask = { ...REVIEW, task_id: 't-quiz', task_type: 'quiz', topic: null };
    const { onQuiz } = await mount({ plan: planOf(quiz), api: stubApi({ taskServe }) });

    expect(onQuiz).toHaveBeenCalledTimes(1);
    expect(onQuiz.mock.calls[0][0]).toEqual(quiz);
    // WITH the task, never the id alone: the quiz clock reads the task budget.
    expect(taskServe).not.toHaveBeenCalled();
  });

  it('posts the answer and the working, then paints the verdict and the solution', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => graded({ correct: false, next: null, error_tags: ['sign-error'], work_quality: 'passable' }));
    await mount({ api: stubApi({ taskAnswer }) });

    typeAnswer('3/4');
    fireEvent.change(workInput(), { target: { value: 'divide by two' } });
    await act(async () => { fireEvent.click(submitButton()); });

    expect(taskAnswer).toHaveBeenCalledTimes(1);
    expect(taskAnswer.mock.calls[0]).toEqual([
      't-review',
      { problem_id: 'p1', answer: '3/4', work: 'divide by two' },
    ]);
    expect(screen.getByText('Not quite')).toBeTruthy();
    expect(screen.getByText('sign-error')).toBeTruthy();
    expect(screen.getByText('Divide both parts by 2.')).toBeTruthy();
    // Hard Rule 2: the structural verdict and the work quality are both shown, and neither
    // is derived from the other.
    expect(screen.getByText('passable')).toBeTruthy();
  });

  it('an empty answer posts nothing and returns the focus to the field', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => graded());
    await mount({ api: stubApi({ taskAnswer }) });

    await act(async () => { fireEvent.click(submitButton()); });

    expect(taskAnswer).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(answerInput());
  });

  it('a failed grade returns the problem to the learner instead of locking the card', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => { throw new Error('the network went away'); });
    await mount({ api: stubApi({ taskAnswer }) });

    typeAnswer('3/4');
    await act(async () => { fireEvent.click(submitButton()); });

    expect(taskAnswer).toHaveBeenCalledTimes(1);
    expect(submitButton().hasAttribute('disabled')).toBe(false);
    typeAnswer('3/4');
    await act(async () => { fireEvent.click(submitButton()); });
    expect(taskAnswer).toHaveBeenCalledTimes(2);
  });
});

describe('F-37-1c: the phase gate', () => {
  it('F-37-1c: Enter during grading posts nothing', async () => {
    let release!: (value: AnswerResponse) => void;
    const pending = new Promise<AnswerResponse>((r) => { release = r; });
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(() => pending);
    await mount({ api: stubApi({ taskAnswer }) });

    typeAnswer('3/4');
    fireEvent.keyDown(answerInput(), { key: 'Enter' });
    expect(taskAnswer).toHaveBeenCalledTimes(1);

    // The field stays enabled while the grade runs — the gate, not the attribute, is what
    // stops the second post. Three more Enters, and a click, land inside the window.
    expect(answerInput().disabled).toBe(false);
    fireEvent.keyDown(answerInput(), { key: 'Enter' });
    fireEvent.keyDown(answerInput(), { key: 'Enter' });
    fireEvent.keyDown(workInput(), { key: 'Enter' });
    fireEvent.click(submitButton());
    expect(taskAnswer).toHaveBeenCalledTimes(1);

    await act(async () => { release(graded()); });
    expect(taskAnswer).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Correct')).toBeTruthy();
  });

  it('F-37-1c: Enter after the verdict posts nothing more', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => graded({ next: null }));
    await mount({ api: stubApi({ taskAnswer }) });

    typeAnswer('3/4');
    await act(async () => { fireEvent.keyDown(answerInput(), { key: 'Enter' }); });
    expect(screen.getByText('Correct')).toBeTruthy();

    fireEvent.keyDown(answerInput(), { key: 'Enter' });
    expect(taskAnswer).toHaveBeenCalledTimes(1);
  });

  it('F-37-1c: a hint asked for during grading is dropped', async () => {
    let release!: (value: AnswerResponse) => void;
    const pending = new Promise<AnswerResponse>((r) => { release = r; });
    const taskHint = vi.fn<ApiClient['taskHint']>(async () => ({ hint: 'Look at the factors.', hint_number: 1 }));
    await mount({ api: stubApi({ taskAnswer: () => pending, taskHint }) });

    typeAnswer('3/4');
    fireEvent.keyDown(answerInput(), { key: 'Enter' });
    // The `H` key reaches the field while the buttons are disabled.
    fireEvent.change(answerInput(), { target: { value: '' } });
    fireEvent.keyDown(answerInput(), { key: 'h' });

    expect(taskHint).not.toHaveBeenCalled();
    await act(async () => { release(graded()); });
  });
});

describe('DD-3/P1: the re-solve', () => {
  it('DD-3/P1: an assisted pass returns to ready and the same submit sends the re-solve', async () => {
    const replies = [REWORK, graded({ next: null, task_status: 'task_passed' })];
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => replies.shift()!);
    await mount({ api: stubApi({ taskAnswer }) });

    typeAnswer('3/4');
    await act(async () => { fireEvent.click(submitButton()); });

    // Not terminal: the problem is still on screen, the field is cleared, and Submit is live.
    expect(screen.getByText('Make it stick')).toBeTruthy();
    expect(screen.getByText(REWORK.re_solve)).toBeTruthy();
    expect(screen.getByText('Divide both parts by 2.')).toBeTruthy();
    expect(answerInput().value).toBe('');
    expect(submitButton().hasAttribute('disabled')).toBe(false);
    expect(screen.queryByRole('button', { name: 'Next problem →' })).toBeNull();

    // The SAME submit, and the SAME problem id.
    typeAnswer('3/4');
    await act(async () => { fireEvent.click(submitButton()); });

    expect(taskAnswer).toHaveBeenCalledTimes(2);
    expect(taskAnswer.mock.calls[1][1].problem_id).toBe('p1');
    expect(taskAnswer.mock.calls[1][1]).not.toHaveProperty('assisted');
    expect(screen.getByText('Correct')).toBeTruthy();
    expect(screen.queryByText('Make it stick')).toBeNull();
  });

  it('DD-3/P1: the re-solve panel arms no auto-advance', async () => {
    vi.useFakeTimers();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => REWORK);
    await mount({ api: stubApi({ taskAnswer }) });

    typeAnswer('3/4');
    await act(async () => { fireEvent.click(submitButton()); });
    await act(async () => { vi.advanceTimersByTime(5000); });

    expect(taskAnswer).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Make it stick')).toBeTruthy();
  });
});

describe('the hint ladder', () => {
  it('a hint body never contains expected', async () => {
    const taskHint = vi.fn<ApiClient['taskHint']>(async () => ({ hint: 'Find the common factor.', hint_number: 1 }));
    await mount({ api: stubApi({ taskHint }) });

    await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Hint' })); });

    expect(taskHint.mock.calls[0]).toEqual(['t-review', 'p1']);
    expect(screen.getByText('Hint 1:')).toBeTruthy();
    expect(screen.getByText('Find the common factor.')).toBeTruthy();
    // The expected answer of this problem reaches the screen only from a grade reply.
    expect(document.body.textContent).not.toContain(REWORK.expected);
    expect(document.body.textContent).not.toContain('Divide both parts by 2.');
  });

  it('W-A4: the reference lesson is shown once', async () => {
    const reference = { topic: 'fractions', name: 'Simplifying fractions' };
    let n = 0;
    const taskHint = vi.fn<ApiClient['taskHint']>(async () => {
      n += 1;
      return { hint: `Hint body ${n}`, hint_number: n, reference_lesson: reference };
    });
    await mount({ api: stubApi({ taskHint }) });

    const hintButton = screen.getByRole('button', { name: 'Hint' });
    await act(async () => { fireEvent.click(hintButton); });
    await act(async () => { fireEvent.click(hintButton); });
    await act(async () => { fireEvent.click(hintButton); });

    expect(taskHint).toHaveBeenCalledTimes(3);
    expect(document.querySelectorAll('.hint').length).toBe(3);
    expect(document.querySelectorAll('.reference-lesson').length).toBe(1);
    expect(document.querySelector('.reference-lesson')!.textContent).toContain(
      'Simplifying fractions',
    );
  });

  it('W-C5: Submit is dominant; Hint and Exit stay quiet', async () => {
    await mount();

    const primaries = document.querySelectorAll('.view-session .btn-primary');
    expect(primaries.length).toBe(1);
    expect(primaries[0].textContent).toBe('Submit');
    expect(screen.getByRole('button', { name: 'Hint' }).className).toContain('btn-ghost');
    expect(screen.getByRole('button', { name: 'Exit' }).className).toContain('btn-ghost');
  });
});

describe('the advance', () => {
  it('auto-advance fires only on correct-with-next', async () => {
    vi.useFakeTimers();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => graded());
    await mount({ api: stubApi({ taskAnswer }) });

    typeAnswer('3/4');
    await act(async () => { fireEvent.click(submitButton()); });
    expect(document.querySelector('.progress-count')!.textContent).toBe('1 / 3');

    // Nothing at 1399 ms; the next problem at 1400.
    await act(async () => { vi.advanceTimersByTime(1399); });
    expect(screen.getByText('Correct')).toBeTruthy();
    await act(async () => { vi.advanceTimersByTime(1); });

    expect(document.querySelector('.progress-count')!.textContent).toBe('2 / 3');
    expect(screen.queryByText('Correct')).toBeNull();
    expect(answerInput().value).toBe('');
  });

  it('auto-advance never fires on a miss', async () => {
    vi.useFakeTimers();
    await mount({ api: stubApi({ taskAnswer: async () => graded({ correct: false }) }) });

    typeAnswer('3/4');
    await act(async () => { fireEvent.click(submitButton()); });
    await act(async () => { vi.advanceTimersByTime(5000); });

    expect(screen.getByText('Not quite')).toBeTruthy();
    expect(document.querySelector('.progress-count')!.textContent).toBe('1 / 3');
  });

  it('auto-advance never fires when the service could draw no next problem', async () => {
    vi.useFakeTimers();
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    await mount({
      api: stubApi({ taskServe, taskAnswer: async () => graded({ next: null, next_unavailable: true }) }),
    });

    typeAnswer('3/4');
    await act(async () => { fireEvent.click(submitButton()); });
    await act(async () => { vi.advanceTimersByTime(5000); });

    // Still on the verdict, and the button says the task is not over (`next_unavailable`).
    expect(screen.getByRole('button', { name: 'Next problem →' })).toBeTruthy();
    expect(taskServe).toHaveBeenCalledTimes(1);
  });

  it('auto-advance cancels on click', async () => {
    vi.useFakeTimers();
    const replies = [graded(), graded({ next: P(3), attempt_id: 'a-2' })];
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => replies.shift() ?? graded({ next: null }));
    await mount({ api: stubApi({ taskAnswer }) });

    typeAnswer('3/4');
    await act(async () => { fireEvent.click(submitButton()); });
    // The click lands inside the 1400 ms window and takes the one transition out of
    // `feedback`. The timer then finds the gate shut.
    await act(async () => { vi.advanceTimersByTime(600); });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Next problem →' }));
    });
    expect(document.querySelector('.progress-count')!.textContent).toBe('2 / 3');

    await act(async () => { vi.advanceTimersByTime(5000); });
    expect(document.querySelector('.progress-count')!.textContent).toBe('2 / 3');
    expect(taskAnswer).toHaveBeenCalledTimes(1);
  });

  it('next_unavailable re-serves the same task rather than skipping what is owed', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(2));
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => graded({ next: null, next_unavailable: true }));
    await mount({ api: stubApi({ taskServe, taskAnswer }) });

    typeAnswer('3/4');
    await act(async () => { fireEvent.click(submitButton()); });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Next problem →' }));
    });

    expect(taskServe).toHaveBeenCalledTimes(2);
    expect(taskServe.mock.calls[1]).toEqual(['t-review']);
    expect(document.querySelector('.progress-count')!.textContent).toBe('2 / 3');
  });

  it('key={problem_id} prevents cross-problem answer bleed', async () => {
    const replies = [graded(), graded({ next: null, attempt_id: 'a-2' })];
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => replies.shift()!);
    await mount({ api: stubApi({ taskAnswer }) });

    typeAnswer('3/4');
    fireEvent.change(workInput(), { target: { value: 'the working of problem one' } });
    await act(async () => { fireEvent.click(submitButton()); });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Next problem →' }));
    });

    // A fresh subtree: both fields are new nodes, and both are empty.
    expect(document.querySelector('.progress-count')!.textContent).toBe('2 / 3');
    expect(answerInput().value).toBe('');
    expect(workInput().value).toBe('');

    typeAnswer('1');
    await act(async () => { fireEvent.click(submitButton()); });

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

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: "I've got it — practice ▸" }));
    });

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

  it('a failed teach falls through to practice instead of stranding the lesson', async () => {
    const taskTeach = vi.fn<ApiClient['taskTeach']>(async () => { throw new Error('the teach route is down'); });
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    await mount({ plan: planOf(LESSON), api: stubApi({ taskTeach, taskServe }) });

    await waitFor(() => expect(taskServe).toHaveBeenCalledTimes(1));
    expect(answerInput()).toBeTruthy();
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

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Continue studying' }));
    });
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
    const sessionEnd = vi.fn<ApiClient['sessionEnd']>(async () => ({
      session: 's-1',
      xp_earned: 10,
      minutes: 4,
      xp: { total: 350, today: 22, goal: 40, streak_days: 3 },
      anki: { pending: 0 },
    }));
    const { onExit } = await mount({ api: stubApi({ sessionEnd }) });

    fireEvent.click(screen.getByRole('button', { name: 'Exit' }));

    expect(onExit).toHaveBeenCalledTimes(1);
    expect(sessionEnd).not.toHaveBeenCalled();
  });

  it('End session closes with no minutes argument and shows the summary', async () => {
    const sessionEnd = vi.fn<ApiClient['sessionEnd']>(async () => ({
      session: 's-1',
      xp_earned: 12,
      minutes: 8,
      xp: { total: 352, today: 24, goal: 40, streak_days: 3 },
      anki: { pending: 0 },
    }));
    const { onExit } = await mount({
      api: stubApi({ sessionEnd, taskAnswer: async () => graded({ next: null }) }),
    });

    typeAnswer('3/4');
    await act(async () => { fireEvent.click(submitButton()); });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'End session' }));
    });

    // NO ARGUMENTS: the service measures the session itself and that value prices the XP.
    expect(sessionEnd).toHaveBeenCalledWith();
    expect(screen.getByText('Session complete')).toBeTruthy();
    expect(screen.getByText(signed(12))).toBeTruthy();
    expect(screen.getByText('8')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Back to dashboard' }));
    expect(onExit).toHaveBeenCalledTimes(1);
  });

  it('the last answer of the last task closes the session', async () => {
    const sessionEnd = vi.fn<ApiClient['sessionEnd']>(async () => ({
      session: 's-1',
      xp_earned: 10,
      minutes: 5,
      xp: { total: 350, today: 22, goal: 40, streak_days: 3 },
      anki: { pending: 0 },
    }));
    await mount({
      api: stubApi({
        sessionEnd,
        taskAnswer: async () => graded({ next: null, task_status: 'task_passed' }),
      }),
    });

    typeAnswer('3/4');
    await act(async () => { fireEvent.click(submitButton()); });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Continue →' }));
    });

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

    typeAnswer('3/4');
    await act(async () => { fireEvent.click(submitButton()); });
    expect(screen.getByText('review: fractions')).toBeTruthy();

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Continue →' }));
    });

    expect(getPlan).toHaveBeenCalledTimes(1);
    expect(taskServe).toHaveBeenCalledTimes(2);
    expect(taskServe.mock.calls[1]).toEqual(['t-remedial']);
  });
});

describe('the drill countdown', () => {
  it('counts down only for a drill with a budget', () => {
    expect(isDrill(DRILL, P(1, { countdown: true, time_budget_secs: 30 }))).toBe(true);
    expect(isDrill(DRILL, P(1, { countdown: false, time_budget_secs: 30 }))).toBe(false);
    expect(isDrill(DRILL, P(1, { countdown: true, time_budget_secs: 0 }))).toBe(false);
    expect(isDrill(REVIEW, P(1, { countdown: true, time_budget_secs: 30 }))).toBe(false);
  });

  it('auto-submits once at zero, through the same gate', async () => {
    vi.useFakeTimers();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => graded({ next: null, correct: false }));
    await mount({
      plan: planOf(DRILL),
      api: stubApi({
        taskAnswer,
        taskServe: async () => P(1, { countdown: true, time_budget_secs: 3 }),
      }),
    });

    expect(document.querySelector('.timer')!.textContent).toBe(fmtClock(3));
    // Three seconds, one tick each. The clock turns urgent at three and posts at zero.
    expect(document.querySelector('.timer')!.className).toContain('urgent');
    await act(async () => { vi.advanceTimersByTime(3000); });

    expect(document.querySelector('.timer')!.textContent).toBe('0:00');
    expect(taskAnswer).toHaveBeenCalledTimes(1);
    // A blank answer is an honest miss, not a skipped problem.
    expect(taskAnswer.mock.calls[0][1]).toEqual({ problem_id: 'p1', answer: '' });

    await act(async () => { vi.advanceTimersByTime(5000); });
    expect(taskAnswer).toHaveBeenCalledTimes(1);
  });
});

describe('accessibility', () => {
  it('the problem card reports no axe violation', async () => {
    const { container } = await mount();
    expect(await axe(container, AXE_IN_JSDOM)).toHaveNoViolations();
  });

  it('the answer field takes the focus on a fresh problem', async () => {
    await mount();
    expect(document.activeElement).toBe(answerInput());
  });

  it('the Continue button takes the focus on the verdict', async () => {
    await mount({ api: stubApi({ taskAnswer: async () => graded({ next: null }) }) });
    typeAnswer('3/4');
    await act(async () => { fireEvent.click(submitButton()); });
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Continue →' }));
  });
});
