import { act, fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { createDemoApi } from '@/api';
import { Integrated } from '@/views/session/Integrated';
import { QuizResults } from '@/views/QuizResults';
import { Diagnostic } from '@/views/Diagnostic';
import { applyReportCorrection } from '@/views/session/applyReportCorrection';
import type { ProblemReportReceipt } from '@/api/types';
import { held } from './helpers/held';
import { graded, mount, planOf, REVIEW, stubApi, submitAnswer } from './helpers/session';
import { PROBLEM, gradeReply } from './helpers/integrated';

const corrected: ProblemReportReceipt = { report_id: 'fixed', status: 'completed', stage: 'Finished',
  attempt: 1, max_attempts: 3, retryable: false, result: { resolution: 'confirmed_issue',
    message: 'Verified correction saved.', qwen_verdict: 'correct', verification: 'proved',
    grade_corrected: true, corrected_outcome: 'correct', content_published: true, solution: 'Verified solution' } };
const context = { task_id: 'task', problem_id: 'p1', attempt_id: 'a-1', problem_text: 'Question', answer: '3', work: '' };
const click = async (name: string) => { await act(async () => { fireEvent.click(screen.getByRole('button', { name })); }); };

describe('committed report corrections', () => {
  it('requires both committed flag and authoritative outcome on the matching attempt', () => {
    const previous = graded({ correct: false });
    expect(applyReportCorrection(previous, { ...corrected, result: { ...corrected.result!, grade_corrected: false } }, context)).toBe(previous);
    expect(applyReportCorrection(previous, { ...corrected, result: { ...corrected.result!, corrected_outcome: null } }, context)).toBe(previous);
    expect(applyReportCorrection(previous, corrected, { ...context, attempt_id: 'older-attempt' })).toBe(previous);
    expect(applyReportCorrection(previous, corrected, context)).toMatchObject({ outcome: 'correct', correct: true, report_corrected: true, solution: 'Verified solution' });
    expect(previous.correct).toBe(false);
  });

  it('changes the visible session grade only after the committed report response arrives', async () => {
    const pending = held<ProblemReportReceipt>();
    const api = stubApi({ taskAnswer: async () => graded({ correct: false, next: null }), taskReport: vi.fn(() => pending.promise) });
    const navigation = await mount({ api, plan: planOf(REVIEW) });
    await submitAnswer('3');
    await click('Report submitted question');
    await click('Send report');
    expect(screen.getByText('Not quite')).toBeTruthy();
    await act(async () => { pending.release(corrected); });
    expect(screen.getByText('Correct')).toBeTruthy();
    expect(screen.queryByText('Not quite')).toBeNull();
    expect(screen.queryByText('nearly passable')).toBeNull();
    expect(screen.getByText('Grade corrected after verification.')).toBeTruthy();
    await click('Continue with updated progress');
    expect(navigation.onExit).toHaveBeenCalledTimes(1);
  });

  it('reloads quiz scores and answers from the service after a committed correction', async () => {
    const api = createDemoApi();
    const initial = { score: 0, xp: 0, inconclusive: false, practice_available: false, practice_pending: false,
      answers: [{ problem_id: 'q', text: 'Quiz question', given_answer: '3', outcome: 'incorrect', correct: false }] };
    api.taskQuizResult = vi.fn().mockResolvedValueOnce(initial).mockResolvedValue({ ...initial, score: 1, xp: 10,
      answers: [{ ...initial.answers[0], correct: true, outcome: 'correct' }] });
    api.taskReport = vi.fn().mockResolvedValue(corrected);
    render(<QuizResults api={api} taskId="quiz" onUnauthorized={vi.fn()} />);
    await click('Review results');
    await click('Report submitted question');
    await click('Send report');
    expect(api.taskQuizResult).toHaveBeenCalledTimes(2);
    expect(screen.getByText('100% of graded answers correct · 10 XP')).toBeTruthy();
    expect(screen.getByText('Correct')).toBeTruthy();
  });

  it('updates placement feedback without revealing the report solution during placement', async () => {
    vi.useFakeTimers();
    const api = createDemoApi();
    api.taskReport = vi.fn().mockResolvedValue(corrected);
    render(<Diagnostic reportApi={api} demo onUnauthorized={vi.fn()} onExit={vi.fn()} diag={{
      diagStart: async () => ({ probe: { problem_id: 'd1', text: 'Placement question' } }),
      diagAnswer: async () => ({ correct: false, next_probe: { problem_id: 'd2', text: 'Next placement question' } }),
      diagFinish: vi.fn(),
    }} />);
    await click('Begin placement');
    fireEvent.change(screen.getByLabelText('Answer'), { target: { value: '3' } });
    await click('Submit');
    await click('Report submitted question');
    await click('Send report');
    expect(screen.getByText('Correct')).toBeTruthy();
    expect(screen.queryByText('Verified solution')).toBeNull();
    vi.useRealTimers();
  });

  it('refreshes an integrated result with the exact recorded submission', async () => {
    const api = createDemoApi();
    const original = gradeReply();
    api.taskIntegratedAnswer = vi.fn().mockResolvedValueOnce(original).mockResolvedValue({ ...original,
      correct_steps: 2, steps: original.steps.map((step) => ({ ...step, correct: true })) });
    api.taskReport = vi.fn().mockResolvedValue(corrected);
    render(<Integrated api={api} reportApi={api} taskId="integrated" problem={PROBLEM} />);
    fireEvent.change(screen.getByLabelText('Step 2'), { target: { value: '240' } });
    await click('Submit the whole task');
    await click('Report step 2');
    await click('Send report');
    expect(api.taskIntegratedAnswer).toHaveBeenCalledTimes(2);
    const calls = vi.mocked(api.taskIntegratedAnswer).mock.calls;
    expect(calls[1]).toEqual(calls[0]);
    expect(screen.getByText(/2 of 2 steps/)).toBeTruthy();
  });
});
