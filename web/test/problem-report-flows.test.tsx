import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { createDemoApi } from '@/api';
import { Quiz } from '@/views/Quiz';
import { QuizResults } from '@/views/QuizResults';
import { Diagnostic, DIAG_BEAT_MS } from '@/views/Diagnostic';
import { Integrated } from '@/views/session/Integrated';
import type { ProblemReportReceipt } from '@/api/types';
import type { DiagnosticApi } from '@/api/diag';
import { mount, planOf, REVIEW } from './helpers/session';
import { QUIZ, Q } from './helpers/quiz';
import { PROBLEM, gradeReply } from './helpers/integrated';

const reviewed: ProblemReportReceipt = { report_id: 'report', status: 'completed', stage: 'Secret reviewed answer',
  attempt: 1, max_attempts: 3, retryable: false, result: { resolution: 'confirmed_issue',
    message: 'Secret reviewed explanation', qwen_verdict: 'correct', verification: 'proved',
    corrected_answer: 'Secret answer', solution: 'Secret solution', grade_corrected: false, content_published: false } };
const click = async (name: string) => { await act(async () => { fireEvent.click(screen.getByRole('button', { name })); }); };
function reportApi() {
  const api = createDemoApi();
  api.taskReport = vi.fn().mockResolvedValue(reviewed);
  api.taskAnswer = vi.fn();
  return api;
}

describe('reports across question flows', () => {
  it('reports an unanswered session question using only its server identity', async () => {
    const api = reportApi();
    api.taskServe = vi.fn().mockResolvedValue(Q(1));
    await mount({ api, plan: planOf(REVIEW) });
    await click('Report question');
    await click('Send report');
    expect(api.taskReport).toHaveBeenCalledWith(REVIEW.task_id, {
      problem_id: 'q1', report_kind: 'served', request_id: expect.any(String),
    }, expect.any(AbortSignal));
    expect(api.taskAnswer).not.toHaveBeenCalled();
    expect(screen.queryByText('Secret answer')).toBeNull();
  });

  it('withholds report findings during a timed quiz and does not submit an answer', async () => {
    const api = reportApi();
    api.taskServe = vi.fn().mockResolvedValue(Q(1));
    await act(async () => { render(<Quiz api={api} task={QUIZ} demo onUnauthorized={vi.fn()} onDone={vi.fn()} />); });
    await click('Report question');
    await click('Send report');
    expect(api.taskReport).toHaveBeenCalledWith(QUIZ.task_id, expect.objectContaining({ problem_id: 'q1', report_kind: 'served' }), expect.any(AbortSignal));
    expect(api.taskAnswer).not.toHaveBeenCalled();
    expect(screen.queryByText(/Secret/)).toBeNull();
    expect(screen.getByText(/Review details are withheld/)).toBeTruthy();
  });

  it('reports each revealed quiz answer with its own problem identity', async () => {
    const api = reportApi();
    api.taskQuizResult = vi.fn().mockResolvedValue({ score: 0.5, xp: 2, inconclusive: false,
      practice_available: false, practice_pending: false, answers: [
        { problem_id: 'first', text: 'First question', given_answer: '1', correct: true, outcome: 'correct' },
        { problem_id: 'second', text: 'Second question', given_answer: '2', correct: false, outcome: 'incorrect' },
      ] });
    render(<QuizResults api={api} taskId="quiz" onUnauthorized={vi.fn()} />);
    await click('Review results');
    const second = within(screen.getAllByRole('article')[1]);
    fireEvent.click(second.getByRole('button', { name: 'Report submitted question' }));
    await act(async () => { fireEvent.click(second.getByRole('button', { name: 'Send report' })); });
    expect(api.taskReport).toHaveBeenCalledWith('quiz', {
      problem_id: 'second', report_kind: 'attempt', request_id: expect.any(String),
    }, expect.any(AbortSignal));
    expect(second.getByText('Secret reviewed explanation')).toBeTruthy();
  });

  it('reports the integrated item before and after submission without inventing an attempt ID', async () => {
    const api = reportApi();
    api.taskIntegratedAnswer = vi.fn().mockResolvedValue(gradeReply());
    render(<Integrated api={api} reportApi={api} taskId="integrated" problem={PROBLEM} />);
    await click('Report question');
    await click('Send report');
    expect(api.taskIntegratedAnswer).not.toHaveBeenCalled();
    expect(api.taskReport).toHaveBeenLastCalledWith('integrated', expect.objectContaining({
      problem_id: PROBLEM.item_id, item_digest: PROBLEM.item_digest, report_kind: 'served',
    }), expect.any(AbortSignal));
    await click('Close report');
    fireEvent.change(screen.getByLabelText('Final answer'), { target: { value: '7' } });
    await click('Submit the whole task');
    await click('Report submitted question');
    await click('Send report');
    expect(api.taskReport).toHaveBeenLastCalledWith('integrated', {
      problem_id: PROBLEM.item_id, item_digest: PROBLEM.item_digest, field_id: 'final', report_kind: 'integrated', request_id: expect.any(String),
    }, expect.any(AbortSignal));
    expect(screen.getByText('Secret reviewed explanation')).toBeTruthy();
    await click('Close report');
    await click('Report step 1');
    await click('Send report');
    expect(api.taskReport).toHaveBeenLastCalledWith('integrated', expect.objectContaining({
      field_id: PROBLEM.steps[0].id, report_kind: 'integrated',
    }), expect.any(AbortSignal));
  });

  it('keeps a diagnostic report attached to the answered probe after the next probe arrives', async () => {
    vi.useFakeTimers();
    const api = reportApi();
    const diag: DiagnosticApi = {
      diagStart: vi.fn().mockResolvedValue({ probe: { problem_id: 'd1', text: 'First probe' } }),
      diagAnswer: vi.fn().mockResolvedValue({ correct: true, next_probe: { problem_id: 'd2', text: 'Next probe' } }),
      diagFinish: vi.fn(),
    };
    render(<Diagnostic diag={diag} reportApi={api} demo onUnauthorized={vi.fn()} onExit={vi.fn()} />);
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: /Begin/ })); });
    await click('Report question');
    await click('Send report');
    expect(diag.diagAnswer).not.toHaveBeenCalled();
    expect(api.taskReport).toHaveBeenLastCalledWith('diag', expect.objectContaining({ problem_id: 'd1', report_kind: 'served' }), expect.any(AbortSignal));
    await click('Close report');
    fireEvent.change(screen.getByLabelText('Answer'), { target: { value: '7' } });
    await click('Submit');
    await click('Report submitted question');
    await click('Send report');
    await act(async () => { vi.advanceTimersByTime(DIAG_BEAT_MS); });
    expect(screen.queryByText('Next probe')).toBeNull();
    await click('Close report');
    await act(async () => { vi.advanceTimersByTime(DIAG_BEAT_MS); });
    await click('Report submitted question');
    expect(screen.queryByRole('button', { name: 'Send report' })).toBeNull();
    expect(api.taskReport).toHaveBeenLastCalledWith('diag', expect.objectContaining({ problem_id: 'd1', report_kind: 'diagnostic' }), expect.any(AbortSignal));
    expect(screen.queryByText(/Secret/)).toBeNull();
    expect(screen.getByText('Next probe')).toBeTruthy();
    vi.useRealTimers();
  });
});
