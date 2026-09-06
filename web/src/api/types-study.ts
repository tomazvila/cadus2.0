/**
 * The 2.0 HTTP contract, part 2: the session and the study loop.
 *
 * `types.ts` carries the conventions and the source-of-truth order. Every rule there holds
 * here.
 */
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
export interface SessionPlanResponse {
  session: string;
  tasks: PlanTask[];
  quiz_due: boolean;
  constraints: PlanConstraints;
  course_complete: boolean;
  /** RFC 3339, or null when nothing blocks the frontier. */
  frontier_blocked_until: string | null;
}

/**
 * `POST /api/task/{task_id}/serve` — the task's live problem.
 *
 * Seven keys on every serve, and an eighth on a QUIZ serve (`serve.rs`
 * `serve_payload`). `expected` and `solution_sketch` are named out of this payload on
 * purpose (Hard Rule 1).
 *
 * The route is IDEMPOTENT: calling it twice re-serves the same problem and re-stamps
 * `started_at`. A mock that advances a cursor per call makes the learner practise the
 * wrong problem (trap T13, SERVE-idem).
 */
export interface ServedProblem {
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
}

/** `POST /api/task/{task_id}/teach` — the authored teach page (L4). */
export interface TeachResponse {
  kp: string;
  concept: string;
  worked_example: { problem: string; steps: string };
}

/** `POST /api/task/{task_id}/hint` — one rung of the authored ladder (L5). */
export interface HintResponse {
  /** A hint never contains the expected answer (Hard Rule 1). */
  hint: string;
  hint_number: number;
  /** W-A4: sent once, after three hints on a review or a multi-step part. */
  reference_lesson?: { topic: string; name: string };
}

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
  | 'task_failed'
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
export interface AnswerResponse {
  attempt_id: string;
  /** Mathematical correctness only. Partial credit lives in `work_quality`. */
  correct: boolean;
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
   * The attempt IS recorded and the task is NOT finished — no next problem could be drawn.
   * A bare `next: null` on an open task reads as "task over" and silently skips the
   * problems the learner still owes, so the client re-serves the same task instead.
   */
  next_unavailable?: boolean;
  xp?: number;
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

/** Narrow a grade reply to the H3 rework branch. */
export function isRework(reply: TaskAnswerResponse): reply is ReworkResponse {
  return (reply as ReworkResponse).rework_required === true;
}

/** Narrow a grade reply to the quiz receipt. */
export function isQuizReceipt(reply: TaskAnswerResponse): reply is QuizReceiptResponse {
  return (reply as QuizReceiptResponse).accepted === true;
}

