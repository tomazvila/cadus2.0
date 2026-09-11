/** Integrated input, sparse receipts, and request recovery contracts. */
import { act, fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { ApiError } from '@/api';
import type { IntegratedApi, IntegratedGrade, IntegratedProblem } from '@/api/types';
import { Integrated } from '@/views/session/Integrated';
import { PROBLEM, gradeReply } from './helpers/integrated';

function setup(receipt: IntegratedGrade = gradeReply(), problem: IntegratedProblem = PROBLEM) {
  const api: IntegratedApi = {
    taskIntegratedAnswer: vi.fn(async () => receipt),
    taskIntegratedHint: vi.fn(async () => ({
      field: 'work', index: 0, hint: 'Find the total workload.', hints_used: 1, hints_available: 2,
    })),
  };
  const onUnauthorized = vi.fn();
  const view = render(<Integrated api={api} taskId="integrated" problem={problem} onUnauthorized={onUnauthorized} />);
  return { api, onUnauthorized, ...view };
}

async function click(name: string) {
  await act(async () => { fireEvent.click(screen.getByRole('button', { name })); });
}

it('submits the chosen method and preserves the learner reasoning verbatim', async () => {
  const { api } = setup();
  fireEvent.click(screen.getByLabelText('Divide the person-minutes by the minutes of one nurse.'));
  fireEvent.change(screen.getByLabelText(/Your reasoning/), { target: { value: '  Count work, then divide.  ' } });
  await click('Submit the whole task');
  expect(api.taskIntegratedAnswer).toHaveBeenCalledWith('integrated', expect.objectContaining({
    method: 'person-minutes', reasoning: '  Count work, then divide.  ',
  }));
});

it('shows a missing step receipt without inventing its verdict and labels blank answers', async () => {
  const receipt = gradeReply({ steps: [], method: null, reasoning: { recorded: false, graded: false, note: null } });
  receipt.final.answered = false;
  receipt.final.correct = false;
  setup(receipt, { ...PROBLEM, method: null, final_ask: { prompt: 'How many nurses?', unit: null, hints_available: 0 } });
  await click('Submit the whole task');
  expect(document.querySelector('.integrated-verdict-value')?.textContent).toBe('');
  expect(screen.getByText('not answered')).toBeTruthy();
  expect(screen.getByText('You wrote no note.')).toBeTruthy();
  expect(screen.queryByText(/Method:/)).toBeNull();
});

it('shows an incorrect selected method even when the receipt supplies no explanation', async () => {
  setup(gradeReply({ method: { chosen: 'patients-per-hour', correct: false, why: null } }));
  await click('Submit the whole task');
  expect(screen.getByText('Method: not correct')).toBeTruthy();
});

it.each([
  ['network', new Error('offline'), 0],
  ['service', new ApiError(503, 'unavailable', 'Unavailable'), 0],
  ['expired', new ApiError(401, 'unauthorized', 'Expired'), 1],
] as const)('recovers from a %s hint failure without spending a rung', async (_name, error, authCalls) => {
  const { api, onUnauthorized } = setup();
  vi.mocked(api.taskIntegratedHint).mockRejectedValueOnce(error);
  await click('Hint (0/2)');
  expect(screen.getByText('The hint did not arrive. Try again.')).toBeTruthy();
  expect(onUnauthorized).toHaveBeenCalledTimes(authCalls);
  await click('Hint (0/2)');
  expect(screen.getByText('Find the total workload.')).toBeTruthy();
  expect(api.taskIntegratedHint).toHaveBeenNthCalledWith(2, 'integrated', { field: 'work', index: 0 });
});

it('keeps submission retryable after a service error and omits whitespace-only reasoning', async () => {
  const { api, onUnauthorized } = setup();
  vi.mocked(api.taskIntegratedAnswer).mockRejectedValueOnce(new ApiError(503, 'unavailable', 'Unavailable'));
  fireEvent.change(screen.getByLabelText(/Your reasoning/), { target: { value: '   ' } });
  await click('Submit the whole task');
  expect(screen.getByText('The submission did not reach the service. Try again.')).toBeTruthy();
  await click('Submit the whole task');
  expect(screen.getByText('Answers')).toBeTruthy();
  expect(onUnauthorized).not.toHaveBeenCalled();
  expect(vi.mocked(api.taskIntegratedAnswer).mock.calls[1]?.[1]).not.toHaveProperty('reasoning');
  expect(screen.queryByText('The submission did not reach the service. Try again.')).toBeNull();
});
