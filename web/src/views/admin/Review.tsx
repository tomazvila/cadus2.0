/**
 * The review screen (`/review`): the C6 queue of spec section 3.2.
 *
 * WHAT MAKES THIS SCREEN DIFFERENT FROM EVERY OTHER ONE. Approve and Reject are the only
 * writes in the SPA that are irreversible for the row they touch (spec section 3.2):
 * approval binds to the digest, and an edited body is a new digest with no approval carried
 * over. So both go through a confirmation step, and neither is reachable by one keystroke —
 * `a` and `r` open the dialog, they do not decide.
 *
 * THE ROW LEAVES THE PENDING LIST BECAUSE THE SERVICE SAYS SO. A decision reloads the queue
 * instead of splicing the row out locally. A local removal is a claim about a write this
 * screen cannot see the result of; a queue filtered on `status=pending` that still holds
 * the row after a reload is a write that did not land, and the reviewer has to know that.
 *
 * THE SELECTION MOVES ON, NOT BACK. A decision selects the row that followed it, so a
 * reviewer works down a knowledge point without re-finding their place after every write.
 *
 * THE KEYBOARD IS DOCUMENT-WIDE AND YIELDS. `j`, `k`, `a` and `r` are bare letters, so they
 * are ignored while a dialog is open, while focus is in a text field, and under any
 * modifier — otherwise typing a rejection reason with the word "jar" in it walks the queue
 * and opens two dialogs.
 *
 * A DECISION NEEDS THE BODY ON SCREEN (C6, F5). Approve and Reject stay disabled until the
 * pane reports that it rendered the body of the SELECTED digest, and the two writes post
 * that reported digest. The pane owns its own read, so a `GET /api/admin/content/{digest}`
 * that fails replaces the body, the instances and the gate with a failure block. Live
 * buttons over that block let a reviewer approve — irreversibly, and for the digest — a
 * document nobody read. The rule covers the keyboard too: `a` and `r` decide on the same
 * value the buttons do.
 *
 * THE REFUSAL OWNS THE WHOLE SCREEN. A non-admin gets the refusal block and no queue, no
 * filter, and no buttons (REVIEW-admin). The service is the only gate there is: `is_admin`
 * is outside the runtime role's column grants, so no reply the SPA reads carries the flag.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AdminFailureBlock } from './AdminFailure';
import { ReviewDocumentPane } from './ReviewDocument';
import { shortDigest } from './GateBlock';
import { alerts, usd } from './cost';
import { groupByKp, step, walkOrder } from './groups';
import { REASON_MAX_CHARS, usableReason } from './reason';
import { useAdminLoad } from './adminLoad';
import { useDialogs } from '@/components/Modal';
import { LoadingBlock } from '@/components/primitives';
import { useBusy } from '@/hooks/useBusy';
import { useCall } from '@/hooks/useCall';
import { useLifetime } from '@/hooks/useLifetime';
import { toast } from '@/app/toast';
import { num } from '@/lib/format';
import type { ApiClient, ReviewItem } from '@/api/types';

/** The heading, and the string the click-through of S13 looks for. */
const REVIEW_TITLE = 'Review queue';

/** The line of an empty queue. It is good news, and it says so. */
export const REVIEW_EMPTY = 'Nothing is waiting for review.';

/** The status the queue reads. The screen shows the pending work and nothing else. */
const PENDING = 'pending';

/** The line under the two disabled writes. It names the condition that opens them. */
export const REVIEW_UNREAD = 'Approve and Reject open when the body of this digest is on screen.';

/** The busy key of the two writes. One key, because one row decides at a time. */
const DECIDE_KEY = 'decide';

// --------------------------------------------------------------------------- //
// The two dialogs
// --------------------------------------------------------------------------- //

/**
 * The approval confirmation.
 *
 * It restates the digest, because approval binds to the digest and to nothing else, and a
 * reviewer who confirms the wrong row has approved a body they did not read.
 */
function ApproveConfirm({
  item,
  onDone,
}: {
  item: ReviewItem;
  onDone: (value: true | null) => void;
}) {
  return (
    <div className="modal" role="dialog" aria-modal="true" aria-labelledby="approve-h">
      <h2 id="approve-h">Approve this document?</h2>
      <p className="small">
        <span className="mono">{item.kind}</span> for <span className="mono">{item.kp_id}</span>
      </p>
      <p className="mono small muted">{item.digest}</p>
      <p className="small muted">
        Approval binds to this digest. An edited body is a new digest and needs its own
        approval.
      </p>
      <div className="modal-actions">
        <button type="button" className="btn btn-ghost" onClick={() => onDone(null)}>
          Cancel
        </button>
        <button type="button" className="btn btn-primary" onClick={() => onDone(true)}>
          Approve
        </button>
      </div>
    </div>
  );
}

/**
 * The rejection prompt. It resolves a usable reason, or nothing at all (REVIEW-reason).
 *
 * The Reject button is NOT disabled while the box is empty. A disabled button with no line
 * beside it is a dead end: the reviewer presses it, nothing happens, and the screen has not
 * said why. It stays pressable, and pressing it with an empty box renders the rule.
 */
function RejectPrompt({
  item,
  onDone,
}: {
  item: ReviewItem;
  onDone: (value: string | null) => void;
}) {
  const [raw, setRaw] = useState('');
  const [error, setError] = useState('');

  const submit = () => {
    const checked = usableReason(raw);
    if ('error' in checked) {
      setError(checked.error);
      return;
    }
    onDone(checked.reason);
  };

  return (
    <div className="modal" role="dialog" aria-modal="true" aria-labelledby="reject-h">
      <h2 id="reject-h">Reject this document?</h2>
      <p className="mono small muted">{item.digest}</p>
      <label className="auth-label" htmlFor="reject-reason">
        Reason
      </label>
      {/* The box is first in the DOM, so the modal's own focus pass lands the caret here. */}
      <textarea
        id="reject-reason"
        className="work-input"
        rows={4}
        maxLength={REASON_MAX_CHARS}
        value={raw}
        onChange={(e) => {
          setRaw(e.target.value);
          // The line clears as soon as the reviewer acts on it. A message that outlives the
          // condition it describes reads as a second, unrelated failure.
          if (error) setError('');
        }}
      />
      {error ? (
        <p className="field-error" role="alert">
          {error}
        </p>
      ) : null}
      <div className="modal-actions">
        <button type="button" className="btn btn-ghost" onClick={() => onDone(null)}>
          Cancel
        </button>
        <button type="button" className="btn btn-primary" onClick={submit}>
          Reject
        </button>
      </div>
    </div>
  );
}

// --------------------------------------------------------------------------- //
// The screen
// --------------------------------------------------------------------------- //

export interface ReviewScreenProps {
  api: ApiClient;
  /** Demo mode. A 401 then keeps the reader on the screen. */
  demo?: boolean;
  /** The session-expired path. */
  onUnauthorized: () => void;
}

export function ReviewScreen({ api, demo = false, onUnauthorized }: ReviewScreenProps) {
  const life = useLifetime();
  const dialogs = useDialogs();
  const busy = useBusy();
  const call = useCall({ demo, onUnauthorized });

  const load = useCallback(() => api.listContent({ status: PENDING }), [api]);
  const queue = useAdminLoad({ load, demo, onUnauthorized });

  const [selected, setSelected] = useState<string | null>(null);

  // The digest of the body the pane has on screen, reported by the pane itself. It is null
  // before the first reply, and null again on a failed read. See the module note.
  const [readDigest, setReadDigest] = useState<string | null>(null);

  const groups = useMemo(() => groupByKp(queue.data?.items ?? []), [queue.data]);
  const order = useMemo(() => walkOrder(groups), [groups]);
  const items = useMemo(() => groups.flatMap((group) => group.items), [groups]);

  // THE SELECTION IS DERIVED, NOT RECONCILED IN AN EFFECT. The stored digest counts while
  // the queue still holds it, and the first row counts otherwise. A digest the reload
  // dropped — the row just approved — therefore stops being selected in the same render
  // that drops it, with no second render in between: an effect that corrected the stored
  // value would paint one frame of the decided row, live Approve and Reject buttons and
  // all, over a queue that no longer contains it.
  const active = selected !== null && order.includes(selected) ? selected : (order[0] ?? null);
  const selectedItem = items.find((item) => item.digest === active) ?? null;

  // The document a decision may name: the selected row, and only while the pane has the body
  // of THAT digest on screen. Null disables both writes and both keys.
  const decidable =
    selectedItem !== null && readDigest === selectedItem.digest ? selectedItem : null;

  // True from the moment a dialog is asked for until it settles. The keyboard reads it, so
  // it is a ref and not state: a render is not needed, and the keydown that follows the
  // opening click in the same tick must already see it.
  const dialogOpen = useRef(false);

  const decide = useCallback(
    async (kind: 'approve' | 'reject', item: ReviewItem) => {
      dialogOpen.current = true;
      let reason: string | null = null;
      try {
        if (kind === 'approve') {
          const ok = await dialogs.open<true>((resolve) => (
            <ApproveConfirm item={item} onDone={resolve} />
          ));
          if (!ok) return;
        } else {
          reason = await dialogs.open<string>((resolve) => (
            <RejectPrompt item={item} onDone={resolve} />
          ));
          // Cancelled, or the prompt refused the reason. Nothing is posted either way.
          if (!reason) return;
        }
      } finally {
        dialogOpen.current = false;
      }
      // The dialog settles on provider unmount too, so the view may already be gone.
      if (!life.alive()) return;

      const res = await call(() =>
        kind === 'approve' ? api.approveContent(item.digest) : api.rejectContent(item.digest, reason!),
      );
      if (!res || !life.alive()) return;

      // Move on before the reload, so the reviewer lands on the next row rather than at the
      // top of a queue that is one row shorter.
      setSelected(order[order.indexOf(item.digest) + 1] ?? null);
      toast(
        `${kind === 'approve' ? 'Approved' : 'Rejected'} ${shortDigest(item.digest)}.`,
        { kind: kind === 'approve' ? 'success' : 'info' },
      );
      queue.reload();
    },
    [api, call, dialogs, life, order, queue],
  );

  // The live handler, read through a ref by a listener that binds once. Bound to the
  // document rather than to a node, because the reviewer's focus is on a row button, on the
  // pane, or nowhere at all, and the walk has to work from all three.
  const keyRef = useRef<(e: KeyboardEvent) => void>(() => {});
  useEffect(() => {
    keyRef.current = (e: KeyboardEvent) => {
      if (dialogOpen.current) return;
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || target?.isContentEditable) return;

      if (e.key === 'j' || e.key === 'k') {
        const next = step(order, active, e.key === 'j' ? 1 : -1);
        if (next === null) return;
        e.preventDefault();
        setSelected(next);
        return;
      }
      if ((e.key === 'a' || e.key === 'r') && decidable) {
        e.preventDefault();
        busy.run(DECIDE_KEY, () => decide(e.key === 'a' ? 'approve' : 'reject', decidable));
      }
    };
  });
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => keyRef.current(e);
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, []);

  if (queue.failure) {
    return (
      <section className="view-review">
        <AdminFailureBlock
          failure={queue.failure}
          message={queue.message}
          onRetry={queue.reload}
        />
      </section>
    );
  }

  if (!queue.data) {
    return (
      <section className="view-review">
        <LoadingBlock label="Loading the review queue…" />
      </section>
    );
  }

  // Read out of the payload here, not inside the group loop: a `queue.data.limit` inside a
  // `map` callback loses the narrowing the guard above just established.
  const { items: rows, limit, bank_target: bankTarget } = queue.data;

  return (
    <section className="view-review">
      <div className="admin-head">
        <h1>{REVIEW_TITLE}</h1>
        <p className="muted small review-keys">
          j / k move · a approves · r rejects
        </p>
        <button type="button" className="btn" onClick={queue.reload} disabled={queue.loading}>
          Refresh
        </button>
      </div>

      {rows.length === 0 ? (
        <div className="empty">
          <p>{REVIEW_EMPTY}</p>
        </div>
      ) : (
        <div className="review-layout">
          <nav className="review-queue" aria-label="Pending documents by knowledge point">
            {rows.length >= num(limit) ? (
              <p className="gate-line gate-warn small">
                The queue is capped at {num(limit)} rows. Filter by knowledge point
                to see the rest.
              </p>
            ) : null}
            {groups.map((group) => (
              <div className="review-group" key={group.kp_id}>
                <h2 className="review-group-head">
                  <span className="mono">{group.kp_id}</span>
                  <span className="muted small">{group.items.length} pending</span>
                </h2>
                {group.bank_warning ? (
                  <p className="gate-line gate-warn small">
                    {num(group.approved_templates)} approved templates — under the bank target
                    of {num(bankTarget)}.
                  </p>
                ) : null}
                <ul className="review-rows">
                  {group.items.map((item) => (
                    <li key={item.digest}>
                      <button
                        type="button"
                        className={
                          item.digest === active ? 'review-row is-selected' : 'review-row'
                        }
                        aria-current={item.digest === active ? 'true' : undefined}
                        onClick={() => setSelected(item.digest)}
                      >
                        <span className="review-row-top">
                          <span className="chip">{item.kind}</span>
                          <span className={alerts(item) ? 'chip chip-bad' : 'chip'}>
                            {num(item.authoring_attempts)} attempts
                          </span>
                          <span className="chip">{usd(item.authoring_cost_usd)}</span>
                        </span>
                        <span className="review-row-summary">{item.summary}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </nav>

          <div className="review-pane card">
            {selectedItem ? (
              <>
                <div className="review-actions">
                  <button
                    type="button"
                    className={busy.cls(DECIDE_KEY, 'btn btn-primary')}
                    disabled={busy.is(DECIDE_KEY) || decidable === null}
                    onClick={() => {
                      if (decidable) busy.run(DECIDE_KEY, () => decide('approve', decidable));
                    }}
                  >
                    Approve
                  </button>
                  <button
                    type="button"
                    className={busy.cls(DECIDE_KEY, 'btn')}
                    disabled={busy.is(DECIDE_KEY) || decidable === null}
                    onClick={() => {
                      if (decidable) busy.run(DECIDE_KEY, () => decide('reject', decidable));
                    }}
                  >
                    Reject
                  </button>
                  {decidable === null ? (
                    <p className="muted small review-unread">{REVIEW_UNREAD}</p>
                  ) : null}
                </div>
                {/* Keyed by digest: a new digest builds a new pane. See its module note. */}
                <ReviewDocumentPane
                  key={selectedItem.digest}
                  api={api}
                  digest={selectedItem.digest}
                  demo={demo}
                  onUnauthorized={onUnauthorized}
                  onLoaded={setReadDigest}
                />
              </>
            ) : (
              <p className="muted">Select a document to review it.</p>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
