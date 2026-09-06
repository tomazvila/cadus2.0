/**
 * The retention card of unit f19 (D-F11): what the delayed probes answered.
 *
 * It sits under the "More" disclosure, so it obeys W-C5 and adds no second primary
 * action (W-C2). It reads ONE route, `GET /api/report/retention`, and it decides
 * nothing: every number on the screen is a number the service reported.
 *
 * IT LOADS ON DEMAND. The report reads the whole event log for its integrated-task
 * block, so the card never fetches on mount. The learner presses "Load the report"
 * and the fetch runs once.
 *
 * THREE RULES THIS SCREEN KEEPS, and all three are about honesty:
 *
 *   1. NO EVIDENCE IS NOT A ZERO. `retained_accuracy` of `null` prints "no answer
 *      yet" and never "0%".
 *   2. A SMALL SAMPLE SAYS SO. A row with `sufficient: false` carries the count and
 *      the words "too few to read".
 *   3. THE PROVENANCE IS ON THE CARD. Assisted and repeated answers are printed
 *      beside the rate they are excluded from, so no reader takes a familiarity
 *      effect for recall.
 *
 * The policy line prints the version and whether real delayed outcomes calibrated
 * it (D-F12). The shipped answer is "uncalibrated", and the card says so.
 */
import { useState } from 'react';
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

/** One row of the retention table. */
function Row({ row }: { row: RetentionRow }) {
  const p = row.provenance;
  return (
    <tr>
      <th scope="row">{delayLabel(row)}</th>
      <td>
        {rateText(row.retained_accuracy)}
        {row.retained_accuracy !== null && !row.sufficient ? (
          <span className="muted small"> · too few to read</span>
        ) : null}
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
  const [loading, setLoading] = useState(false);

  const load = async () => {
    setLoading(true);
    const res = await call(() => api.getRetentionReport());
    setLoading(false);
    if (!res) return;
    setReport(res);
  };

  if (loading) return <LoadingBlock label="Reading the retention report…" />;

  if (!report) {
    return (
      <div className="retention-card">
        <h3>Delayed retention</h3>
        <p className="muted small">
          What you still answered right days after the lesson, on items you never saw.
        </p>
        <button type="button" className="btn" onClick={() => void load()}>
          Load the report
        </button>
      </div>
    );
  }

  const { policy, retention, placement, integrated } = report;
  return (
    <div className="retention-card">
      <h3>Delayed retention</h3>
      <p className="muted small">
        Policy {policy.label} · digest {policy.digest} · probes at{' '}
        {policy.probe_delays_days.join(', ')} days · a rate needs {policy.min_sample} answers.
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
            <Row key={row.delay_days} row={row} />
          ))}
          <Row row={retention.total} />
        </tbody>
      </table>
      <p className="muted small">
        Placement: {placement.failed_confirmation.length} topic(s) failed their confirmation,{' '}
        {placement.awaiting_confirmation.length} still owe one.
      </p>
      <p className="muted small">
        Integrated tasks: {integrated.served} served, {integrated.passed} passed,{' '}
        {integrated.failed} failed, {integrated.inconclusive} without a decision,{' '}
        {integrated.open} open · pass rate {rateText(integrated.pass_rate)}.
      </p>
    </div>
  );
}
