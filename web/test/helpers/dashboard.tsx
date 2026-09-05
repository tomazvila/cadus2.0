/**
 * The fixtures and the moves of the dashboard tests.
 *
 * The status fixture is the frozen `GET /api/status` contract of the web-service spec, so
 * every number in the parts is a literal a reader checks by hand: 12 of 40 XP is 30
 * percent, and a course progress of 0.18 is 18 percent.
 */
import { vi } from 'vitest';
import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createDemoApi } from '@/api';
import { DialogProvider } from '@/components/Modal';
import { Dashboard, type DashboardProps } from '@/views/Dashboard';
import { resetToasts } from '@/app/toast';
import { renderInView } from './render';
import type { ApiClient, StatusResponse } from '@/api/types';

const STATUS: StatusResponse = {
  course: { id: 'foundations', name: 'Foundations' },
  placed: true,
  courses: [
    { id: 'foundations', name: 'Foundations', current: true },
    { id: 'proofs', name: 'Proofs', current: false },
  ],
  test_prep: null,
  xp: { total: 340, today: 12, goal: 40, streak_days: 3 },
  velocity: {
    xp_per_day_28d: 21.5,
    topics_per_week_28d: 2.25,
    course_progress: 0.18,
    eta: '2026-11-04',
  },
  quiz: { last_at: null, xp_since: 0, retake_pending: false },
  pending_remediation: [],
  quiz_due: false,
  drill_due: false,
  frontier: 4,
  due_reviews: 2,
  nearly_due: 1,
};

export const status = (over: Partial<StatusResponse> = {}): StatusResponse => ({ ...STATUS, ...over });

/** Nothing scheduled: no review, no nearly-due review, no frontier, no quiz, no drill. */
export const EMPTY_PLAN: Partial<StatusResponse> = {
  due_reviews: 0,
  nearly_due: 0,
  frontier: 0,
  quiz_due: false,
  drill_due: false,
};

/** One course, and it is the current one — so no next course takes the primary slot. */
export const ONE_COURSE = [{ id: 'foundations', name: 'Foundations', current: true }];

/**
 * A client built from the demo backend, so every method of `ApiClient` exists and a missing
 * override is a type error rather than a `not a function` inside a handler.
 */
export function stubApi(over: Partial<ApiClient> = {}): ApiClient {
  return { ...createDemoApi(), getStatus: async () => status(), ...over };
}

const nav = () => ({
  onUnauthorized: vi.fn(),
  onSession: vi.fn(),
  onQuiz: vi.fn(),
  onDiagnostic: vi.fn(),
  onMap: vi.fn(),
});

/** Mount into the `<main>` the shell owns, and settle the status fetch. */
export async function mount(over: Partial<DashboardProps> = {}) {
  resetToasts();
  const handlers = nav();
  const props: DashboardProps = { api: stubApi(), demo: false, ...handlers, ...over };
  const view = await renderInView(
    <DialogProvider>
      <Dashboard {...props} />
    </DialogProvider>,
  );
  return { ...view, ...handlers };
}

export const primaries = () => document.querySelectorAll('.view-dashboard .btn-primary');
const actionBlock = () =>
  document.querySelector<HTMLElement>('.primary-action, .onboard-card')!;

/**
 * Press the diagnostic control in the OPEN — in the action block, never inside the quiet
 * disclosure — and count the navigations it made.
 */
export async function pressInTheOpen(name: string, onDiagnostic: ReturnType<typeof vi.fn>) {
  const cta = within(actionBlock()).getByRole('button', { name });
  const insideDetails = cta.closest('details') !== null;
  await userEvent.click(cta);
  return { insideDetails, calls: onDiagnostic.mock.calls.length };
}

/** Open the quiet menu and the course picker behind "Switch course". */
export async function openPicker() {
  const user = userEvent.setup();
  const enroll = vi.fn(createDemoApi().enroll);
  await mount({ api: stubApi({ enroll }) });
  await user.click(screen.getByText('More'));
  await user.click(screen.getByRole('button', { name: 'Switch course' }));
  return { user, enroll };
}

/** Open the quiet menu and press one of its buttons. */
export async function pressInMenu(name: string): Promise<void> {
  await userEvent.click(screen.getByText('More'));
  await userEvent.click(screen.getByRole('button', { name }));
}

/** A status read the test settles by hand, one attempt at a time. */
export function heldStatus() {
  const held: Array<{ resolve: (s: StatusResponse) => void; reject: (e: Error) => void }> = [];
  const getStatus = vi.fn(() => new Promise<StatusResponse>((resolve, reject) => {
    held.push({ resolve, reject });
  }));
  return { getStatus, held };
}
