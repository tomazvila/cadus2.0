import { act, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import { ApiError, createDemoApi } from '@/api';
import type { RetentionReportResponse } from '@/api/types';
import { toastStore } from '@/app/toast';
import { mount, status, stubApi } from './helpers/dashboard';
import { held } from './helpers/held';

it('shows zero confirmations without highlighting the count when all practiced work is confirmed', async () => {
  const mastery = { total: 11, practiced: 8, inferred: 3, to_confirm: [] };
  const view = await mount({ api: stubApi({ getStatus: async () => status({ mastery }) }) });
  const tile = screen.getByText('to confirm').closest('.stat')!;
  expect(within(tile as HTMLElement).getByText('0')).toBeTruthy();
  expect(tile.className).toBe('stat');
  expect(view.container.querySelectorAll('.mastery-grid .stat')).toHaveLength(3);
  expect(view.container.querySelector('.mastery-grid')?.textContent).toContain('8practiced');
});

it('offers another on-demand retention read after a failure and shows the successful report', async () => {
  const pending = held<RetentionReportResponse>();
  const getRetentionReport = vi.fn<() => Promise<RetentionReportResponse>>()
    .mockRejectedValueOnce(new ApiError(503, 'unavailable', 'Report temporarily unavailable.'))
    .mockReturnValueOnce(pending.promise);
  await mount({ api: stubApi({ getRetentionReport }) });
  await userEvent.click(screen.getByText('More'));
  await userEvent.click(screen.getByRole('button', { name: 'Load the report' }));
  expect(toastStore.getSnapshot()[0].message).toBe('Report temporarily unavailable.');
  expect(screen.queryByRole('table')).toBeNull();

  await userEvent.click(screen.getByRole('button', { name: 'Load the report' }));
  expect(screen.getByText('Reading the retention report…')).toBeTruthy();
  expect(screen.queryByRole('button', { name: 'Load the report' })).toBeNull();
  await act(async () => { pending.release(await createDemoApi().getRetentionReport()); });
  expect(getRetentionReport).toHaveBeenCalledTimes(2);
  expect(screen.getByRole('table')).toBeTruthy();
  expect(screen.getByRole('rowheader', { name: 'Every delay' })).toBeTruthy();
  expect(screen.queryByText('Reading the retention report…')).toBeNull();
  expect(screen.queryByRole('button', { name: 'Load the report' })).toBeNull();
});
