/**
 * The two operator screens (S12), part 3: the operator screen.
 *
 * `admin.test.tsx` carries the module note and the fixtures live in
 * `test/helpers/admin.tsx`.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, cleanup, screen } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { ApiError } from '@/api';
import { BILL_MAX_PAGES, BILL_TRUNCATED, COST_EMPTY, COST_UNAVAILABLE, OPS_EMPTY } from '@/views/admin/Ops';
import { FORBIDDEN_TITLE, UNAVAILABLE_TITLE } from '@/views/admin/adminLoad';
import { AXE_IN_JSDOM } from './axe';
import {
  FLAGS, QUEUE, costTable, forbidden, mountOps, mountReview, page, stubApi,
} from './helpers/admin';
import type { ContentFilter, ReviewListResponse } from '@/api/types';

/** The rows of one table, as text. */
const rowsOf = (table: Element) =>
  Array.from(table.querySelectorAll('tbody tr')).map((r) => r.textContent);

/** A queue of `first` documents on page 0 and `rest` on page 1, starting at `from`. */
const pagedQueue = (first: number, rest: number, from: number) =>
  vi.fn(async (filter?: ContentFilter) => ({
    ...QUEUE,
    items: (filter?.page ?? 0) === 0 ? page(first, 0) : page(rest, from),
  }));

describe('the operator screen', () => {
  it('renders one serving-health row per knowledge point, with its state', async () => {
    await mountOps();
    const rows = rowsOf(document.querySelector('.admin-table')!);

    expect(rows.slice(0, 2)).toEqual([
      'algebra:linear112templateneeds template',
      'arith:borrow440exemplarok',
    ]);
  });

  it('rolls the T3 bill up per knowledge point, most expensive first', async () => {
    await mountOps();
    const cost = costTable();

    // 0.0100 + 0.0300 over two documents and six attempts, one of them over the bound.
    expect(rowsOf(cost)).toEqual([
      'algebra:linear26$0.04001 over 3 attempts',
      // A null cost adds nothing, and the two documents still count.
      'arith:borrow22$0.0050—',
    ]);
    expect(cost.querySelector('tfoot')!.textContent).toBe('Total4 documents$0.0450');
  });

  it('T3/F20: the bill reads every page, so 250 stored documents total 250', async () => {
    // The service caps one read at `limit` rows. 250 documents are two reads: a full page
    // of 200, then a short page of 50, which is the page that ends the walk.
    const list = pagedQueue(200, 50, 200);
    await mountOps(stubApi({ listContent: list }));

    const cost = costTable();
    // 250 documents at $0.0100 each, and every one of them counted once.
    expect(cost.querySelector('tfoot')!.textContent).toBe('Total250 documents$2.5000');
    expect(rowsOf(cost)).toEqual(['algebra:linear250250$2.5000—']);
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
    expect(costTable().querySelector('tfoot')!.textContent).toBe('Total5000 documents$50.0000');
  });

  it('T3/F20: one digest on two pages is one document on the bill', async () => {
    // A document stored between two reads shifts the rows under the walk, and the same
    // digest lands on both pages. It is one document, and it cost its money once.
    await mountOps(stubApi({ listContent: pagedQueue(200, 50, 190) }));

    // 200 digests, then 50 of which the first 10 are already counted: 240 documents.
    expect(costTable().querySelector('tfoot')!.textContent).toBe('Total240 documents$2.4000');
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

  it('renders a null source as a dash and an exhausted source as its own chip', async () => {
    await mountOps(stubApi({
      getOperatorFlags: async () => ({
        ...FLAGS,
        flags: [{ ...FLAGS.flags[0], last_source: null, needs_template: false, source_exhausted: true }],
      }),
    }));
    expect(document.querySelector('.admin-table tbody tr')!.textContent)
      .toBe('algebra:linear112—source exhausted');
  });

  it('says the gate read was truncated when the service says so', async () => {
    await mountOps(stubApi({ getOperatorFlags: async () => ({ ...FLAGS, gate: [], gate_truncated: true }) }));
    expect(screen.getByText('Truncated: more approved templates exist than this read gated.')).toBeTruthy();
    expect(screen.getByText('No approved template was gated by this read.')).toBeTruthy();
  });

  it('shows the bill loading beside the flags, then re-reads both on Refresh', async () => {
    let release!: (q: ReviewListResponse) => void;
    const listContent = vi.fn(() => new Promise<ReviewListResponse>((r) => { release = r; }));
    const getOperatorFlags = vi.fn(async () => FLAGS);
    await mountOps(stubApi({ listContent, getOperatorFlags }));

    expect(screen.getByText('Serving health')).toBeTruthy();
    expect(screen.getByText('Loading the authoring bill…')).toBeTruthy();
    await act(async () => { release(QUEUE); });
    expect(screen.queryByText('Loading the authoring bill…')).toBeNull();

    await act(async () => { screen.getByRole('button', { name: 'Refresh' }).click(); });
    await act(async () => { release(QUEUE); });
    expect(getOperatorFlags).toHaveBeenCalledTimes(2);
    expect(listContent).toHaveBeenCalledTimes(2);
  });

  it('reads one page and stops when the service names no limit', async () => {
    const listContent = vi.fn(async () => ({ ...QUEUE, limit: 0 }));
    await mountOps(stubApi({ listContent }));
    expect(listContent).toHaveBeenCalledTimes(1);
    expect(screen.queryByText(BILL_TRUNCATED)).toBeNull();
    expect(costTable().querySelector('tfoot')!.textContent).toBe('Total4 documents$0.0450');
  });

  it('says so when the pool and the bill are empty', async () => {
    await mountOps(stubApi({
      getOperatorFlags: async () => ({ ...FLAGS, flags: [], gate: [] }),
      listContent: async () => ({ ...QUEUE, items: [] }),
    }));
    expect(screen.getByText(OPS_EMPTY)).toBeTruthy();
    expect(screen.getByText(COST_EMPTY)).toBeTruthy();
    expect(document.querySelectorAll('.admin-table')).toHaveLength(0);
  });

  it('renders one block per gate note, each under its own digest', async () => {
    const second = { ...FLAGS.gate[0]!, digest: 'e2e2e2e2e2e2e2e2', kp_id: 'arith:borrow' };
    await mountOps(stubApi({ getOperatorFlags: async () => ({ ...FLAGS, gate: [FLAGS.gate[0]!, second] }) }));
    const heads = Array.from(document.querySelectorAll('.gate-note .gate-head .mono'));
    expect(heads.map((h) => h.textContent)).toEqual(['d2', 'e2e2e2e2e2e2']);
    expect(heads.map((h) => h.getAttribute('title'))).toEqual(['d2', 'e2e2e2e2e2e2e2e2']);
    expect(screen.queryByText('No approved template was gated by this read.')).toBeNull();
  });

  it('routes a 401 to sign-in outside demo mode, and keeps it on screen inside it', async () => {
    const unauthorized = () => { throw new ApiError(401, 'unauthorized', 'No session.'); };
    const out = await mountOps(stubApi({ getOperatorFlags: unauthorized }));
    expect(out.onUnauthorized).toHaveBeenCalledTimes(1);
    out.unmount();
    cleanup();

    const demo = await mountOps(stubApi({ getOperatorFlags: unauthorized }), true);
    expect(demo.onUnauthorized).not.toHaveBeenCalled();
    expect(screen.getByText('No session.')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeTruthy();
  });

  it('has no accessibility violation axe can see in jsdom', async () => {
    const view = await mountOps();
    expect(await axe(view.container, AXE_IN_JSDOM)).toHaveNoViolations();
  });
});
