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
 * THE BILL READS EVERY PAGE, and that is what makes it a bill (T3, A6). One
 * `GET /api/admin/content` answers at most `limit` rows — 200, `cadus_store::content::
 * LIST_LIMIT` — so a deployment with more stored documents than that priced a fraction of
 * its spend and printed the fraction as a Total. Money that is silently short is the one
 * wrong answer this panel can give, so the read walks the pages until one comes back short.
 * The walk has its own bound, and a walk that ends on the bound says so on screen.
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
import type { ApiClient, ReviewItem } from '@/api/types';

/** The heading, and the string the click-through of S13 looks for. */
export const OPS_TITLE = 'Operator';

/** The line of a deployment whose pool holds no knowledge point at all. */
const OPS_EMPTY = 'The pool holds no knowledge point yet.';

/** The line of the cost panel when the review queue could not be read. */
export const COST_UNAVAILABLE = 'The authoring bill could not be read.';

/** The line of a queue that priced nothing. */
const COST_EMPTY = 'No authored document carries a bill yet.';

/**
 * The pages one bill reads, at most.
 *
 * The route serves 100 pages (`admin.rs` `MAX_PAGE`), and this screen asks for a quarter of
 * them: 25 pages of 200 rows price 5,000 documents, which is past the size of every
 * deployment this build sizes for. The bound exists because the walk is a loop over a reply
 * this screen does not control, and a loop with no bound is a browser tab that never
 * answers.
 */
export const BILL_MAX_PAGES = 25;

/** The line of a bill that stopped on that bound. It names the bound it stopped on. */
export const BILL_TRUNCATED =
  `The bill stopped after ${BILL_MAX_PAGES} pages. More documents are stored than the `
  + 'numbers below count.';

/** Every stored document, and whether the walk read all of them. */
export interface Bill {
  items: ReviewItem[];
  /** True when the walk stopped on [`BILL_MAX_PAGES`] with a full page in hand. */
  truncated: boolean;
}

/**
 * Read the whole review queue, one page at a time.
 *
 * THE STOP IS THE SHORT PAGE, not a count of rows: `limit` is the service's own bound
 * (`ReviewListResponse.limit`), and a page that carries fewer rows than the bound is the
 * last page there is. A `limit` the service did not send stops the walk after one page,
 * because a bound of zero would otherwise read 25 identical pages.
 *
 * A DIGEST COUNTS ONCE. The pages are cut from one order, and a document stored between two
 * reads shifts the rows under the walk, so one digest can land on two pages. A bill that
 * counted it twice would charge twice for it.
 *
 * A PAGE THAT FAILS ENDS THE WALK, and the screen renders the failure in place of the table.
 * The pages before it are a part of the bill, and a part of a bill on screen under the word
 * Total is a number that reads as the whole one.
 */
async function readBill(api: ApiClient): Promise<Bill> {
  const items: ReviewItem[] = [];
  const seen = new Set<string>();
  for (let page = 0; page < BILL_MAX_PAGES; page += 1) {
    const reply = await api.listContent({ page });
    for (const row of reply.items) {
      if (seen.has(row.digest)) continue;
      seen.add(row.digest);
      items.push(row);
    }
    const limit = num(reply.limit);
    if (limit <= 0 || reply.items.length < limit) return { items, truncated: false };
  }
  return { items, truncated: true };
}

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
  const loadQueue = useCallback(() => readBill(api), [api]);
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
        {/* The line belongs to the payload on screen. A read that failed renders its own
            line below, and the two together would name two states of one panel. */}
        {!queue.failure && queue.data?.truncated ? (
          <p className="gate-line gate-warn">{BILL_TRUNCATED}</p>
        ) : null}
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
                  {/* The count is the documents the walk read, and it says the word: a bare
                      number under a column head reads as a row count of the table above it,
                      which holds one row per knowledge point and not one per document. */}
                  <td className="mono">{queue.data.items.length} documents</td>
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
