/**
 * The two operator screens (S12), part 2: Approve, Reject, and the keyboard.
 *
 * `admin.test.tsx` carries the module note and the fixtures live in
 * `test/helpers/admin.tsx`.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent, { type UserEvent } from '@testing-library/user-event';
import { ApiError } from '@/api';
import { REVIEW_UNREAD } from '@/views/admin/Review';
import { shortDigest } from '@/views/admin/GateBlock';
import { toastStore } from '@/app/toast';
import { REASON_REQUIRED, REASON_TOO_LONG, usableReason } from '@/views/admin/reason';
import {
  QUEUE, dialogButton, docOf, listThenWithout, mountReview, rejectWith, rowButtons, stubApi,
  writeButton,
} from './helpers/admin';
import type { ReviewDocument } from '@/api/types';

/** The approve route, answering the digest it was given. */
const approving = () => vi.fn(async (
  digest: string,
  policy: string | null,
  bank: string | null,
  curriculum: string,
  engine: string,
) => ({
  digest,
  status: 'approved',
  approved_at: '2026-08-30T12:00:00+00:00',
  approved_policy_digest: policy,
  approved_template_context_digest: bank,
  approved_curriculum_digest: curriculum,
  approved_review_engine_digest: engine,
}));

/** Press Approve, confirm it, and wait for the one post. */
async function approveSelected(user: UserEvent, approve: ReturnType<typeof vi.fn>) {
  await user.click(writeButton('Approve'));
  await user.click(dialogButton('Approve'));
  await waitFor(() => expect(approve).toHaveBeenCalledTimes(1));
}

/** The reject route, answering the digest it was given. */
const rejecting = () => vi.fn(async (digest: string) => ({ digest, status: 'rejected' }));

// --------------------------------------------------------------------------- //
// Approve
// --------------------------------------------------------------------------- //

describe('Approve', () => {
  it('binds instruction approval to the template bank shown with the document', async () => {
    const user = userEvent.setup();
    const approve = approving();
    await mountReview(stubApi({
      getContent: async (digest) => ({
        ...docOf(digest), template_context_digest: 'reviewed-bank',
      }),
      approveContent: approve,
    }));
    await approveSelected(user, approve);
    expect(approve).toHaveBeenCalledWith(
      'd2', null, 'reviewed-bank', 'curriculum-v1', 'engine-v1',
    );
  });

  it('binds approval to the finite policy shown with the reviewed document', async () => {
    const user = userEvent.setup();
    const approve = approving();
    await mountReview(stubApi({
      getContent: async (digest) => ({ ...docOf(digest), policy_digest: 'reviewed-policy' }),
      approveContent: approve,
    }));
    await approveSelected(user, approve);
    expect(approve).toHaveBeenCalledWith(
      'd2', 'reviewed-policy', null, 'curriculum-v1', 'engine-v1',
    );
  });

  it('posts the digest and the row leaves the pending list', async () => {
    const user = userEvent.setup();
    const approve = approving();
    // The second read is the truth about what is still pending.
    const list = listThenWithout('d2');

    await mountReview(stubApi({ listContent: list, approveContent: approve }));
    expect(rowButtons()).toHaveLength(4);

    await user.click(screen.getByRole('button', { name: 'Approve' }));
    // The confirmation step: nothing is posted by the first press.
    expect(screen.getByText('Approve this document?')).toBeTruthy();
    expect(approve).not.toHaveBeenCalled();

    await user.click(dialogButton('Approve'));

    await waitFor(() => expect(approve).toHaveBeenCalledWith(
      'd2', null, null, 'curriculum-v1', 'engine-v1',
    ));
    await waitFor(() => expect(rowButtons()).toHaveLength(3));
    // The row is gone from the queue, and the pane has moved to the row below it rather
    // than keeping a decided document under two live buttons.
    expect(rowButtons().map((r) => r.textContent)).not.toContain(
      'template4 attempts$0.0300Solve $5x = 20$ for $x$.',
    );
    expect(screen.getByText('d1 instance 1: Solve $5x = 20$ for $x$.')).toBeTruthy();
    expect(list).toHaveBeenCalledTimes(2);
    expect(toastStore.getSnapshot()).toEqual([
      { id: 1, message: `Approved ${shortDigest('d2')}.`, kind: 'success' },
    ]);
  });

  it('lands on the row below the one approved, not on the top of the queue', async () => {
    const user = userEvent.setup();
    const approve = approving();
    const list = listThenWithout('d1');
    await mountReview(stubApi({ listContent: list, approveContent: approve }));

    await user.click(rowButtons()[1]!);
    await waitFor(() => expect(writeButton('Approve').disabled).toBe(false));
    await approveSelected(user, approve);
    expect(approve).toHaveBeenCalledWith('d1', null, null, 'curriculum-v1', 'engine-v1');

    await waitFor(() => expect(rowButtons()).toHaveLength(3));
    expect(document.querySelector('.review-row.is-selected')!.textContent).toContain('Borrowing');
  });

  it('routes a 401 on the write to sign-in', async () => {
    const user = userEvent.setup();
    const approve = vi.fn(async () => { throw new ApiError(401, 'unauthorized', 'No session.'); });
    const view = await mountReview(stubApi({ approveContent: approve }));
    await approveSelected(user, approve);
    await waitFor(() => expect(view.onUnauthorized).toHaveBeenCalledTimes(1));
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
    const approve = approving();
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

    await waitFor(() => expect(approve).toHaveBeenCalledWith(
      'd1', null, null, 'curriculum-v1', 'engine-v1',
    ));
    expect(approve).toHaveBeenCalledTimes(1);
    expect(approve).not.toHaveBeenCalledWith('d2');
  });

  it('C6/F16: rejects the digest of the row the reviewer read, not the first row', async () => {
    const user = userEvent.setup();
    const reject = rejecting();
    await mountReview(stubApi({ rejectContent: reject }));

    await user.click(rowButtons()[1]);
    expect(await screen.findByText('d1 instance 1: Solve $5x = 20$ for $x$.')).toBeTruthy();
    await waitFor(() => expect(writeButton('Reject').disabled).toBe(false));

    await rejectWith(user, 'the second row is the wrong one');

    await waitFor(() =>
      expect(reject).toHaveBeenCalledWith('d1', 'the second row is the wrong one'),
    );
    expect(reject).toHaveBeenCalledTimes(1);
  });

  it('selects nothing after the last row is approved, and the reload starts the walk over', async () => {
    const user = userEvent.setup();
    const approve = approving();
    const list = listThenWithout('d4');
    await mountReview(stubApi({ listContent: list, approveContent: approve }));

    // Down to the last row, and approve it.
    await user.keyboard('jjj');
    expect(await screen.findByText('d4 instance 1: Solve $5x = 20$ for $x$.')).toBeTruthy();
    await waitFor(() => expect(writeButton('Approve').disabled).toBe(false));
    await user.click(writeButton('Approve'));
    await user.click(dialogButton('Approve'));

    await waitFor(() => expect(approve).toHaveBeenCalledWith(
      'd4', null, null, 'curriculum-v1', 'engine-v1',
    ));
    await waitFor(() => expect(rowButtons()).toHaveLength(3));
    // Nothing followed the last row, so the walk restarts at the top.
    expect(document.querySelector('.review-row.is-selected')!.textContent)
      .toContain('Solve $5x = 20$ for $x$.');
  });

  it('keeps the row and the selection when the write is refused', async () => {
    const user = userEvent.setup();
    const approve = vi.fn(async () => { throw new ApiError(500, 'server_error', 'Down.'); });
    const list = vi.fn(async () => QUEUE);
    await mountReview(stubApi({ listContent: list, approveContent: approve }));

    await approveSelected(user, approve);
    // No reload, and the row still stands: the failure is toasted with a Retry.
    expect(list).toHaveBeenCalledTimes(1);
    expect(rowButtons()).toHaveLength(4);
    expect(toastStore.getSnapshot()[0].label).toBe('Retry');
  });

  it('moves nothing when the write lands after the screen left', async () => {
    const user = userEvent.setup();
    let release!: () => void;
    const approve = vi.fn(async (
      _digest: string, policy: string | null, bank: string | null,
      curriculum: string, engine: string,
    ) => {
      await new Promise<void>((r) => { release = r; });
      return {
        digest: 'd2', status: 'approved', approved_at: null,
        approved_policy_digest: policy, approved_template_context_digest: bank,
        approved_curriculum_digest: curriculum, approved_review_engine_digest: engine,
      };
    });
    const list = vi.fn(async () => QUEUE);
    const view = await mountReview(stubApi({ listContent: list, approveContent: approve }));

    await approveSelected(user, approve);

    view.unmount();
    await act(async () => { release(); });
    expect(list).toHaveBeenCalledTimes(1);
    expect(toastStore.getSnapshot()).toEqual([]);
  });

  it('posts nothing when the confirmation is cancelled', async () => {
    const user = userEvent.setup();
    const approve = vi.fn();
    await mountReview(stubApi({ approveContent: approve }));

    await user.click(screen.getByRole('button', { name: 'Approve' }));
    await user.click(dialogButton('Cancel'));

    expect(screen.queryByRole('dialog')).toBeNull();
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

    await rejectWith(user, '');

    expect(reject).not.toHaveBeenCalled();
    expect(screen.getByRole('alert').textContent).toBe(REASON_REQUIRED);
    // The dialog stays open, so the reviewer can act on the line they were just given.
    expect(screen.getByRole('dialog')).toBeTruthy();
  });

  it('REVIEW-reason: the rule clears as soon as the reviewer types, and Cancel posts nothing', async () => {
    const user = userEvent.setup();
    const reject = vi.fn();
    await mountReview(stubApi({ rejectContent: reject }));

    await rejectWith(user, '');
    expect(screen.getByRole('alert')).toBeTruthy();
    await user.type(screen.getByLabelText('Reason'), 'a');
    expect(screen.queryByRole('alert')).toBeNull();

    await user.click(dialogButton('Cancel'));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(reject).not.toHaveBeenCalled();
  });

  it('REVIEW-reason: a box of spaces alone is no reason either', async () => {
    const user = userEvent.setup();
    const reject = vi.fn();
    await mountReview(stubApi({ rejectContent: reject }));

    await rejectWith(user, '   ');

    expect(reject).not.toHaveBeenCalled();
    expect(screen.getByRole('alert').textContent).toBe(REASON_REQUIRED);
  });

  it('REVIEW-reason: the prompt opens with no rule on screen', async () => {
    const user = userEvent.setup();
    await mountReview();
    await user.click(screen.getByRole('button', { name: 'Reject' }));
    expect(screen.getByRole('dialog')).toBeTruthy();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('REVIEW-reason: a written reason posts trimmed, and the row leaves the list', async () => {
    const user = userEvent.setup();
    const reject = rejecting();
    const list = listThenWithout('d2');

    await mountReview(stubApi({ listContent: list, rejectContent: reject }));

    await rejectWith(user, '  the answer is in the statement  ');

    await waitFor(() =>
      expect(reject).toHaveBeenCalledWith('d2', 'the answer is in the statement'),
    );
    await waitFor(() => expect(rowButtons()).toHaveLength(3));
    expect(toastStore.getSnapshot()).toEqual([
      { id: 1, message: `Rejected ${shortDigest('d2')}.`, kind: 'info' },
    ]);
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

  it('opens the rejection prompt with r', async () => {
    const user = userEvent.setup();
    await mountReview();
    await user.keyboard('r');
    expect(screen.getByText('Reject this document?')).toBeTruthy();
  });

  it('leaves the letters to the browser under a modifier, and to a field that has focus', async () => {
    const user = userEvent.setup();
    await mountReview();
    const selected = () => document.querySelector('.review-row.is-selected')!.textContent;
    const first = selected();

    await user.keyboard('{Control>}j{/Control}');
    await user.keyboard('{Meta>}j{/Meta}');
    await user.keyboard('{Alt>}j{/Alt}');
    expect(selected()).toBe(first);

    // A text input and an editable region both own their letters.
    const input = document.createElement('input');
    document.body.append(input);
    input.focus();
    await user.keyboard('j');
    expect(selected()).toBe(first);

    const editable = document.createElement('div');
    editable.setAttribute('contenteditable', 'true');
    Object.defineProperty(editable, 'isContentEditable', { get: () => true });
    document.body.append(editable);
    editable.focus();
    await user.keyboard('a');
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(selected()).toBe(first);
  });

  it('swallows the letters it acts on, and no other', async () => {
    await mountReview();
    await waitFor(() => expect(writeButton('Approve').disabled).toBe(false));
    expect(fireEvent.keyDown(document.body, { key: 'x' })).toBe(true);
    expect(fireEvent.keyDown(document.body, { key: 'a' })).toBe(false);
    await waitFor(() => expect(screen.getByRole('dialog')).toBeTruthy());
    await act(async () => { dialogButton('Cancel').click(); });
    let swallowed = false;
    // The move loads the next pane, so the press is awaited like the click that it stands for.
    await act(async () => { swallowed = !fireEvent.keyDown(document.body, { key: 'j' }); });
    expect(swallowed).toBe(true);
  });

  it('leaves the letters to a text area that has focus', async () => {
    const user = userEvent.setup();
    await mountReview();
    const first = document.querySelector('.review-row.is-selected')!.textContent;
    const area = document.createElement('textarea');
    document.body.append(area);
    area.focus();
    await user.keyboard('j');
    expect(document.querySelector('.review-row.is-selected')!.textContent).toBe(first);
  });

  it('holds the walk while a dialog is open, and frees it when the dialog closes', async () => {
    const user = userEvent.setup();
    await mountReview();
    const selected = () => document.querySelector('.review-row.is-selected')!.textContent;
    const first = selected();

    await user.click(screen.getByRole('button', { name: 'Approve' }));
    await user.keyboard('j');
    expect(selected()).toBe(first);
    await user.keyboard('a');
    expect(screen.getAllByRole('dialog')).toHaveLength(1);

    await user.click(dialogButton('Cancel'));
    await user.keyboard('j');
    expect(selected()).not.toBe(first);
  });

  it('unbinds its key listener with the screen', async () => {
    const added = vi.spyOn(document, 'addEventListener');
    const removed = vi.spyOn(document, 'removeEventListener');
    const view = await mountReview();
    view.unmount();
    const keydowns = (spy: typeof added) => spy.mock.calls.filter(([type]) => type === 'keydown');
    expect(keydowns(added).length).toBeGreaterThan(0);
    expect(keydowns(removed).map(([, fn]) => fn)).toEqual(keydowns(added).map(([, fn]) => fn));
  });

  it('moves nothing on j when the queue is empty', async () => {
    const user = userEvent.setup();
    await mountReview(stubApi({ listContent: async () => ({ ...QUEUE, items: [] }) }));
    await user.keyboard('j');
    expect(document.querySelector('.review-row.is-selected')).toBeNull();
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
