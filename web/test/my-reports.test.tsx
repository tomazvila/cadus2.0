/** "My reports": the list of the own reports, and the plain words of each status. */
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, screen, waitFor } from '@testing-library/react';
import { createDemoApi } from '@/api';
import { MyReportsScreen, MY_REPORTS_EMPTY } from '@/views/reports/MyReports';
import { reportListText, reportStatusText } from '@/lib/stageCopy';
import { renderInView } from './helpers/render';
import type { ApiClient, MyReport } from '@/api/types';

const row = (over: Partial<MyReport> = {}): MyReport => ({
  report_id: 'r-1', task_id: 't-1', problem_id: 'p-1', topic: 'parity', question: 'Compute 8 - 5.',
  note: 'The key looks wrong.', status: 'completed', stage: 'completed', outcome: 'no_issue_found',
  grade_corrected: false, details_hidden: false, explanation: 'Eight minus five is three.',
  corrected_answer: '3', created_at: '2026-10-01T09:00:00Z', updated_at: '2026-10-01T09:05:00Z', ...over,
});

async function show(list: MyReport[], over: Partial<ApiClient> = {}) {
  const api: ApiClient = { ...createDemoApi(), listMyReports: vi.fn(async () => ({ reports: list })), ...over };
  const onExit = vi.fn();
  await renderInView(<MyReportsScreen api={api} demo={false} onUnauthorized={vi.fn()} onExit={onExit} />);
  return { api, onExit };
}

describe('my reports', () => {
  it('shows the status in plain words with the explanation and the reviewed answer', async () => {
    await show([row()]);
    await waitFor(() => expect(screen.getByText('Checked: the key was right.')).toBeTruthy());
    expect(screen.getByText(/Eight minus five is three\./)).toBeTruthy();
    expect(screen.getByText('Reviewed answer')).toBeTruthy();
    expect(screen.getByText(/The key looks wrong\./)).toBeTruthy();
  });

  it('names each state of the check', async () => {
    await show([
      row({ report_id: 'a', status: 'queued', outcome: null, explanation: null, corrected_answer: null }),
      row({ report_id: 'b', outcome: 'regraded', grade_corrected: true }),
      row({ report_id: 'c', status: 'unresolved', outcome: 'needs_review' }),
      row({ report_id: 'd', status: 'failed', outcome: 'failed' }),
    ]);
    await waitFor(() => expect(screen.getByText('Waiting for the check.')).toBeTruthy());
    expect(screen.getByText('Checked: your answer was right, the grade changed.')).toBeTruthy();
    expect(screen.getByText('Checked: no decision; the owner reads it.')).toBeTruthy();
    expect(screen.getByText('The check could not run; try again later.')).toBeTruthy();
  });

  it('names the flag states', async () => {
    await show([
      row({ report_id: 'w', kind: 'question_unclear', status: 'waiting_owner', outcome: null, explanation: null, corrected_answer: null }),
      row({ report_id: 'k', kind: 'question_unclear', outcome: 'question_kept', explanation: null, corrected_answer: null }),
      row({ report_id: 'r', kind: 'question_unclear', outcome: 'question_retired', explanation: null, corrected_answer: null }),
    ]);
    await waitFor(() => expect(screen.getByText('Waiting for the owner. The question is set aside for you.')).toBeTruthy());
    expect(screen.getByText(/the owner kept the question/)).toBeTruthy();
    expect(screen.getByText(/the owner removed the question/)).toBeTruthy();
  });

  it('keeps the details back while the service hides them', async () => {
    await show([row({ outcome: null, details_hidden: true })]);
    await waitFor(() => expect(screen.getByText(/details show after/)).toBeTruthy());
  });

  it('says so when the learner sent no report, and Done leaves', async () => {
    const { onExit } = await show([]);
    await waitFor(() => expect(screen.getByText(MY_REPORTS_EMPTY)).toBeTruthy());
    fireEvent.click(screen.getByRole('button', { name: 'Done' }));
    expect(onExit).toHaveBeenCalled();
  });

  it('gives the unresolved report no promise that a second send helps', () => {
    expect(reportStatusText('unresolved', 'adjudicator', 2, 3)).not.toMatch(/send the report again/i);
    expect(reportStatusText('unresolved', 'adjudicator', 2, 3)).toMatch(/the owner reads it/);
    expect(reportStatusText('completed', 'completed', 1, 3, 'regraded')).toMatch(/grade changed/);
    expect(reportListText('running', null)).toBe('The check is running.');
  });
});
