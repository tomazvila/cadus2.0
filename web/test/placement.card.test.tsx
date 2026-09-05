/**
 * The placement card, control by control: the intro line, the busy buttons, the locked
 * field, the progress bar and the beat that reads the next probe.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, screen } from '@testing-library/react';
import { ApiError } from '@/api';
import { DIAG_BEAT_MS, DIAG_START_FAILED } from '@/views/Diagnostic';
import { failThenHold } from './helpers/api';
import { blurThenSubmitEmpty } from './helpers/field';
import { held } from './helpers/held';
import { pressRetry } from './helpers/toasts';
import { tick } from './helpers/timers';
import {
  START, SUMMARY, answer, answerInput, begin, beginButton, mount, probe, skipButton, stubDiag,
  submitButton,
} from './helpers/placement';
import type { DiagAnswerResponse, DiagStartResponse, DiagnosticApi } from '@/api/diag';

const fill = () => document.querySelector('.progress-fill') as HTMLElement;

describe('the intro', () => {
  it('opens with no failure line, and drops the line once a start succeeds', async () => {
    const diagStart = vi.fn<DiagnosticApi['diagStart']>()
      .mockRejectedValueOnce(new ApiError(500, 'server_error', 'Down.'))
      .mockResolvedValue(START);
    await mount({ diag: stubDiag({ diagStart }) });
    expect(screen.queryByText(DIAG_START_FAILED)).toBeNull();

    await begin();
    expect(screen.getByText(DIAG_START_FAILED)).toBeTruthy();
    await begin();
    expect(screen.queryByText(DIAG_START_FAILED)).toBeNull();
    expect(screen.getByText('Question 1 of up to 40')).toBeTruthy();
  });

  it('reads every ground rule as its head, a space, and its body', async () => {
    await mount();
    for (const li of Array.from(document.querySelectorAll('.intro-rules li'))) {
      const head = li.querySelector('strong')!.textContent!;
      const body = li.querySelector('span')!.textContent!;
      expect(body.startsWith(' ')).toBe(true);
      expect(li.textContent).toBe(`${head}${body}`);
    }
    expect(beginButton()).toBeTruthy();
  });

  it('routes a 401 on the start to sign-in', async () => {
    const view = await mount({
      diag: stubDiag({ diagStart: async () => { throw new ApiError(401, 'unauthorized', 'No session.'); } }),
    });
    await begin();
    expect(view.onUnauthorized).toHaveBeenCalledTimes(1);
  });

  it('commits nothing from a start with no probe that lands after the view left', async () => {
    const start = held<DiagStartResponse>();
    const diagFinish = vi.fn<DiagnosticApi['diagFinish']>(async () => SUMMARY);
    const view = await mount({ diag: stubDiag({ diagStart: () => start.promise, diagFinish }) });
    await begin();
    view.unmount();
    await act(async () => { start.release({ probe: null }); });
    expect(diagFinish).not.toHaveBeenCalled();
  });
});

describe('the probe card', () => {
  it('marks both buttons busy while an answer is out, and locks the field', async () => {
    const grade = held<DiagAnswerResponse>();
    await mount({ diag: stubDiag({ diagAnswer: () => grade.promise }) });
    await begin();
    expect(submitButton().className).toBe('btn btn-primary');
    expect(skipButton().className).toBe('btn btn-ghost');

    await answer('5');
    expect(submitButton().className).toBe('btn btn-primary is-busy');
    expect(skipButton().className).toBe('btn btn-ghost is-busy');
    expect(answerInput().disabled).toBe(true);

    await act(async () => { grade.release({ correct: true, next_probe: probe({ problem_id: 'd2' }) }); });
    // The verdict locks the card too, until the beat moves it on.
    expect(answerInput().disabled).toBe(true);
    expect(submitButton().hasAttribute('disabled')).toBe(true);
  });

  it('marks Submit busy again on the Retry of a failed answer', async () => {
    const grade = failThenHold<DiagAnswerResponse>();
    await mount({ diag: stubDiag({ diagAnswer: grade.fn }) });
    await begin();
    await answer('5');
    expect(submitButton().className).toBe('btn btn-primary');
    await pressRetry();
    expect(submitButton().className).toBe('btn btn-primary is-busy');
    await act(async () => { grade.release({ correct: true, next_probe: { done: true } }); });
  });

  it('returns the focus to an empty field on Submit', async () => {
    await mount();
    await begin();
    await blurThenSubmitEmpty(answerInput(), submitButton());
    expect(submitButton().hasAttribute('disabled')).toBe(false);
  });

  it('fills the progress bar by the question over the cap', async () => {
    vi.useFakeTimers();
    await mount({
      diag: stubDiag({ diagAnswer: async () => ({ correct: true, next_probe: probe({ problem_id: 'd2' }) }) }),
    });
    await begin();
    expect(fill().style.width).toBe('2.5%');
    await answer('5');
    await tick(DIAG_BEAT_MS);
    expect(screen.getByText('Question 2 of up to 40')).toBeTruthy();
    expect(fill().style.width).toBe('5%');
  });

  it('commits when the next probe names no problem', async () => {
    vi.useFakeTimers();
    const diagFinish = vi.fn<DiagnosticApi['diagFinish']>(async () => SUMMARY);
    await mount({
      diag: stubDiag({
        diagFinish,
        diagAnswer: async () => ({ correct: true, next_probe: probe({ problem_id: '' }) }),
      }),
    });
    await begin();
    await answer('5');
    await tick(DIAG_BEAT_MS);
    expect(diagFinish).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Placement complete')).toBeTruthy();
  });
});
