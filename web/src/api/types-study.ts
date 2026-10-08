/**
 * The 2.0 HTTP contract, part 2: the session and the study loop.
 *
 * `types.ts` carries the conventions and the source-of-truth order. Every rule there holds
 * here.
 */
import type { RenderedVisual } from '@/lib/visual';
import type { Remediation, XpState } from './types';

// ---------------------------------------------------------------------------
// The session and the study loop
// ---------------------------------------------------------------------------

export interface SessionStartResponse {
  session: string;
  reopened: boolean;
  xp: XpState;
  frontier: number;
  due_reviews: number;
}

export interface SessionEndResponse {
  session: string;
  xp_earned: number;
  minutes: number;
  xp: XpState;
  /** The Anki family defers past M5 (D-M5-5), so `pending` is always 0. */
  anki: { pending: number };
}

type TaskType = 'lesson' | 'review' | 'quiz' | 'drill' | 'multi-step';

/** The client-safe topic of a plan task. No exemplar and no expected answer. */
interface TopicRef {
  id: string;
  name: string | null;
  module: string;
}

/** One planned task (`session.rs` `trim_task`). */
export interface PlanTask {
  integrated_instruction_required?: boolean;
  integrated_assessment?: boolean;
  task_id: string;
  task_type: TaskType;
  /** Null for a quiz, which mixes several topics. */
  topic: TopicRef | null;
  kp: string | null;
  start_at_kp: string | null;
  n_problems: number | null;
  mix: string[] | null;
  component_topics: string[] | null;
  /** The WHOLE-task budget. Never the per-question value of a serve (QUIZ-budget). */
  time_budget_secs: number | null;
  difficulty_target: number | null;
  /**
   * Display copy AND scheduler control state. The selector re-parses substrings of this
   * prose, so the SPA renders it verbatim and never rewrites it.
   */
  why: string | null;
  /** D-F6: the task confirms a topic the course inferred from a placement. */
  confirm?: boolean;
  /**
   * D-PR1: the open proof revision this lesson carries. The plan carries the lesson FIRST
   * while its chain is open, across a reload, a session end and the day rollover.
   */
  proof_revision?: OpenRevision;
  /**
   * Server-side completion. The session view filters the WHOLE task list on
   * `progress.done`: per-mount memory left a reload restarting at a closed task and
   * serving a `409 task_complete` the learner could not escape.
   */
  progress: { answered: number; done: boolean };
}

interface PlanConstraints {
  lesson_ratio_ok: boolean;
  lesson_ratio: number;
  throttle_ok: boolean;
  reviews: number;
  lessons: number;
}

/** `GET /api/session/plan`. */
/**
 * One planned task the readiness rule of D-F5 stopped.
 *
 * The service plans a lesson only where an approved teach page, three practice
 * items and one held-out item exist. A task it stops is NOT hidden: it stands
 * here with the conditions the content does not meet, so the learner reads why
 * the topic is absent instead of finding a silent gap.
 */
export interface BlockedTask {
  task_type: TaskType;
  topic: string;
  /** The knowledge point of a blocked lesson, or null. */
  kp: string | null;
  /** `teachable`, `practicable`, `assessable`, and the rest of the seven. */
  blockers: string[];
}

export interface SessionPlanResponse {
  session: string;
  tasks: PlanTask[];
  quiz_due: boolean;
  constraints: PlanConstraints;
  course_complete: boolean;
  /** The planned tasks the readiness rule stopped. */
  blocked: BlockedTask[];
  /** RFC 3339, or null when nothing blocks the frontier. */
  frontier_blocked_until: string | null;
  /**
   * D-PR1: every open revision chain (lesson and review), oldest first. A lesson chain names
   * the plan task that carries it.
   */
  open_revisions?: OpenRevision[];
}

/** What a learner may do next with one revision chain (D-PR1). */
export type ProofPhase =
  | 'draft'
  | 'grading'
  | 'passed'
  | 'revise'
  | 'reveal'
  | 'rewrite'
  | 'unavailable'
  | 'closed';

/** Where a written proof was drafted. A lesson proof blocks its knowledge point. */
export type ProofContext = 'lesson' | 'review' | 'quiz' | 'selfcheck' | 'legacy';

/** One open revision chain, as the plan names it. */
export interface OpenRevision {
  job_id: string;
  context: ProofContext;
  topic: string;
  topic_name: string | null;
  kp: string | null;
  phase: ProofPhase;
  revision: number;
  /** Whether the learner saw the verdict of the head (`seen_at`). */
  seen: boolean;
  /** The plan task that carries a lesson chain. */
  task_id: string | null;
}

/** The first check a revision fixes, with the learner's own words. */
export interface ProofUnmet {
  id: string;
  text: string;
  evidence: string;
  quote_verified?: boolean;
}

/**
 * The `proof` field of a served written proof (D-PR1): the chain state the screen draws.
 * It never carries the reference solution.
 */
export interface ProofField {
  context: ProofContext;
  phase: ProofPhase;
  job_id?: string;
  status?: ProofGradingStatus;
  revision: number;
  cap: number;
  revisions_left: number;
  /** The learner's last draft, to revise. */
  draft?: string;
  feedback?: string;
  first_unmet?: ProofUnmet | null;
  seen?: boolean;
  disputed?: boolean;
  /** On a grade reply: the unaided rewrite after the cap. */
  rewrite?: boolean;
}

/**
 * `POST /api/task/{task_id}/serve` — the task's live problem.
 *
 * Seven keys on every serve, `hint_available beside them on every non-quiz serve (H-3),
 * and the quiz clock as the eighth on a QUIZ serve (`serve.rs`
 * `serve_payload`). `expected` and `solution_sketch` are named out of this payload on
 * purpose (Hard Rule 1).
 *
 * The route is IDEMPOTENT: calling it twice re-serves the same problem and re-stamps
 * `started_at`. A mock that advances a cursor per call makes the learner practise the
 * wrong problem (trap T13, SERVE-idem).
 */
export interface ServedProblem {
  /** A fresh practice item outside the original assessment count. */
  feedback_practice?: boolean;
  /**
   * The task this problem belongs to, on a problem of a mixed review block alone. The
   * block serves the questions of its reviews interleaved, so a serve or a grade can
   * hand back another review's problem; the view answers it on this task.
   */
  task_id?: string;
  /** The block's progress, on a problem of a mixed review block alone. 1-based. */
  mixed_review?: { position: number; total: number };
  problem_id: string;
  /** 1-based. */
  index: number;
  /** Null when the task type fixes no count. */
  total: number | null;
  text: string;
  kp: string | null;
  /** Per-question expected time. Never the whole-task clock. */
  time_budget_secs: number | null;
  /** Only a drill counts down. */
  countdown: boolean;
  /**
   * The seconds the WHOLE quiz has run, on a quiz serve alone (QUIZ-budget).
   *
   * The quiz clock is server state: `crates/web/src/state.rs` `QuizBuffer.started_at`
   * holds the start, and the serve reports the seconds since it. A page reload therefore
   * resumes the running clock. Absent on every other task type, and absent on an open
   * quiz stamped by no serve yet.
   */
  quiz_elapsed_secs?: number;
  /**
   * True on an item whose answer is a sentence (the contract `written`, D-PR1). It takes the
   * proof loop with its screen worded for an answer. ABSENT on every other item.
   */
  written?: boolean;
  /**
   * The drawn figures of this knowledge point (unit f9).
   *
   * The server renders the SVG and the text equivalent together
   * (`cadus_core::visual::render_all`), so the browser repeats no geometry. The key is
   * ABSENT when the knowledge point authors no figure, and a figure the check refuses
   * never reaches this list.
   */
  visuals?: RenderedVisual[];
  /**
   * Whether this knowledge point holds an approved hint ladder (H-3).
   *
   * The serve stamps it from the same content store read the readiness gate
   * uses. Absent on a payload whose serving key the index does not name; a
   * `false` means the hint route can only refuse, so the view hides the
   * affordance instead of spending the round trip on `409 no_hint_ladder`.
   */
  hint_available?: boolean;
  /**
   * The display text of each option of a top-level Label item, in the order to show.
   *
   * The key is ABSENT for each other item. The order of the service is stable for one
   * `problem_id`, so the view does not sort and does not shuffle. The options are not the
   * key: the answer is not in this payload (Hard Rule 1).
   */
  choices?: string[];
  /** The answer contract of the item, when the service serves one. It only picks the input hint. */
  answer_contract?: AnswerContractHint;
  /**
   * D-PR1: a written proof. Its solution waits for a pass or for the revision cap, so the
   * payload never carries one.
   */
  proof?: ProofField;
}

/** `POST /api/task/{task_id}/teach` — the authored teach page (L4). */
export interface TeachResponse {
  kp: string;
  concept: string;
  /** The service sends one entry per step; an older page may send one string. */
  worked_example: { problem: string; steps: string | string[] };
  /**
   * A motivating problem attempted BEFORE the worked example (step 5a). No answer and no
   * reveal here: both arrive from `taskTeachCheck` after the attempt. `choices` follows the
   * `ServedProblem` rule.
   */
  try_first?: { problem: string; choices?: string[] };
}

/** The body of `POST /api/task/{task_id}/teach/check`. */
export type TeachCheckRequest = { part: 'try_first'; answer: string };

/**
 * The reply of `POST /api/task/{task_id}/teach/check`. The service writes nothing for it:
 * no event, no XP, no mastery, no scheduling.
 */
export type TeachCheckResponse = { part: 'try_first'; outcome: AttemptOutcome; correct: boolean; answer: string; reveal: string };

/** `POST /api/task/{task_id}/hint` — one rung of the authored ladder (L5). */
export interface HintResponse {
  /** A hint never contains the expected answer (Hard Rule 1). */
  hint: string;
  hint_number: number;
  /** W-A4: sent once, after three hints on a review or a multi-step part. */
  reference_lesson?: { topic: string; name: string };
}

/**
 * The three outcomes of one attempt (D-F2).
 *
 * `ungraded` is the third one: the checker had no deterministic verdict, so the attempt
 * moved nothing and the reply names a `reason` instead of a `correct`.
 */
export type AttemptOutcome = 'correct' | 'incorrect' | 'ungraded';

type WorkQuality =
  | 'perfect'
  | 'nearly_perfect'
  | 'passable'
  | 'nearly_passable'
  | 'poor'
  | 'blowoff';

type TaskStatus =
  | 'continue'
  | 'kp_advance'
  | 'task_passed'
  | 'task_inconclusive'
  | 'task_failed'
  /** D-PR1: a lesson proof waits for its verdict; the point closes on a pass. */
  | 'proof_pending'
  /** NOTHING was recorded twice: the attempt already stood. Never a normal advance. */
  | 'already_recorded';

/** The `diagnosis` field of a grade reply: no job row was written. */
interface DiagnosisNotOffered {
  status: 'not_offered';
}

/** A pre-authored distractor diagnosis matched, in the grade transaction. No model call. */
interface DiagnosisReady {
  status: 'ready';
  error_tags: string[];
  prose: string;
}

/** A `diagnosis_jobs` row was inserted. `id` is its primary key; subscribe or poll. */
interface DiagnosisPending {
  id: string;
  status: 'pending';
}

/** The `diagnosis` field of a grade reply. Null for a quiz, which reveals nothing. */
export type DiagnosisField = DiagnosisNotOffered | DiagnosisReady | DiagnosisPending | null;

/** One placement probe. `topic` is a bare name in 1.0 and a record in 2.0; both render. */
export interface DiagProbe {
  problem_id: string;
  topic?: TopicRef | string | null;
  text: string;
  /** The options of a top-level Label item, as on `ServedProblem`. Absent for each other item. */
  choices?: string[];
  answer_contract?: AnswerContractHint;
}

/**
 * `POST /api/diag/start`.
 *
 * `probe` is NULLABLE. A diagnostic whose probe list is exhausted answers `{"probe": null}`,
 * and a screen that reads `probe.text` off it goes blank.
 */
export interface DiagStartResponse {
  probe: DiagProbe | null;
  asked?: number;
  cap?: number;
}

/**
 * `POST /api/diag/answer`.
 *
 * The verdict is deterministic-only and the reply carries NO solution and NO expected
 * answer. The screen renders a tick or a cross from `correct` and nothing else, even if a
 * future payload grew a field (DIAG-nosol).
 */
export interface DiagAnswerResponse {
  outcome?: 'correct' | 'incorrect' | 'ungraded';
  reason?: string;
  correct?: boolean;
  next_probe?: DiagProbe | { done: true } | null;
}

/** `POST /api/diag/finish`. Three arrays of topic ids. */
export interface DiagFinishResponse {
  placed: string[];
  conditional: string[];
  frontier: string[];
}

/** `GET /api/diagnosis/{id}` and the `event: diagnosis` frame of the stream. */
export interface DiagnosisJob {
  id: string;
  /** A job still `pending` 30 s after the grade reads `failed`, never open forever. */
  status: 'pending' | 'ready' | 'failed' | 'capped';
  /** Empty unless `status` is `ready`. */
  error_tags: string[];
  prose?: string;
  model_id?: string;
}

/**
 * `POST /api/task/{id}/answer` on a non-quiz task: the whole verdict, from local CPU.
 *
 * `solution` and `re_solve` are absent on a quiz and `re_solve` is absent on a correct
 * answer, so both are optional. `next_unavailable` and `xp` are inserted only when they
 * apply.
 */
/** The served shape of an answer contract: `{kind, form}`, as in `required_form` / `mixed_number`. */
export interface AnswerContractHint {
  kind: string;
  form?: string;
  /** Set on a list contract: whether the order of the members counts. */
  ordered?: boolean;
}

export interface AnswerResponse {
  /** The notation hint text of a wrong-form answer, when the grader sends one. */
  notation?: string | boolean;
  /** Set after the report service confirms a committed grade correction. */
  report_corrected?: boolean;
  /** The next question confirms independent work after feedback. */
  feedback_practice?: boolean;
  /** A fresh item is unavailable; the original answer remains saved. */
  feedback_blocked?: boolean;
  attempt_id: string;
  /**
   * The graded outcome (D-F2). `ungraded` means the checker reached no verdict.
   *
   * An ungraded attempt is NOT a miss: nothing about the learner moved, no solution is
   * revealed, and no diagnosis fires.
   */
  outcome: AttemptOutcome;
  /**
   * Mathematical correctness only. Partial credit lives in `work_quality`.
   *
   * ABSENT on an `ungraded` reply: the service claims no correctness there, and a
   * `false` would read as a miss.
   */
  correct?: boolean;
  /** Why the attempt has no verdict. Present on an `ungraded` reply only. */
  reason?: string;
  work_quality: WorkQuality;
  /** Rendered verbatim, never re-interpreted (trap T3). */
  error_tags: string[];
  secs: number;
  task_status: TaskStatus;
  remediation: Remediation[];
  /** The next problem. `null` means the task closed, unless `next_unavailable`. */
  next: ServedProblem | null;
  diagnosis: DiagnosisField;
  /** Revealed once the attempt commits (Hard Rule 1). Never on a quiz. */
  solution?: string;
  /** The stock re-solve instruction. Only on a miss, and never on a quiz. */
  re_solve?: string;
  /**
   * Amendment K (note 114): the background equivalence field. `pending` = the
   * answer is being checked and the verdict may flip to correct; `accepted` =
   * a cached EQUIVALENT verdict already graded it correct; `refused` = the
   * check kept the wrong verdict and `equivalence_reason` carries the why.
   */
  equivalence?: {
    id?: string;
    status: 'pending' | 'accepted' | 'refused' | 'failed';
    reason?: string;
    model?: string;
    /** What the check has done so far, in order. Absent until the service sends it. */
    steps?: EquivalenceStep[];
    /** The standard form of an accepted answer whose form differed. */
    accepted_form?: string;
  };
  /** The model's one-line why of a refused equivalence verdict. */
  equivalence_reason?: string;
  /** The learner line of a refused check: "Expected 6/5 (1.2). You entered 2." */
  checker_text?: string;
  /**
   * Amendment K point 6: the background grading of a written proof. Present (pending,
   * with the job id to poll) only on an ungraded written proof; `null` otherwise.
   */
  proof_grading?: ProofGradingField;
  /**
   * The attempt IS recorded and the task is NOT finished — no next problem could be drawn.
   * A bare `next: null` on an open task reads as "task over" and silently skips the
   * problems the learner still owes, so the client re-serves the same task instead.
   */
  next_unavailable?: boolean;
  xp?: number;
  /** D-PR1: the chain state after a lesson proof's draft or rewrite. */
  proof?: ProofField;
}

/** The reply field of a written proof whose background grading is pending. */
export type ProofGradingField = { id: string; status: 'pending' } | null;

/** Where one background proof grading stands. */
export type ProofGradingStatus = 'pending' | 'pass' | 'needs_revision' | 'failed' | 'capped';

/** One yes/no check of a graded proof, with the learner's own words as evidence. */
export interface ProofCheck {
  id: string;
  text: string;
  met: boolean;
  minor: boolean;
  /** A short quote from the learner's text, or "not found". */
  evidence: string;
  /** Whether the quote was found in the learner's text. A met check needs one. */
  quote_verified?: boolean;
}

/** The poll reply of one background proof grading (Amendment K point 6). */
export interface ProofGradingPoll {
  id: string;
  attempt_id: string;
  status: ProofGradingStatus;
  feedback?: string;
  checks?: ProofCheck[];
  /** The first check to fix, with the learner's own words. */
  first_unmet?: ProofUnmet | null;
  /**
   * The reference solution. A quiz proof shows it once graded; a lesson or review proof
   * only after a pass or once its chain closed (D-PR1).
   */
  solution?: string;
  model?: string;
  context?: ProofContext;
  revision?: number;
  rewrite?: boolean;
  disputed?: boolean;
  /** The chain the job belongs to. */
  chain?: {
    root_id: string;
    head_id: string;
    phase: ProofPhase;
    revision: number;
    cap: number;
    revisions_left: number;
  };
}

/** One draft of a revision chain. */
export interface ProofVersion {
  id: string;
  attempt_id: string;
  revision: number;
  rewrite: boolean;
  status: ProofGradingStatus;
  answer: string;
  created_at: string;
  seen: boolean;
  disputed: boolean;
  human_verdict: 'pass' | 'needs_revision' | null;
  feedback?: string;
  checks?: ProofCheck[];
  first_unmet?: ProofUnmet | null;
}

/** One problem's revision chain (D-PR1). */
export interface ProofChain {
  root_id: string;
  head_id: string;
  context: ProofContext;
  task_id: string | null;
  topic: string | null;
  topic_name?: string | null;
  kp: string | null;
  problem: string | null;
  phase: ProofPhase;
  revision: number;
  cap: number;
  revisions_left: number;
  passed: boolean;
  /** The head's text. */
  draft: string;
  versions: ProofVersion[];
  feedback?: string;
  first_unmet?: ProofUnmet | null;
  /** Present only after a pass, once the chain closed, or in the ONE reveal at the cap. */
  solution?: string;
}

/** `POST /api/proof-grading/{id}/seen`. */
export interface ProofSeenResponse {
  job: ProofGradingPoll;
  chain: ProofChain;
}

/** `GET /api/proofs`. */
export interface ProofsResponse {
  chains: ProofChain[];
}

/** `POST /api/proofs/{id}/revise`. */
export interface ProofReviseResponse {
  proof_grading: ProofGradingField;
  chain: ProofChain;
}

/** `POST /api/task/{task_id}/proof/continue`: a passed proof closed its knowledge point. */
export interface ProofContinueResponse {
  task_status: TaskStatus;
  remediation: Remediation[];
  next: ServedProblem | null;
  next_unavailable?: boolean;
  xp?: number;
}

/** One learner-facing line of a background check. */
export interface EquivalenceStep { text: string }

/** The poll reply of one background equivalence check (Amendment K, note 114). */
export interface EquivalencePoll {
  id: string;
  attempt_id: string;
  verdict: {
    status: 'pending' | 'accepted' | 'refused' | 'failed';
    reason?: string;
    model?: string;
    steps?: EquivalenceStep[];
    accepted_form?: string;
    /** The XP the corrected attempt earned. */
    xp?: number;
  };
}

/**
 * The H3 first branch (DD-3/P1): an ASSISTED answer that graded correct.
 *
 * Nothing is recorded. The problem stays live, the solution is revealed to study, and the
 * next submission re-posts the SAME `problem_id` as the unaided re-solve. `feedback` is
 * not terminal.
 */
export interface ReworkResponse {
  rework_required: true;
  problem_id: string;
  solution: string | null;
  expected: string;
  re_solve: string;
}

/**
 * A quiz answer before the batch reveal. A receipt and nothing else (trap W7).
 *
 * There is no verdict, no solution and no expected answer to leak — the guarantee is
 * structural (QUIZ-reveal). `quiz_complete` says the quiz closed with this answer.
 */
export interface QuizReceiptResponse {
  accepted: true;
  remaining: number;
  quiz_complete: boolean;
}

export type TaskAnswerResponse = AnswerResponse | ReworkResponse | QuizReceiptResponse;

/** Narrow a grade reply to the third outcome (D-F2). */
export function isUngraded(reply: AnswerResponse): boolean {
  return reply.outcome === 'ungraded';
}

/** Narrow a grade reply to the H3 rework branch. */
export function isRework(reply: TaskAnswerResponse): reply is ReworkResponse {
  return (reply as ReworkResponse).rework_required === true;
}

/** Narrow a grade reply to the quiz receipt. */
export function isQuizReceipt(reply: TaskAnswerResponse): reply is QuizReceiptResponse {
  return (reply as QuizReceiptResponse).accepted === true;
}


/** Evidence released after every original quiz answer is recorded. */
export interface QuizResultResponse {
  inconclusive: boolean;
  score: number;
  xp: number;
  practice_pending: boolean;
  practice_available: boolean;
  answers: {
    problem_id: string; text: string; given_answer: string; correct: boolean; outcome: string;
    reason?: string | null; solution_sketch?: string | null;
    /** Amendment K point 6: a written proof's background grading, enqueued at the reveal. */
    attempt_id?: string; proof_grading?: ProofGradingField;
  }[];
}
