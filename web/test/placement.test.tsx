/**
 * The placement diagnostic (S10).
 *
 * Placement is the most leveraged input in the system. Every probe answer appends a row to
 * the `events` table, and `cadus_app` holds no UPDATE and no DELETE on it, so a defect here
 * places the learner at the wrong frontier with no undo. Four invariants live here:
 *
 *   P3          Three ground rules BEFORE probe 1, and an honest-skip control beside
 *               Submit. `diagStart` waits for the Begin button, so reading the rules costs
 *               no probe time.
 *   R15         The intro focuses the CARD, never the CTA.
 *   DIAG-750    The 750 ms post-answer beat lives in the view lifetime, so "Save & exit"
 *               inside the window commits nothing.
 *   DIAG-nosol  Placement feedback is a tick or a cross and one word. Nothing else.
 *
 * The literals come from the 1.0 view and spec section 6 (`test/helpers/placement.tsx`):
 * the beat is 750 ms, the cap default is 40, and the progress line reads
 * `Question 1 of up to 40`. This part holds the four invariants; `placement.loop.test.tsx`
 * holds the probe loop, the commit, the demo port and accessibility.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { Diagnostic, DIAG_BEAT_MS, DIAG_NO_SOLUTIONS_NOTE } from '@/views/Diagnostic';
import { tick } from './helpers/timers';
import {
  START, SUMMARY, answer, answerFirst, answerInput, begin, beginButton, mount, placementProps,
  probe, progressCount, skipButton, stubDiag, submitButton,
} from './helpers/placement';
import type { DiagAnswerResponse, DiagnosticApi } from '@/api/diag';

/**
 * A placement whose port is REBUILT on every render — the default shape of a caller that
 * builds the port in its own render. The test re-renders it by hand.
 */
async function mountRebuilt(parts: Partial<DiagnosticApi>) {
  vi.useFakeTimers();
  const { handlers } = placementProps();
  let view!: ReturnType<typeof render>;
  await act(async () => {
    view = render(<Diagnostic diag={stubDiag(parts)} {...handlers} />, {
      container: document.getElementById('view')!,
    });
  });
  await begin();
  await answer('5');
  return {
    rerender: () => { view.rerender(<Diagnostic diag={stubDiag(parts)} {...handlers} />); },
  };
}

describe('P3 and R15: the ground rules before probe 1', () => {
  it('P3: renders the three ground rules before any probe', async () => {
    await mount();

    const heads = Array.from(document.querySelectorAll('.intro-rules li strong'))
      .map((n) => n.textContent);
    expect(heads).toEqual([
      'Don’t guess — skip instead.',
      'No external resources.',
      'Answer honestly.',
    ]);
    expect(document.querySelectorAll('.intro-rules li')).toHaveLength(3);
    expect(screen.getByText('Before we start')).toBeTruthy();
  });

  it('P3: issues no diagStart before Begin, so reading costs no probe time', async () => {
    // An eager mount effect — the most natural React port of a data view — would put 60 to
    // 120 s of reading into probe 1's `secs`. Placement reads that as slow and places the
    // learner low, silently.
    const diagStart = vi.fn<DiagnosticApi['diagStart']>(async () => START);
    await mount({ diag: stubDiag({ diagStart }) });

    expect(diagStart).not.toHaveBeenCalled();
    expect(document.querySelector('.problem-card')).toBeNull();

    await begin();
    expect(diagStart).toHaveBeenCalledTimes(1);
    expect(document.querySelector('.problem-card')).toBeTruthy();
  });

  it('P3: starts exactly one placement however many times Begin is pressed', async () => {
    const diagStart = vi.fn<DiagnosticApi['diagStart']>(async () => START);
    await mount({ diag: stubDiag({ diagStart }) });

    await act(async () => {
      const button = beginButton();
      fireEvent.click(button);
      fireEvent.click(button);
      fireEvent.click(button);
    });

    expect(diagStart).toHaveBeenCalledTimes(1);
  });

  it('P3: Skip posts an empty answer, which the checker always grades incorrect', async () => {
    const diagAnswer = vi.fn<DiagnosticApi['diagAnswer']>(
      async () => ({ correct: false, next_probe: { done: true } }),
    );
    await mount({ diag: stubDiag({ diagAnswer }) });
    await begin();

    await act(async () => { fireEvent.click(skipButton()); });

    // An honest skip, never a lucky guess: the learner places a little lower instead of
    // over-placing and then being over-challenged.
    expect(diagAnswer).toHaveBeenCalledTimes(1);
    expect(diagAnswer).toHaveBeenCalledWith({ problem_id: 'd1', answer: '' });
    expect(document.querySelector('.feedback-skip .feedback-title')!.textContent).toBe('Skipped');
    expect(skipButton().title).toBe('Records an honest skip (counts as incorrect — no guessing)');
  });

  it('P3: a blank Submit posts nothing and returns the focus to the field', async () => {
    const diagAnswer = vi.fn<DiagnosticApi['diagAnswer']>(
      async () => ({ correct: true, next_probe: { done: true } }),
    );
    await mount({ diag: stubDiag({ diagAnswer }) });
    await begin();

    await act(async () => { fireEvent.click(submitButton()); });

    // The honest way past a probe is Skip, which is recorded. A blank Submit is not.
    expect(diagAnswer).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(answerInput());
  });

  it('P3: leaves the intro without a start when the learner picks Not now', async () => {
    const diagStart = vi.fn<DiagnosticApi['diagStart']>(async () => START);
    const { onExit } = await mount({ diag: stubDiag({ diagStart }) });

    fireEvent.click(screen.getByRole('button', { name: 'Not now' }));

    expect(onExit).toHaveBeenCalledTimes(1);
    expect(diagStart).not.toHaveBeenCalled();
  });

  it('R15: the intro focuses the card, never the Begin button', async () => {
    await mount();

    // A held Enter carried over from the dashboard auto-repeats onto a focused button and
    // skips the rules. A non-interactive container with tabindex -1 swallows that keydown.
    const card = document.querySelector('.intro-card')!;
    expect(card.getAttribute('tabindex')).toBe('-1');
    expect(document.activeElement).toBe(card);
    expect(document.activeElement).not.toBe(beginButton());
  });
});

describe('DIAG-nosol: placement reveals nothing', () => {
  it('DIAG-nosol: renders a tick or a cross only, even when the payload carries a solution', async () => {
    const leaky: DiagAnswerResponse & { solution: string; expected: string } = {
      correct: false,
      solution: 'The answer is 5.',
      expected: '5',
      next_probe: probe({ problem_id: 'd2' }),
    };
    const diagAnswer = vi.fn<DiagnosticApi['diagAnswer']>(async () => leaky);
    await mount({ diag: stubDiag({ diagAnswer }) });
    await begin();
    await answer('9');

    // A revealed answer turns the next probe into a copy exercise and corrupts the
    // plus-minus balance placement is built from. `Feedback` of the session view renders a
    // solution from the SAME class names, so sharing that component would leak it here.
    expect(document.querySelector('.solution')).toBeNull();
    expect(document.querySelector('.solution-text')).toBeNull();
    expect(document.body.textContent).not.toContain('The answer is 5.');
    expect(document.body.textContent).not.toContain('expected');
    expect(document.querySelector('.feedback-incorrect .feedback-title')!.textContent)
      .toBe('Not this time');
    expect(document.querySelector('.feedback')!.textContent).toBe('Not this time');
  });

  it('DIAG-nosol: promises as much on probe 1, and says it only once', async () => {
    const diagAnswer = vi.fn<DiagnosticApi['diagAnswer']>(
      async () => ({ correct: true, next_probe: probe({ problem_id: 'd2', text: 'Second.' }) }),
    );
    vi.useFakeTimers();
    await mount({ diag: stubDiag({ diagAnswer }) });
    await begin();

    expect(screen.getByText(DIAG_NO_SOLUTIONS_NOTE)).toBeTruthy();
    expect(DIAG_NO_SOLUTIONS_NOTE)
      .toBe('No solutions are shown during placement — just answer as best you can.');

    await answer('5');
    await tick(DIAG_BEAT_MS);

    expect(progressCount()).toBe('Question 2 of up to 40');
    expect(screen.queryByText(DIAG_NO_SOLUTIONS_NOTE)).toBeNull();
  });
});

describe('DIAG-750: the post-answer beat', () => {
  it('DIAG-750: leaving inside the beat commits no placement', async () => {
    vi.useFakeTimers();
    const diagFinish = vi.fn<DiagnosticApi['diagFinish']>(async () => SUMMARY);
    const { unmount } = await answerFirst({ diag: stubDiag({ diagFinish }) });

    expect(diagFinish).not.toHaveBeenCalled();

    // "Save & exit" inside the beat. 1.0 registered no teardown at all here, so the
    // timeout outlived the view, ran finish(), and committed placement — flatly against
    // the button the learner had just pressed.
    unmount();
    await tick(5000);

    expect(diagFinish).not.toHaveBeenCalled();
  });

  it('DIAG-750: the beat does commit while the view is alive', async () => {
    // The positive half. Without it the test above passes on a view that never finishes.
    vi.useFakeTimers();
    const diagFinish = vi.fn<DiagnosticApi['diagFinish']>(async () => SUMMARY);
    await answerFirst({ diag: stubDiag({ diagFinish }) });

    await tick(DIAG_BEAT_MS - 1);
    expect(diagFinish).not.toHaveBeenCalled();
    await tick(1);
    expect(diagFinish).toHaveBeenCalledTimes(1);
    expect(DIAG_BEAT_MS).toBe(750);
  });

  it('DIAG-750: a re-render inside the beat arms no second timer', async () => {
    const diagFinish = vi.fn<DiagnosticApi['diagFinish']>(async () => SUMMARY);
    const view = await mountRebuilt({ diagFinish });

    // Two re-renders inside the window, each with a FRESH transport object — the default
    // shape of a caller that builds the port in its own render. With `finish` in the
    // effect's dependency list and no cleanup, each armed another timer, and two
    // concurrent commits both pass the service's "no placement yet" check and write two
    // placement rows to a log with no DELETE.
    await act(async () => {
      view.rerender();
      view.rerender();
    });
    await tick(DIAG_BEAT_MS);

    expect(diagFinish).toHaveBeenCalledTimes(1);
  });

  it('DIAG-750: a re-render inside the beat does not skip a probe', async () => {
    const view = await mountRebuilt({
      diagAnswer: async () => ({
        correct: true,
        next_probe: probe({ problem_id: 'd2', text: 'Second.' }),
      }),
    });

    await act(async () => { view.rerender(); });
    await tick(DIAG_BEAT_MS);

    // Two timers would have advanced the counter twice for one answer.
    expect(progressCount()).toBe('Question 2 of up to 40');
    expect(document.querySelector('.problem-text')!.textContent).toBe('Second.');
  });
});
