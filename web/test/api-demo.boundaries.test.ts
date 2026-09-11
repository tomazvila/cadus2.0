import { expect, it, vi } from 'vitest';
import { createDemoApi } from '@/api/demo';

it('reports zero retention evidence with null rates for every planned delay and the total', async () => {
  vi.useFakeTimers();
  const pending = createDemoApi().getRetentionReport();
  await vi.runAllTimersAsync();
  const report = await pending;
  expect(report.policy).toMatchObject({ calibrated: false, min_sample: 20, probe_delays_days: [7, 30, 90] });
  expect(report.retention.by_delay.map((row) => row.delay_days)).toEqual([7, 30, 90]);
  expect(report.retention.total.delay_days).toBe(0);
  for (const row of [...report.retention.by_delay, report.retention.total]) {
    expect(row.probes).toBe(0);
    expect(row.sufficient).toBe(false);
    expect([row.retained_accuracy, row.assistance_dependence, row.mean_independent_secs]).toEqual([null, null, null]);
    expect(row.provenance).toEqual({
      independent: 0, independent_correct: 0, correct: 0, assisted: 0,
      repeated: 0, unknown_exposure: 0, ungraded: 0,
    });
  }
  expect(report.placement).toEqual({ failed_confirmation: [], awaiting_confirmation: [] });
  expect(report.integrated).toEqual({
    served: 0, passed: 0, failed: 0, inconclusive: 0, open: 0, pass_rate: null,
  });
});

it('returns the component-fallback error on every integrated operation', async () => {
  const demo = createDemoApi();
  const calls = [
    () => demo.taskIntegrated('demo-lesson'),
    () => demo.taskIntegratedHint('demo-lesson', { field: 'final', index: 0 }),
    () => demo.taskIntegratedAnswer('demo-lesson', {
      steps: [], final_answer: { id: 'final', answer: '1', hints_used: 0 },
    }),
  ];
  for (const call of calls) {
    await expect(call()).rejects.toMatchObject({
      status: 409,
      code: 'no_integrated_item',
      message: 'This task has no integrated problem; serve it part by part.',
    });
  }
});

it('protects both human-grading recovery routes with the admin-only response', async () => {
  const demo = createDemoApi();
  const recoveryCalls = [() => demo.listUngraded(), () => demo.regradeUngraded('attempt', 'correct')];
  for (const recover of recoveryCalls) {
    await expect(recover()).rejects.toMatchObject({
      status: 403, code: 'forbidden', message: 'This route serves an admin account only.',
    });
  }
});

it('reports an empty quiz receipt and the same-origin diagnosis stream URL', async () => {
  const demo = createDemoApi();
  await expect(demo.taskQuizResult('demo-lesson')).resolves.toEqual({
    inconclusive: false, score: 0, xp: 0, answers: [], practice_pending: false, practice_available: false,
  });
  expect(demo.diagnosisStreamUrl()).toBe('/api/diagnosis/stream');
});
