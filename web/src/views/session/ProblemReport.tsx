import { useId, useRef } from 'react';
import { MathBlock } from '@/components/MathBlock';
import type { ProblemReportReceipt, SubmittedProblemContext } from '@/api/types';
import { useProblemReport, type ProblemReportApi, type ProblemReportState, type ReportApplied } from './useProblemReport';

function ReportResult({ result }: { result: NonNullable<ProblemReportReceipt['result']> }) {
  return <div className="report-result">
    <p>{result.message}</p>
    <dl>
      <dt>Qwen assessment</dt><dd>{result.qwen_verdict}</dd>
      <dt>Mathematical verification</dt><dd>{result.verification}</dd>
      <dt>Resolution</dt><dd>{result.resolution.replace(/_/g, ' ')}</dd>
      <dt>Grade correction applied</dt><dd>{result.grade_corrected ? 'Yes' : 'No'}</dd>
      <dt>Verified content published</dt><dd>{result.content_published ? 'Yes' : 'No'}</dd>
    </dl>
    {result.corrected_answer ? <><p className="solution-label">Reviewed answer</p><MathBlock>{result.corrected_answer}</MathBlock></> : null}
    {result.solution ? <><p className="solution-label">Reviewed explanation</p><MathBlock>{result.solution}</MathBlock></> : null}
  </div>;
}

function ReportActions({ report, close }: { report: ProblemReportState; close: () => void }) {
  const pending = report.receipt?.status === 'queued' || report.receipt?.status === 'running';
  return <div className="actions">
    {!report.receipt ? <button type="button" className="btn btn-primary" disabled={report.busy}
      onClick={() => { void report.send(); }}>{report.error ? 'Retry sending report' : 'Send report'}</button> : null}
    {report.receipt && report.error ? <button type="button" className="btn" disabled={report.busy}
      onClick={report.refresh}>Retry loading status</button> : null}
    {report.receipt?.retryable && !pending ? <button type="button" className="btn" disabled={report.busy}
      onClick={() => { void report.send(); }}>Retry report</button> : null}
    <button type="button" className="btn btn-ghost" onClick={close}>Close report</button>
  </div>;
}

function ReportDetails({ report, submitted, hideResult, close }: {
  report: ProblemReportState; submitted: boolean; hideResult: boolean; close: () => void;
}) {
  const noteId = useId();
  const context = report.context!;
  return <>
    <h3>{submitted ? 'Report this question or its grading' : 'Report this question'}</h3>
    <MathBlock>{context.problem_text}</MathBlock>
    {submitted ? <p>Your submitted answer: <span>{context.answer || '(blank)'}</span></p> : <p>No answer submission is required to send a report.</p>}
    <p className="muted">Qwen reviews the report. Mathematical verification and any applied changes are shown separately.</p>
    <label htmlFor={noteId}>What should we check? (optional)</label>
    <textarea id={noteId} maxLength={2000} rows={3} value={report.note} disabled={report.busy || report.noteLocked}
      onChange={(event) => report.setNote(event.target.value)} />
    <div role="status" aria-live="polite" aria-atomic="true">
      {report.busy && !report.receipt ? <p>Sending report...</p> : null}
      {report.receipt ? <p>{report.receipt.status}: {hideResult ? 'Review details are withheld during this assessment.' : report.receipt.stage} (attempt {report.receipt.attempt} of {report.receipt.max_attempts})</p> : null}
    </div>
    {report.error ? <p role="alert">{report.error}</p> : null}
    {report.receipt?.result && !hideResult ? <ReportResult result={report.receipt.result} /> : null}
    <ReportActions report={report} close={close} />
  </>;
}

/** The context identifies the question independently from the answer controls. */
export function ProblemReport({ report, hideResult = false, label }: {
  report: ProblemReportState; hideResult?: boolean; label?: string | undefined;
}) {
  const panelId = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  if (!report.context) return null;
  const submitted = report.context.submitted ?? report.context.report_kind !== 'served';
  function close() { report.close(); trigger.current?.focus(); }
  return <aside className="card problem-report" aria-label={submitted ? 'Report a submitted question' : 'Report a question'}>
    <button ref={trigger} type="button" className="btn btn-ghost" aria-expanded={report.open}
      aria-controls={panelId} onClick={report.open ? close : report.show}>
      {label ?? (submitted ? 'Report submitted question' : 'Report question')}
    </button>
    <div id={panelId} hidden={!report.open}>{report.open ?
      <ReportDetails report={report} submitted={submitted} hideResult={hideResult} close={close} /> : null}</div>
  </aside>;
}

/** Mount with a question identity key to keep each report independent. */
export function QuestionReport({ api, context, hideResult = false, enabled = true, label, onApplied }: {
  api: ProblemReportApi; context: SubmittedProblemContext; hideResult?: boolean; enabled?: boolean; label?: string | undefined; onApplied?: ReportApplied;
}) {
  const report = useProblemReport(api, context, onApplied);
  if (!enabled) return null;
  return <ProblemReport report={report} hideResult={hideResult} label={label} />;
}
