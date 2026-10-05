/**
 * The retention card of unit f19 (D-F11): what the delayed probes answered.
 *
 * It stands on the main dashboard and loads by itself, once per mount. It reads ONE
 * route, `GET /api/report/retention?scope=probes`, which answers from the cached learner
 * model and reads no event row, so the automatic load costs one projection. It decides
 * nothing: every number on the screen is a number the service reported. It adds no
 * button, so it obeys W-C2 (one primary action).
 *
 * THREE RULES THIS SCREEN KEEPS, and all three are about honesty:
 *
 *   1. NO EVIDENCE IS NOT A ZERO. `retained_accuracy` of `null` prints "no answer
 *      yet" and never "0%".
 *   2. A SMALL SAMPLE SAYS SO. A row with `sufficient: false` prints its rate as it
 *      comes in, with the count and the words "too few to read yet".
 *   3. THE PROVENANCE IS ON THE CARD. Assisted and repeated answers are printed
 *      beside the rate they are excluded from, so no reader takes a familiarity
 *      effect for recall.
 *
 * The policy line prints the version and whether real delayed outcomes calibrated
 * it (D-F12). The shipped answer is "uncalibrated", and the card says so.
 */
import { useEffect, useState } from 'react';
import { LoadingBlock } from '@/components/primitives';
import { pct } from '@/lib/format';
import type { ApiClient, RetentionReportResponse, RetentionRow } from '@/api/types';
import type { Call } from '@/hooks/useCall';

export interface RetentionCardProps {
  api: ApiClient;
  /** The one request wrapper of the screen (F-36-1). */
  call: Call;
}

/** The label of one delay row. */
function delayLabel(row: RetentionRow): string {
  return row.delay_days === 0 ? 'Every delay' : `${row.delay_days} days later`;
}

/** The rate as text, with "no answer yet" for a rate the service left null. */
function rateText(rate: number | null): string {
  return rate === null ? 'no answer yet' : `${pct(rate)}%`;
}

/** One row of the retention table. `minSample` is the count a rate needs. */
function Row({ row, minSample }: { row: RetentionRow; minSample: number }) {
  const p = row.provenance;
  return (
    <tr>
      <th scope="row">{delayLabel(row)}</th>
      <td>
        {rateText(row.retained_accuracy)}
        {row.sufficient ? null : (
          <span className="muted small">
            {' '}· too few to read yet ({p.independent} of {minSample})
          </span>
        )}
      </td>
      <td>{p.independent_correct} of {p.independent}</td>
      <td>{rateText(row.assistance_dependence)}</td>
      <td className="muted small">
        {p.assisted} with help · {p.repeated} repeated · {p.ungraded} ungraded
      </td>
    </tr>
  );
}

export function RetentionCard({ api, call }: RetentionCardProps) {
  const [report, setReport] = useState<RetentionReportResponse | null>(null);
  const [failed, setFailed] = useState(false);

  // ONE read per mount. A reply after the view left writes state nobody renders, so the
  // effect drops it.
  useEffect(() => {
    let alive = true;
    void call(
      () => api.getRetentionReport('probes'),
      (res) => { if (alive) setReport(res); },
      { onFail: () => { if (alive) setFailed(true); } },
    );
    return () => { alive = false; };
  }, [api, call]);

  if (!report) {
    return (
      <div className="retention-card">
        <h3>Delayed retention</h3>
        {failed ? (
          <p className="muted small">The retention report did not load.</p>
        ) : (
          <LoadingBlock label="Reading the retention report…" />
        )}
      </div>
    );
  }

  const { policy, retention, placement, integrated } = report;
  return (
    <div className="retention-card">
      <h3>Delayed retention</h3>
      <p className="muted small">
        What you still answered right days after the lesson, on items you never saw.
      </p>
      <table className="retention-table">
        <thead>
          <tr>
            <th scope="col">Delay</th>
            <th scope="col">Answered right on your own</th>
            <th scope="col">Independent answers</th>
            <th scope="col">Used help</th>
            <th scope="col">Provenance</th>
          </tr>
        </thead>
        <tbody>
          {retention.by_delay.map((row) => (
            <Row key={row.delay_days} row={row} minSample={policy.min_sample} />
          ))}
          <Row row={retention.total} minSample={policy.min_sample} />
        </tbody>
      </table>
      <p className="muted small">
        Policy {policy.label} · digest {policy.digest} · probes at{' '}
        {policy.probe_delays_days.join(', ')} days · a rate needs {policy.min_sample} answers.
      </p>
      <p className="muted small">
        Placement: {placement.failed_confirmation.length} topic(s) failed their confirmation,{' '}
        {placement.awaiting_confirmation.length} still owe one.
      </p>
      {integrated ? (
        <p className="muted small">
          Integrated tasks: {integrated.served} served, {integrated.passed} passed,{' '}
          {integrated.failed} failed, {integrated.inconclusive} without a decision,{' '}
          {integrated.open} open · pass rate {rateText(integrated.pass_rate)}.
        </p>
      ) : null}
    </div>
  );
}
