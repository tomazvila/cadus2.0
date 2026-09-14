import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { createDemoApi } from '@/api';
import { ProblemReport } from '@/views/session/ProblemReport';
import { useProblemReport } from '@/views/session/useProblemReport';
import type { ApiClient, ProblemReportReceipt, SubmittedProblemContext } from '@/api/types';

const context: SubmittedProblemContext = { task_id: 'old-task', problem_id: 'old-problem',
  attempt_id: 'old-attempt', problem_text: 'List the factors.', answer: '24x1,12x2,8x3,6x4', work: '' };
const completed: ProblemReportReceipt = { report_id: 'report-1', status: 'completed', stage: 'Finished',
  attempt: 1, max_attempts: 3, retryable: false, result: { resolution: 'confirmed_issue',
    message: 'The representation is valid.', qwen_verdict: 'correct', verification: 'proved',
    grade_corrected: false, content_published: false } };
function Harness({ api }: { api: ApiClient }) {
  const report = useProblemReport(api);
  return <><button onClick={() => report.remember(context)}>Record submission</button><ProblemReport report={report} /></>;
}
function openReport(api: ApiClient) {
  const view = render(<Harness api={api} />);
  fireEvent.click(screen.getByText('Record submission'));
  fireEvent.click(screen.getByRole('button', { name: 'Report submitted question' }));
  return view;
}

describe('submitted question reports', () => {
  it('has no report action before an attempt is recorded', () => {
    render(<Harness api={createDemoApi()} />);
    expect(screen.queryByRole('button', { name: 'Report submitted question' })).toBeNull();
  });
  it('posts frozen attempt identity and distinguishes model, verification, and applied changes', async () => {
    const api = createDemoApi();
    api.taskReport = vi.fn().mockResolvedValue(completed);
    openReport(api);
    fireEvent.change(screen.getByLabelText('What should we check? (optional)'), { target: { value: 'Factor pairs are valid.' } });
    fireEvent.click(screen.getByRole('button', { name: 'Send report' }));
    await screen.findByText('The representation is valid.');
    expect(api.taskReport).toHaveBeenCalledWith('old-task', expect.objectContaining({
      problem_id: 'old-problem', attempt_id: 'old-attempt', note: 'Factor pairs are valid.', request_id: expect.any(String),
    }), expect.any(AbortSignal));
    expect(screen.getByText('Qwen assessment')).toBeTruthy();
    expect(screen.getByText('Mathematical verification')).toBeTruthy();
    expect(screen.getAllByText('No')).toHaveLength(2);
  });
  it('preserves the idempotency key when retrying a failed transport', async () => {
    const api = createDemoApi();
    const post = vi.fn().mockRejectedValueOnce(new Error('Network unavailable')).mockResolvedValue(completed);
    api.taskReport = post;
    openReport(api);
    fireEvent.click(screen.getByRole('button', { name: 'Send report' }));
    await screen.findByRole('alert');
    fireEvent.click(screen.getByRole('button', { name: 'Retry sending report' }));
    await screen.findByText('The representation is valid.');
    expect(post.mock.calls[0][1]).toEqual(post.mock.calls[1][1]);
  });
  it('starts a fresh retry once and reuses its UUID after a network failure', async () => {
    const api = createDemoApi();
    const failed: ProblemReportReceipt = { report_id: 'failed-report', status: 'failed',
      stage: 'Worker unavailable', attempt: 1, max_attempts: 3, retryable: true };
    const post = vi.fn().mockResolvedValueOnce(failed)
      .mockRejectedValueOnce(new Error('Network unavailable')).mockResolvedValue(completed);
    api.taskReport = post;
    openReport(api);
    fireEvent.click(screen.getByRole('button', { name: 'Send report' }));
    await screen.findByRole('button', { name: 'Retry report' });
    expect((screen.getByLabelText('What should we check? (optional)') as HTMLTextAreaElement).disabled).toBe(false);
    fireEvent.click(screen.getByRole('button', { name: 'Retry report' }));
    await screen.findByRole('alert');
    expect((screen.getByLabelText('What should we check? (optional)') as HTMLTextAreaElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: 'Retry sending report' }));
    await screen.findByText('The representation is valid.');
    expect(post.mock.calls[0][1].request_id).not.toEqual(post.mock.calls[1][1].request_id);
    expect(post.mock.calls[1][1]).toEqual(post.mock.calls[2][1]);
  });
  it('aborts its pending request on unmount', async () => {
    const api = createDemoApi();
    const post = vi.fn().mockImplementation(() => new Promise<ProblemReportReceipt>(() => {}));
    api.taskReport = post;
    const view = openReport(api);
    fireEvent.click(screen.getByRole('button', { name: 'Send report' }));
    await waitFor(() => expect(post).toHaveBeenCalledTimes(1));
    const signal = post.mock.calls[0][2] as AbortSignal;
    view.unmount();
    expect(signal.aborted).toBe(true);
  });
});
