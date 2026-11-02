import { useId, useRef } from 'react';
import { MathBlock } from '@/components/MathBlock';
import type { ProblemReportReceipt, SubmittedProblemContext } from '@/api/types';
import { reportStatusText } from '@/lib/stageCopy';
import type { ReportConcern } from '@/api/types';
import { useOpenReports } from '@/app/reportsLink';
import { canFlagQuestion, useProblemReport, type ProblemReportApi, type ProblemReportState, type ReportApplied } from './useProblemReport';

function ReportResult({ result }: { result: NonNullable<ProblemReportReceipt['result']> }) {
  return <div className="report-result">
    <p>{result.message}</p>
    {result.explanation ? <MathBlock>{result.explanation}</MathBlock> : null}
    {result.content_published ? <p className="muted">The stored answer was corrected for everyone.</p> : null}
    {result.corrected_answer ? <><p className="solution-label">Reviewed answer</p><MathBlock>{result.corrected_answer}</MathBlock></> : null}
    {result.solution ? <><p className="solution-label">Reviewed explanation</p><MathBlock>{result.solution}</MathBlock></> : null}
  </div>;
}

function ReportActions({ report, close }: { report: ProblemReportState; close: () => void }) {
  const pending = report.receipt?.status === 'queued' || report.receipt?.status === 'running';
  const openReports = useOpenReports();
  return <div className="actions">
    {!report.receipt ? <button type="button" className="btn btn-primary" disabled={report.busy}
      onClick={() => { void report.send(); }}>{report.error ? 'Retry sending report' : 'Send report'}</button> : null}
    {report.receipt && report.error ? <button type="button" className="btn" disabled={report.busy}
      onClick={report.refresh}>Retry loading status</button> : null}
    {report.receipt?.status === 'failed' && !pending ? <button type="button" className="btn" disabled={report.busy}
      onClick={() => { void report.send(); }}>Retry report</button> : null}
    {openReports ? <button type="button" className="btn btn-ghost" onClick={openReports}>See my reports</button> : null}
    <button type="button" className="btn btn-ghost" onClick={close}>Close report</button>
  </div>;
}

const CONCERNS: { value: ReportConcern; label: string }[] = [
  { value: 'answer_right', label: 'My answer is right' },
  { value: 'key_wrong', label: 'The answer key is wrong' },
  { value: 'question_unclear', label: 'The question is unclear or wrong' },
];

/** Three plain choices. The learner says what is wrong before the note. */
function ConcernChoice({ report, submitted }: { report: ProblemReportState; submitted: boolean }) {
  const choices = CONCERNS.filter((c) =>
    (c.value !== 'answer_right' || submitted) && (c.value !== 'question_unclear' || canFlagQuestion(report.context)));
  const locked = report.busy || report.noteLocked;
  return <div role="radiogroup" aria-label="What is wrong?" className="report-concerns">
    <p>What is wrong?</p>
    {choices.map((c) => <button key={c.value} type="button" role="radio" aria-checked={report.concern === c.value}
      className={report.concern === c.value ? 'btn btn-primary' : 'btn'} disabled={locked}
      onClick={() => report.setConcern(c.value)}>{c.label}</button>)}
    {report.concern === 'question_unclear' ? <p className="field-hint">The question is set aside for you at once. The owner reads your report and decides. No automatic check runs.</p>
      : <p className="field-hint">A reviewer checks the answer, explains the grade, and corrects it when your answer is right.</p>}
  </div>;
}

function ReportDetails({ report, submitted, hideResult, close }: {
  report: ProblemReportState; submitted: boolean; hideResult: boolean; close: () => void;
}) {
  const noteId = useId();
  const context = report.context!;
  return <>
    <h3>Report this question</h3>
    {report.receipt ? <p>Report sent</p> : null}
    <MathBlock>{context.problem_text}</MathBlock>
    {submitted ? <p>Your submitted answer: <span>{context.answer || '(blank)'}</span></p> : <p>No answer submission is required to send a report.</p>}
    {report.receipt ? null : <ConcernChoice report={report} submitted={submitted} />}
    <label className="auth-label" htmlFor={noteId}>What do you think is wrong? (optional)</label>
    <p className="field-hint">For example the answer you expected, or why your spelling of it is the same value. The reviewer answers this in the result.</p>
    {/* A styled textarea, not a bare UA one: unstyled, the textarea
        flows inline with its label and the two overlap. */}
    <textarea id={noteId} className="work-input" maxLength={2000} rows={3} value={report.note} disabled={report.busy || report.noteLocked}
      onChange={(event) => report.setNote(event.target.value)} />
    <div role="status" aria-live="polite" aria-atomic="true">
      {report.busy && !report.receipt ? <p>Sending report...</p> : null}
      {report.receipt ? <p>{hideResult && report.receipt.status !== 'waiting_owner' ? 'Review details stay hidden until the quiz ends.' : reportStatusText(report.receipt.status, report.receipt.stage, report.receipt.attempt, report.receipt.max_attempts, report.receipt.result?.resolution)}</p> : null}
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
      {label ?? 'Report this question'}
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
