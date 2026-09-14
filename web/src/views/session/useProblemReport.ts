import { useEffect, useRef, useState } from 'react';
import type { ApiClient, ProblemReportReceipt, ProblemReportSubmission, SubmittedProblemContext } from '@/api/types';

const active = (receipt: ProblemReportReceipt) => receipt.status === 'queued' || receipt.status === 'running';

/** One independent report request, with cancellation and idempotent transport retries. */
export type ProblemReportApi = Pick<ApiClient, 'taskReport' | 'getProblemReport'>;
export type ReportApplied = (receipt: ProblemReportReceipt, context: SubmittedProblemContext) => void;

export function useProblemReport(api: ProblemReportApi | null, initialContext?: SubmittedProblemContext, onApplied?: ReportApplied) {
  const [context, setContext] = useState<SubmittedProblemContext | null>(() => initialContext ? { ...initialContext } : null);
  const [open, setOpen] = useState(false);
  const [note, setNote] = useState('');
  const [receipt, setReceipt] = useState<ProblemReportReceipt | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [noteLocked, setNoteLocked] = useState(false);
  const controller = useRef<AbortController | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const submission = useRef<ProblemReportSubmission | null>(null);
  const applied = useRef<string | null>(null);

  function stop() {
    controller.current?.abort();
    controller.current = null;
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
  }

  useEffect(() => () => {
    controller.current?.abort();
    if (timer.current !== null) clearTimeout(timer.current);
  }, []);

  function remember(value: SubmittedProblemContext) {
    stop();
    submission.current = null;
    applied.current = null;
    setNoteLocked(false);
    setContext({ ...value });
    setReceipt(null);
    setNote('');
    setError('');
    setBusy(false);
    setOpen(false);
  }

  function accept(value: ProblemReportReceipt) {
    setReceipt(value);
    if (context && onApplied && applied.current !== value.report_id
      && value.result?.grade_corrected && value.result.corrected_outcome === 'correct') {
      applied.current = value.report_id;
      onApplied(value, context);
    }
    setNoteLocked(!value.retryable);
    setBusy(false);
    if (active(value)) timer.current = setTimeout(() => { void refresh(value.report_id); }, 2000);
  }

  async function refresh(reportId: string) {
    if (!api) return;
    stop();
    const request = new AbortController();
    controller.current = request;
    setError('');
    setBusy(true);
    try {
      const value = await api.getProblemReport(reportId, request.signal);
      if (request.signal.aborted) return;
      controller.current = null;
      accept(value);
    } catch (failure) {
      if (request.signal.aborted) return;
      controller.current = null;
      setBusy(false);
      setError(failure instanceof Error ? failure.message : 'Report status could not be loaded.');
    }
  }

  async function send() {
    if (!api || !context || controller.current || (receipt && !receipt.retryable)) return;
    stop();
    // A failed POST may already have queued work. Reuse the exact request on transport retry.
    if (receipt?.retryable) {
      submission.current = null;
      setReceipt(null);
    }
    submission.current ??= {
      problem_id: context.problem_id,
      ...(context.attempt_id ? { attempt_id: context.attempt_id } : {}),
      ...(context.report_kind ? { report_kind: context.report_kind } : {}),
      ...(context.item_digest ? { item_digest: context.item_digest } : {}),
      ...(context.field_id ? { field_id: context.field_id } : {}),
      request_id: crypto.randomUUID(), ...(note.trim() ? { note: note.trim().slice(0, 2000) } : {}),
    };
    setNoteLocked(true);
    const request = new AbortController();
    controller.current = request;
    setBusy(true);
    setError('');
    try {
      const value = await api.taskReport(context.task_id, submission.current, request.signal);
      if (request.signal.aborted) return;
      controller.current = null;
      accept(value);
    } catch (failure) {
      if (request.signal.aborted) return;
      controller.current = null;
      setBusy(false);
      setError(failure instanceof Error ? failure.message : 'The report could not be submitted.');
    }
  }

  function show() {
    setOpen(true);
    if (receipt && active(receipt) && !controller.current) void refresh(receipt.report_id);
  }

  function close() {
    stop();
    setBusy(false);
    setOpen(false);
  }

  return { context, open, note, setNote, receipt, error, busy, remember, show, close, send,
    refresh: () => { if (receipt) void refresh(receipt.report_id); },
    noteLocked,
  };
}

export type ProblemReportState = ReturnType<typeof useProblemReport>;
