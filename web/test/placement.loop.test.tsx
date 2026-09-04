/**
 * The placement diagnostic (S10), part 2: the probe loop, the commit, the demo port, and
 * accessibility.
 *
 * `placement.test.tsx` carries the module note and the fixtures live in
 * `test/helpers/placement.tsx`.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, screen } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { ApiError } from '@/api';
import { DIAG_BEAT_MS, DIAG_DEFAULT_CAP, DIAG_START_FAILED } from '@/views/Diagnostic';
import { ROUTE_ABSENT_CODE, createDemoDiagApi } from '@/api/diag';
import { AXE_IN_JSDOM } from './axe';
import { networkFailure } from './helpers/api';
import { tick } from './helpers/timers';
import {
  SUMMARY, answer, answerFirst, answerInput, begin, beginButton, mount, probe, progressCount,
  skipButton, stubDiag, submitButton,
} from './helpers/placement';
import type { DiagAnswerResponse, DiagFinishResponse, DiagnosticApi } from '@/api/diag';

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
    const diagAnswer = vi.fn<DiagnosticApi['diagAnswer']>(async () => { throw networkFailure(); });
    await answerFirst({ diag: stubDiag({ diagAnswer }) });

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
    await answerFirst({ diag: stubDiag({ diagFinish: () => held }) });
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
    await answerFirst({
      diag: stubDiag({
        diagFinish: async () => ({
          placed: ['a', 'b', 'c'],
          conditional: ['d'],
          frontier: ['adding-fractions', 'powers'],
        }),
      }),
    });
    await tick(DIAG_BEAT_MS);

    expect(Array.from(document.querySelectorAll('.stat')).map((s) => s.textContent))
      .toEqual(['3topics placed', '1conditional', '2frontier topics']);
    expect(Array.from(document.querySelectorAll('.frontier-list li')).map((li) => li.textContent))
      .toEqual(['adding-fractions', 'powers']);
    expect(screen.getByText('Start here')).toBeTruthy();
  });

  it('still ends the screen when the commit fails: the summary is a receipt, not the record', async () => {
    vi.useFakeTimers();
    const { onExit } = await answerFirst({
      diag: stubDiag({ diagFinish: async () => { throw new Error('server down'); } }),
    });
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
    await answerFirst();
    await tick(DIAG_BEAT_MS);

    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Back to dashboard' }));
  });
});
