import { describe, expect, it, vi } from 'vitest';
import { screen } from '@testing-library/react';
import type { ApiClient } from '@/api/types';
import { P, graded, mount, press, stubApi, submitAnswer } from './helpers/session';

describe('independent practice after feedback', () => {
  it('keeps the solution behind a study step and re-serves the fresh problem on that step', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>()
      .mockResolvedValueOnce(P(1)).mockResolvedValue(P(2));
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>().mockResolvedValue(graded({
      feedback_practice: true, solution: 'Study this worked solution.', next: P(2),
    }));
    await mount({ api: stubApi({ taskServe, taskAnswer }) });
    await submitAnswer('3/4');
    expect(screen.getByText('Study this worked solution.')).toBeTruthy();
    expect(taskServe).toHaveBeenCalledTimes(1);
    await press('Done studying — try a fresh problem →');
    expect(taskServe).toHaveBeenCalledTimes(2);
    expect(screen.queryByText('Study this worked solution.')).toBeNull();
  });
});
