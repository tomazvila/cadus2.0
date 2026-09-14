/** Reports name server-owned questions and optional recorded attempts. */
export interface ProblemReportSubmission {
  problem_id: string;
  attempt_id?: string;
  report_kind?: 'served' | 'attempt' | 'integrated' | 'diagnostic';
  item_digest?: string;
  field_id?: string;
  request_id: string;
  note?: string;
}

export interface ProblemReportReceipt {
  report_id: string;
  status: 'queued' | 'running' | 'completed' | 'unresolved' | 'failed';
  stage: string;
  attempt: number;
  max_attempts: number;
  retryable: boolean;
  result?: {
    resolution: 'confirmed_issue' | 'no_issue_found' | 'needs_review';
    message: string;
    qwen_verdict: 'correct' | 'incorrect' | 'ambiguous';
    verification: 'proved' | 'disproved' | 'unresolved';
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
