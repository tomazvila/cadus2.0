import { describe, expect, it } from 'vitest';

import { createIntegratedDemoApi } from '../../src/api/integrated-demo';

const fresh = 'demo-integrated-application';
const delayed = 'demo-integrated-assessment';

const submission = (hintsUsed: number) => ({
  method: 'person-minutes',
  steps: [
    { id: 'work', answer: '1440', hints_used: hintsUsed },
    { id: 'worker-minutes', answer: '240', hints_used: 0 },
  ],
  final_answer: { id: 'final', answer: '6', hints_used: 0 },
  reasoning: 'Total work divided by minutes per worker.',
});

describe('the integrated journey demo', () => {
  it('serves both application variants and refuses tasks outside the journey', async () => {
    const api = createIntegratedDemoApi();
    const plan = await api.getPlan();
    expect(plan.tasks.map(({ task_id }) => task_id)).toEqual([fresh, delayed]);
    expect(plan.tasks.map(({ integrated_assessment }) => integrated_assessment)).toEqual([
      false,
      true,
    ]);

    const teaching = await api.taskTeach(fresh);
    expect(teaching.worked_example.steps).toContain('240');
    await expect(api.taskTeach('outside-the-journey')).rejects.toMatchObject({
      status: 404,
      code: 'unknown_task',
    });

    const application = await api.taskIntegrated(fresh);
    expect(application).toMatchObject({
      item_id: 'integrated-clinic-window',
      title: 'Staff the vaccination window',
    });
    expect(application.steps[0]?.ask.hints_available).toBe(2);
    expect(application.final_ask.hints_available).toBe(1);

    const assessment = await api.taskIntegrated(delayed);
    expect(assessment).toMatchObject({
      item_id: 'integrated-food-bank-shift',
      title: 'Plan the food-bank packing shift',
    });
    expect(assessment.steps[0]?.ask.hints_available).toBe(0);
    expect(assessment.final_ask.hints_available).toBe(0);
  });

  it('reveals one application hint and keeps its persisted count', async () => {
    const api = createIntegratedDemoApi();
    const first = await api.taskIntegratedHint(fresh, { field: 'work', index: 0 });
    expect(first).toMatchObject({ hint: expect.any(String), hints_available: 2, hints_used: 1 });

    const repeated = await api.taskIntegratedHint(fresh, { field: 'work', index: 1 });
    expect(repeated).toMatchObject({ hint: null, hints_available: 2, hints_used: 1 });
    await expect(api.taskIntegratedHint(fresh, { field: 'final', index: 0 })).resolves.toMatchObject({
      hint: null,
      hints_available: 0,
      hints_used: 0,
    });
    await expect(api.taskIntegratedHint(delayed, { field: 'work', index: 0 })).resolves.toMatchObject({
      hint: null,
      hints_available: 0,
      hints_used: 0,
    });
  });

  it('moves the retention fixture from empty through application and assessment', async () => {
    const api = createIntegratedDemoApi();
    const initial = await api.getRetentionReport();
    expect(initial.integrated).toMatchObject({ served: 0, pass_rate: null });
    expect(initial.retention.by_delay[0]).toMatchObject({ probes: 0, retained_accuracy: null });

    const assisted = await api.taskIntegratedAnswer(fresh, submission(1));
    expect(assisted).toMatchObject({ item_id: 'integrated-clinic-window', assisted: true });
    expect(assisted.steps.map(({ assisted: used }) => used)).toEqual([true, false]);
    const afterApplication = await api.getRetentionReport();
    expect(afterApplication.integrated).toMatchObject({ served: 1, pass_rate: 1 });
    expect(afterApplication.retention.by_delay[0]).toMatchObject({ probes: 0 });

    const independent = await api.taskIntegratedAnswer(delayed, submission(0));
    expect(independent).toMatchObject({ item_id: 'integrated-food-bank-shift', assisted: false });
    const afterAssessment = await api.getRetentionReport();
    expect(afterAssessment.integrated).toMatchObject({ served: 2, passed: 2, pass_rate: 1 });
    expect(afterAssessment.retention.by_delay[0]).toMatchObject({
      probes: 1,
      retained_accuracy: 1,
      assistance_dependence: 0,
      mean_independent_secs: 74,
    });
    expect(afterAssessment.retention.total.probes).toBe(1);
  });
});
