/**
 * Mount one screen into the `<main>` the shell owns, and settle its first effects.
 *
 * Into `#view`, the `<main>` index.html ships — never into a bare div. The topbar portals
 * into a sibling, and landmark structure is part of what every axe assertion checks.
 */
import { act, render } from '@testing-library/react';

export async function renderInView(node: React.ReactElement): Promise<ReturnType<typeof render>> {
  let view!: ReturnType<typeof render>;
  await act(async () => {
    view = render(node, { container: document.getElementById('view')! });
  });
  return view;
}
