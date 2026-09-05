/**
 * The fixtures and the moves of the study-loop tests.
 *
 * Every fixture is the frozen contract of `docs/reference/web-service-1.0-spec.md`, so
 * each assertion in the parts is a literal a reader checks by hand: three problems in the
 * task, the progress count `1 / 3`, the clock `0:00`, the auto-advance at 1400 ms.
 */
import { StrictMode } from 'react';
import { vi } from 'vitest';
import { act, fireEvent, screen } from '@testing-library/react';
import { createDemoApi } from '@/api';
import { Session, type SessionProps } from '@/views/session/Session';
import { resetToasts, toastStore } from '@/app/toast';
import { held } from './held';
import { renderInView } from './render';
import type {
  AnswerResponse,
  ApiClient,
  PlanTask,
  ReworkResponse,
  ServedProblem,
  SessionEndResponse,
  SessionPlanResponse,
  TeachResponse,
} from '@/api/types';

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const TOPIC = { id: 'fractions', name: 'Fractions', module: 'Arithmetic' };

export const REVIEW: PlanTask = {
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

export const LESSON: PlanTask = { ...REVIEW, task_id: 't-lesson', task_type: 'lesson', why: null };

export const DRILL: PlanTask = { ...REVIEW, task_id: 't-drill', task_type: 'drill', why: null };

/** The `n`-th problem of a three-problem task. */
export const P = (n: number, over: Partial<ServedProblem> = {}): ServedProblem => ({
  problem_id: `p${n}`,
  index: n,
  total: 3,
  text: `Simplify $\\frac{${n}}{2}$.`,
  kp: 'kp-simplify',
  time_budget_secs: 60,
  countdown: false,
  ...over,
});

export const planOf = (...tasks: PlanTask[]): SessionPlanResponse => ({
  session: 's-1',
  tasks,
  quiz_due: false,
  constraints: { lesson_ratio_ok: true, lesson_ratio: 0.5, throttle_ok: true, reviews: 1, lessons: 0 },
  course_complete: false,
  frontier_blocked_until: null,
});

export const graded = (over: Partial<AnswerResponse> = {}): AnswerResponse => ({
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
export const REWORK: ReworkResponse = {
  rework_required: true,
  problem_id: 'p1',
  solution: 'Divide both parts by 2.',
  expected: '17/23',
  re_solve: 'Study the solution, then solve it again with no help.',
};

export const TEACHING: TeachResponse = {
  kp: 'kp-simplify',
  concept: 'A fraction names the same number when both parts divide by the same factor.',
  worked_example: { problem: 'Simplify $\\frac{4}{6}$.', steps: 'Both parts divide by 2.' },
};

/** The close receipt. `xp_earned` and `minutes` are the two numbers the summary paints. */
export const closed = (over: Partial<SessionEndResponse> = {}): SessionEndResponse => ({
  session: 's-1',
  xp_earned: 10,
  minutes: 5,
  xp: { total: 350, today: 22, goal: 40, streak_days: 3 },
  anki: { pending: 0 },
  ...over,
});

/**
 * A client built on the demo backend, so every method exists and a missing override is a
 * type error rather than a `not a function` inside a handler (F-F6-1).
 */
export function stubApi(over: Partial<ApiClient> = {}): ApiClient {
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
export type MountOver =
  Partial<Omit<SessionProps, 'plan'>> & { plan?: SessionPlanResponse | undefined };

export async function mount(over: MountOver = {}) {
  resetToasts();
  const handlers = nav();
  const props: SessionProps = {
    api: stubApi(),
    ...handlers,
    ...(over as Partial<Omit<SessionProps, 'plan'>>),
  };
  const plan = 'plan' in over ? over.plan : planOf(REVIEW);
  if (plan) props.plan = plan;
  const view = await renderInView(<Session {...props} />);
  return { ...view, ...handlers };
}

/** A drill whose one problem counts down from three seconds. Fake timers are on. */
/** Mount inside StrictMode, with the plan given and the client given. */
export async function mountStrict(api: ApiClient, plan?: SessionPlanResponse) {
  const handlers = nav();
  const props: SessionProps = { api, ...handlers };
  if (plan) props.plan = plan;
  return renderInView(
    <StrictMode>
      <Session {...props} />
    </StrictMode>,
  );
}

/**
 * Answer the one problem with a remediation, press on, and leave while the fresh plan is
 * still out. Gives back the held plan, for the test to release after the view left.
 */
export async function leaveDuringReplan(over: Partial<ApiClient> = {}) {
  const plan = held<SessionPlanResponse>();
  const remediated = graded({
    next: null,
    task_status: 'task_passed',
    remediation: [{ kind: 'review', targets: ['fractions'] }],
  });
  const view = await mount({
    api: stubApi({ getPlan: () => plan.promise, taskAnswer: async () => remediated, ...over }),
  });
  await submitAnswer('3/4');
  await press('Continue →');
  view.unmount();
  return plan;
}

export async function mountDrill(taskAnswer: ApiClient['taskAnswer']) {
  vi.useFakeTimers();
  return mount({
    plan: planOf(DRILL),
    api: stubApi({
      taskAnswer,
      taskServe: async () => P(1, { countdown: true, time_budget_secs: 3 }),
    }),
  });
}

// ---------------------------------------------------------------------------
// The moves
// ---------------------------------------------------------------------------

export const answerInput = () => screen.getByLabelText('Answer') as HTMLInputElement;
export const workInput = () => screen.getByLabelText('Working') as HTMLTextAreaElement;
export const submitButton = () => screen.getByRole('button', { name: 'Submit' });
export const progressCount = () => document.querySelector('.progress-count')!.textContent;
export const timer = () => document.querySelector('.timer')!;
export const toasts = () => toastStore.getSnapshot();

/** Type into the uncontrolled field the way a keyboard does: value first, then the event. */
export function typeAnswer(text: string): void {
  fireEvent.change(answerInput(), { target: { value: text } });
}

/** Press one named button and let its continuations settle. */
export async function press(name: string | RegExp): Promise<void> {
  await act(async () => { fireEvent.click(screen.getByRole('button', { name })); });
}

/** Type an answer and press Submit. */
export async function submitAnswer(text: string): Promise<void> {
  typeAnswer(text);
  await act(async () => { fireEvent.click(submitButton()); });
}

/** Submit, then move the fake clock. */
export async function submitThenWait(text: string, ms: number): Promise<void> {
  await submitAnswer(text);
  await act(async () => { vi.advanceTimersByTime(ms); });
}

/** Press "Next problem →" and read the count that follows. */
export async function clickNext(): Promise<string | null> {
  await press('Next problem →');
  return progressCount();
}
