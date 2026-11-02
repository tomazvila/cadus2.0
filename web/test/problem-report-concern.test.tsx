/** D-RP2: the report form asks what is wrong before the note, with three plain choices. */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { createDemoApi } from '@/api';
import { ProblemReport } from '@/views/session/ProblemReport';
import { useProblemReport } from '@/views/session/useProblemReport';
import { FLAG_SENT } from '@/lib/stageCopy';
import type { ProblemReportReceipt, SubmittedProblemContext } from '@/api/types';

const waiting: ProblemReportReceipt = { report_id: 'f-1', status: 'waiting_owner', stage: 'waiting_owner',
  attempt: 0, max_attempts: 3, retryable: false, set_aside: true };

const sent = vi.fn().mockResolvedValue(waiting);
const api = { ...createDemoApi(), taskReport: sent };

function Panel({ context }: { context: SubmittedProblemContext }) {
  const report = useProblemReport(api, context);
  return <ProblemReport report={report} />;
}
const base: SubmittedProblemContext = { task_id: 't', problem_id: 'p', attempt_id: 'a', problem_text: 'Compute 8 - 5.', answer: '3', work: '' };

describe('report concern', () => {
  beforeEach(() => { sent.mockReset(); sent.mockResolvedValue(waiting); });
  it('offers three plain choices, with the first one chosen', () => {
    render(<Panel context={base} />);
    fireEvent.click(screen.getByRole('button', { name: 'Report this question' }));
    expect(screen.getByRole('radio', { name: 'My answer is right' }).getAttribute('aria-checked')).toBe('true');
    expect(screen.getByRole('radio', { name: 'The answer key is wrong' })).toBeTruthy();
    expect(screen.getByRole('radio', { name: 'The question is unclear or wrong' })).toBeTruthy();
  });

  it('sends the unclear concern, states what happened, and starts no polling', async () => {
    render(<Panel context={base} />);
    fireEvent.click(screen.getByRole('button', { name: 'Report this question' }));
    fireEvent.click(screen.getByRole('radio', { name: 'The question is unclear or wrong' }));
    fireEvent.click(screen.getByRole('button', { name: 'Send report' }));
    await screen.findByText(FLAG_SENT);
    expect(sent).toHaveBeenCalledWith('t', expect.objectContaining({ concern: 'question_unclear' }), expect.any(AbortSignal));
  });

  it('sends no concern for the first two choices, so the check chain stays as it was', async () => {
    render(<Panel context={base} />);
    fireEvent.click(screen.getByRole('button', { name: 'Report this question' }));
    fireEvent.click(screen.getByRole('radio', { name: 'The answer key is wrong' }));
    fireEvent.click(screen.getByRole('button', { name: 'Send report' }));
    await screen.findByText(FLAG_SENT);
    expect(sent.mock.calls[0]![1]).not.toHaveProperty('concern');
  });

  it('hides "My answer is right" before an answer, and the third choice for a placement question', () => {
    const served = { ...base, report_kind: 'served' as const, answer: '' };
    const { unmount } = render(<Panel context={served} />);
    fireEvent.click(screen.getByRole('button', { name: 'Report this question' }));
    expect(screen.queryByRole('radio', { name: 'My answer is right' })).toBeNull();
    expect(screen.getByRole('radio', { name: 'The answer key is wrong' }).getAttribute('aria-checked')).toBe('true');
    unmount();
    render(<Panel context={{ ...base, report_kind: 'diagnostic' }} />);
    fireEvent.click(screen.getByRole('button', { name: 'Report this question' }));
    expect(screen.queryByRole('radio', { name: 'The question is unclear or wrong' })).toBeNull();
  });
});
