/**
 * The Label problem of the demo backend: the fourth problem of the demo lesson.
 *
 * The serve payload of that problem has the key `choices`, so `?demo=1` shows the answer
 * buttons with no service. Each other demo problem has no `choices` key, as on the service.
 */
import { describe, expect, it, vi } from 'vitest';
import { createDemoApi } from '@/api';
import type { ApiClient } from '@/api';

const LABEL_CHOICES = [
  'Step 3: put $x = 4$ back in the equation and find that $2 \\cdot 4 + 6 = 10$ is true',
  'Step 1: subtract 6 from each side to get $2x = 4$',
  'All three steps are valid',
  'Step 2: divide each side by 2 to get $x = 4$',
];

/** Let the reply delay of the demo elapse and give back the value. */
async function settle<T>(promise: Promise<T>): Promise<T> {
  await vi.runAllTimersAsync();
  return promise;
}

/** Answer the three typed problems, so the Label problem is the live problem. */
async function reachLabelProblem(demo: ApiClient) {
  const typed = [['demo-p1', '3/4'], ['demo-p2', '4'], ['demo-p3', '7']] as const;
  let last = null;
  for (const [problem_id, answer] of typed) {
    last = await settle(demo.taskAnswer('demo-lesson', { problem_id, answer }));
  }
  return last;
}

describe('the Label problem of the demo', () => {
  it('serves the fourth problem with the choices key, literally', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    await reachLabelProblem(demo);
    expect(await settle(demo.taskServe('demo-lesson'))).toEqual({
      problem_id: 'demo-p4',
      index: 4,
      total: 4,
      text: 'A student solves $2x + 6 = 10$ in three steps. Which step is the first step that is not valid?',
      kp: 'kp-find-the-error',
      time_budget_secs: 120,
      countdown: false,
      choices: LABEL_CHOICES,
    });
  });

  it('gives the Label problem as `next` of the third answer', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    const third = await reachLabelProblem(demo);
    expect(third).toMatchObject({ task_status: 'continue', next: { problem_id: 'demo-p4', choices: LABEL_CHOICES } });
  });

  it('SERVE-idem: gives the same option order on each serve, in a new list each time', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    await reachLabelProblem(demo);
    const first = await settle(demo.taskServe('demo-lesson'));
    // A view that changes its list does not change the list of the next serve.
    first.choices!.reverse();
    const second = await settle(demo.taskServe('demo-lesson'));
    expect(second.choices).toEqual(LABEL_CHOICES);
  });

  it('has no choices key on a typed problem', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    const served = await settle(demo.taskServe('demo-lesson'));
    expect(served.problem_id).toBe('demo-p1');
    expect('choices' in served).toBe(false);
  });

  it('grades the raw option text correct, and closes the lesson', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    await reachLabelProblem(demo);
    const reply = await settle(demo.taskAnswer('demo-lesson', { problem_id: 'demo-p4', answer: LABEL_CHOICES[3] }));
    expect(reply).toMatchObject({
      correct: true, outcome: 'correct', task_status: 'task_passed', next: null, xp: 10,
      solution: 'Step 1 is valid. Step 2 is not valid: $4 \\div 2 = 2$, so $x = 2$.',
    });
  });

  it.each([0, 1, 2])('grades option %i incorrect', async (index) => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    await reachLabelProblem(demo);
    const reply = await settle(demo.taskAnswer('demo-lesson', { problem_id: 'demo-p4', answer: LABEL_CHOICES[index] }));
    expect(reply).toMatchObject({ correct: false, outcome: 'incorrect', task_status: 'task_passed' });
  });

  it('does not put the answer in the serve payload', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    await reachLabelProblem(demo);
    const served = await settle(demo.taskServe('demo-lesson'));
    expect(Object.keys(served).sort()).toEqual([
      'choices', 'countdown', 'index', 'kp', 'problem_id', 'text', 'time_budget_secs', 'total',
    ]);
  });
});
