/** Placement carries an unmarked answer without a failure mark or answer disclosure. */
import { expect, it, vi } from 'vitest';
import { answerFirst, probe, stubDiag } from './helpers/placement';

it('shows Not marked and hides provider reason, expected answer, and solution', async () => {
  vi.useFakeTimers();
  const payload = {
    outcome: 'ungraded' as const,
    reason: 'Private parser detail: expected 7',
    expected: '7',
    solution: 'The answer is 7',
    next_probe: probe({ problem_id: 'd2' }),
  };
  await answerFirst({ diag: stubDiag({ diagAnswer: async () => payload }) });
  expect(document.querySelector('.feedback-ungraded .feedback-title')!.textContent).toBe('Not marked');
  expect(document.querySelector('.feedback-incorrect')).toBeNull();
  expect(document.querySelector('.feedback-mark')!.textContent).toBe('—');
  expect(document.body.textContent).not.toContain('Private parser detail');
  expect(document.body.textContent).not.toContain('The answer is 7');
});
