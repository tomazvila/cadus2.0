import type { AnswerResponse, ProblemReportReceipt, SubmittedProblemContext } from '@/api/types';

/** Update only the displayed attempt that the server corrected. */
export function applyReportCorrection(previous: AnswerResponse | null, receipt: ProblemReportReceipt, context: SubmittedProblemContext): AnswerResponse | null {
  if (!previous || previous.attempt_id !== context.attempt_id
    || !receipt.result?.grade_corrected || receipt.result.corrected_outcome !== 'correct') return previous;
  const corrected: AnswerResponse = { ...previous, correct: true, outcome: 'correct',
    report_corrected: true, error_tags: [], diagnosis: null };
  delete corrected.reason;
  delete corrected.re_solve;
  if (receipt.result.solution) corrected.solution = receipt.result.solution;
  return corrected;
}
