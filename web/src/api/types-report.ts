/** Reports name server-owned questions and optional recorded attempts. */
export interface ProblemReportSubmission {
  problem_id: string;
  attempt_id?: string;
  report_kind?: 'served' | 'attempt' | 'integrated' | 'diagnostic';
  item_digest?: string;
  field_id?: string;
  request_id: string;
  note?: string;
  /** Sent only for `question_unclear`: the server sets the question aside and starts no review. */
  concern?: ReportConcern;
}

/** What the learner says is wrong. */
export type ReportConcern = 'answer_right' | 'key_wrong' | 'question_unclear';

export interface ProblemReportReceipt {
  report_id: string;
  status: 'queued' | 'running' | 'completed' | 'unresolved' | 'failed' | 'waiting_owner';
  stage: string;
  attempt: number;
  max_attempts: number;
  retryable: boolean;
  /** True when the server set the question aside for this learner. */
  set_aside?: boolean;
  result?: {
    resolution: 'confirmed_issue' | 'regraded' | 'no_issue_found' | 'needs_review';
    message: string;
    explanation?: string;
    qwen_verdict: 'correct' | 'incorrect' | 'ambiguous';
    verification: 'proved' | 'disproved' | 'model_judged' | 'unresolved';
    corrected_answer?: string;
    solution?: string;
    grade_corrected: boolean;
    corrected_outcome?: 'correct' | null;
    content_published: boolean;
  };
}

/** Frozen display context; the server reconstructs trusted grading evidence. */
export interface SubmittedProblemContext {
  task_id: string;
  problem_id: string;
  attempt_id?: string;
  report_kind?: ProblemReportSubmission['report_kind'];
  item_digest?: string;
  field_id?: string;
  submitted?: boolean;
  problem_text: string;
  answer: string;
  work: string;
}

/** The word that names how a finished report ended. `null` while the check runs. */
export type ReportOutcome =
  | 'regraded' | 'no_issue_found' | 'confirmed_issue' | 'needs_review' | 'failed'
  | 'question_kept' | 'question_retired';

/** One row of `GET /api/reports`: a report of the signed-in learner. */
export interface MyReport {
  report_id: string;
  kind?: 'check' | 'question_unclear';
  task_id: string;
  problem_id: string;
  topic: string | null;
  question: string | null;
  note: string | null;
  status: ProblemReportReceipt['status'];
  stage: string;
  outcome: ReportOutcome | null;
  grade_corrected: boolean;
  /** True while an open quiz or placement keeps the details back. */
  details_hidden: boolean;
  explanation?: string | null;
  corrected_answer?: string | null;
  created_at: string;
  updated_at: string;
}

export interface MyReportsResponse {
  reports: MyReport[];
}

/** One row of `GET /api/admin/reports`. */
export interface AdminReport {
  report_id: string;
  kind?: 'check' | 'question_unclear';
  user_id: string;
  task_id: string;
  problem_id: string;
  topic: string | null;
  kp: string | null;
  question: string | null;
  note: string | null;
  status: ProblemReportReceipt['status'] | 'kept' | 'retired';
  attempt: number;
  result_message: string | null;
  last_step: { stage: string | null; message: string | null };
  created_at: string;
  updated_at: string;
}

export interface AdminReportsResponse {
  reports: AdminReport[];
}
