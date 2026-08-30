/**
 * The two operator screens (S12).
 *
 * The acceptance check of the unit, clause by clause:
 *   * a non-admin sees neither route          — REVIEW-admin, four tests;
 *   * the review list groups by KP and shows attempts and cost;
 *   * Approve posts the digest and the row leaves the pending list;
 *   * Reject requires a reason                — REVIEW-reason, three tests.
 *
 * Every number below is a literal a reader checks by hand. The queue fixture holds four
 * documents over two knowledge points: `algebra:linear` spent 2 + 4 attempts and
 * $0.0100 + $0.0300, and `arith:borrow` spent 1 attempt and $0.0050 plus one document the
 * service priced at null. So the T3 roll-up reads $0.0400 for the first and $0.0050 for the
 * second, one document over the three-attempt bound, and a $0.0450 total.
 *
 * THE NON-ADMIN OF THESE TESTS IS THE REAL ONE. The refusal is `403 forbidden` from the
 * service, because `users.is_admin` is outside the runtime role's column grants and no
 * reply the SPA reads carries the flag. The demo client refuses all five admin routes the
 * same way, which is what makes `createDemoApi()` an honest non-admin here.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axe } from 'vitest-axe';
import { ApiError, createDemoApi } from '@/api';
import { DialogProvider } from '@/components/Modal';
import { Root } from '@/app/Root';
import { adminRouteFor } from '@/app/routes';
import {
  BILL_MAX_PAGES, BILL_TRUNCATED, COST_UNAVAILABLE, OperatorScreen,
} from '@/views/admin/Ops';
import { ReviewScreen, REVIEW_EMPTY, REVIEW_UNREAD } from '@/views/admin/Review';
import { FORBIDDEN_MESSAGE, FORBIDDEN_TITLE, UNAVAILABLE_TITLE } from '@/views/admin/adminLoad';
import { SAMPLED_LINE } from '@/views/admin/GateBlock';
import { REASON_REQUIRED, REASON_TOO_LONG, usableReason } from '@/views/admin/reason';
import { ATTEMPT_ALERT, alerts, costByKp, usd } from '@/views/admin/cost';
import { groupByKp, step, walkOrder } from '@/views/admin/groups';
import { resetToasts } from '@/app/toast';
import { AXE_IN_JSDOM } from './axe';
import type {
  ApiClient,
  ContentFilter,
  OperatorFlagsResponse,
  ReviewDocument,
  ReviewItem,
  ReviewListResponse,
} from '@/api/types';

// --------------------------------------------------------------------------- //
// Fixtures
// --------------------------------------------------------------------------- //

const item = (over: Partial<ReviewItem> = {}): ReviewItem => ({
  digest: 'd1',
  kp_id: 'algebra:linear',
  kind: 'template',
  status: 'pending',
  authoring_attempts: 2,
  authoring_cost_usd: '0.0100',
  created_at: '2026-08-30T10:00:00+00:00',
  summary: 'Solve $2x + 1 = 9$ for $x$.',
  approved_templates: 1,
  bank_warning: true,
  ...over,
});

/** Four documents over two knowledge points. See the module note for the arithmetic. */
const QUEUE: ReviewListResponse = {
  items: [
    item(),
    item({
      digest: 'd2',
      authoring_attempts: 4,
      authoring_cost_usd: '0.0300',
      created_at: '2026-08-30T11:00:00+00:00',
      summary: 'Solve $5x = 20$ for $x$.',
    }),
    item({
      digest: 'd3',
      kp_id: 'arith:borrow',
      kind: 'teach',
      authoring_attempts: 1,
      authoring_cost_usd: '0.0050',
      created_at: '2026-08-30T09:00:00+00:00',
      summary: 'Borrowing across a zero.',
      approved_templates: 4,
      bank_warning: false,
    }),
    item({
      digest: 'd4',
      kp_id: 'arith:borrow',
      kind: 'hint_ladder',
      authoring_attempts: 1,
      // No `model_call_log` row priced this run. It is not free; it is unknown.
      authoring_cost_usd: null,
      created_at: '2026-08-30T08:00:00+00:00',
      summary: 'Three rungs, none naming the answer.',
      approved_templates: 4,
      bank_warning: false,
    }),
  ],
  bank_target: 3,
  limit: 200,
};

const DOC: ReviewDocument = {
  ...item({ digest: 'd2', authoring_attempts: 4, authoring_cost_usd: '0.0300' }),
  approved_at: null,
  review_reason: null,
  body: { statement: 'Solve $5x = 20$ for $x$.', answer: '4' },
  gate: {
    kp_id: 'algebra:linear',
    digest: 'd2',
    gated: true,
    exhaustive: false,
    instances_checked: 64,
    notes: ['the envelope check sampled 64 of 4096 tuples'],
  },
  instances: [
    { text: 'Solve $5x = 20$ for $x$.', answer: '4' },
    { text: 'Solve $3x = 9$ for $x$.', answer: '3' },
  ],
  instances_note: null,
  sample_instances: 8,
};

const FLAGS: OperatorFlagsResponse = {
  flags: [
    {
      kp_id: 'algebra:linear',
      approved_templates: 1,
      pool_depth: 12,
      last_source: 'template',
      last_exemplar_at: null,
      needs_template: true,
      source_exhausted: false,
    },
    {
      kp_id: 'arith:borrow',
      approved_templates: 4,
      pool_depth: 40,
      last_source: 'exemplar',
      last_exemplar_at: '2026-08-29T00:00:00+00:00',
      needs_template: false,
      source_exhausted: false,
    },
  ],
  gate: [DOC.gate!],
  gate_limit: 20,
  gate_truncated: false,
};

/**
 * The document behind one digest.
 *
 * It is digest-aware on purpose: a stub that answered ONE document for every digest would
 * let the pane show the row the reviewer just decided on, and the test that proves the row
 * left the queue would then read that pane and pass anyway.
 */
const docOf = (digest: string): ReviewDocument => {
  const row = QUEUE.items.find((i) => i.digest === digest) ?? item({ digest });
  return {
    ...DOC,
    ...row,
    gate: DOC.gate ? { ...DOC.gate, digest: row.digest, kp_id: row.kp_id } : null,
    // The digest goes in the instance text, so no assertion about the pane can be satisfied
    // by a pane still showing a different document.
    instances: DOC.instances.map((instance, i) => ({
      ...instance,
      text: `${row.digest} instance ${i + 1}: ${instance.text}`,
    })),
  };
};

/** The `403` the service answers a signed-in learner on all five admin routes. */
const forbidden = () => {
  throw new ApiError(403, 'forbidden', 'This route serves an admin account only.');
};

/**
 * A client built from the demo backend, so every `ApiClient` method exists and a missing
 * override is a type error rather than a `not a function` inside a handler.
 */
function stubApi(over: Partial<ApiClient> = {}): ApiClient {
  return {
    ...createDemoApi(),
    getOperatorFlags: async () => FLAGS,
    listContent: async () => QUEUE,
    getContent: async (digest: string) => docOf(digest),
    ...over,
  };
}

async function mountReview(api: ApiClient = stubApi(), demo = false) {
  resetToasts();
  const onUnauthorized = vi.fn();
  let view!: ReturnType<typeof render>;
  await act(async () => {
    view = render(
      <DialogProvider>
        <ReviewScreen api={api} demo={demo} onUnauthorized={onUnauthorized} />
      </DialogProvider>,
      { container: document.getElementById('view')! },
    );
  });
  return { ...view, onUnauthorized };
}

async function mountOps(api: ApiClient = stubApi()) {
  resetToasts();
  const onUnauthorized = vi.fn();
  let view!: ReturnType<typeof render>;
  await act(async () => {
    view = render(<OperatorScreen api={api} onUnauthorized={onUnauthorized} />, {
      container: document.getElementById('view')!,
    });
  });
  return { ...view, onUnauthorized };
}

/** The queue rows on screen, in render order. */
const rowButtons = () => Array.from(document.querySelectorAll<HTMLElement>('.review-row'));

/** One of the two write buttons of the review screen, as the DOM has it right now. */
const writeButton = (name: 'Approve' | 'Reject') =>
  screen.getByRole('button', { name }) as HTMLButtonElement;

/**
 * `n` priced documents of one knowledge point, with digests that start at `from`.
 *
 * Each one costs $0.0100 and spent one attempt, so a page of 200 is $2.0000 and the T3
 * alert stays off: the arithmetic of a paging test is about the count and nothing else.
 */
const page = (n: number, from: number): ReviewItem[] =>
  Array.from({ length: n }, (_, i) =>
    item({
      digest: `p${from + i}`,
      kp_id: 'algebra:linear',
      authoring_attempts: 1,
      authoring_cost_usd: '0.0100',
    }));

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
    const api = stubApi({ listContent: list, me: forbidden });

    await act(async () => {
      render(<Root api={api} initialUser={null} pathname="/review" />, {
        container: document.getElementById('view')!,
      });
    });

    expect(screen.getByRole('button', { name: 'Sign in' })).toBeTruthy();
    expect(screen.queryByText(REVIEW_EMPTY)).toBeNull();
    expect(rowButtons()).toHaveLength(0);
    // The screen never mounted, so it never asked the service anything.
    expect(list).not.toHaveBeenCalled();
  });

  it('REVIEW-admin: no signed-in screen offers a way to either route', async () => {
    const user = {
      id: 'u1',
      email: 'learner@cadus.local',
      email_verified: true,
      created_at: '2026-01-01T00:00:00+00:00',
    };
    await act(async () => {
      render(<Root api={stubApi()} initialUser={user} pathname="/" />, {
        container: document.getElementById('view')!,
      });
    });

    // The two screens are reached by URL alone (`app/routes.ts`): nothing links to them,
    // because the SPA cannot know whether this account is an operator.
    expect(document.querySelector('a[href="/ops"]')).toBeNull();
    expect(document.querySelector('a[href="/review"]')).toBeNull();
    expect(screen.queryByText('Review queue')).toBeNull();
    expect(screen.queryByText('Operator')).toBeNull();
  });

  it('routes a signed-in operator to each screen by path alone', async () => {
    const user = {
      id: 'u1',
      email: 'ops@cadus.local',
      email_verified: true,
      created_at: '2026-01-01T00:00:00+00:00',
    };
    await act(async () => {
      render(<Root api={stubApi()} initialUser={user} pathname="/ops" />, {
        container: document.getElementById('view')!,
      });
    });
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

  it('has no accessibility violation axe can see in jsdom', async () => {
    const view = await mountReview();
    expect(await axe(view.container, AXE_IN_JSDOM)).toHaveNoViolations();
  });
});

// --------------------------------------------------------------------------- //
// Approve
// --------------------------------------------------------------------------- //

describe('Approve', () => {
  it('posts the digest and the row leaves the pending list', async () => {
    const user = userEvent.setup();
    const approve = vi.fn(async (digest: string) => ({
      digest,
      status: 'approved',
      approved_at: '2026-08-30T12:00:00+00:00',
    }));
    // The second read is the truth about what is still pending.
    const list = vi
      .fn<() => Promise<ReviewListResponse>>()
      .mockResolvedValueOnce(QUEUE)
      .mockResolvedValue({ ...QUEUE, items: QUEUE.items.filter((i) => i.digest !== 'd2') });

    await mountReview(stubApi({ listContent: list, approveContent: approve }));
    expect(rowButtons()).toHaveLength(4);

    await user.click(screen.getByRole('button', { name: 'Approve' }));
    // The confirmation step: nothing is posted by the first press.
    expect(screen.getByText('Approve this document?')).toBeTruthy();
    expect(approve).not.toHaveBeenCalled();

    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Approve' }));

    await waitFor(() => expect(approve).toHaveBeenCalledWith('d2'));
    await waitFor(() => expect(rowButtons()).toHaveLength(3));
    // The row is gone from the queue, and the pane has moved to the row below it rather
    // than keeping a decided document under two live buttons.
    expect(rowButtons().map((r) => r.textContent)).not.toContain(
      'template4 attempts$0.0300Solve $5x = 20$ for $x$.',
    );
    expect(screen.getByText('d1 instance 1: Solve $5x = 20$ for $x$.')).toBeTruthy();
    expect(list).toHaveBeenCalledTimes(2);
  });

  it('C6/F5: a document that did not load leaves both writes disabled', async () => {
    const user = userEvent.setup();
    const approve = vi.fn();
    const reject = vi.fn();
    // The pane of the SECOND row fails, and the first row loads. So the screen starts with
    // two live buttons, and the failed read is the only thing that changes.
    const getContent = vi.fn(async (digest: string) => {
      if (digest === 'd1') {
        throw new ApiError(500, 'server_error', 'The document read failed.');
      }
      return docOf(digest);
    });
    await mountReview(stubApi({ getContent, approveContent: approve, rejectContent: reject }));
    await waitFor(() => expect(writeButton('Approve').disabled).toBe(false));

    await user.click(rowButtons()[1]);

    // The body, the instances and the gate are gone, and the failure block stands where they
    // were. An irreversible decision on that screen decides a document nobody read.
    await waitFor(() => expect(screen.getByText('The document read failed.')).toBeTruthy());
    expect(document.querySelector('.review-doc')).toBeNull();
    expect(writeButton('Approve').disabled).toBe(true);
    expect(writeButton('Reject').disabled).toBe(true);
    // The screen says which condition opens them again.
    expect(screen.getByText(REVIEW_UNREAD)).toBeTruthy();

    // The keyboard reads the same value: `a` and `r` open nothing and post nothing.
    await user.keyboard('a');
    await user.keyboard('r');
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(approve).not.toHaveBeenCalled();
    expect(reject).not.toHaveBeenCalled();
  });

  it('C6/F5: both writes are disabled while the document is still loading', async () => {
    let release!: (doc: ReviewDocument) => void;
    const pending = new Promise<ReviewDocument>((r) => { release = r; });
    await mountReview(stubApi({ getContent: () => pending }));

    expect(screen.getByText('Loading the document…')).toBeTruthy();
    expect(writeButton('Approve').disabled).toBe(true);
    expect(writeButton('Reject').disabled).toBe(true);

    await act(async () => { release(docOf('d2')); });
    expect(writeButton('Approve').disabled).toBe(false);
    expect(writeButton('Reject').disabled).toBe(false);
  });

  it('C6/F16: approves the digest of the row the reviewer read, not the first row', async () => {
    const user = userEvent.setup();
    const approve = vi.fn(async (digest: string) => ({
      digest,
      status: 'approved',
      approved_at: '2026-08-30T12:00:00+00:00',
    }));
    await mountReview(stubApi({ approveContent: approve }));

    // The screen opens on d2. The reviewer moves to the second row and reads d1 there.
    await user.click(rowButtons()[1]);
    expect(await screen.findByText('d1 instance 1: Solve $5x = 20$ for $x$.')).toBeTruthy();
    await waitFor(() => expect(writeButton('Approve').disabled).toBe(false));

    await user.click(writeButton('Approve'));
    const dialog = screen.getByRole('dialog');
    // The confirmation restates the digest of the document in the pane, and no other.
    expect(within(dialog).getByText('d1')).toBeTruthy();
    await user.click(within(dialog).getByRole('button', { name: 'Approve' }));

    await waitFor(() => expect(approve).toHaveBeenCalledWith('d1'));
    expect(approve).toHaveBeenCalledTimes(1);
    expect(approve).not.toHaveBeenCalledWith('d2');
  });

  it('C6/F16: rejects the digest of the row the reviewer read, not the first row', async () => {
    const user = userEvent.setup();
    const reject = vi.fn(async (digest: string) => ({ digest, status: 'rejected' }));
    await mountReview(stubApi({ rejectContent: reject }));

    await user.click(rowButtons()[1]);
    expect(await screen.findByText('d1 instance 1: Solve $5x = 20$ for $x$.')).toBeTruthy();
    await waitFor(() => expect(writeButton('Reject').disabled).toBe(false));

    await user.click(writeButton('Reject'));
    await user.type(screen.getByLabelText('Reason'), 'the second row is the wrong one');
    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Reject' }));

    await waitFor(() =>
      expect(reject).toHaveBeenCalledWith('d1', 'the second row is the wrong one'),
    );
    expect(reject).toHaveBeenCalledTimes(1);
  });

  it('posts nothing when the confirmation is cancelled', async () => {
    const user = userEvent.setup();
    const approve = vi.fn();
    await mountReview(stubApi({ approveContent: approve }));

    await user.click(screen.getByRole('button', { name: 'Approve' }));
    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Cancel' }));

    expect(approve).not.toHaveBeenCalled();
    expect(rowButtons()).toHaveLength(4);
  });
});

// --------------------------------------------------------------------------- //
// REVIEW-reason
// --------------------------------------------------------------------------- //

describe('REVIEW-reason: Reject requires a reason', () => {
  it('REVIEW-reason: an empty box posts nothing and renders the rule', async () => {
    const user = userEvent.setup();
    const reject = vi.fn();
    await mountReview(stubApi({ rejectContent: reject }));

    await user.click(screen.getByRole('button', { name: 'Reject' }));
    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Reject' }));

    expect(reject).not.toHaveBeenCalled();
    expect(screen.getByRole('alert').textContent).toBe(REASON_REQUIRED);
    // The dialog stays open, so the reviewer can act on the line they were just given.
    expect(screen.getByRole('dialog')).toBeTruthy();
  });

  it('REVIEW-reason: a box of spaces alone is no reason either', async () => {
    const user = userEvent.setup();
    const reject = vi.fn();
    await mountReview(stubApi({ rejectContent: reject }));

    await user.click(screen.getByRole('button', { name: 'Reject' }));
    await user.type(screen.getByLabelText('Reason'), '   ');
    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Reject' }));

    expect(reject).not.toHaveBeenCalled();
    expect(screen.getByRole('alert').textContent).toBe(REASON_REQUIRED);
  });

  it('REVIEW-reason: a written reason posts trimmed, and the row leaves the list', async () => {
    const user = userEvent.setup();
    const reject = vi.fn(async (digest: string) => ({ digest, status: 'rejected' }));
    const list = vi
      .fn<() => Promise<ReviewListResponse>>()
      .mockResolvedValueOnce(QUEUE)
      .mockResolvedValue({ ...QUEUE, items: QUEUE.items.filter((i) => i.digest !== 'd2') });

    await mountReview(stubApi({ listContent: list, rejectContent: reject }));

    await user.click(screen.getByRole('button', { name: 'Reject' }));
    await user.type(screen.getByLabelText('Reason'), '  the answer is in the statement  ');
    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Reject' }));

    await waitFor(() =>
      expect(reject).toHaveBeenCalledWith('d2', 'the answer is in the statement'),
    );
    await waitFor(() => expect(rowButtons()).toHaveLength(3));
  });

  it('REVIEW-reason: the rule is one function, and it counts code points', () => {
    expect(usableReason('')).toEqual({ error: REASON_REQUIRED });
    expect(usableReason('  \n ')).toEqual({ error: REASON_REQUIRED });
    expect(usableReason(' ok ')).toEqual({ reason: 'ok' });
    // 1000 characters pass; 1001 do not. An emoji is ONE character to the service, which
    // counts code points, so 1000 of them pass a bound `String.length` would refuse.
    expect(usableReason('x'.repeat(1000))).toEqual({ reason: 'x'.repeat(1000) });
    expect(usableReason('x'.repeat(1001))).toEqual({ error: REASON_TOO_LONG });
    expect(usableReason('🙂'.repeat(1000))).toEqual({ reason: '🙂'.repeat(1000) });
  });
});

// --------------------------------------------------------------------------- //
// The keyboard
// --------------------------------------------------------------------------- //

describe('the review keyboard', () => {
  it('walks the queue with j and k, across a group boundary and no further', async () => {
    const user = userEvent.setup();
    await mountReview();
    const selected = () => document.querySelector('.review-row.is-selected')!.textContent;

    expect(selected()).toContain('Solve $5x = 20$ for $x$.');
    await user.keyboard('j');
    expect(selected()).toContain('Solve $2x + 1 = 9$ for $x$.');
    // Across the group boundary, into `arith:borrow`.
    await user.keyboard('j');
    expect(selected()).toContain('Borrowing across a zero.');
    await user.keyboard('jjj');
    // It stops at the last row rather than wrapping to the top of a worked queue.
    expect(selected()).toContain('Three rungs, none naming the answer.');
    await user.keyboard('k');
    expect(selected()).toContain('Borrowing across a zero.');
  });

  it('opens the dialogs with a and r, and decides nothing by itself', async () => {
    const user = userEvent.setup();
    const approve = vi.fn();
    await mountReview(stubApi({ approveContent: approve }));

    await user.keyboard('a');
    expect(screen.getByText('Approve this document?')).toBeTruthy();
    expect(approve).not.toHaveBeenCalled();
  });

  it('ignores the letters while a reason is being typed', async () => {
    const user = userEvent.setup();
    await mountReview();
    const first = document.querySelector('.review-row.is-selected')!.textContent;

    await user.click(screen.getByRole('button', { name: 'Reject' }));
    // "jar" holds all three letters the screen binds. Typed into the box, it moves nothing
    // and opens nothing.
    await user.type(screen.getByLabelText('Reason'), 'jar: the hint names the answer');

    expect(screen.getAllByRole('dialog')).toHaveLength(1);
    expect(document.querySelector('.review-row.is-selected')!.textContent).toBe(first);
  });
});

// --------------------------------------------------------------------------- //
// The operator screen
// --------------------------------------------------------------------------- //

describe('the operator screen', () => {
  it('renders one serving-health row per knowledge point, with its state', async () => {
    await mountOps();
    const rows = Array.from(
      document.querySelectorAll('.admin-table tbody tr'),
    ).map((r) => r.textContent);

    expect(rows.slice(0, 2)).toEqual([
      'algebra:linear112templateneeds template',
      'arith:borrow440exemplarok',
    ]);
  });

  it('rolls the T3 bill up per knowledge point, most expensive first', async () => {
    await mountOps();
    const tables = document.querySelectorAll('.admin-table');
    const cost = tables[1]!;
    const rows = Array.from(cost.querySelectorAll('tbody tr')).map((r) => r.textContent);

    // 0.0100 + 0.0300 over two documents and six attempts, one of them over the bound.
    expect(rows).toEqual([
      'algebra:linear26$0.04001 over 3 attempts',
      // A null cost adds nothing, and the two documents still count.
      'arith:borrow22$0.0050—',
    ]);
    expect(cost.querySelector('tfoot')!.textContent).toBe('Total4 documents$0.0450');
  });

  it('T3/F20: the bill reads every page, so 250 stored documents total 250', async () => {
    // The service caps one read at `limit` rows. 250 documents are two reads: a full page
    // of 200, then a short page of 50, which is the page that ends the walk.
    const list = vi.fn(async (filter?: ContentFilter) => ({
      ...QUEUE,
      items: (filter?.page ?? 0) === 0 ? page(200, 0) : page(50, 200),
    }));
    await mountOps(stubApi({ listContent: list }));

    const cost = document.querySelectorAll('.admin-table')[1]!;
    // 250 documents at $0.0100 each, and every one of them counted once.
    expect(cost.querySelector('tfoot')!.textContent).toBe('Total250 documents$2.5000');
    expect(Array.from(cost.querySelectorAll('tbody tr')).map((r) => r.textContent)).toEqual([
      'algebra:linear250250$2.5000—',
    ]);
    expect(list).toHaveBeenCalledTimes(2);
    expect(list).toHaveBeenNthCalledWith(1, { page: 0 });
    expect(list).toHaveBeenNthCalledWith(2, { page: 1 });
    expect(screen.queryByText(BILL_TRUNCATED)).toBeNull();
  });

  it('T3/F20: a queue longer than the walk is said to be, never quietly cut', async () => {
    // Every page comes back full, so the walk never meets its short page and stops on its
    // own bound. A6: the numbers below the line are partial, and the line says so.
    const list = vi.fn(async (filter?: ContentFilter) =>
      ({ ...QUEUE, items: page(200, (filter?.page ?? 0) * 200) }));
    await mountOps(stubApi({ listContent: list }));

    expect(list).toHaveBeenCalledTimes(BILL_MAX_PAGES);
    expect(screen.getByText(BILL_TRUNCATED)).toBeTruthy();
    const cost = document.querySelectorAll('.admin-table')[1]!;
    expect(cost.querySelector('tfoot')!.textContent).toBe('Total5000 documents$50.0000');
  });

  it('T3/F20: one digest on two pages is one document on the bill', async () => {
    // A document stored between two reads shifts the rows under the walk, and the same
    // digest lands on both pages. It is one document, and it cost its money once.
    const list = vi.fn(async (filter?: ContentFilter) => ({
      ...QUEUE,
      items: (filter?.page ?? 0) === 0 ? page(200, 0) : page(50, 190),
    }));
    await mountOps(stubApi({ listContent: list }));

    const cost = document.querySelectorAll('.admin-table')[1]!;
    // 200 digests, then 50 of which the first 10 are already counted: 240 documents.
    expect(cost.querySelector('tfoot')!.textContent).toBe('Total240 documents$2.4000');
  });

  it('keeps the serving health when only the bill could not be read', async () => {
    await mountOps(stubApi({ listContent: forbidden }));

    expect(screen.getByText('Serving health')).toBeTruthy();
    // The panel names the failure AND repeats what the service said about it.
    expect(
      screen.getByText(`${COST_UNAVAILABLE} This route serves an admin account only.`),
    ).toBeTruthy();
    // The one wrong answer this panel can give is a table of zeroes.
    expect(document.querySelectorAll('.admin-table')).toHaveLength(1);
    expect(screen.queryByText(/\$0\.0000/)).toBeNull();
  });

  it('names a closed admin path as its own state, not as a refusal', async () => {
    const closed = () => {
      throw new ApiError(
        503,
        'admin_path_unavailable',
        'This deployment configured no admin database connection, so the review writes are closed.',
      );
    };
    await mountReview(stubApi({ listContent: closed }));

    expect(screen.getByText(UNAVAILABLE_TITLE)).toBeTruthy();
    expect(screen.queryByText(FORBIDDEN_TITLE)).toBeNull();
  });

  it('has no accessibility violation axe can see in jsdom', async () => {
    const view = await mountOps();
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
    expect(usd('0')).toBe('$0.0000');
    expect(usd('0.0300')).toBe('$0.0300');
  });

  it('orders the groups by id and the rows newest first', () => {
    const groups = groupByKp(QUEUE.items);
    expect(groups.map((g) => g.kp_id)).toEqual(['algebra:linear', 'arith:borrow']);
    expect(walkOrder(groups)).toEqual(['d2', 'd1', 'd3', 'd4']);
  });

  it('stops the walk at both ends and starts it at the top', () => {
    const order = ['d2', 'd1', 'd3'];
    expect(step(order, null, 1)).toBe('d2');
    expect(step(order, 'd2', -1)).toBe('d2');
    expect(step(order, 'd3', 1)).toBe('d3');
    expect(step([], 'd2', 1)).toBeNull();
    // A digest the reload dropped restarts the walk rather than stranding it.
    expect(step(order, 'gone', 1)).toBe('d2');
  });
});
