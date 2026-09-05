/**
 * The two operator screens (S12).
 *
 * The acceptance check of the unit, clause by clause:
 *   * a non-admin sees neither route          — REVIEW-admin, four tests;
 *   * the review list groups by KP and shows attempts and cost;
 *   * Approve posts the digest and the row leaves the pending list;
 *   * Reject requires a reason                — REVIEW-reason, three tests.
 *
 * The fixtures and their arithmetic live in `test/helpers/admin.tsx`. This part holds the
 * paths, the refusal, the queue and the pure parts; `admin.decide.test.tsx` holds the two
 * writes and the keyboard, and `admin.ops.test.tsx` the operator screen.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axe } from 'vitest-axe';
import { createDemoApi } from '@/api';
import { Root } from '@/app/Root';
import { adminRouteFor } from '@/app/routes';
import { REVIEW_EMPTY, REVIEW_UNREAD } from '@/views/admin/Review';
import { FORBIDDEN_MESSAGE, FORBIDDEN_TITLE } from '@/views/admin/adminLoad';
import { SAMPLED_LINE } from '@/views/admin/GateBlock';
import { ATTEMPT_ALERT, alerts, costByKp, usd } from '@/views/admin/cost';
import { groupByKp, step, walkOrder } from '@/views/admin/groups';
import { AXE_IN_JSDOM } from './axe';
import {
  QUEUE, docOf, forbidden, item, mountOps, mountReview, rowButtons, stubApi,
} from './helpers/admin';
import type { ReviewListResponse, User } from '@/api/types';

/** A signed-in account. The service, not the SPA, knows whether it is an operator. */
const user = (email: string): User => ({
  id: 'u1',
  email,
  email_verified: true,
  created_at: '2026-01-01T00:00:00+00:00',
});

/** Mount the router on one path, signed in as `account`. */
async function mountRootAt(pathname: string, account: User | null, api = stubApi()) {
  await act(async () => {
    render(<Root api={api} initialUser={account} pathname={pathname} />, {
      container: document.getElementById('view')!,
    });
  });
}

// --------------------------------------------------------------------------- //
// The paths
// --------------------------------------------------------------------------- //

describe('the two operator paths', () => {
  it('matches /ops and /review exactly, and one trailing slash', () => {
    expect(adminRouteFor('/ops')).toBe('ops');
    expect(adminRouteFor('/ops/')).toBe('ops');
    expect(adminRouteFor('/review')).toBe('review');
    expect(adminRouteFor('/review/')).toBe('review');
  });

  it('claims no other path, including one that starts with the same letters', () => {
    expect(adminRouteFor('/')).toBeNull();
    expect(adminRouteFor('/opsfoo')).toBeNull();
    expect(adminRouteFor('/review-notes')).toBeNull();
    expect(adminRouteFor('/session')).toBeNull();
    expect(adminRouteFor('/ops/content')).toBeNull();
  });
});

// --------------------------------------------------------------------------- //
// REVIEW-admin
// --------------------------------------------------------------------------- //

describe('REVIEW-admin: a non-admin reaches neither screen', () => {
  it('REVIEW-admin: /review renders the refusal and no queue at all', async () => {
    await mountReview(stubApi({ listContent: forbidden }));

    expect(screen.getByText(FORBIDDEN_TITLE)).toBeTruthy();
    // Our own line, not the service's: the block says what to do next, and it offers no
    // Try again, because retrying a `403` re-asks a question already answered.
    expect(screen.getByText(FORBIDDEN_MESSAGE)).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Try again' })).toBeNull();
    // Not one row, not one group heading, and neither write button.
    expect(rowButtons()).toHaveLength(0);
    expect(screen.queryByText('algebra:linear')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Approve' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Reject' })).toBeNull();
  });

  it('REVIEW-admin: /ops renders the refusal and no flags table', async () => {
    await mountOps(stubApi({ getOperatorFlags: forbidden, listContent: forbidden }));

    expect(screen.getByText(FORBIDDEN_TITLE)).toBeTruthy();
    expect(document.querySelectorAll('.admin-table')).toHaveLength(0);
    expect(screen.queryByText('Serving health')).toBeNull();
    expect(screen.queryByText('Authoring cost per knowledge point')).toBeNull();
  });

  it('REVIEW-admin: the demo client is a non-admin, so ?demo=1 rehearses the refusal', async () => {
    await mountReview(createDemoApi(), true);
    expect(screen.getByText(FORBIDDEN_TITLE)).toBeTruthy();
    expect(rowButtons()).toHaveLength(0);
  });

  it('REVIEW-admin: a signed-out visitor to either path gets the auth card', async () => {
    const list = vi.fn(async () => QUEUE);
    await mountRootAt('/review', null, stubApi({ listContent: list, me: forbidden }));

    expect(screen.getByRole('button', { name: 'Sign in' })).toBeTruthy();
    expect(screen.queryByText(REVIEW_EMPTY)).toBeNull();
    expect(rowButtons()).toHaveLength(0);
    // The screen never mounted, so it never asked the service anything.
    expect(list).not.toHaveBeenCalled();
  });

  it('REVIEW-admin: no signed-in screen offers a way to either route', async () => {
    await mountRootAt('/', user('learner@cadus.local'));

    // The two screens are reached by URL alone (`app/routes.ts`): nothing links to them,
    // because the SPA cannot know whether this account is an operator.
    expect(document.querySelector('a[href="/ops"]')).toBeNull();
    expect(document.querySelector('a[href="/review"]')).toBeNull();
    expect(screen.queryByText('Review queue')).toBeNull();
    expect(screen.queryByText('Operator')).toBeNull();
  });

  it('routes a signed-in operator to each screen by path alone', async () => {
    await mountRootAt('/ops', user('ops@cadus.local'));
    expect(screen.getByRole('heading', { name: 'Operator' })).toBeTruthy();
  });
});

// --------------------------------------------------------------------------- //
// The queue
// --------------------------------------------------------------------------- //

describe('the review queue', () => {
  it('groups the pending list by knowledge point, in id order', async () => {
    await mountReview();

    const heads = Array.from(document.querySelectorAll('.review-group-head')).map(
      (h) => h.textContent,
    );
    expect(heads).toEqual(['algebra:linear2 pending', 'arith:borrow2 pending']);
  });

  it('asks for the pending status and nothing else', async () => {
    const list = vi.fn(async () => QUEUE);
    await mountReview(stubApi({ listContent: list }));
    expect(list).toHaveBeenCalledWith({ status: 'pending' });
  });

  it('shows the attempts and the cost of every row, newest first inside a group', async () => {
    await mountReview();
    const rows = rowButtons().map((r) => r.textContent);

    expect(rows).toEqual([
      // `algebra:linear`, newest first: d2 at 11:00 before d1 at 10:00.
      'template4 attempts$0.0300Solve $5x = 20$ for $x$.',
      'template2 attempts$0.0100Solve $2x + 1 = 9$ for $x$.',
      'teach1 attempts$0.0050Borrowing across a zero.',
      // A cost the service sent as null reads as an em dash, never as $0.0000.
      'hint_ladder1 attempts—Three rungs, none naming the answer.',
    ]);
  });

  it('marks the T3 alert on the row that passed three attempts, and on no other', async () => {
    await mountReview();
    const alerting = Array.from(document.querySelectorAll('.review-row .chip-bad')).map(
      (c) => c.textContent,
    );
    expect(alerting).toEqual(['4 attempts']);
    // The same chip, in the row and in the pane: the alert class on top of the plain one.
    const chips = Array.from(document.querySelectorAll('.review-row .chip, .review-doc .chip'));
    expect(chips.find((c) => c.textContent === '4 attempts')?.className).toBe('chip chip-bad');
    expect(chips.find((c) => c.textContent === '1 attempts')?.className).toBe('chip');
    expect(document.querySelector('.review-doc .chip-bad')?.textContent).toBe('4 attempts');
  });

  it('marks the selected row for assistive technology, and no other', async () => {
    await mountReview();
    expect(rowButtons().map((r) => r.getAttribute('aria-current'))).toEqual(['true', null, null, null]);
  });

  it('styles the two writes as the primary and the plain button', async () => {
    await mountReview();
    expect(screen.getByRole('button', { name: 'Approve' }).className).toBe('btn btn-primary');
    expect(screen.getByRole('button', { name: 'Reject' }).className).toBe('btn');
    expect(screen.queryByText(REVIEW_UNREAD)).toBeNull();
  });

  it('returns to the first row when the reload drops the selected one', async () => {
    const user = userEvent.setup();
    const list = vi
      .fn<() => Promise<ReviewListResponse>>()
      .mockResolvedValueOnce(QUEUE)
      .mockResolvedValue({ ...QUEUE, items: QUEUE.items.filter((i) => i.digest !== 'd3') });
    await mountReview(stubApi({ listContent: list }));
    await user.click(rowButtons()[2]!);
    expect(document.querySelector('.review-row.is-selected')!.textContent).toContain('Borrowing');

    await user.click(screen.getByRole('button', { name: 'Refresh' }));
    await waitFor(() => expect(rowButtons()).toHaveLength(3));
    expect(document.querySelector('.review-row.is-selected')!.textContent).toContain('Solve $5x = 20$');
  });

  it('names the bank warning with the target the service sent', async () => {
    await mountReview();
    expect(
      screen.getByText('1 approved templates — under the bank target of 3.'),
    ).toBeTruthy();
    // The second knowledge point holds four, so it carries no warning.
    expect(document.querySelectorAll('.review-group .gate-warn')).toHaveLength(1);
  });

  it('says so, and offers no pane, when nothing is waiting', async () => {
    await mountReview(stubApi({ listContent: async () => ({ ...QUEUE, items: [] }) }));
    expect(screen.getByText(REVIEW_EMPTY)).toBeTruthy();
    expect(rowButtons()).toHaveLength(0);
  });

  it('opens on the first row and renders its instances above its body', async () => {
    await mountReview();

    // The first row of the first group is d2, and the pane loaded that digest.
    expect(screen.getByText('Rendered instances (2 of 8)')).toBeTruthy();
    expect(screen.getByText('d2 instance 2: Solve $3x = 9$ for $x$.')).toBeTruthy();
    // A6: the gate sampled, and the pane says which.
    expect(screen.getByText(`${SAMPLED_LINE} 64 instances checked.`)).toBeTruthy();
    expect(screen.getByText('the envelope check sampled 64 of 4096 tuples')).toBeTruthy();
  });

  it('says the queue is capped when the page is full, and only then', async () => {
    const full = await mountReview(stubApi({ listContent: async () => ({ ...QUEUE, limit: 4 }) }));
    expect(screen.getByText(/The queue is capped at 4 rows/)).toBeTruthy();
    full.unmount();
    cleanup();

    await mountReview();
    expect(screen.queryByText(/The queue is capped/)).toBeNull();
  });

  it('renders the refusal, the note of an empty instance list, and no gate', async () => {
    await mountReview(stubApi({
      getContent: async (digest) => ({
        ...docOf(digest),
        review_reason: 'the hint names the answer',
        instances: [],
        instances_note: 'the body does not compile',
        gate: null,
      }),
    }));
    expect(screen.getByText('Refused: the hint names the answer')).toBeTruthy();
    expect(screen.getByText('Rendered instances (0 of 8)')).toBeTruthy();
    expect(screen.getByText('the body does not compile')).toBeTruthy();
    expect(screen.queryByText('Gate')).toBeNull();
  });

  it('says a kind renders no instance when the service sends no note either', async () => {
    await mountReview(stubApi({
      getContent: async (digest) => ({ ...docOf(digest), instances: [], instances_note: null }),
    }));
    expect(screen.getByText('This document renders no instance.')).toBeTruthy();
  });

  it('has no accessibility violation axe can see in jsdom', async () => {
    const view = await mountReview();
    expect(await axe(view.container, AXE_IN_JSDOM)).toHaveNoViolations();
  });
});

// --------------------------------------------------------------------------- //
// The pure parts
// --------------------------------------------------------------------------- //

describe('the roll-up and the grouping', () => {
  it('counts a rejected document into the bill of its knowledge point', () => {
    const rolled = costByKp([
      item({ digest: 'a', status: 'rejected', authoring_cost_usd: '0.0200', authoring_attempts: 5 }),
      item({ digest: 'b', status: 'approved', authoring_cost_usd: '0.0100', authoring_attempts: 1 }),
    ]);
    expect(rolled).toEqual([
      { kp_id: 'algebra:linear', documents: 2, attempts: 6, cost: 0.03, alerting: 1 },
    ]);
  });

  it('alerts above three attempts, and not at three', () => {
    // T3 names the number, and `crates/worker/src/authoring/cost.rs` pins it: "alert when a
    // KP exceeds 3 authoring attempts". A pass that landed on attempt 3 is inside the bound.
    expect(ATTEMPT_ALERT).toBe(3);
    expect(alerts(item({ authoring_attempts: 2 }))).toBe(false);
    expect(alerts(item({ authoring_attempts: 3 }))).toBe(false);
    expect(alerts(item({ authoring_attempts: 4 }))).toBe(true);
  });

  it('renders an unknown cost as an em dash and a known zero as zero', () => {
    expect(usd(null)).toBe('—');
    expect(usd('')).toBe('—');
    expect(usd('abc')).toBe('—');
    expect(usd(undefined)).toBe('—');
    expect(usd('0')).toBe('$0.0000');
    expect(usd('0.0300')).toBe('$0.0300');
    expect(usd(2.5)).toBe('$2.5000');
  });

  it('breaks a tie on cost by knowledge point id', () => {
    const rolled = costByKp([
      item({ digest: 'a', kp_id: 'b:kp', authoring_cost_usd: '0.0100' }),
      item({ digest: 'b', kp_id: 'a:kp', authoring_cost_usd: '0.0100' }),
    ]);
    expect(rolled.map((r) => r.kp_id)).toEqual(['a:kp', 'b:kp']);
  });

  it('orders the groups by id and the rows newest first', () => {
    const groups = groupByKp(QUEUE.items);
    expect(groups.map((g) => g.kp_id)).toEqual(['algebra:linear', 'arith:borrow']);
    expect(walkOrder(groups)).toEqual(['d2', 'd1', 'd3', 'd4']);
    // By id, not by first appearance.
    const reversed = groupByKp([...QUEUE.items].reverse());
    expect(reversed.map((g) => g.kp_id)).toEqual(['algebra:linear', 'arith:borrow']);
  });

  it('breaks a tie on the timestamp by digest', () => {
    const groups = groupByKp([
      item({ digest: 'z', created_at: '2026-08-30T10:00:00+00:00' }),
      item({ digest: 'a', created_at: '2026-08-30T10:00:00+00:00' }),
    ]);
    expect(walkOrder(groups)).toEqual(['a', 'z']);
  });

  it('stops the walk at both ends and starts it at the top', () => {
    const order = ['d2', 'd1', 'd3'];
    expect(step(order, null, 1)).toBe('d2');
    expect(step(order, 'd2', -1)).toBe('d2');
    expect(step(order, 'd3', 1)).toBe('d3');
    expect(step([], 'd2', 1)).toBeNull();
    expect(step([], null, -1)).toBeNull();
    // A digest the reload dropped restarts the walk rather than stranding it.
    expect(step(order, 'gone', 1)).toBe('d2');
    expect(step(order, 'gone', -1)).toBe('d2');
    expect(step(order, null, -1)).toBe('d2');
    expect(step(order, 'd1', 1)).toBe('d3');
    expect(step(order, 'd1', -1)).toBe('d2');
  });
});
