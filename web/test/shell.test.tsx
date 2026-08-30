/**
 * The app shell and the topbar (S4).
 *
 * The shell is the part of the page no screen owns: the bar, the dialog host, the error
 * boundary, and the toast host. Its acceptance is accessibility — this is the frame every
 * later screen renders inside, so a landmark or a focus fault here is a fault on every
 * screen at once.
 *
 * Everything renders into the document `index.html` ships: `#topbar` is the `<header>` the
 * bar portals into, `#view` is the `<main>` the root mounts on, and `#toasts` is the live
 * region. `test/setup.ts` seeds all three. A test that mounts into a bare `<div>` instead
 * would report a clean axe run while the real page has no landmarks at all.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { App } from '@/app/App';
import { Topbar } from '@/app/Topbar';
import { AXE_IN_JSDOM } from './axe';
import { allowConsoleError } from './setup';
import type { User } from '@/api/types';

const USER: User = {
  id: 'u1',
  email: 'learner@example.com',
  email_verified: true,
  created_at: '2026-08-30T00:00:00Z',
};

/** Mount into the `<main>` the shell actually uses. */
const mount = (node: React.ReactElement) =>
  render(node, { container: document.getElementById('view')! });

const topbar = () => document.getElementById('topbar')!;

describe('the topbar', () => {
  it('portals into the header, not into the view', () => {
    // `<header id="topbar">` is the banner landmark and it exists before the first render.
    // Rendered inside `#view` it would sit inside `<main>`, and the landmark would move.
    mount(<Topbar user={null} demo={false} onHome={vi.fn()} onMap={vi.fn()} onLogout={vi.fn()} />);
    expect(topbar().querySelector('.brand')).not.toBeNull();
    expect(document.getElementById('view')!.querySelector('.brand')).toBeNull();
  });

  it('offers the brand alone while signed out', () => {
    mount(<Topbar user={null} demo={false} onHome={vi.fn()} onMap={vi.fn()} onLogout={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Cadus' })).toBeTruthy();
    // A Map link before sign-in is a dead end: the graph route needs a session.
    expect(screen.queryByRole('button', { name: 'Map' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Log out' })).toBeNull();
    expect(topbar().querySelector('.demo-badge')).toBeNull();
  });

  it('shows the address and the way out while signed in', () => {
    mount(<Topbar user={USER} demo={false} onHome={vi.fn()} onMap={vi.fn()} onLogout={vi.fn()} />);
    expect(screen.getByText('learner@example.com')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Map' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Log out' })).toBeTruthy();
  });

  it('renders the address in the mono face, truncated, with the full value in the title', () => {
    mount(<Topbar user={USER} demo={false} onHome={vi.fn()} onMap={vi.fn()} onLogout={vi.fn()} />);
    const email = topbar().querySelector('.user-email')!;
    // `.user-email` truncates rather than pushing Log out off a narrow bar, so the whole
    // address has to stay reachable somewhere.
    expect(email.className).toBe('user-email mono');
    expect(email.getAttribute('title')).toBe('learner@example.com');
  });

  it('badges demo mode and offers no sign-out there', () => {
    mount(<Topbar user={null} demo onHome={vi.fn()} onMap={vi.fn()} onLogout={vi.fn()} />);
    expect(topbar().querySelector('.demo-badge')?.textContent).toBe('DEMO');
    expect(screen.getByRole('button', { name: 'Map' })).toBeTruthy();
    // Demo has no session, so a Log out button would report a failure it invented.
    expect(screen.queryByRole('button', { name: 'Log out' })).toBeNull();
  });

  it('calls back on the brand, the map, and the sign-out', () => {
    const onHome = vi.fn();
    const onMap = vi.fn();
    const onLogout = vi.fn();
    mount(<Topbar user={USER} demo={false} onHome={onHome} onMap={onMap} onLogout={onLogout} />);

    act(() => { screen.getByRole('button', { name: 'Cadus' }).click(); });
    act(() => { screen.getByRole('button', { name: 'Map' }).click(); });
    act(() => { screen.getByRole('button', { name: 'Log out' }).click(); });

    expect(onHome).toHaveBeenCalledTimes(1);
    expect(onMap).toHaveBeenCalledTimes(1);
    expect(onLogout).toHaveBeenCalledTimes(1);
  });

  it('renders nothing at all when the host element is missing', () => {
    // A host that returns null must not throw: the bar is not the reason to lose the page.
    topbar().remove();
    expect(() => {
      mount(<Topbar user={USER} demo={false} onHome={vi.fn()} onMap={vi.fn()} onLogout={vi.fn()} />);
    }).not.toThrow();
  });
});

describe('the app shell', () => {
  it('mounts the bar, the view, and the toast host together', () => {
    mount(<App user={USER} />);
    expect(topbar().querySelector('.brand')).not.toBeNull();
    expect(screen.getByRole('heading', { level: 1 }).textContent).toBe('Cadus');
    // The host is mounted for the life of the app, so a toast raised after a view leaves
    // still lands. It is empty here, and that is the point: the region predates its content.
    expect(document.getElementById('toasts')!.getAttribute('aria-live')).toBe('polite');
  });

  it('renders the routed view in place of the placeholder', () => {
    mount(<App user={USER}><h1>Dashboard</h1></App>);
    expect(screen.getByRole('heading', { level: 1 }).textContent).toBe('Dashboard');
  });

  it('reports zero axe violations on the shell', async () => {
    mount(<App user={USER} />);
    // Over the whole document: the bar, the view, and the live region are three siblings,
    // and the landmark structure is only assertable across all three.
    expect(await axe(document.body, AXE_IN_JSDOM)).toHaveNoViolations();
  });

  it('reports zero axe violations in demo mode', async () => {
    mount(<App demo />);
    expect(await axe(document.body, AXE_IN_JSDOM)).toHaveNoViolations();
  });

  it('keeps the bar while a screen crashes, and offers the way back', () => {
    allowConsoleError(/view crashed/);
    allowConsoleError(/The above error occurred/);
    allowConsoleError(/An error occurred in/);

    function Boom(): React.ReactElement { throw new Error('the grade continuation threw'); }
    mount(<App user={USER}><Boom /></App>);

    // The boundary catches the throw. The bar is a sibling of the boundary, so the one way
    // out of the broken screen survives it.
    expect(screen.getByRole('alert')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeTruthy();
    expect(topbar().querySelector('.brand')).not.toBeNull();
  });
});
