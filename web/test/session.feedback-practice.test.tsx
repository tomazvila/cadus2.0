import { describe, expect, it, vi } from 'vitest';
import { screen } from '@testing-library/react';
import type { ApiClient } from '@/api/types';
import { P, graded, mount, press, stubApi, submitAnswer } from './helpers/session';

describe('independent practice after feedback', () => {
  it('labels supplemental work separately from the assessment count', async () => {
    const taskServe = vi.fn<ApiClient['taskServe']>().mockResolvedValue(P(21, { total: 20, feedback_practice: true }));
    await mount({ api: stubApi({ taskServe }) });
    expect(screen.getByText('Practice on your own')).toBeTruthy();
    expect(screen.queryByText('21 / 20')).toBeNull();
  });
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
    await press('Try a fresh problem →');
    expect(taskServe).toHaveBeenCalledTimes(2);
    expect(screen.queryByText('Study this worked solution.')).toBeNull();
  });

  it('after a miss offers a fresh problem as the main button, and a stop with its note', async () => {
    const taskAnswer = vi.fn<ApiClient['taskAnswer']>().mockResolvedValue(graded({
      correct: false, feedback_practice: true, solution: 'Study this worked solution.', next: P(2),
    }));
    await mount({ api: stubApi({ taskAnswer }) });
    await submitAnswer('3/4');
    const primary = screen.getByRole('button', { name: 'Try a fresh problem →' });
    expect(primary.className).toContain('btn-primary');
    expect(screen.getByRole('button', { name: 'Stop for now' }).className).toContain('btn-ghost');
    expect(screen.getByText('Your work is saved. An unfinished lesson comes back next time.')).toBeTruthy();
    expect(screen.queryByText(/Done studying|End session/)).toBeNull();
  });
});
