/** Recover a task removed from a cached plan using the server's current plan. */
import { ApiError } from '@/api';
import type { ApiClient, SessionPlanResponse } from '@/api/types';

type TaskLoad<T> =
  | { kind: 'loaded'; value: T }
  | { kind: 'replanned'; plan: SessionPlanResponse };

export async function loadPlannedTask<T>(
  api: ApiClient,
  taskId: string,
  request: () => Promise<T>,
): Promise<TaskLoad<T>> {
  try {
    return { kind: 'loaded', value: await request() };
  } catch (error) {
    if (!(error instanceof ApiError)
      || !((error.status === 404 && error.code === 'unknown_task')
        || (error.status === 409 && error.code === 'task_complete'))) throw error;

    const plan = await api.getPlan();
    // An inconsistent response stays retryable without an automatic request loop.
    if (plan.tasks.some((task) => task.task_id === taskId && !task.progress.done)) throw error;
    return { kind: 'replanned', plan };
  }
}
