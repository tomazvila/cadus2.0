/** Production session selection, fallback, and append-only write boundaries. */
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, screen, waitFor } from '@testing-library/react';
import { ApiError } from '@/api';
import type { IntegratedGrade, IntegratedHintResponse, PlanTask } from '@/api/types';
import { held } from './helpers/held';
import { PROBLEM, gradeReply } from './helpers/integrated';
import { P, REVIEW, closed, mount, mountStrict, planOf, stubApi } from './helpers/session';

const MULTI: PlanTask = { ...REVIEW, task_id: 't-integrated', task_type: 'multi-step', component_topics: ['unit-rates'] };

function integratedApi() {
  return stubApi({
    taskIntegrated: vi.fn(async () => PROBLEM),
    taskIntegratedAnswer: vi.fn(async () => gradeReply()),
    taskServe: vi.fn(async () => P(1)),
    taskAnswer: vi.fn(),
    sessionEnd: vi.fn(async () => closed()),
  });
}

async function click(name: string) {
  await act(async () => { fireEvent.click(screen.getByRole('button', { name })); });
}

describe('integrated tasks in the production Session', () => {
  it('renders the integrated scenario once in StrictMode without a component serve', async () => {
    const api = integratedApi();
    await mountStrict(api, planOf(MULTI));
    expect(screen.getByText(PROBLEM.title)).toBeTruthy();
    expect(screen.getByLabelText('Final answer')).toBeTruthy();
    expect(api.taskIntegrated).toHaveBeenCalledTimes(1);
    expect(api.taskServe).not.toHaveBeenCalled();
    expect(api.taskAnswer).not.toHaveBeenCalled();
  });

  it('falls back once for the specific unavailable-item response', async () => {
    const api = integratedApi();
    api.taskIntegrated = vi.fn(async () => { throw new ApiError(409, 'no_integrated_item', 'Unavailable'); });
    await mountStrict(api, planOf(MULTI));
    expect(api.taskIntegrated).toHaveBeenCalledTimes(1);
    expect(api.taskServe).toHaveBeenCalledExactlyOnceWith(MULTI.task_id);
    expect(screen.getByRole('button', { name: 'Submit' })).toBeTruthy();
    expect(screen.queryByText(PROBLEM.title)).toBeNull();
  });

  it('keeps transport failures from starting a second serving path', async () => {
    const api = integratedApi();
    api.taskIntegrated = vi.fn(async () => { throw new ApiError(500, 'internal', 'Offline'); });
    await mount({ api, plan: planOf(MULTI) });
    expect(api.taskServe).not.toHaveBeenCalled();
    expect(screen.queryByRole('button', { name: 'Submit' })).toBeNull();
  });

  it('finishes the whole task once and advances to ordinary practice', async () => {
    const api = integratedApi();
    const receipt = held<IntegratedGrade>();
    api.taskIntegratedAnswer = vi.fn(() => receipt.promise);
    await mount({ api, plan: planOf(MULTI, REVIEW) });
    const submit = screen.getByRole('button', { name: 'Submit the whole task' });
    await act(async () => { submit.click(); submit.click(); });
    expect(api.taskIntegratedAnswer).toHaveBeenCalledTimes(1);
    expect(api.taskServe).not.toHaveBeenCalled();
    await act(async () => { receipt.release(gradeReply()); });
    expect(screen.getByText('Answers')).toBeTruthy();
    const next = screen.getByRole('button', { name: 'Continue' });
    await act(async () => { next.click(); next.click(); });
    expect(api.taskServe).toHaveBeenCalledExactlyOnceWith(REVIEW.task_id);
    expect(api.taskAnswer).not.toHaveBeenCalled();
    expect(api.sessionEnd).not.toHaveBeenCalled();
    expect(screen.queryByText(PROBLEM.title)).toBeNull();
  });

  it('ends a one-task session once after the learner reads the receipt', async () => {
    const api = integratedApi();
    await mount({ api, plan: planOf(MULTI) });
    await click('Submit the whole task');
    expect(api.sessionEnd).not.toHaveBeenCalled();
    await click('Continue');
    await waitFor(() => expect(api.sessionEnd).toHaveBeenCalledTimes(1));
    expect(api.taskServe).not.toHaveBeenCalled();
  });

  it('serializes hint requests with submission so the opened hint reaches the answer', async () => {
    const api = integratedApi();
    const hint = held<IntegratedHintResponse>();
    api.taskIntegratedHint = vi.fn(() => hint.promise);
    await mount({ api, plan: planOf(MULTI) });
    const hintButton = screen.getAllByRole('button', { name: /^Hint/ })[0]!;
    await act(async () => { hintButton.click(); hintButton.click(); });
    await click('Submit the whole task');
    expect(api.taskIntegratedHint).toHaveBeenCalledTimes(1);
    expect(api.taskIntegratedAnswer).not.toHaveBeenCalled();
    await act(async () => { hint.release({ field: 'work', index: 0, hint: 'Count the work.', hints_used: 1, hints_available: 2 }); });
    await click('Submit the whole task');
    expect(api.taskIntegratedAnswer).toHaveBeenCalledWith(MULTI.task_id, expect.objectContaining({
      steps: expect.arrayContaining([expect.objectContaining({ id: 'work', hints_used: 1 })]),
    }));
  });

  it('hydrates persisted hint counts and continues at the next server rung', async () => {
    const api = integratedApi();
    api.taskIntegrated = vi.fn(async () => ({ ...PROBLEM, hints_used: { work: 1 } }));
    api.taskIntegratedHint = vi.fn(async () => ({
      field: 'work', index: 1, hint: 'Now divide.', hints_used: 2, hints_available: 2,
    }));
    await mount({ api, plan: planOf(MULTI) });
    await click('Hint (1/2)');
    expect(api.taskIntegratedHint).toHaveBeenCalledWith(MULTI.task_id, { field: 'work', index: 1 });
    await click('Submit the whole task');
    expect(api.taskIntegratedAnswer).toHaveBeenCalledWith(MULTI.task_id, expect.objectContaining({
      steps: expect.arrayContaining([expect.objectContaining({ id: 'work', hints_used: 2 })]),
    }));
  });

  it('routes expired integrated submissions through session authentication', async () => {
    const api = integratedApi();
    api.taskIntegratedAnswer = vi.fn(async () => { throw new ApiError(401, 'unauthorized', 'Expired'); });
    const nav = await mount({ api, plan: planOf(MULTI) });
    await click('Submit the whole task');
    expect(nav.onUnauthorized).toHaveBeenCalledTimes(1);
    expect(api.taskServe).not.toHaveBeenCalled();
  });
});
