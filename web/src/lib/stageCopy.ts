/**
 * The one place that turns server routing words into learner words. The plan sends `task_type`
 * and a `why` line that the selector reads back as control state, so the data stays verbatim
 * and only the display passes through here. Operator and diag views show the raw values.
 */

/** The chip of a task: what the learner is about to do. */
export function taskKindText(taskType: string, why: string | null | undefined): string {
  const reason = why ?? '';
  if (reason.startsWith('remediation (')) return 'Extra practice on a skill you missed';
  if (reason.startsWith('confirmation;')) return 'Checking a skill the starting questions assumed';
  switch (taskType) {
    case 'lesson': return 'Lesson';
    case 'review': return 'Review';
    case 'quiz': return 'Quiz';
    case 'drill': return 'Speed practice';
    case 'multi-step': return 'Mixed problem';
    default: return 'Practice';
  }
}

/** The one-line reason under the header; null when the server line has no learner meaning. */
export function taskReasonText(why: string | null | undefined): string | null {
  const reason = why ?? '';
  if (reason.startsWith('remediation (')) {
    if (reason.includes('peel-back lesson')) return 'You missed this skill earlier, so the lesson comes again.';
    if (reason.includes('remedial review')) return 'You missed this skill earlier, so these questions check it again.';
    return 'You missed this skill earlier, so you get more practice on it.';
  }
  if (reason.startsWith('confirmation;')) return 'The starting questions assumed you know this skill, so these questions check it.';
  if (reason.startsWith('due review') || reason.startsWith('nearly-due review')) return 'This is a skill to review so that you keep it.';
  if (reason.startsWith('gap-fill lesson')) return 'This is a skill you need before the course goes on.';
  if (reason.startsWith('frontier lesson')) return 'This is the next new skill in your course.';
  if (reason.startsWith('quiz due')) return 'This is a quiz on skills you learned.';
  if (reason.startsWith('automaticity drill')) return 'This is speed practice on a skill you know.';
  if (reason.startsWith('multi-part integration')) return 'This is one problem that mixes several skills you learned.';
  return null;
}

const REPORT_STAGE: Record<string, string> = {
  starting: 'Starting the check',
  preparing: 'Reading your report',
  judge: 'Judging the answer',
  producer: 'Working out a solution',
  formalizer: 'Writing the problem as exact math',
  verification: 'Checking the math',
  critic: 'Looking for mistakes',
  adjudicator: 'Checking your report',
  regressions: 'Checking your report',
  core_checks: 'Checking your report',
};

/** The status line of a problem report. Only the end states name the count of checks. */
export function reportStatusText(
  status: string, stage: string, attempt: number, maxAttempts: number,
): string {
  const tries = `check ${String(attempt)} of ${String(maxAttempts)}`;
  switch (status) {
    case 'queued': return `Your report is waiting to be checked (${tries}).`;
    case 'running': return `${REPORT_STAGE[stage] ?? 'Checking your report'}.`;
    case 'completed': return `The check is finished (${tries}).`;
    case 'unresolved': return `The check could not settle this question (${tries}). You can send the report again.`;
    default: return `The check stopped before it finished (${tries}). You can send the report again.`;
  }
}
