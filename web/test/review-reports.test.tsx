/** The "Reports" section of the review screen: read only, with the curriculum place. */
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor, fireEvent } from '@testing-library/react';
import { ApiError } from '@/api';
import { REPORTS_EMPTY, REPORTS_TITLE, ReviewReports } from '@/views/admin/ReviewReports';
import { renderInView } from './helpers/render';
import { stubApi } from './helpers/admin';
import type { AdminReport, ApiClient } from '@/api/types';

const report = (over: Partial<AdminReport> = {}): AdminReport => ({
  report_id: 'r-1', user_id: 'u-77', task_id: 't-9', problem_id: 'p-1', topic: 'parity', kp: 'kp2',
  question: 'Compute 8 - 5.', note: 'The key looks wrong.', status: 'unresolved', attempt: 3,
  result_message: 'The check did not settle this.',
  last_step: { stage: 'verification', message: 'Lean could not prove it.' },
  created_at: '2026-10-01T09:00:00Z', updated_at: '2026-10-01T09:05:00Z', ...over,
});

async function show(over: Partial<ApiClient>, onOpenTopic?: (t: string) => void) {
  await renderInView(<ReviewReports api={stubApi(over)} demo={false} onUnauthorized={vi.fn()} onOpenTopic={onOpenTopic} />);
}

describe('review reports', () => {
  it('lists the unresolved reports with learner, task, question, note, last step and place', async () => {
    const listAdminReports = vi.fn(async () => ({ reports: [report()] }));
    const open = vi.fn();
    await show({ listAdminReports }, open);
    await waitFor(() => expect(screen.getByText('u-77')).toBeTruthy());
    expect(listAdminReports).toHaveBeenCalledWith('unresolved');
    expect(screen.getByText('t-9')).toBeTruthy();
    expect(screen.getByText(/The key looks wrong\./)).toBeTruthy();
    expect(screen.getByText('verification')).toBeTruthy();
    expect(screen.getByText(/Lean could not prove it\./)).toBeTruthy();
    expect(screen.getByText('kp2')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: /Open topic/ }));
    expect(open).toHaveBeenCalledWith('parity');
  });

  it('offers Keep and Retire for a flagged question and reloads after the decision', async () => {
    const flagged = report({ kind: 'question_unclear', status: 'waiting_owner', last_step: { stage: null, message: null } });
    const listAdminReports = vi.fn()
      .mockResolvedValueOnce({ reports: [flagged] })
      .mockResolvedValue({ reports: [] });
    const keepReportQuestion = vi.fn(async () => ({ report_id: 'r-1', status: 'kept' }));
    const retireReportQuestion = vi.fn(async () => ({ report_id: 'r-1', status: 'retired' }));
    await show({ listAdminReports, keepReportQuestion, retireReportQuestion });
    await waitFor(() => expect(screen.getByRole('button', { name: 'Retire the question' })).toBeTruthy());
    expect(screen.getByText(/set aside for this learner/)).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Keep the question' }));
    await waitFor(() => expect(keepReportQuestion).toHaveBeenCalledWith('r-1'));
    await waitFor(() => expect(screen.getByText(REPORTS_EMPTY)).toBeTruthy());
    expect(retireReportQuestion).not.toHaveBeenCalled();
  });

  it('retires a flagged question', async () => {
    const flagged = report({ kind: 'question_unclear', status: 'waiting_owner' });
    const retireReportQuestion = vi.fn(async () => ({ report_id: 'r-1', status: 'retired' }));
    await show({ listAdminReports: async () => ({ reports: [flagged] }), retireReportQuestion });
    await waitFor(() => expect(screen.getByRole('button', { name: 'Retire the question' })).toBeTruthy());
    fireEvent.click(screen.getByRole('button', { name: 'Retire the question' }));
    await waitFor(() => expect(retireReportQuestion).toHaveBeenCalledWith('r-1'));
  });

  it('says so when no report waits', async () => {
    await show({ listAdminReports: async () => ({ reports: [] }) });
    await waitFor(() => expect(screen.getByText(REPORTS_EMPTY)).toBeTruthy());
    expect(screen.getByRole('heading', { name: REPORTS_TITLE })).toBeTruthy();
  });

  it('shows nothing for an account that is not an operator', async () => {
    const listAdminReports = vi.fn(async () => { throw new ApiError(403, 'forbidden', 'no'); });
    await show({ listAdminReports });
    await waitFor(() => expect(listAdminReports).toHaveBeenCalled());
    await waitFor(() => expect(screen.queryByRole('heading', { name: REPORTS_TITLE })).toBeNull());
  });
});
