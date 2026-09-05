/**
 * The fixtures and the moves of the timed-quiz tests.
 *
 * Every fixture is the frozen contract of `docs/reference/web-service-1.0-spec.md`, so
 * each assertion in the parts is a literal a reader checks by hand: the clock `10:00`, the
 * count `2 remaining`, the posted pairs of `problem_id` and `answer`.
 */
import { vi, type Mock } from 'vitest';
import { act, fireEvent, screen } from '@testing-library/react';
import { createDemoApi } from '@/api';
import { Quiz, type QuizProps } from '@/views/Quiz';
import { resetToasts, toastStore } from '@/app/toast';
import { renderInView } from './render';
import type { ApiClient, PlanTask, QuizReceiptResponse, ServedProblem } from '@/api/types';

/** The plan task. `topic` is null: a quiz mixes several topics. */
export const QUIZ: PlanTask = {
  task_id: 't-quiz',
  task_type: 'quiz',
  topic: null,
  kp: null,
  start_at_kp: null,
  n_problems: 3,
  mix: ['fractions', 'integers'],
  component_topics: null,
  time_budget_secs: 600,
  difficulty_target: 0.6,
  why: null,
  progress: { answered: 0, done: false },
};

/** The `n`-th question of a three-question quiz. 90 s is ONE topic's expected time. */
export const Q = (n: number, over: Partial<ServedProblem> = {}): ServedProblem => ({
  problem_id: `q${n}`,
  index: n,
  total: 3,
  text: `Question ${n}.`,
  kp: null,
  time_budget_secs: 90,
  countdown: false,
  ...over,
});

export const receipt = (over: Partial<QuizReceiptResponse> = {}): QuizReceiptResponse => ({
  accepted: true,
  remaining: 2,
  quiz_complete: false,
  ...over,
});

/** The receipt of the last answer: the quiz closed with it. */
export const completed = () => receipt({ remaining: 0, quiz_complete: true });

/** A serve that numbers the three questions in order, and repeats the last one. */
export function threeQuestions(): Mock<ApiClient['taskServe']> {
  return vi.fn<ApiClient['taskServe']>()
    .mockResolvedValueOnce(Q(1))
    .mockResolvedValueOnce(Q(2))
    .mockResolvedValue(Q(3));
}

/**
 * A client on the demo backend, so every method exists and a missing override is a type
 * error rather than a `not a function` inside a handler (F-F6-1).
 */
export function stubApi(over: Partial<ApiClient> = {}): ApiClient {
  return {
    ...createDemoApi(),
    taskServe: async () => Q(1),
    taskAnswer: async () => receipt(),
    ...over,
  };
}

export async function mount(over: Partial<QuizProps> = {}) {
  resetToasts();
  const handlers = { onUnauthorized: vi.fn(), onDone: vi.fn() };
  const props: QuizProps = { api: stubApi(), task: QUIZ, demo: false, ...handlers, ...over };
  const view = await renderInView(<Quiz {...props} />);
  return { ...view, ...handlers };
}

export const answerInput = () => screen.getByLabelText('Answer') as HTMLInputElement;
export const submitButton = () => screen.getByRole('button', { name: 'Submit answer' });
export const timer = () => document.querySelector('.timer');
export const remaining = () => document.querySelector('.remaining');
export const toasts = () => toastStore.getSnapshot();

export function typeAnswer(text: string): void {
  fireEvent.change(answerInput(), { target: { value: text } });
}

/** Press Submit and let the grade settle. */
export async function pressSubmit(): Promise<void> {
  await act(async () => { fireEvent.click(submitButton()); });
}

/** Type an answer and press Submit. */
export async function submitAnswer(text: string): Promise<void> {
  typeAnswer(text);
  await pressSubmit();
}

/** The `[problem_id, answer]` pairs the quiz posted, in order. */
export const posted = (fn: Mock<ApiClient['taskAnswer']>): [string, string][] =>
  fn.mock.calls.map(([, body]) => [body.problem_id, body.answer]);
