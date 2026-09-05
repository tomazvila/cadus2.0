/**
 * The one refusal block both operator screens render.
 *
 * It exists in one place because the acceptance check is a claim about BOTH screens — "a
 * non-admin sees neither route" (REVIEW-admin) — and two copies of a refusal are two
 * chances for one of them to grow a data table under it.
 *
 * A refused screen renders THIS AND NOTHING ELSE. No empty table, no filter bar, no
 * disabled Approve button: a shell of an operator screen with a refusal inside it still
 * shows a learner what the screen is and how it is laid out, and the test that proves the
 * refusal would pass with the queue rendered beside it.
 *
 * There is no Try again on a refusal. Retrying a `403` re-asks a question already answered,
 * and a button that will fail every time is a worse dead end than no button.
 */
import { FORBIDDEN_MESSAGE, FORBIDDEN_TITLE, UNAVAILABLE_TITLE } from './adminLoad';
import type { AdminFault } from './adminLoad';

export interface AdminFailureBlockProps {
  /** The refusal, with the message the service sent; the message renders verbatim. */
  fault: AdminFault;
  /** Offered on `error` only. */
  onRetry: () => void;
}

export function AdminFailureBlock({ fault: { failure, message }, onRetry }: AdminFailureBlockProps) {
  if (failure === 'forbidden') {
    return (
      <div className="empty admin-refused">
        <h2>{FORBIDDEN_TITLE}</h2>
        <p className="muted">{FORBIDDEN_MESSAGE}</p>
      </div>
    );
  }
  if (failure === 'unavailable') {
    return (
      <div className="empty admin-refused">
        <h2>{UNAVAILABLE_TITLE}</h2>
        {/* The service names the deployment fault; repeating it in our own words would
            describe a configuration this build cannot see. */}
        <p className="muted">{message}</p>
      </div>
    );
  }
  return (
    <div className="empty">
      <p>{message}</p>
      <button type="button" className="btn btn-primary" onClick={onRetry}>
        Try again
      </button>
    </div>
  );
}
