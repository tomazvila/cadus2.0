/**
 * The timed quiz (S10).
 *
 * Three invariants live here, and each one shipped as a defect in 1.0:
 *
 *   QUIZ-budget   The clock is the plan task's WHOLE-quiz budget. The per-question serve
 *                 value is one topic's raw expected time; as the whole-quiz clock it
 *                 expired mid-quiz and blank-submitted the rest.
 *   QUIZ-reveal   Nothing about correctness before the last answer.
 *   QUIZ-timeout  A timeout SKIPS the question already in flight. A second post of the
 *                 same `problem_id` writes a second attempt to an append-only log and
 *                 gives the loser `404 unknown_problem`.
 *
 * Every fixture is the frozen contract of `docs/reference/web-service-1.0-spec.md`, so
 * each assertion is a literal a reader checks by hand: the clock `10:00`, the count
 * `2 remaining`, the posted pairs of `problem_id` and `answer`.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { createDemoApi } from '@/api';
import { Quiz, QUIZ_SILENCE_NOTE, QUIZ_TIMEOUT_MESSAGE, type QuizProps } from '@/views/Quiz';
import { resetToasts, toastStore } from '@/app/toast';
import { AXE_IN_JSDOM } from './axe';
import type {
  ApiClient,
  PlanTask,
  QuizReceiptResponse,
  ServedProblem,
  TaskAnswerResponse,
} from '@/api/types';

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** The plan task. `topic` is null: a quiz mixes several topics. */
const QUIZ: PlanTask = {
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
const Q = (n: number, over: Partial<ServedProblem> = {}): ServedProblem => ({
  problem_id: `q${n}`,
  index: n,
  total: 3,
  text: `Question ${n}.`,
  kp: null,
  time_budget_secs: 90,
  countdown: false,
  ...over,
});

const receipt = (over: Partial<QuizReceiptResponse> = {}): QuizReceiptResponse => ({
  accepted: true,
  remaining: 2,
  quiz_complete: false,
  ...over,
});

/**
 * A client on the demo backend, so every method exists and a missing override is a type
 * error rather than a `not a function` inside a handler (F-F6-1).
 */
function stubApi(over: Partial<ApiClient> = {}): ApiClient {
  return {
    ...createDemoApi(),
    taskServe: async () => Q(1),
    taskAnswer: async () => receipt(),
    ...over,
  };
}

async function mount(over: Partial<QuizProps> = {}) {
  resetToasts();
  const handlers = { onUnauthorized: vi.fn(), onDone: vi.fn() };
  const props: QuizProps = { api: stubApi(), task: QUIZ, ...handlers, ...over };
  let view!: ReturnType<typeof render>;
  await act(async () => {
    view = render(<Quiz {...props} />, { container: document.getElementById('view')! });
  });
  return { ...view, ...handlers };
}

const answerInput = () => screen.getByLabelText('Answer') as HTMLInputElement;
const submitButton = () => screen.getByRole('button', { name: 'Submit answer' });
const timer = () => document.querySelector('.timer');
const remaining = () => document.querySelector('.remaining');

function typeAnswer(text: string): void {
  fireEvent.change(answerInput(), { target: { value: text } });
}

/** Move the clock and let every continuation the move released settle. */
async function tick(ms: number): Promise<void> {
  await act(async () => { await vi.advanceTimersByTimeAsync(ms); });
}

/** The `[problem_id, answer]` pairs the quiz posted, in order. */
const posted = (fn: { mock: { calls: unknown[][] } }): [string, string][] =>
  fn.mock.calls.map((c) => {
    const body = c[1] as { problem_id: string; answer: string };
    return [body.problem_id, body.answer];
  });

// ---------------------------------------------------------------------------

describe('QUIZ-budget: the whole-quiz clock', () => {
  it('QUIZ-budget: reads the task budget, never the per-question serve value', async () => {
    // 600 s for the whole quiz against 90 s for one question. The 1.0 defect read 90.
    // The fake clock goes in BEFORE the mount: the interval is armed in a mount effect,
    // and a real interval armed first does not answer to `advanceTimersByTime`.
    vi.useFakeTimers();
    await mount({ api: stubApi({ taskServe: async () => Q(1, { time_budget_secs: 90 }) }) });

    expect(timer()!.textContent).toBe('10:00');
    await tick(1000);
    expect(timer()!.textContent).toBe('9:59');
  });

  it('QUIZ-budget: falls back to the serve value when the task carries none', async () => {
    await mount({
      task: { ...QUIZ, time_budget_secs: null },
      api: stubApi({ taskServe: async () => Q(1, { time_budget_secs: 90 }) }),
    });

    expect(timer()!.textContent).toBe('1:30');
  });

  it('QUIZ-budget: renders no clock at all when neither budget exists', async () => {
    vi.useFakeTimers();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => receipt());
    await mount({
      task: { ...QUIZ, time_budget_secs: null },
      api: stubApi({ taskServe: async () => Q(1, { time_budget_secs: null }), taskAnswer }),
    });

    expect(timer()).toBeNull();
    // With no clock there is no timeout: the quiz waits for the learner, for an hour.
    await tick(3_600_000);
    expect(taskAnswer).not.toHaveBeenCalled();
    expect(screen.queryByText('Quiz complete')).toBeNull();
  });

  it('QUIZ-budget: a re-mount resumes the running clock and keeps the answered count', async () => {
    // V6. The topbar offers the map from the quiz and the map's Done gives the quiz back,
    // so React unmounts the screen and mounts it again. A clock seeded from the budget on
    // every mount hands the whole budget back once per trip, and the count on screen
    // restarts at the full quiz: the timed quiz then has no end.
    vi.useFakeTimers();
    const taskServe = vi.fn<ApiClient['taskServe']>()
      .mockResolvedValueOnce(Q(1))
      .mockResolvedValue(Q(2));
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => receipt({ remaining: 2 }));
    // ONE client for both mounts, as the router holds one for the whole page.
    const api = stubApi({ taskServe, taskAnswer });

    await mount({ api });
    typeAnswer('7/12');
    await act(async () => { fireEvent.click(submitButton()); });
    await tick(30_000);
    expect(timer()!.textContent).toBe('9:30');
    expect(remaining()!.textContent).toBe('2 remaining');

    // The map takes the screen: React unmounts the quiz, and Done mounts it again.
    cleanup();
    await mount({ api });

    // 30 seconds of the 600 are spent, and the serve numbers the live question 2 of 3, so
    // one answer is in: `10:00` and `3 remaining` here are the map round trip as a reset.
    expect(timer()!.textContent).toBe('9:30');
    expect(remaining()!.textContent).toBe('2 remaining');
    expect(taskServe).toHaveBeenCalledTimes(3);
  });

  it('QUIZ-budget: a re-mount after the budget ran out blank-fills at once', async () => {
    // The screen was away while the clock ran out. The resumed clock is at zero on the
    // first render, so the timeout path runs on the spot instead of waiting out a second
    // budget the learner never had.
    vi.useFakeTimers();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(
      async () => receipt({ remaining: 0, quiz_complete: true }),
    );
    const api = stubApi({ taskAnswer });
    const short: PlanTask = { ...QUIZ, time_budget_secs: 5 };

    await mount({ task: short, api });
    cleanup();
    // The clock dies with the view (F-37-1b), so nothing is posted while the quiz is off.
    await tick(10_000);
    expect(taskAnswer).not.toHaveBeenCalled();

    await mount({ task: short, api });
    await tick(0);

    expect(posted(taskAnswer)).toEqual([['q1', '']]);
    expect(screen.getByText('Quiz complete')).toBeTruthy();
    expect(toastStore.getSnapshot().map((t) => t.message)).toContain(QUIZ_TIMEOUT_MESSAGE);
  });

  it('QUIZ-budget: a clock the browser froze gives no frozen second back', async () => {
    // A hidden tab throttles the interval to about one tick a minute. A clock that counts
    // ticks hands every skipped second back, so the whole-quiz budget stretches for as long
    // as the learner keeps the tab in the background.
    vi.useFakeTimers();
    await mount();
    expect(timer()!.textContent).toBe('10:00');

    // Two minutes pass with the tab hidden, and the interval fires ONCE at the end of them.
    vi.setSystemTime(Date.now() + 120_000);
    await tick(1000);

    // 121 seconds of the 600 are gone. A tick count says 9:59.
    expect(timer()!.textContent).toBe('7:59');
  });

  it('turns the clock urgent in the last minute, at 60 seconds left', async () => {
    vi.useFakeTimers();
    await mount({ task: { ...QUIZ, time_budget_secs: 65 } });

    expect(timer()!.className).toBe('timer');
    await tick(4000);
    expect(timer()!.className).toBe('timer');
    await tick(1000);
    expect(timer()!.className).toBe('timer urgent');
  });
});

describe('QUIZ-reveal: silence until the last answer', () => {
  it('QUIZ-reveal: an accepted answer shows a count and nothing about correctness', async () => {
    // The reply carries `correct` — a service that grew the field, or a defect. Rendering
    // it defeats the batch reveal, so the screen renders none of it.
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(
      async () => ({ ...receipt(), correct: true, solution: 'Divide by two.' }) as unknown as TaskAnswerResponse,
    );
    await mount({
      api: stubApi({
        taskServe: vi.fn<ApiClient['taskServe']>()
          .mockResolvedValueOnce(Q(1))
          .mockResolvedValue(Q(2)),
        taskAnswer,
      }),
    });

    typeAnswer('7/12');
    await act(async () => { fireEvent.click(submitButton()); });

    expect(remaining()!.textContent).toBe('2 remaining');
    expect(screen.getByText(QUIZ_SILENCE_NOTE)).toBeTruthy();
    expect(QUIZ_SILENCE_NOTE).toBe('No feedback until the end.');
    expect(document.querySelector('.feedback')).toBeNull();
    expect(document.querySelector('.solution')).toBeNull();
    expect(document.body.textContent).not.toContain('Divide by two.');
    expect(document.body.textContent!.toLowerCase()).not.toContain('correct');
  });

  it('QUIZ-reveal: the next question arrives with an empty field and a new node', async () => {
    await mount({
      api: stubApi({
        taskServe: vi.fn<ApiClient['taskServe']>()
          .mockResolvedValueOnce(Q(1, { text: 'First.' }))
          .mockResolvedValue(Q(2, { text: 'Second.' })),
      }),
    });

    const first = answerInput();
    typeAnswer('7/12');
    await act(async () => { fireEvent.click(submitButton()); });

    expect(document.querySelector('.problem-text')!.textContent).toBe('Second.');
    // A DIFFERENT node, which is what `key={problem.problem_id}` guarantees: without it
    // React reuses the input and the previous answer pre-fills the next question.
    expect(answerInput()).not.toBe(first);
    expect(answerInput().value).toBe('');
    expect(document.querySelector('.progress-count')!.textContent).toBe('2 / 3');
  });

  it('QUIZ-reveal: the end screen states the count and invents no score', async () => {
    const { onDone } = await mount({
      api: stubApi({ taskAnswer: async () => receipt({ remaining: 0, quiz_complete: true }) }),
    });

    typeAnswer('7/12');
    await act(async () => { fireEvent.click(submitButton()); });

    expect(screen.getByText('Quiz complete')).toBeTruthy();
    expect(screen.getByText('All 3 answers are recorded.')).toBeTruthy();
    // M5 mounts no close route, so there is no score and no per-question breakdown to
    // show. A number invented here would be a lie about a recorded attempt.
    expect(document.querySelector('.score-big')).toBeNull();
    expect(document.body.textContent).not.toContain('%');
    fireEvent.click(screen.getByRole('button', { name: 'Back to dashboard' }));
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it('QUIZ-reveal: a grade reply that is not a receipt ends the quiz revealing nothing', async () => {
    // A verdict payload on a quiz route is a service defect. The honest response is to end
    // the quiz, never to paint a verdict this screen is not allowed to show.
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(
      async () => ({ correct: false, solution: 'Divide by two.', error_tags: ['sign-error'] }) as unknown as TaskAnswerResponse,
    );
    await mount({ api: stubApi({ taskAnswer }) });

    typeAnswer('7/12');
    await act(async () => { fireEvent.click(submitButton()); });

    expect(screen.getByText('Quiz complete')).toBeTruthy();
    expect(document.body.textContent).not.toContain('Divide by two.');
    expect(document.body.textContent).not.toContain('sign-error');
  });

  it('names the way back to the session when the quiz was entered from one', async () => {
    await mount({
      fromSession: true,
      api: stubApi({ taskAnswer: async () => receipt({ remaining: 0, quiz_complete: true }) }),
    });

    typeAnswer('7/12');
    await act(async () => { fireEvent.click(submitButton()); });

    expect(screen.getByRole('button', { name: 'Continue session' })).toBeTruthy();
  });
});

describe('QUIZ-timeout: the clock runs out', () => {
  it('QUIZ-timeout: blank-fills the questions that are left and ends the quiz', async () => {
    vi.useFakeTimers();
    const taskServe = vi.fn<ApiClient['taskServe']>()
      .mockResolvedValueOnce(Q(1))
      .mockResolvedValueOnce(Q(2))
      .mockResolvedValue(Q(3));
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>()
      .mockResolvedValueOnce(receipt({ remaining: 2 }))
      .mockResolvedValueOnce(receipt({ remaining: 1 }))
      .mockResolvedValue(receipt({ remaining: 0, quiz_complete: true }));
    await mount({ task: { ...QUIZ, time_budget_secs: 5 }, api: stubApi({ taskServe, taskAnswer }) });

    await tick(5000);

    expect(posted(taskAnswer)).toEqual([['q1', ''], ['q2', ''], ['q3', '']]);
    expect(screen.getByText('Quiz complete')).toBeTruthy();
    expect(toastStore.getSnapshot().map((t) => t.message)).toContain(QUIZ_TIMEOUT_MESSAGE);
    expect(QUIZ_TIMEOUT_MESSAGE).toBe('Time’s up — grading your answers.');
  });

  it('QUIZ-timeout: never re-posts a problem_id already in flight', async () => {
    vi.useFakeTimers();
    let release!: (value: TaskAnswerResponse) => void;
    const held = new Promise<TaskAnswerResponse>((r) => { release = r; });
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(() => held);
    await mount({ task: { ...QUIZ, time_budget_secs: 3 }, api: stubApi({ taskAnswer }) });

    typeAnswer('7');
    fireEvent.click(submitButton());
    expect(taskAnswer).toHaveBeenCalledTimes(1);

    // The clock expires while that answer is in flight. A second post of `q1` writes a
    // second attempt to an append-only log and 404s the loser.
    await tick(3000);
    expect(taskAnswer).toHaveBeenCalledTimes(1);
    expect(posted(taskAnswer)).toEqual([['q1', '7']]);

    await act(async () => { release(receipt({ remaining: 0, quiz_complete: true })); });
    expect(screen.getByText('Quiz complete')).toBeTruthy();
  });

  it('QUIZ-timeout: the skipped question resumes the blank fill when its answer lands', async () => {
    vi.useFakeTimers();
    let release!: (value: TaskAnswerResponse) => void;
    const held = new Promise<TaskAnswerResponse>((r) => { release = r; });
    const taskServe = vi.fn<ApiClient['taskServe']>()
      .mockResolvedValueOnce(Q(1))
      .mockResolvedValueOnce(Q(2))
      .mockResolvedValue(Q(3));
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>()
      .mockReturnValueOnce(held)
      .mockResolvedValueOnce(receipt({ remaining: 1 }))
      .mockResolvedValue(receipt({ remaining: 0, quiz_complete: true }));
    await mount({ task: { ...QUIZ, time_budget_secs: 3 }, api: stubApi({ taskServe, taskAnswer }) });

    typeAnswer('7');
    fireEvent.click(submitButton());
    await tick(3000);
    expect(taskAnswer).toHaveBeenCalledTimes(1);

    // The in-flight continuation resumes the fill as soon as the next question is served,
    // so the quiz still closes with every question answered exactly once.
    await act(async () => { release(receipt({ remaining: 2 })); });

    expect(posted(taskAnswer)).toEqual([['q1', '7'], ['q2', ''], ['q3', '']]);
    expect(screen.getByText('Quiz complete')).toBeTruthy();
  });

  it('QUIZ-timeout: fires once, however long the clock runs past zero', async () => {
    vi.useFakeTimers();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(
      async () => receipt({ remaining: 0, quiz_complete: true }),
    );
    await mount({ task: { ...QUIZ, time_budget_secs: 2 }, api: stubApi({ taskAnswer }) });

    await tick(60_000);

    // One post, not one per second past zero. The interval keeps running; `timedOutRef`
    // is what makes the timeout a single event.
    expect(taskAnswer).toHaveBeenCalledTimes(1);
  });
});

describe('the phase gate and the view lifetime', () => {
  it('F-37-1c: a second submit inside the grading window posts nothing', async () => {
    let release!: (value: TaskAnswerResponse) => void;
    const held = new Promise<TaskAnswerResponse>((r) => { release = r; });
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(() => held);
    await mount({ api: stubApi({ taskAnswer }) });

    typeAnswer('7/12');
    fireEvent.keyDown(answerInput(), { key: 'Enter' });
    expect(taskAnswer).toHaveBeenCalledTimes(1);

    // The field stays enabled while the grade runs: the gate, not the attribute, is what
    // stops the second post. Two more Enters and a click land inside the window.
    fireEvent.keyDown(answerInput(), { key: 'Enter' });
    fireEvent.keyDown(answerInput(), { key: 'Enter' });
    fireEvent.click(submitButton());
    expect(taskAnswer).toHaveBeenCalledTimes(1);

    await act(async () => { release(receipt({ remaining: 0, quiz_complete: true })); });
    expect(taskAnswer).toHaveBeenCalledTimes(1);
  });

  it('F-37-1b: the clock dies with the view and blank-fills nothing after it', async () => {
    vi.useFakeTimers();
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => receipt());
    const { unmount } = await mount({
      task: { ...QUIZ, time_budget_secs: 5 },
      api: stubApi({ taskAnswer }),
    });

    unmount();
    await tick(60_000);

    expect(taskAnswer).not.toHaveBeenCalled();
  });

  it('NO-2BILL: one mount serves exactly one question', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => Q(1));
    await mount({ api: stubApi({ taskServe }) });

    expect(taskServe).toHaveBeenCalledTimes(1);
    expect(taskServe).toHaveBeenCalledWith('t-quiz');
  });

  it('refuses a blank submit without posting, and returns the focus to the field', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => receipt());
    await mount({ api: stubApi({ taskAnswer }) });

    await act(async () => { fireEvent.click(submitButton()); });

    expect(taskAnswer).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(answerInput());
    expect(submitButton().hasAttribute('disabled')).toBe(false);
  });

  it('a failed grade returns the question to the learner instead of locking the card', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>(async () => {
      throw new Error('the network went away');
    });
    await mount({ api: stubApi({ taskAnswer }) });

    typeAnswer('7/12');
    await act(async () => { fireEvent.click(submitButton()); });

    expect(submitButton().hasAttribute('disabled')).toBe(false);
    typeAnswer('7/12');
    await act(async () => { fireEvent.click(submitButton()); });
    expect(taskAnswer).toHaveBeenCalledTimes(2);
  });
});

describe('accessibility', () => {
  it('the quiz card reports no axe violation', async () => {
    const { container } = await mount();
    expect(await axe(container, AXE_IN_JSDOM)).toHaveNoViolations();
  });

  it('the answer field takes the focus on a fresh question', async () => {
    await mount();
    expect(document.activeElement).toBe(answerInput());
  });

  it('the end screen moves the focus to the way out', async () => {
    await mount({ api: stubApi({ taskAnswer: async () => receipt({ remaining: 0, quiz_complete: true }) }) });

    typeAnswer('7/12');
    await act(async () => { fireEvent.click(submitButton()); });

    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Back to dashboard' }));
  });

  it('offers no hint control: a hint inside a quiz is 409 no_hints_in_quiz', async () => {
    await mount();
    expect(screen.queryByRole('button', { name: 'Hint' })).toBeNull();
  });
});
