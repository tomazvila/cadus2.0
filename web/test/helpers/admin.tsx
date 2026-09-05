/**
 * The fixtures and the moves of the operator-screen tests.
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
import { vi } from 'vitest';
import { screen, within } from '@testing-library/react';
import type { UserEvent } from '@testing-library/user-event';
import { ApiError, createDemoApi } from '@/api';
import { DialogProvider } from '@/components/Modal';
import { OperatorScreen } from '@/views/admin/Ops';
import { ReviewScreen } from '@/views/admin/Review';
import { resetToasts } from '@/app/toast';
import { renderInView } from './render';
import type {
  ApiClient,
  OperatorFlagsResponse,
  ReviewDocument,
  ReviewItem,
  ReviewListResponse,
} from '@/api/types';

export const item = (over: Partial<ReviewItem> = {}): ReviewItem => ({
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
export const QUEUE: ReviewListResponse = {
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

/** The queue as the second read sees it: `d2` is decided and gone. */
function queueWithout(digest: string): ReviewListResponse {
  return { ...QUEUE, items: QUEUE.items.filter((i) => i.digest !== digest) };
}

/** A list read whose first reply is the queue and whose later replies drop `digest`. */
export function listThenWithout(digest: string) {
  return vi
    .fn<() => Promise<ReviewListResponse>>()
    .mockResolvedValueOnce(QUEUE)
    .mockResolvedValue(queueWithout(digest));
}

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

export const FLAGS: OperatorFlagsResponse = {
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
export const docOf = (digest: string): ReviewDocument => {
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
export const forbidden = () => {
  throw new ApiError(403, 'forbidden', 'This route serves an admin account only.');
};

/**
 * A client built from the demo backend, so every `ApiClient` method exists and a missing
 * override is a type error rather than a `not a function` inside a handler.
 */
export function stubApi(over: Partial<ApiClient> = {}): ApiClient {
  return {
    ...createDemoApi(),
    getOperatorFlags: async () => FLAGS,
    listContent: async () => QUEUE,
    getContent: async (digest: string) => docOf(digest),
    ...over,
  };
}

export async function mountReview(api: ApiClient = stubApi(), demo = false) {
  resetToasts();
  const onUnauthorized = vi.fn();
  const view = await renderInView(
    <DialogProvider>
      <ReviewScreen api={api} demo={demo} onUnauthorized={onUnauthorized} />
    </DialogProvider>,
  );
  return { ...view, onUnauthorized };
}

export async function mountOps(api: ApiClient = stubApi(), demo = false) {
  resetToasts();
  const onUnauthorized = vi.fn();
  const view = await renderInView(
    <OperatorScreen api={api} demo={demo} onUnauthorized={onUnauthorized} />,
  );
  return { ...view, onUnauthorized };
}

/** The queue rows on screen, in render order. */
export const rowButtons = () => Array.from(document.querySelectorAll<HTMLElement>('.review-row'));

/** One of the two write buttons of the review screen, as the DOM has it right now. */
export const writeButton = (name: 'Approve' | 'Reject') =>
  screen.getByRole('button', { name }) as HTMLButtonElement;

/** The button of the same name inside the open dialog. */
export const dialogButton = (name: string) =>
  within(screen.getByRole('dialog')).getByRole('button', { name });

/** Open the rejection prompt, type `reason` into it, and press its Reject. */
export async function rejectWith(user: UserEvent, reason: string): Promise<void> {
  await user.click(screen.getByRole('button', { name: 'Reject' }));
  if (reason) await user.type(screen.getByLabelText('Reason'), reason);
  await user.click(dialogButton('Reject'));
}

/**
 * `n` priced documents of one knowledge point, with digests that start at `from`.
 *
 * Each one costs $0.0100 and spent one attempt, so a page of 200 is $2.0000 and the T3
 * alert stays off: the arithmetic of a paging test is about the count and nothing else.
 */
export const page = (n: number, from: number): ReviewItem[] =>
  Array.from({ length: n }, (_, i) =>
    item({
      digest: `p${from + i}`,
      kp_id: 'algebra:linear',
      authoring_attempts: 1,
      authoring_cost_usd: '0.0100',
    }));

/** The cost table of the operator screen: the second `.admin-table`. */
export const costTable = () => document.querySelectorAll('.admin-table')[1]!;
