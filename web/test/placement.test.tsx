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
 * The literals come from the 1.0 view and spec section 6: the beat is 750 ms, the cap
 * default is 40, and the progress line reads `Question 1 of up to 40`.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { ApiError } from '@/api';
import {
  Diagnostic,
  DIAG_BEAT_MS,
  DIAG_DEFAULT_CAP,
  DIAG_NO_SOLUTIONS_NOTE,
  DIAG_START_FAILED,
  type DiagnosticProps,
} from '@/views/Diagnostic';
import { ROUTE_ABSENT_CODE, createDemoDiagApi } from '@/api/diag';
import { resetToasts } from '@/app/toast';
import { AXE_IN_JSDOM } from './axe';
import type {
  DiagAnswerResponse,
  DiagFinishResponse,
  DiagProbe,
  DiagStartResponse,
  DiagnosticApi,
} from '@/api/diag';

// ---------------------------------------------------------------------------
// Fixtures — the frozen payloads of the three `/api/diag/*` rows.
// ---------------------------------------------------------------------------

const probe = (over: Partial<DiagProbe> = {}): DiagProbe => ({
  problem_id: 'd1',
  topic: 'Adding integers',
  text: 'Work out $-7 + 12$.',
  ...over,
});

const START: DiagStartResponse = { probe: probe(), asked: 0, cap: 40 };

const SUMMARY: DiagFinishResponse = { placed: ['a', 'b'], conditional: [], frontier: ['c'] };

function stubDiag(over: Partial<DiagnosticApi> = {}): DiagnosticApi {
  return {
    diagStart: async () => START,
    diagAnswer: async () => ({ correct: true, next_probe: { done: true } }),
    diagFinish: async () => SUMMARY,
    ...over,
  };
}

async function mount(over: Partial<DiagnosticProps> = {}) {
  resetToasts();
  const handlers = { onUnauthorized: vi.fn(), onExit: vi.fn() };
  const props: DiagnosticProps = { diag: stubDiag(), ...handlers, ...over };
  let view!: ReturnType<typeof render>;
  await act(async () => {
    view = render(<Diagnostic {...props} />, { container: document.getElementById('view')! });
  });
  return { ...view, ...handlers };
}

const answerInput = () => screen.getByLabelText('Answer') as HTMLInputElement;
const beginButton = () => screen.getByRole('button', { name: 'Begin placement' });
const submitButton = () => screen.getByRole('button', { name: 'Submit' });
const skipButton = () => screen.getByRole('button', { name: 'Skip — I don’t know' });
const progressCount = () => document.querySelector('.progress-count')!.textContent;

/** Press Begin and let the start settle. */
async function begin(): Promise<void> {
  await act(async () => { fireEvent.click(beginButton()); });
}

/** Answer the probe on screen. */
async function answer(text: string): Promise<void> {
  fireEvent.change(answerInput(), { target: { value: text } });
  await act(async () => { fireEvent.click(submitButton()); });
}

/** Move the clock and let every continuation the move released settle. */
async function tick(ms: number): Promise<void> {
  await act(async () => { await vi.advanceTimersByTimeAsync(ms); });
}

// ---------------------------------------------------------------------------

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
    const { unmount } = await mount({ diag: stubDiag({ diagFinish }) });
    await begin();
    await answer('5');

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
    await mount({ diag: stubDiag({ diagFinish }) });
    await begin();
    await answer('5');

    await tick(DIAG_BEAT_MS - 1);
    expect(diagFinish).not.toHaveBeenCalled();
    await tick(1);
    expect(diagFinish).toHaveBeenCalledTimes(1);
    expect(DIAG_BEAT_MS).toBe(750);
  });

  it('DIAG-750: a re-render inside the beat arms no second timer', async () => {
    vi.useFakeTimers();
    const diagFinish = vi.fn<DiagnosticApi['diagFinish']>(async () => SUMMARY);
    const parts = { diagFinish };
    const handlers = { onUnauthorized: vi.fn(), onExit: vi.fn() };
    let view!: ReturnType<typeof render>;
    await act(async () => {
      view = render(<Diagnostic diag={stubDiag(parts)} {...handlers} />, {
        container: document.getElementById('view')!,
      });
    });
    await begin();
    await answer('5');

    // Two re-renders inside the window, each with a FRESH transport object — the default
    // shape of a caller that builds the port in its own render. With `finish` in the
    // effect's dependency list and no cleanup, each armed another timer, and two
    // concurrent commits both pass the service's "no placement yet" check and write two
    // placement rows to a log with no DELETE.
    await act(async () => {
      view.rerender(<Diagnostic diag={stubDiag(parts)} {...handlers} />);
      view.rerender(<Diagnostic diag={stubDiag(parts)} {...handlers} />);
    });
    await tick(DIAG_BEAT_MS);

    expect(diagFinish).toHaveBeenCalledTimes(1);
  });

  it('DIAG-750: a re-render inside the beat does not skip a probe', async () => {
    vi.useFakeTimers();
    const parts = {
      diagAnswer: async () => ({
        correct: true,
        next_probe: probe({ problem_id: 'd2', text: 'Second.' }),
      }),
    };
    const handlers = { onUnauthorized: vi.fn(), onExit: vi.fn() };
    let view!: ReturnType<typeof render>;
    await act(async () => {
      view = render(<Diagnostic diag={stubDiag(parts)} {...handlers} />, {
        container: document.getElementById('view')!,
      });
    });
    await begin();
    await answer('5');

    await act(async () => {
      view.rerender(<Diagnostic diag={stubDiag(parts)} {...handlers} />);
    });
    await tick(DIAG_BEAT_MS);

    // Two timers would have advanced the counter twice for one answer.
    expect(progressCount()).toBe('Question 2 of up to 40');
    expect(document.querySelector('.problem-text')!.textContent).toBe('Second.');
  });
});

describe('the probe loop', () => {
  it('paints the probe, its topic and the progress line after Begin', async () => {
    await mount();
    await begin();

    expect(document.querySelector('.topic-name')!.textContent).toBe('Adding integers');
    expect(document.querySelector('.problem-text')!.textContent).toContain('-7 + 12');
    expect(progressCount()).toBe('Question 1 of up to 40');
    expect(DIAG_DEFAULT_CAP).toBe(40);
    expect(screen.getByText('placement')).toBeTruthy();
  });

  it('reads the topic record as well as the bare 1.0 name', async () => {
    await mount({
      diag: stubDiag({
        diagStart: async () => ({
          probe: probe({ topic: { id: 'integers', name: 'Integers', module: 'Arithmetic' } }),
          asked: 3,
          cap: 12,
        }),
      }),
    });
    await begin();

    expect(document.querySelector('.topic-name')!.textContent).toBe('Integers');
    expect(progressCount()).toBe('Question 4 of up to 12');
  });

  it('advances to the next probe with a fresh, empty field', async () => {
    vi.useFakeTimers();
    await mount({
      diag: stubDiag({
        diagAnswer: async () => ({
          correct: true,
          next_probe: probe({ problem_id: 'd2', text: 'Second question.' }),
        }),
      }),
    });
    await begin();

    const first = answerInput();
    await answer('5');
    expect(document.querySelector('.feedback-correct')!.textContent).toBe('Correct');

    await tick(DIAG_BEAT_MS);

    expect(document.querySelector('.problem-text')!.textContent).toBe('Second question.');
    // Keyed on `problem_id`, so the previous answer cannot pre-fill the next probe.
    expect(answerInput()).not.toBe(first);
    expect(answerInput().value).toBe('');
  });

  it('F-37-1c: a second submit inside the grading window posts nothing', async () => {
    let release!: (value: DiagAnswerResponse) => void;
    const held = new Promise<DiagAnswerResponse>((r) => { release = r; });
    const diagAnswer = vi.fn<DiagnosticApi['diagAnswer']>(() => held);
    await mount({ diag: stubDiag({ diagAnswer }) });
    await begin();

    fireEvent.change(answerInput(), { target: { value: '5' } });
    fireEvent.click(submitButton());
    expect(diagAnswer).toHaveBeenCalledTimes(1);

    // Enter bypasses the disabled button entirely; only the phase gate stops it. So does
    // the Skip button, which is a second write path onto the same `problem_id`.
    fireEvent.keyDown(answerInput(), { key: 'Enter' });
    fireEvent.click(submitButton());
    fireEvent.click(skipButton());
    expect(diagAnswer).toHaveBeenCalledTimes(1);

    await act(async () => { release({ correct: true, next_probe: { done: true } }); });
    expect(diagAnswer).toHaveBeenCalledTimes(1);
  });

  it('a failed answer returns the probe to the learner instead of locking the card', async () => {
    const diagAnswer = vi.fn<DiagnosticApi['diagAnswer']>(async () => {
      throw new Error('the network went away');
    });
    await mount({ diag: stubDiag({ diagAnswer }) });
    await begin();
    await answer('5');

    expect(submitButton().hasAttribute('disabled')).toBe(false);
    await answer('5');
    expect(diagAnswer).toHaveBeenCalledTimes(2);
  });
});

describe('the placement commit', () => {
  it('shows the commit in progress, not the answered probe', async () => {
    vi.useFakeTimers();
    let release!: (value: DiagFinishResponse) => void;
    const held = new Promise<DiagFinishResponse>((r) => { release = r; });
    await mount({ diag: stubDiag({ diagFinish: () => held }) });
    await begin();
    await answer('5');
    await tick(DIAG_BEAT_MS);

    // The commit takes seconds. The answered question left on screen — with its tick and a
    // live "Save & exit" — reads as "nothing happened", and the learner leaves mid-commit.
    expect(screen.getByText('Working out your placement…')).toBeTruthy();
    expect(document.querySelector('.problem-card')).toBeNull();
    expect(document.querySelector('.feedback')).toBeNull();

    await act(async () => { release(SUMMARY); });
    expect(screen.getByText('Placement complete')).toBeTruthy();
  });

  it('renders the counts and the frontier list', async () => {
    vi.useFakeTimers();
    await mount({
      diag: stubDiag({
        diagFinish: async () => ({
          placed: ['a', 'b', 'c'],
          conditional: ['d'],
          frontier: ['adding-fractions', 'powers'],
        }),
      }),
    });
    await begin();
    await answer('5');
    await tick(DIAG_BEAT_MS);

    expect(Array.from(document.querySelectorAll('.stat')).map((s) => s.textContent))
      .toEqual(['3topics placed', '1conditional', '2frontier topics']);
    expect(Array.from(document.querySelectorAll('.frontier-list li')).map((li) => li.textContent))
      .toEqual(['adding-fractions', 'powers']);
    expect(screen.getByText('Start here')).toBeTruthy();
  });

  it('still ends the screen when the commit fails: the summary is a receipt, not the record', async () => {
    vi.useFakeTimers();
    const { onExit } = await mount({
      diag: stubDiag({ diagFinish: async () => { throw new Error('server down'); } }),
    });
    await begin();
    await answer('5');
    await tick(DIAG_BEAT_MS);

    expect(screen.getByText('Placement finished.')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Back to dashboard' }));
    expect(onExit).toHaveBeenCalledTimes(1);
  });

  it('finishes rather than blanking when a restarted placement has no probe left', async () => {
    const diagFinish = vi.fn<DiagnosticApi['diagFinish']>(async () => SUMMARY);
    await mount({ diag: stubDiag({ diagStart: async () => ({ probe: null }), diagFinish }) });
    await begin();

    // Reading `.text` off `{"probe": null}` blanks the screen. Finishing is the honest
    // answer: there is nothing left to ask.
    expect(diagFinish).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Placement complete')).toBeTruthy();
  });

  it('returns to the intro with a stated line when the route is absent', async () => {
    // M5 mounts no `/api/diag/*` route, so the live adapter answers `404 not_found` until
    // the Rust unit lands. A spinner with no way on is the wrong screen for that.
    const diagStart = vi.fn<DiagnosticApi['diagStart']>(async () => {
      throw new ApiError(404, ROUTE_ABSENT_CODE, 'Not found.');
    });
    await mount({ diag: stubDiag({ diagStart }) });
    await begin();

    expect(screen.getByText(DIAG_START_FAILED)).toBeTruthy();
    expect(DIAG_START_FAILED).toBe('The placement did not start. Try again in a moment.');
    expect(beginButton()).toBeTruthy();
    await begin();
    expect(diagStart).toHaveBeenCalledTimes(2);
  });
});

describe('the demo placement port', () => {
  it('walks three probes and places the learner with no service at all', async () => {
    const demo = createDemoDiagApi();
    const start = await demo.diagStart();
    expect(start.probe!.problem_id).toBe('demo-d1');
    expect(start.cap).toBe(3);

    const first = await demo.diagAnswer({ problem_id: 'demo-d1', answer: '5' });
    expect(first.correct).toBe(true);
    // The blank answer is the honest skip, and it grades incorrect (P3).
    const second = await demo.diagAnswer({ problem_id: 'demo-d2', answer: '' });
    expect(second.correct).toBe(false);
    const third = await demo.diagAnswer({ problem_id: 'demo-d3', answer: '5' });
    expect(third.next_probe).toEqual({ done: true });

    expect(await demo.diagFinish()).toEqual({
      placed: ['integers', 'fractions'],
      conditional: ['linear-equations'],
      frontier: ['linear-equations'],
    });
  });

  it('holds its state per client, so one test cannot poison the next', async () => {
    const a = createDemoDiagApi();
    await a.diagStart();
    await a.diagAnswer({ problem_id: 'demo-d1', answer: '5' });

    const b = createDemoDiagApi();
    expect((await b.diagStart()).probe!.problem_id).toBe('demo-d1');
  });

  it('refuses an answer before a start', async () => {
    const demo = createDemoDiagApi();
    await expect(demo.diagAnswer({ problem_id: 'demo-d1', answer: '5' })).rejects.toThrow(
      'No diagnostic is open.',
    );
  });
});

describe('accessibility', () => {
  it('the intro card reports no axe violation', async () => {
    const { container } = await mount();
    expect(await axe(container, AXE_IN_JSDOM)).toHaveNoViolations();
  });

  it('the probe card reports no axe violation', async () => {
    const { container } = await mount();
    await begin();
    expect(await axe(container, AXE_IN_JSDOM)).toHaveNoViolations();
  });

  it('the answer field takes the focus on a fresh probe', async () => {
    await mount();
    await begin();
    expect(document.activeElement).toBe(answerInput());
  });

  it('the summary moves the focus to the way out', async () => {
    vi.useFakeTimers();
    await mount();
    await begin();
    await answer('5');
    await tick(DIAG_BEAT_MS);

    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Back to dashboard' }));
  });
});
