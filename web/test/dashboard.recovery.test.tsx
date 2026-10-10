import { act, screen, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { ApiError, createDemoApi } from '@/api';
import type { RetentionReportResponse } from '@/api/types';
import { toastStore } from '@/app/toast';
import { mount, status, stubApi } from './helpers/dashboard';
import { held } from './helpers/held';

it('shows zero confirmations without highlighting the count when all practiced work is confirmed', async () => {
  const mastery = { total: 11, practiced: 8, inferred: 3, to_confirm: [] };
  const view = await mount({ api: stubApi({ getStatus: async () => status({ mastery }) }) });
  const tile = screen.getByText('to check again').closest('.stat')!;
  expect(within(tile as HTMLElement).getByText('0')).toBeTruthy();
  expect(tile.className).toBe('stat');
  expect(view.container.querySelectorAll('.progress-grid .stat')).toHaveLength(5);
  expect(view.container.querySelector('.progress-grid')?.textContent).toContain('8practiced');
});

it('says the retention read failed and shows the report after the toast retry', async () => {
  const pending = held<RetentionReportResponse>();
  const getRetentionReport = vi.fn<() => Promise<RetentionReportResponse>>()
    .mockRejectedValueOnce(new ApiError(503, 'unavailable', 'Report temporarily unavailable.'))
    .mockReturnValueOnce(pending.promise);
  await mount({ api: stubApi({ getRetentionReport }) });
  await screen.findByText('Your results did not load. Reload the page to try again.');
  expect(toastStore.getSnapshot()[0].message).toBe('Report temporarily unavailable.');
  expect(screen.queryByRole('table')).toBeNull();

  await act(async () => { toastStore.getSnapshot()[0].onAction?.(); });
  await act(async () => { pending.release(await createDemoApi().getRetentionReport()); });
  expect(getRetentionReport).toHaveBeenCalledTimes(2);
  expect(screen.getByRole('table')).toBeTruthy();
  expect(screen.getByRole('rowheader', { name: 'Every delay' })).toBeTruthy();
  expect(screen.queryByText('Your results did not load. Reload the page to try again.')).toBeNull();
});
