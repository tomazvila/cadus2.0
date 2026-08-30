/**
 * One gate run, rendered.
 *
 * A6 — A SKIPPED CHECK IS EXPLICIT, NEVER SILENT — is the whole reason this block exists,
 * and it is why `exhaustive` is rendered as a sentence and not as a tick. The gate either
 * walked every satisfying tuple of the space or it sampled some of them, and a sampled
 * space is where a bad corner survives (spec section 3.2). A reviewer who reads "gated" and
 * approves has been told the check was complete when it was not.
 *
 * The three shapes `operator.rs` `gate_json` writes are three different facts, and each one
 * gets its own sentence:
 *   * no `gated` — the curriculum does not name the knowledge point, so nothing was checked;
 *   * `gated` with `rejected` — the gate REFUSED the body, with the code and the message;
 *   * `gated` with `exhaustive` and `instances_checked` — the gate passed, over that many
 *     instances, walked or sampled.
 *
 * The notes are rendered verbatim. They are the gate's own words about what it did, and a
 * screen that summarized them would be the second opinion A6 exists to prevent.
 */
import type { OperatorGateNote } from '@/api/types';

/** The line a sampled gate run carries. It is a warning, and it reads as one. */
export const SAMPLED_LINE = 'Sampled: the gate checked a draw of the space, not all of it.';

/** The line an exhaustive gate run carries. */
export const EXHAUSTIVE_LINE = 'Exhaustive: the gate walked every satisfying tuple.';

/** The line of a knowledge point the curriculum does not name. */
export const NOT_GATED_LINE = 'Not gated.';

/** Shorten a digest for a heading. The full digest stays reachable through `title`. */
export function shortDigest(digest: string): string {
  return digest.slice(0, 12);
}

export function GateBlock({ note }: { note: OperatorGateNote }) {
  return (
    <div className="gate-note">
      <p className="gate-head">
        <span className="mono" title={note.digest}>{shortDigest(note.digest)}</span>{' '}
        <span className="muted">{note.kp_id}</span>
      </p>
      {!note.gated ? (
        <p className="gate-line gate-warn">
          {NOT_GATED_LINE} {note.reason ?? ''}
        </p>
      ) : note.rejected ? (
        <p className="gate-line gate-bad">
          Rejected — <span className="mono">{note.rejected.code}</span>: {note.rejected.message}
        </p>
      ) : (
        <p className={note.exhaustive ? 'gate-line gate-good' : 'gate-line gate-warn'}>
          {note.exhaustive ? EXHAUSTIVE_LINE : SAMPLED_LINE}{' '}
          {note.instances_checked ?? 0} instances checked.
        </p>
      )}
      {note.notes.length > 0 ? (
        <ul className="gate-notes">
          {note.notes.map((line, i) => (
            // The gate writes free text with no id of its own, and two runs of one space can
            // repeat a line, so the index is the only stable key here.
            <li key={`${i}-${line}`} className="small">{line}</li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
