/**
 * The "Reports" section of the review screen: the learner reports that the check could not
 * decide. It is read only. Each row shows the learner, the task, the question, the note of
 * the learner, the last step of the check with its message, and the place of the item in
 * the curriculum.
 *
 * A `403` hides the section: the queue above it already shows the refusal.
 */
import { useState } from 'react';
import { useCall } from '@/hooks/useCall';
import { toast } from '@/app/toast';
import { useAdminLoad } from './adminLoad';
import { LoadingBlock } from '@/components/primitives';
import { MathBlock } from '@/components/MathBlock';
import type { AdminReport, ApiClient } from '@/api/types';

export const REPORTS_TITLE = 'Reports';
export const REPORTS_EMPTY = 'No report waits for the owner.';

function ReportItem({ report, onOpenTopic, onDecide, deciding }: {
  report: AdminReport;
  onOpenTopic?: ((topic: string) => void) | undefined;
  onDecide: (report: AdminReport, action: 'keep' | 'retire') => void;
  deciding: boolean;
}) {
  const step = report.last_step;
  const flagged = report.kind === 'question_unclear';
  return (
    <li className="card review-report">
      <p className="small">
        Learner <span className="mono">{report.user_id}</span> · task <span className="mono">{report.task_id}</span>
        {' '}· {new Date(report.created_at).toLocaleString()}
      </p>
      {report.question ? <MathBlock>{report.question}</MathBlock> : null}
      {report.note ? <p className="small">Learner note: {report.note}</p> : null}
      {flagged ? <p className="small"><strong>The learner says the question is unclear or wrong.</strong>{' '}
        {report.status === 'waiting_owner' ? 'The question is set aside for this learner.'
          : report.status === 'kept' ? 'You kept the question.' : 'You retired the question.'}</p> : null}
      {flagged && report.status === 'waiting_owner' ? <div className="actions">
        <button type="button" className="btn" disabled={deciding} onClick={() => { onDecide(report, 'keep'); }}>Keep the question</button>
        <button type="button" className="btn" disabled={deciding} onClick={() => { onDecide(report, 'retire'); }}>Retire the question</button>
        <span className="muted small">Keep gives the question back to the learner. Retire stops it for every learner.</span>
      </div> : null}
      <p className="small" hidden={flagged}>
        Last step: <span className="mono">{step.stage ?? 'none'}</span>
        {step.message ? <> · {step.message}</> : null}
      </p>
      {report.result_message ? <p className="small muted">Result: {report.result_message}</p> : null}
      <p className="small">
        Curriculum: {report.topic ? (
          onOpenTopic
            ? <button type="button" className="btn btn-ghost" onClick={() => { onOpenTopic(report.topic!); }}>
              Open topic <span className="mono">{report.topic}</span>
            </button>
            : <span className="mono">{report.topic}</span>
        ) : 'unknown topic'}
        {report.kp ? <> · knowledge point <span className="mono">{report.kp}</span></> : null}
      </p>
    </li>
  );
}

export function ReviewReports({ api, demo, onUnauthorized, onOpenTopic }: {
  api: ApiClient;
  demo: boolean;
  onUnauthorized: () => void;
  onOpenTopic?: ((topic: string) => void) | undefined;
}) {
  const [load] = useState(() => () => api.listAdminReports('unresolved'));
  const reports = useAdminLoad({ load, demo, onUnauthorized });
  const call = useCall({ demo, onUnauthorized });
  const [deciding, setDeciding] = useState(false);
  async function decide(report: AdminReport, action: 'keep' | 'retire'): Promise<void> {
    setDeciding(true);
    const done = await call(() => (action === 'keep'
      ? api.keepReportQuestion(report.report_id) : api.retireReportQuestion(report.report_id)));
    setDeciding(false);
    if (!done) return;
    toast(action === 'keep' ? 'You kept the question. The learner can get it again.'
      : 'You retired the question. No learner gets it again.', { kind: 'success' });
    reports.reload();
  }
  if (reports.fault?.failure === 'forbidden') return null;
  return (
    <section className="review-reports" aria-labelledby="review-reports-h">
      <div className="admin-head">
        <h2 id="review-reports-h">{REPORTS_TITLE}</h2>
        <button type="button" className="btn" onClick={reports.reload} disabled={reports.loading}>Refresh</button>
      </div>
      {reports.fault ? <p role="status" className="muted small">The reports could not be loaded. {reports.fault.message}</p>
        : !reports.data ? <LoadingBlock label="Loading the reports…" />
          : reports.data.reports.length === 0 ? <p className="muted">{REPORTS_EMPTY}</p>
            : <ul className="review-report-list">
              {reports.data.reports.map((r) => <ReportItem key={r.report_id} report={r} onOpenTopic={onOpenTopic} deciding={deciding}
                onDecide={(item, action) => { void decide(item, action); }} />)}
            </ul>}
    </section>
  );
}
