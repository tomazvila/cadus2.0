/** The `useCall` hook under StrictMode, with its deps and a spy on the 401 path. */
import { StrictMode } from 'react';
import { vi } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useCall, type CallDeps } from '@/hooks/useCall';
import { busy } from './api';

export function mountCall(deps: Partial<CallDeps> = {}) {
  const onUnauthorized = vi.fn();
  const view = renderHook(
    (props: CallDeps) => useCall(props),
    {
      wrapper: StrictMode,
      initialProps: { demo: false, onUnauthorized, ...deps },
    },
  );
  return { ...view, onUnauthorized };
}

/** A request that fails once with `busy()` and then answers `attempt-N`. */
export function flakyAttempts() {
  let attempts = 0;
  return vi.fn(async () => {
    attempts += 1;
    if (attempts === 1) throw busy();
    return `attempt-${attempts}`;
  });
}
