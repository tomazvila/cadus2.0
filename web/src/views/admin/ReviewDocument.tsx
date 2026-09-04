/**
 * The right-hand pane of the review screen: one document, as a reviewer decides on it.
 *
 * THE RENDERED INSTANCES ARE THE POINT. Spec section 3.2 carries the 1.0 docstring's
 * reason: the live failures — `Compute $12 - 70$` on a borrowing topic, `What is the
 * opposite of $0$?` — are obvious in the instances and invisible in the expression
 * (`scripts/review_templates.py:24-28`). So the instances come FIRST, above the body and
 * above the gate notes, and the raw body sits behind a `<details>` that starts closed. A
 * reviewer who reads the expression first has been given the harder job.
 *
 * AN EMPTY INSTANCE LIST ALWAYS CARRIES ITS REASON (A6). The service sends
 * `instances_note` for a body that does not compile, a document no draw satisfies, and a
 * kind that has no instances at all; "no instances" with nothing beside it is the silent
 * skip A6 forbids.
 *
 * THE PANE IS KEYED BY DIGEST BY ITS CALLER, so selecting another row builds a new pane
 * rather than pointing this one at a second document. Without that, the load of the second
 * digest lands into a pane still showing the first, and for one paint the instances of one
 * document sit under the summary of another — which is the exact confusion a reviewer must
 * never be handed before an irreversible decision.
 *
 * THE PANE REPORTS WHAT IT RENDERED, and the two writes hang off that report (C6). The
 * screen approves a digest; the reviewer approves a BODY. `onLoaded` carries the digest of
 * the document that is on screen right now, and it carries null while there is none — before
 * the first reply, and after a failed read, where the block replaces the body and the
 * previous payload stays in the hook. A reviewer who does not see the body must not decide on
 * it, so the caller gates Approve and Reject on this value.
 */
import { useCallback, useEffect } from 'react';
import { AdminFailureBlock } from './AdminFailure';
import { GateBlock } from './GateBlock';
import { alerts, usd } from './cost';
import { useAdminLoad } from './adminLoad';
import { MathBlock } from '@/components/MathBlock';
import { LoadingBlock } from '@/components/primitives';
import { num } from '@/lib/format';
import type { ApiClient } from '@/api/types';

/** The heading above the rendered instances. */
const INSTANCES_TITLE = 'Rendered instances';

/** The line of a kind that renders no instance and carries no note either. */
const NO_INSTANCES = 'This document renders no instance.';

export interface ReviewDocumentPaneProps {
  api: ApiClient;
  /** The digest to show. The caller keys this component on it. */
  digest: string;
  demo?: boolean;
  onUnauthorized: () => void;
  /**
   * The digest of the body on screen, or null while none is. It MUST be stable — a state
   * setter, or a `useCallback` — because it is an effect dependency.
   */
  onLoaded: (digest: string | null) => void;
}

export function ReviewDocumentPane({
  api,
  digest,
  demo = false,
  onUnauthorized,
  onLoaded,
}: ReviewDocumentPaneProps) {
  const load = useCallback(() => api.getContent(digest), [api, digest]);
  const doc = useAdminLoad({ load, demo, onUnauthorized });

  // The digest the SERVICE gave the body below, and null on every path that renders no body.
  // A failed reload keeps the previous payload in the hook — the S11 rule — and renders the
  // failure block over it, so the failure decides this value before the payload does.
  const rendered = doc.failure === null ? (doc.data?.digest ?? null) : null;
  useEffect(() => {
    onLoaded(rendered);
  }, [onLoaded, rendered]);

  if (doc.failure) {
    return (
      <AdminFailureBlock failure={doc.failure} message={doc.message} onRetry={doc.reload} />
    );
  }
  if (!doc.data) return <LoadingBlock label="Loading the document…" />;

  const item = doc.data;
  const body = JSON.stringify(item.body, null, 2);

  return (
    <div className="review-doc">
      <h2 className="review-doc-head">
        {item.kind} · <span className="mono">{item.kp_id}</span>
      </h2>
      <p className="mono small muted review-digest">{item.digest}</p>

      <p className="review-chips">
        <span className="chip">{item.status}</span>
        <span className={alerts(item) ? 'chip chip-bad' : 'chip'}>
          {num(item.authoring_attempts)} attempts
        </span>
        <span className="chip">{usd(item.authoring_cost_usd)}</span>
        {item.bank_warning ? (
          <span className="chip chip-bad">
            {num(item.approved_templates)} approved templates
          </span>
        ) : null}
      </p>

      {item.review_reason ? (
        <p className="review-reason-line">Refused: {item.review_reason}</p>
      ) : null}

      <section aria-labelledby="review-instances-h">
        <h3 id="review-instances-h">
          {INSTANCES_TITLE} ({item.instances.length} of {num(item.sample_instances)})
        </h3>
        {item.instances.length === 0 ? (
          <p className="muted">{item.instances_note ?? NO_INSTANCES}</p>
        ) : (
          <ol className="instance-list">
            {item.instances.map((instance, i) => (
              // Two draws of one space can render the same statement, so the index is the
              // only key that stays stable across a reload of the same digest.
              <li key={`${i}-${instance.text}`} className="instance">
                <MathBlock className="instance-text">{instance.text}</MathBlock>
                <MathBlock className="instance-answer mono">{instance.answer}</MathBlock>
              </li>
            ))}
          </ol>
        )}
      </section>

      {item.gate ? (
        <section aria-labelledby="review-gate-h">
          <h3 id="review-gate-h">Gate</h3>
          <GateBlock note={item.gate} />
        </section>
      ) : null}

      <details className="review-body">
        <summary>The authored body</summary>
        <pre className="mono small">{body}</pre>
      </details>
    </div>
  );
}
