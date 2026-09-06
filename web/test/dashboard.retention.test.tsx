/**
 * The retention card of unit f19 (D-F11): the three honesty rules of the screen.
 *
 * The card sits under the quiet disclosure and loads on demand, so every test here
 * opens "More" and presses "Load the report" first.
 */
import { describe, expect, it, vi } from 'vitest';
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { mount, stubApi } from './helpers/dashboard';
import type { RetentionReportResponse, RetentionRow } from '@/api/types';

/** One row with the counts a test names, and zeros everywhere else. */
const row = (delay_days: number, over: Partial<RetentionRow> = {}): RetentionRow => ({
  delay_days,
  probes: 0,
  retained_accuracy: null,
  assistance_dependence: null,
  mean_independent_secs: null,
  sufficient: false,
  provenance: {
    independent: 0,
    independent_correct: 0,
    correct: 0,
    assisted: 0,
    repeated: 0,
    unknown_exposure: 0,
    ungraded: 0,
  },
  ...over,
});

/** A report with the rows a test names. */
const report = (rows: RetentionRow[], total = row(0)): RetentionReportResponse => ({
  policy: {
    version: 1,
    label: 'v1 (uncalibrated)',
    calibrated: false,
    digest: 'abcdef0123456789',
    probe_delays_days: [7, 30, 90],
    min_sample: 20,
  },
  retention: { by_delay: rows, total },
  placement: { failed_confirmation: ['fractions'], awaiting_confirmation: ['ratios'] },
  integrated: { served: 3, passed: 1, failed: 1, inconclusive: 0, open: 1, pass_rate: 0.5 },
});

/** Open the quiet menu, load the report, and answer with `body`. */
async function load(body: RetentionReportResponse) {
  const getRetentionReport = vi.fn(async () => body);
  await mount({ api: stubApi({ getRetentionReport }) });
  await userEvent.click(screen.getByText('More'));
  await userEvent.click(screen.getByRole('button', { name: 'Load the report' }));
  return getRetentionReport;
}

/** The cells of the row whose header reads `label`. */
const cells = (label: string) => {
  const header = screen.getByRole('rowheader', { name: label });
  return Array.from(header.closest('tr')!.querySelectorAll('td')).map((c) => c.textContent ?? '');
};

describe('the retention card', () => {
  it('reads the report only when the learner asks for it', async () => {
    const getRetentionReport = vi.fn(async () => report([row(7)]));
    await mount({ api: stubApi({ getRetentionReport }) });
    expect(getRetentionReport).not.toHaveBeenCalled();
    await userEvent.click(screen.getByText('More'));
    expect(getRetentionReport).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('button', { name: 'Load the report' }));
    expect(getRetentionReport).toHaveBeenCalledTimes(1);
  });

  it('prints "no answer yet" for a delay with no probe, and never a zero', async () => {
    await load(report([row(7), row(30), row(90)]));
    expect(cells('7 days later')[0]).toBe('no answer yet');
    expect(cells('90 days later')[0]).toBe('no answer yet');
    expect(screen.queryByText('0%')).toBeNull();
  });

  it('marks a rate that rests on too few independent answers', async () => {
    await load(
      report([
        row(7, {
          probes: 2,
          retained_accuracy: 1,
          sufficient: false,
          provenance: { ...row(7).provenance, independent: 2, independent_correct: 2 },
        }),
      ]),
    );
    const seven = cells('7 days later');
    expect(seven[0]).toContain('100%');
    expect(seven[0]).toContain('too few to read');
    expect(seven[1]).toBe('2 of 2');
  });

  it('prints the provenance beside the rate it is excluded from', async () => {
    await load(
      report([
        row(7, {
          probes: 4,
          retained_accuracy: 0.5,
          assistance_dependence: 0.25,
          sufficient: true,
          provenance: {
            independent: 2,
            independent_correct: 1,
            correct: 3,
            assisted: 1,
            repeated: 1,
            unknown_exposure: 0,
            ungraded: 0,
          },
        }),
      ]),
    );
    const seven = cells('7 days later');
    expect(seven[0]).toBe('50%');
    expect(seven[1]).toBe('1 of 2');
    expect(seven[2]).toBe('25%');
    expect(seven[3]).toBe('1 with help · 1 repeated · 0 ungraded');
  });

  it('names the policy version, the placement error, and the integrated tasks', async () => {
    await load(report([row(7)]));
    expect(screen.getByText(/v1 \(uncalibrated\)/)).toBeTruthy();
    expect(screen.getByText(/abcdef0123456789/)).toBeTruthy();
    expect(screen.getByText(/1 topic\(s\) failed their confirmation/)).toBeTruthy();
    expect(screen.getByText(/3 served, 1 passed/)).toBeTruthy();
    expect(screen.getByText(/pass rate 50%/)).toBeTruthy();
  });
});
