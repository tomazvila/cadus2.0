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
 * Every fixture is the frozen contract of `docs/reference/web-service-1.0-spec.md`
 * (`test/helpers/quiz.tsx`), so each assertion is a literal a reader checks by hand: the
 * clock `10:00`, the count `2 remaining`, the posted pairs of `problem_id` and `answer`.
 * This part holds the clock; `quiz.reveal.test.tsx` holds the reveal, the timeout, the
 * gate and the stale Retry.
 */
import { describe, expect, it, vi } from 'vitest';
import { cleanup, screen } from '@testing-library/react';
import { QUIZ_TIMEOUT_MESSAGE } from '@/views/Quiz';
import { tick } from './helpers/timers';
import {
  QUIZ, Q, completed, mount, posted, receipt, remaining, stubApi, submitAnswer, timer, toasts,
} from './helpers/quiz';
import type { ApiClient, PlanTask } from '@/api/types';

/** A grade that closes the quiz, on a fake clock. */
function completingGrade() {
  vi.useFakeTimers();
  return vi.fn<ApiClient['taskAnswer']>(async () => completed());
}

/** What a quiz whose clock was already out at mount did on its first tick. */
async function blankFillAtOnce(taskAnswer: ReturnType<typeof completingGrade>) {
  await tick(0);
  return {
    pairs: posted(taskAnswer),
    complete: screen.queryByText('Quiz complete') !== null,
    timedOut: toasts().map((t) => t.message).includes(QUIZ_TIMEOUT_MESSAGE),
  };
}

/** The blank fill posted `q1` empty, closed the quiz, and said the time was up. */
const BLANK_FILLED = { pairs: [['q1', '']], complete: true, timedOut: true };

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
    await submitAnswer('7/12');
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
    const taskAnswer = completingGrade();
    const api = stubApi({ taskAnswer });
    const short: PlanTask = { ...QUIZ, time_budget_secs: 5 };

    await mount({ task: short, api });
    cleanup();
    // The clock dies with the view (F-37-1b), so nothing is posted while the quiz is off.
    await tick(10_000);
    expect(taskAnswer).not.toHaveBeenCalled();

    await mount({ task: short, api });
    expect(await blankFillAtOnce(taskAnswer)).toEqual(BLANK_FILLED);
  });

  it('QUIZ-budget: a page reload resumes the clock from the server count', async () => {
    // The V6 residual. A reload builds a NEW api client, so the module-scope registry of
    // FIX2-M6-D is empty and the clock it held is gone. The serve carries the seconds the
    // WHOLE quiz has run (`quiz_elapsed_secs`, `crates/web/src/serve.rs`), and the mount
    // prefers it: 120 of the 600 seconds are spent, so the reloaded screen reads 8:00.
    // Without the server count the reload reads 10:00 and the timed quiz has no end.
    vi.useFakeTimers();
    // A client this test never used before, exactly as a reload builds one.
    const api = stubApi({ taskServe: async () => Q(2, { quiz_elapsed_secs: 120 }) });
    await mount({ api });

    expect(timer()!.textContent).toBe('8:00');
    await tick(1000);
    expect(timer()!.textContent).toBe('7:59');
    // The count on screen is server state too: question 2 of 3 means one answer is in.
    expect(remaining()!.textContent).toBe('2 remaining');
  });

  it('QUIZ-budget: a reload after the budget ran out blank-fills at once', async () => {
    // The learner reloads a quiz whose clock ran out while the tab was closed. The resumed
    // clock is at zero on the first render, so the timeout path runs on the spot instead of
    // handing out a second budget the learner never had.
    const taskAnswer = completingGrade();
    // 600 seconds of budget and 900 seconds gone.
    const api = stubApi({ taskServe: async () => Q(1, { quiz_elapsed_secs: 900 }), taskAnswer });
    await mount({ api });
    expect(await blankFillAtOnce(taskAnswer)).toEqual(BLANK_FILLED);
  });

  it('QUIZ-budget: a serve that carries no count keeps the client registry', async () => {
    // The fallback stays exactly as FIX2-M6-D left it. A serve with no `quiz_elapsed_secs`
    // — an open quiz no serve has stamped — resumes from the registry across a re-mount,
    // and 0 is NOT the reading of an absent count.
    vi.useFakeTimers();
    const api = stubApi({ taskServe: async () => Q(1) });

    await mount({ api });
    await tick(30_000);
    expect(timer()!.textContent).toBe('9:30');

    cleanup();
    await mount({ api });
    expect(timer()!.textContent).toBe('9:30');
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
