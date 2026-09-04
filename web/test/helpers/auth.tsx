/** The moves of the auth-card tests: a client with no provider, a StrictMode mount, and the keys. */
import { StrictMode } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { createDemoApi } from '@/api';
import type { ApiClient } from '@/api';

/**
 * A full client with the methods a test cares about replaced.
 *
 * Built from the demo client, so every one of the thirty members exists and a screen that
 * calls something unexpected fails on the assertion rather than on `undefined is not a
 * function`. `demo` is false: the demo flag drives the topbar, not the auth card. No OAuth
 * provider is advertised unless the test says so (AUTH-7).
 */
export function stub(overrides: Partial<ApiClient> = {}): ApiClient {
  return {
    ...createDemoApi(),
    demo: false,
    oauthProviders: async () => ({ providers: [] }),
    ...overrides,
  };
}

/** Mount into the `<main>` the shell ships, under StrictMode — every effect runs twice. */
export function mount(node: React.ReactElement) {
  return render(<StrictMode>{node}</StrictMode>, {
    container: document.getElementById('view')!,
  });
}

export const type = (label: string, value: string) =>
  fireEvent.change(screen.getByLabelText(label), { target: { value } });

export const press = (name: string) => fireEvent.click(screen.getByRole('button', { name }));

export const alertText = async () => (await screen.findByRole('alert')).textContent;
