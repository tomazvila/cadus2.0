/**
 * The operator screen (`/ops`): A6 serving health, and the T3 bill per knowledge point.
 *
 * TWO READS, AND THE FLAGS READ DECIDES THE SCREEN. `GET /api/operator/flags` answers the
 * A6 health and no money; the bill lives on the stored documents, so the T3 panel reads the
 * review queue as well. When the flags read is refused, the screen is refused and NOTHING
 * else renders (REVIEW-admin) — the queue read is refused for the same reason a beat later,
 * and rendering a cost table under a refusal would be the same leak from the other route.
 *
 * WHEN THE FLAGS READ SUCCEEDS AND THE QUEUE READ DOES NOT, the flags still render. The
 * screens are two facts about one deployment, and losing the money must not lose the health.
 * The cost panel then says which read failed, in place of a table of zeroes: a roll-up of a
 * queue that never arrived is `$0.0000` for every knowledge point, which reads as "nothing
 * was spent" and is the one wrong answer this panel can give.
 *
 * NOTHING HERE WRITES. The screen has no button that changes a row, so it needs no
 * confirmation step and no busy guard: a double-press of Refresh starts one more read.
 */
import { useCallback } from 'react';
import { AdminFailureBlock } from './AdminFailure';
import { GateBlock } from './GateBlock';
import { costByKp, usd } from './cost';
import { useAdminLoad } from './adminLoad';
import { LoadingBlock } from '@/components/primitives';
import { num } from '@/lib/format';
import type { ApiClient } from '@/api/types';

/** The heading, and the string the click-through of S13 looks for. */
export const OPS_TITLE = 'Operator';

/** The line of a deployment whose pool holds no knowledge point at all. */
export const OPS_EMPTY = 'The pool holds no knowledge point yet.';

/** The line of the cost panel when the review queue could not be read. */
export const COST_UNAVAILABLE = 'The authoring bill could not be read.';

/** The line of a queue that priced nothing. */
export const COST_EMPTY = 'No authored document carries a bill yet.';

export interface OperatorScreenProps {
  api: ApiClient;
  /** Demo mode. A 401 then keeps the reader on the screen. */
  demo?: boolean;
  /** The session-expired path. */
  onUnauthorized: () => void;
}

export function OperatorScreen({ api, demo = false, onUnauthorized }: OperatorScreenProps) {
  // Both reads are stable per client, so each starts exactly once per mount. An inline
  // arrow here would be a new function on every render and a read loop.
  const loadFlags = useCallback(() => api.getOperatorFlags(), [api]);
  const loadQueue = useCallback(() => api.listContent(), [api]);
  const flags = useAdminLoad({ load: loadFlags, demo, onUnauthorized });
  const queue = useAdminLoad({ load: loadQueue, demo, onUnauthorized });

  const refresh = useCallback(() => {
    flags.reload();
    queue.reload();
  }, [flags, queue]);

  // The refusal owns the whole screen. See the module note.
  if (flags.failure) {
    return (
      <section className="view-ops">
        <AdminFailureBlock
          failure={flags.failure}
          message={flags.message}
          onRetry={flags.reload}
        />
      </section>
    );
  }

  if (!flags.data) {
    return (
      <section className="view-ops">
        <LoadingBlock label="Loading the operator view…" />
      </section>
    );
  }

  const { flags: rows, gate, gate_limit: gateLimit, gate_truncated: truncated } = flags.data;
  const costs = queue.data ? costByKp(queue.data.items) : [];
  const total = costs.reduce((sum, row) => sum + row.cost, 0);

  return (
    <section className="view-ops">
      <div className="admin-head">
        <h1>{OPS_TITLE}</h1>
        <button type="button" className="btn" onClick={refresh} disabled={flags.loading}>
          Refresh
        </button>
      </div>

      <section className="card admin-card" aria-labelledby="ops-flags-h">
        <h2 id="ops-flags-h">Serving health</h2>
        <p className="muted small">
          One row per knowledge point, from <span className="mono">GET /api/operator/flags</span>.
        </p>
        {rows.length === 0 ? (
          <p className="muted">{OPS_EMPTY}</p>
        ) : (
          <div className="table-scroll">
            <table className="admin-table">
              <caption className="visually-hidden">Serving health, one row per knowledge point</caption>
              <thead>
                <tr>
                  <th scope="col">Knowledge point</th>
                  <th scope="col">Approved</th>
                  <th scope="col">Pool depth</th>
                  <th scope="col">Last source</th>
                  <th scope="col">State</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => (
                  <tr key={row.kp_id}>
                    <th scope="row" className="mono">{row.kp_id}</th>
                    <td className="mono">{num(row.approved_templates)}</td>
                    <td className="mono">{num(row.pool_depth)}</td>
                    <td className="mono">{row.last_source ?? '—'}</td>
                    <td>
                      {/* Both flags render, and neither is inferred from the other: a
                          knowledge point can want a template without having exhausted its
                          source, and the operator acts differently on each. */}
                      {row.needs_template ? <span className="chip chip-bad">needs template</span> : null}
                      {row.source_exhausted ? <span className="chip chip-bad">source exhausted</span> : null}
                      {!row.needs_template && !row.source_exhausted ? (
                        <span className="chip chip-good">ok</span>
                      ) : null}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section className="card admin-card" aria-labelledby="ops-cost-h">
        <h2 id="ops-cost-h">Authoring cost per knowledge point</h2>
        <p className="muted small">
          Every stored document, of every status. A rejected document still cost money.
        </p>
        {queue.failure ? (
          <p className="muted">
            {COST_UNAVAILABLE} {queue.message}
          </p>
        ) : !queue.data ? (
          <LoadingBlock label="Loading the authoring bill…" />
        ) : costs.length === 0 ? (
          <p className="muted">{COST_EMPTY}</p>
        ) : (
          <div className="table-scroll">
            <table className="admin-table">
              <caption className="visually-hidden">Authoring cost, one row per knowledge point</caption>
              <thead>
                <tr>
                  <th scope="col">Knowledge point</th>
                  <th scope="col">Documents</th>
                  <th scope="col">Attempts</th>
                  <th scope="col">Cost</th>
                  <th scope="col">T3</th>
                </tr>
              </thead>
              <tbody>
                {costs.map((row) => (
                  <tr key={row.kp_id}>
                    <th scope="row" className="mono">{row.kp_id}</th>
                    <td className="mono">{row.documents}</td>
                    <td className="mono">{row.attempts}</td>
                    <td className="mono">{usd(row.cost)}</td>
                    <td>
                      {row.alerting > 0 ? (
                        <span className="chip chip-bad">{row.alerting} over 3 attempts</span>
                      ) : (
                        <span className="muted small">—</span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
              <tfoot>
                <tr>
                  <th scope="row">Total</th>
                  <td className="mono">{queue.data.items.length}</td>
                  <td />
                  <td className="mono">{usd(total)}</td>
                  <td />
                </tr>
              </tfoot>
            </table>
          </div>
        )}
      </section>

      <section className="card admin-card" aria-labelledby="ops-gate-h">
        <h2 id="ops-gate-h">Gate notes</h2>
        <p className="muted small">
          At most {num(gateLimit)} approved templates are re-gated per read.
        </p>
        {truncated ? (
          <p className="gate-line gate-warn">
            Truncated: more approved templates exist than this read gated.
          </p>
        ) : null}
        {gate.length === 0 ? (
          <p className="muted">No approved template was gated by this read.</p>
        ) : (
          gate.map((note) => <GateBlock key={`${note.kp_id}-${note.digest}`} note={note} />)
        )}
      </section>
    </section>
  );
}
