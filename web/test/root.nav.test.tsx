/** Browser Back inside the app, the tab title, and the top bar's current-page marks. */
import { beforeEach, describe, expect, it } from 'vitest';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Root } from '@/app/Root';
import { createDemoApi } from '@/api';
import { USER } from './helpers/fixtures';

const view = () => document.getElementById('view')!;
const topbar = () => document.getElementById('topbar')!;

beforeEach(() => {
  history.replaceState({}, '', '/');
});

describe('the shell navigation', () => {
  it('Back from the map returns to the dashboard, and the title follows', async () => {
    const person = userEvent.setup();
    render(<Root api={{ ...createDemoApi(), demo: false }} initialUser={USER} />, { container: view() });
    await waitFor(() => expect(document.title).toBe('Dashboard - Cadus'));
    expect(topbar().querySelector('.brand')!.getAttribute('aria-current')).toBe('page');
    expect(topbar().querySelector('nav')!.getAttribute('aria-label')).toBe('Main');

    await person.click(screen.getByRole('button', { name: 'Map' }));
    await waitFor(() => expect(document.title).toBe('Map - Cadus'));
    expect(history.state).toMatchObject({ view: 'map' });
    expect(screen.getByRole('button', { name: 'Map' }).getAttribute('aria-current')).toBe('page');

    act(() => { history.back(); });
    await waitFor(() => expect(document.title).toBe('Dashboard - Cadus'));
  });

  it('hides Map and Log out while a lesson is on screen', async () => {
    render(
      <Root api={{ ...createDemoApi(), demo: false }} initialUser={USER} initialView={{ name: 'session' }} />,
      { container: view() },
    );
    await waitFor(() => expect(document.title).toBe('Lesson - Cadus'));
    expect(screen.queryByRole('button', { name: 'Map' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Log out' })).toBeNull();
    expect(topbar().querySelector('.brand')).not.toBeNull();
  });

  it('titles the sign-in screen', () => {
    render(<Root api={createDemoApi()} initialUser={null} />, { container: view() });
    expect(document.title).toBe('Sign in - Cadus');
  });
});
