/** Late integrated replies must not update a departed learner or navigate them. */
import { act, fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { ApiError } from '@/api';
import type { IntegratedApi, IntegratedGrade, IntegratedHintResponse } from '@/api/types';
import { Integrated } from '@/views/session/Integrated';
import { held } from './helpers/held';
import { PROBLEM, gradeReply } from './helpers/integrated';

it.each([false, true])('ignores an answer settling after unmount (failure=%s)', async (failure) => {
  const receipt = held<IntegratedGrade>();
  const onGraded = vi.fn();
  const onUnauthorized = vi.fn();
  const api: IntegratedApi = {
    taskIntegratedHint: vi.fn(),
    taskIntegratedAnswer: vi.fn(async () => {
      const value = await receipt.promise;
      if (failure) throw new ApiError(401, 'unauthorized', 'Expired');
      return value;
    }),
  };
  const view = render(<Integrated api={api} problem={PROBLEM} taskId="late-answer"
    onGraded={onGraded} onUnauthorized={onUnauthorized} />);
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Submit the whole task' })); });
  view.unmount();
  await act(async () => { receipt.release(gradeReply()); });
  expect(api.taskIntegratedAnswer).toHaveBeenCalledTimes(1);
  expect(onGraded).not.toHaveBeenCalled();
  expect(onUnauthorized).not.toHaveBeenCalled();
  expect(screen.queryByText('Answers')).toBeNull();
});

it.each([false, true])('ignores a hint settling after unmount (failure=%s)', async (failure) => {
  const hint = held<IntegratedHintResponse>();
  const onUnauthorized = vi.fn();
  const api: IntegratedApi = {
    taskIntegratedAnswer: vi.fn(),
    taskIntegratedHint: vi.fn(async () => {
      const value = await hint.promise;
      if (failure) throw new ApiError(401, 'unauthorized', 'Expired');
      return value;
    }),
  };
  const view = render(<Integrated api={api} problem={PROBLEM} taskId="late-hint" onUnauthorized={onUnauthorized} />);
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Hint (0/2)' })); });
  view.unmount();
  await act(async () => {
    hint.release({ field: 'work', index: 0, hint: 'Late assistance', hints_used: 1, hints_available: 2 });
  });
  expect(api.taskIntegratedHint).toHaveBeenCalledTimes(1);
  expect(api.taskIntegratedAnswer).not.toHaveBeenCalled();
  expect(onUnauthorized).not.toHaveBeenCalled();
  expect(screen.queryByText('Late assistance')).toBeNull();
});
