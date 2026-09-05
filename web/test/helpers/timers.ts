/** Move the fake clock and let every continuation the move released settle. */
import { act } from '@testing-library/react';
import { vi } from 'vitest';

export async function tick(ms: number): Promise<void> {
  await act(async () => { await vi.advanceTimersByTimeAsync(ms); });
}
