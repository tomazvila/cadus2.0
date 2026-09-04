/**
 * A bare React root for a component test that needs no screen around it.
 *
 * `createRoot` rather than Testing Library's `render`: the caret and first-frame tests
 * drive the root by hand, and a wrapper that flushes on its own hides the frame under test.
 */
import { act } from 'react';
import { createRoot } from 'react-dom/client';

export interface Mounted {
  container: HTMLDivElement;
  find: <T extends HTMLElement>(selector: string) => T;
  all: (selector: string) => HTMLElement[];
  unmount: () => void;
}

/** Mount `node` into a fresh div under `body`, and register the unmount in `roots`. */
export function mountRoot(node: React.ReactElement, roots: Array<() => void>): Mounted {
  const container = document.createElement('div');
  document.body.append(container);
  const root = createRoot(container);
  act(() => { root.render(node); });
  const unmount = () => { act(() => { root.unmount(); }); container.remove(); };
  roots.push(unmount);
  return {
    container,
    find: <T extends HTMLElement>(selector: string) => container.querySelector<T>(selector)!,
    all: (selector: string) => Array.from(container.querySelectorAll<HTMLElement>(selector)),
    unmount,
  };
}
