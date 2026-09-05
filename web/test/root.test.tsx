/**
 * `Root` as a ROUTER: the location, the session-expired reset, and the reset link.
 *
 * `router.test.tsx` walks the learner screens. This file covers the three faults review 1
 * found in the switch above them (M6-review-1, F8, F21, F22), and each one is a state the
 * walk never enters:
 *
 *   F8   The operator route came from a prop boot fixed once, and it beat the view name
 *        unconditionally. Home and Map on `/ops` and `/review` moved the state and left the
 *        operator screen on. The route now comes from the LOCATION, and the two controls
 *        navigate.
 *   F21  A 401 dropped the account and kept the screen, so the next sign-in on the same
 *        browser landed on the previous account's quiz and posted a serve against a task id
 *        the new account does not own.
 *   F22  The reset card is reachable for a learner who still holds a session.
 *
 * EVERY TEST DRIVES THE REAL DEMO CLIENT, as `router.test.tsx` does: `createDemoApi()` is
 * the object `?demo=1` hands the browser, and an override is added only where the demo
 * refuses the route on purpose (the five admin routes answer `403 forbidden`).
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Root } from '@/app/Root';
import { ApiError, createDemoApi } from '@/api';
import { OPS_TITLE } from '@/views/admin/Ops';
import { MAP_CANVAS_LABEL } from '@/views/map/Map';
import { USER, quizTask } from './helpers/fixtures';
import { instances } from './mocks/cytoscape';
import type { ApiClient, OperatorFlagsResponse, ReviewListResponse, User } from '@/api/types';

/** The account that signs in AFTER the 401. A different id, so the mix-up is visible. */
const NEXT_USER: User = { ...USER, id: 'u2', email: 'second@example.com' };

/** One quiz task, held by the view. The 401 must drop it. */
const QUIZ_TASK = quizTask('task-of-account-one');

/** An operator deployment with nothing in the pool. The screen renders; the rows are empty. */
const FLAGS: OperatorFlagsResponse = {
  flags: [],
  gate: [],
  gate_limit: 20,
  gate_truncated: false,
};

const QUEUE: ReviewListResponse = { items: [], bank_target: 3, limit: 200 };

/**
 * A client that answers the two admin reads.
 *
 * The demo backend refuses all five admin routes with `403 forbidden` on purpose, so the
 * demo object alone renders the refusal card. This unit is about which SCREEN is on, so the
 * two reads succeed and the operator screen renders its own body.
 */
function adminApi(over: Partial<ApiClient> = {}): ApiClient {
  return {
    ...createDemoApi(),
    demo: false,
    oauthProviders: async () => ({ providers: [] }),
    getOperatorFlags: async () => FLAGS,
    listContent: async () => QUEUE,
    ...over,
  };
}

const topbar = () => document.getElementById('topbar')!;
const view = () => document.getElementById('view')!;

/** Press Map in the bar, and wait for the canvas and its live renderer. */
async function openMapFromBar(person: ReturnType<typeof userEvent.setup>): Promise<void> {
  await person.click(screen.getByRole('button', { name: 'Map' }));
  await waitFor(() => expect(screen.getByLabelText(MAP_CANVAS_LABEL)).toBeTruthy());
  await waitFor(() => expect(instances.filter((i) => !i.destroyed)).toHaveLength(1));
}

/** Open `/ops` signed in, and wait for the operator screen. */
async function openOps(): Promise<ReturnType<typeof userEvent.setup>> {
  history.replaceState({}, '', '/ops');
  const person = userEvent.setup();
  render(<Root api={adminApi()} initialUser={USER} pathname="/ops" />, { container: view() });
  await waitFor(() => expect(screen.getByText(OPS_TITLE)).toBeTruthy());
  return person;
}

/** Open `/ops` signed in, press the brand, and land on the dashboard. */
async function leaveOpsThroughBrand(): Promise<ReturnType<typeof userEvent.setup>> {
  const person = await openOps();
  expect(view().querySelector('.view-ops')).not.toBeNull();

  await person.click(topbar().querySelector('.brand')!);

  await waitFor(() => expect(view().querySelector('.view-dashboard')).not.toBeNull());
  return person;
}

beforeEach(() => {
  // `navigate` writes the real history, and the write outlives the test.
  history.replaceState({}, '', '/');
});

describe('the operator route and the topbar', () => {
  it('leaves /ops for the dashboard when the brand is pressed', async () => {
    // F8. The route was a prop boot fixed once and it beat the view name, so Home moved the
    // state and the operator screen stayed on screen.
    await leaveOpsThroughBrand();
    expect(view().querySelector('.view-ops')).toBeNull();
    // The location moved with the screen. One source, and this is it.
    expect(window.location.pathname).toBe('/');
  });

  it('opens the map from /review and gives the dashboard back', async () => {
    // The same defect from the other operator path and the other control.
    history.replaceState({}, '', '/review');
    const person = userEvent.setup();
    render(<Root api={adminApi()} initialUser={USER} pathname="/review" />, { container: view() });

    await waitFor(() => expect(view().querySelector('.view-review')).not.toBeNull());

    await openMapFromBar(person);
    expect(view().querySelector('.view-review')).toBeNull();
    expect(window.location.pathname).toBe('/');

    await person.click(screen.getByRole('button', { name: 'Done' }));
    await waitFor(() => expect(view().querySelector('.view-dashboard')).not.toBeNull());
  });

  it('still moves the screen when the history write is blocked', async () => {
    // A non-browser host, or a blocked write: the state moves anyway, so the learner is
    // not stuck on an operator screen because the address bar refused to change.
    vi.spyOn(window.history, 'pushState').mockImplementation(() => { throw new Error('blocked'); });
    await leaveOpsThroughBrand();
    expect(view().querySelector('.view-ops')).toBeNull();
  });

  it('keeps the map, and where it came from, when Map is pressed twice', async () => {
    const person = userEvent.setup();
    render(<Root api={adminApi()} initialUser={USER} initialView={{ name: 'session' }} />, {
      container: view(),
    });
    await waitFor(() => expect(view().querySelector('.view-session')).not.toBeNull());

    await openMapFromBar(person);
    await openMapFromBar(person);

    // Done gives the SESSION back: a second press did not wrap the map around itself.
    await person.click(screen.getByRole('button', { name: 'Done' }));
    await waitFor(() => expect(view().querySelector('.view-session')).not.toBeNull());
  });

  it('gives /ops back when the browser goes Back', async () => {
    // The push has to be a real history entry, or Back leaves the app and the operator
    // reaches a blank tab instead of the page they came from.
    await leaveOpsThroughBrand();

    await act(async () => {
      history.back();
      // jsdom fires `popstate` on a later task, as a browser does.
      await new Promise((resolve) => { setTimeout(resolve, 0); });
    });

    await waitFor(() => expect(view().querySelector('.view-ops')).not.toBeNull());
    expect(window.location.pathname).toBe('/ops');
  });
});

describe('a session that expires', () => {
  it('sends the next sign-in to the dashboard, not the previous account’s quiz', async () => {
    // F21. The quiz view holds the whole `PlanTask`. A 401 that drops the account and keeps
    // the view leaves that task on screen, and the next account's first act is a serve
    // against a task id it does not own.
    const taskServe = vi.fn(async () => {
      throw new ApiError(401, 'unauthorized', 'No session.');
    });
    const login = vi.fn(async () => ({ user: NEXT_USER }));
    const api = adminApi({ taskServe, login });
    const person = userEvent.setup();

    render(
      <Root
        api={api}
        initialUser={USER}
        initialView={{ name: 'quiz', task: QUIZ_TASK, fromSession: false }}
      />,
      { container: view() },
    );

    // The mount serves the first question, the service refuses it, and the auth card is the
    // screen a caller with no session belongs on.
    await waitFor(() => expect(view().querySelector('.auth-view')).not.toBeNull());
    expect(taskServe).toHaveBeenCalledTimes(1);
    expect(taskServe).toHaveBeenCalledWith('task-of-account-one');

    await person.type(screen.getByLabelText('Email'), 'second@example.com');
    await person.type(screen.getByLabelText('Password'), 'hunter2hunter2');
    await person.click(screen.getByRole('button', { name: 'Sign in' }));

    // The dashboard, and NOT one more serve of the previous account's task.
    await waitFor(() => expect(view().querySelector('.view-dashboard')).not.toBeNull());
    expect(view().querySelector('.view-quiz')).toBeNull();
    expect(taskServe).toHaveBeenCalledTimes(1);
    expect(login).toHaveBeenCalledTimes(1);
  });
});

describe('the reset card', () => {
  it('takes the token from the URL, and only while the card is on', async () => {
    // F22. Boot keeps the single-use token in the URL until the card has it. `Root` says
    // when that moment is, so a screen shown in front of the card cannot spend it.
    const onResetTokenTaken = vi.fn();
    const api = adminApi();

    render(
      <Root
        api={api}
        initialUser={null}
        authMode="reset"
        resetToken="reset-9"
        pathname="/reset"
        onResetTokenTaken={onResetTokenTaken}
      />,
      { container: view() },
    );
    await waitFor(() => expect(screen.getByText('choose a new password')).toBeTruthy());
    expect(onResetTokenTaken).toHaveBeenCalledTimes(1);
  });

  it('leaves the token alone when the card is not the screen', async () => {
    const onResetTokenTaken = await mountWithoutTheCard({ initialUser: USER });
    expect(onResetTokenTaken).not.toHaveBeenCalled();
  });
});

/**
 * Mount `Root` with a token-taken callback and the props given, wait for the first screen,
 * and give the callback back for the assertion.
 */
async function mountWithoutTheCard(props: { initialUser: User | null; resetToken?: string }) {
  const onResetTokenTaken = vi.fn();
  render(
    <Root api={adminApi()} onResetTokenTaken={onResetTokenTaken} {...props} />,
    { container: view() },
  );
  await waitFor(() => expect(view().querySelector('.view-dashboard, .auth-view')).not.toBeNull());
  return onResetTokenTaken;
}

describe('the location', () => {
  it('pushes one entry per move, and none for the path already on', async () => {
    const push = vi.spyOn(window.history, 'pushState');
    const person = await leaveOpsThroughBrand();
    expect(push).toHaveBeenCalledTimes(1);
    expect(push).toHaveBeenCalledWith({}, '', '/');

    // Home, while home: the screen is already the dashboard, and the address bar agrees.
    await person.click(topbar().querySelector('.brand')!);
    await waitFor(() => expect(view().querySelector('.view-dashboard')).not.toBeNull());
    expect(push).toHaveBeenCalledTimes(1);
  });

  it('removes its popstate listener with the view', () => {
    const added = vi.spyOn(window, 'addEventListener');
    const removed = vi.spyOn(window, 'removeEventListener');
    const { unmount } = render(<Root api={adminApi()} initialUser={USER} />, { container: view() });
    const onPop = added.mock.calls.find(([type]) => type === 'popstate')![1];

    unmount();
    expect(removed).toHaveBeenCalledWith('popstate', onPop);
  });

  it('sends the learner home from an operator path on sign-out', async () => {
    const person = await openOps();
    await person.click(topbar().querySelector('.logout-btn')!);

    await waitFor(() => expect(view().querySelector('.auth-view')).not.toBeNull());
    expect(window.location.pathname).toBe('/');
  });
});

describe('the reset card, while a session is still open', () => {
  it('waits for the sign-out, and takes the token then', async () => {
    const onResetTokenTaken = vi.fn();
    const person = userEvent.setup();
    render(
      <Root
        api={adminApi()}
        initialUser={USER}
        authMode="reset"
        resetToken="reset-9"
        onResetTokenTaken={onResetTokenTaken}
      />,
      { container: view() },
    );
    await waitFor(() => expect(view().querySelector('.view-dashboard')).not.toBeNull());
    expect(onResetTokenTaken).not.toHaveBeenCalled();

    await person.click(topbar().querySelector('.logout-btn')!);
    await waitFor(() => expect(screen.getByText('choose a new password')).toBeTruthy());
    expect(onResetTokenTaken).toHaveBeenCalledTimes(1);
  });

  it('leaves a token alone on any other card', async () => {
    const onResetTokenTaken = await mountWithoutTheCard({ initialUser: null, resetToken: 'reset-9' });
    expect(screen.getByText('sign in')).toBeTruthy();
    expect(onResetTokenTaken).not.toHaveBeenCalled();
  });

  it('takes no token from a reset card that has none', async () => {
    const onResetTokenTaken = vi.fn();
    render(
      <Root api={adminApi()} initialUser={null} authMode="reset" onResetTokenTaken={onResetTokenTaken} />,
      { container: view() },
    );
    await waitFor(() => expect(screen.getByText('choose a new password')).toBeTruthy());
    expect(onResetTokenTaken).not.toHaveBeenCalled();
  });

  it('shows the card with nobody to tell', async () => {
    render(
      <Root api={adminApi()} initialUser={null} authMode="reset" resetToken="reset-9" />,
      { container: view() },
    );
    await waitFor(() => expect(screen.getByText('choose a new password')).toBeTruthy());
  });
});
