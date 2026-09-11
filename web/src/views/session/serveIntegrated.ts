/** Resolve the optional integrated item before any per-component serve writes. */
import { ApiError } from '@/api';
import type { ApiClient, IntegratedProblem } from '@/api/types';

export async function serveIntegrated(api: ApiClient, taskId: string): Promise<IntegratedProblem | null> {
  try {
    return await api.taskIntegrated(taskId);
  } catch (error) {
    if (error instanceof ApiError && error.status === 409 && error.code === 'no_integrated_item') {
      return null;
    }
    throw error;
  }
}
