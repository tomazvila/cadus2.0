/**
 * The 2.0 HTTP contract, hand-written from `docs/reference/web-service-1.0-spec.md`
 * section 2 and from the handlers that answer it.
 *
 * WHY HAND-WRITTEN. The axum service publishes no OpenAPI document, and 1.0's attempt to
 * generate this file produced `unknown` for every request and every response, because
 * FastAPI declared no response model on any of its 26 routes. A drift check over `unknown`
 * detects nothing and implies it detects everything.
 *
 * SOURCE OF TRUTH, IN THIS ORDER.
 *  1. `crates/web/src/lib.rs` `create_app` — the route list. A path that is not there is
 *     not a route, whatever the spec table says. See [`SPEC_ROUTES_ABSENT`].
 *  2. The `json!` literal each handler returns — `session.rs`, `serve.rs`, `grade.rs`,
 *     `diagnosis.rs`, `health.rs`, `operator.rs`, `auth/routes.rs`,
 *     `auth/oauth_routes.rs`.
 *  3. The prose of the spec, last. 1.0 wrote this file from the prose first and described
 *     a server that did not exist.
 *
 * CONVENTIONS.
 *  * Optional means "the handler may omit the key". A key the handler always emits with an
 *     empty value is `| null`, never optional.
 *  * A field the SPA never reads is left out on purpose. This is the client's view of the
 *     contract, not a mirror of the server.
 *  * `error_tags` is `string[]`, never a union. Trap T3: the SPA renders a tag verbatim and
 *     never re-interprets it. The server writes `blank-answer` (`grade.rs`) while the
 *     config vocabulary lists `blank_answer` (`core/src/config.rs`); a union here would
 *     make the client the arbiter of that difference.
 */

// ---------------------------------------------------------------------------
// The error envelope
// ---------------------------------------------------------------------------

/** The envelope of every 4xx and 5xx: `{"error": {"code", "message"}}`. */
export interface ApiErrorBody {
  error: { code?: string; message?: string };
}

/**
 * The codes the service emits, plus the one the client synthesizes.
 *
 * A code outside this list is not an error: `ApiError.code` stays `string | undefined` and
 * a caller that does not recognize a code falls back to the message. The union exists so a
 * branch on a MISSPELLED code fails to compile.
 */
export type ErrorCode =
  // The envelope layer (`error.rs`).
  | 'not_found'
  | 'method_not_allowed'
  | 'cross_origin_rejected'
  | 'invalid_request'
  | 'payload_too_large'
  | 'unauthorized'
  | 'invalid_credentials'
  | 'weak_password'
  | 'invalid_token'
  | 'rate_limited'
  | 'oauth_error'
  | 'internal_error'
  // The session and task layer (`state.rs`, `serve.rs`, `grade.rs`).
  | 'unknown_course'
  | 'unknown_task'
  | 'unknown_problem'
  | 'task_complete'
  | 'no_open_session'
  | 'curriculum_unavailable'
  | 'state_unavailable'
  | 'multistep_exhausted'
  | 'quiz_exhausted'
  | 'no_instruction'
  | 'no_hints_in_quiz'
  | 'no_hint_ladder'
  | 'pool_unavailable'
  | 'answer_too_large'
  | 'undecidable_kind'
  // The diagnosis and operator routes (`diagnosis.rs`, `operator.rs`).
  | 'unknown_diagnosis'
  | 'forbidden'
  /** Synthesized when `fetch` itself rejects. The server never sends it. */
  | 'network';

// ---------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------

/**
 * `GET /api/health` — liveness, and nothing else.
 *
 * The body is exactly `{"ok":true}` (`health.rs:50`). 1.0 answered `model`, `engine` and a
 * provider list here, and 1.0's sign-in page read the providers off it (trap T15). 2.0 has
 * `GET /api/auth/oauth/providers` for that, so AUTH-7 reads THAT route, not this one.
 */
export interface HealthResponse {
  ok: boolean;
}

/** `GET /api/ready` — readiness. `200` with `ok:true`, or `503` with `ok:false`. */
export interface ReadyResponse {
  ok: boolean;
  db: 'ok' | 'down';
  /** Worker liveness from the `diagnosis_jobs` claim age (D-M5-6). */
  worker: { claim_age_secs: number | null; stale: boolean };
  /** Present only when a warning fired. A stale worker never decides the status code. */
  warnings?: string[];
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

/** The public view of one account (`auth/routes.rs` `user_public`). */
export interface User {
  id: string;
  email: string;
  email_verified: boolean;
  /** RFC 3339. */
  created_at: string;
}

/**
 * The `200` of login, of `verify-email`, and of the OAuth callback.
 *
 * `session_token` arrives ONLY when the request sent `Accept-Session-Token: true`
 * (D-M5-5). The SPA never sends that header and never reads this field: the session is the
 * HttpOnly `__Host-` cookie (SEC-cookie). It is typed so that a future non-browser caller
 * is described, and so that a reviewer sees the SPA declines it.
 */
export interface SessionResponse {
  user: User;
  session_token?: string;
}

export interface MeResponse {
  user: User;
}

/**
 * `POST /api/auth/signup` — always this object, for a new address and a registered one.
 *
 * It sets no cookie and it is never `409 email_taken`: sign-up is non-enumerable.
 */
export interface SignupResponse {
  status: 'verification_required';
  message: string;
}

/** The receipt of a write that reports nothing but success. */
export interface OkResponse {
  ok: true;
}

/** `POST /api/auth/logout-all` — the receipt plus how many sessions it dropped. */
export interface LogoutAllResponse {
  ok: true;
  revoked: number;
}

/**
 * `GET /api/auth/oauth/providers` — the enabled provider names, in order.
 *
 * A provider with no client id and no secret is not enabled, so the list is empty and the
 * sign-in page renders no button (AUTH-7).
 */
export interface OauthProvidersResponse {
  providers: string[];
}

// ---------------------------------------------------------------------------
// Dashboard and curriculum
// ---------------------------------------------------------------------------

/** `{id, name}`. `name` is null when the learner enrolled in no course. */
export interface CourseRef {
  id: string | null;
  name: string | null;
}

/** One course of the ordered journey, with the enrolled one flagged. */
export interface JourneyCourse {
  id: string;
  name: string;
  current: boolean;
}

/** The XP totals of the learner (`core::learner::XpState`). */
export interface XpState {
  total: number;
  today: number;
  goal: number;
  streak_days: number;
}

/** The pace over the trailing 28 local days (`core::learner::VelocityState`). */
export interface VelocityState {
  xp_per_day_28d: number;
  topics_per_week_28d: number;
  course_progress: number;
  /** A local date, `YYYY-MM-DD`, or null when the pace projects no date. */
  eta: string | null;
}

/** The quiz cadence of the learner (`core::learner::QuizState`). */
export interface QuizState {
  last_at: string | null;
  xp_since: number;
  retake_pending: boolean;
}

/** One remediation the core scheduled. The SPA renders it and decides nothing. */
export interface Remediation {
  kind: string;
  targets: string[];
}

/** `GET /api/status` (`session.rs:425`). Every key is always present. */
export interface StatusResponse {
  course: CourseRef;
  placed: boolean;
  courses: JourneyCourse[];
  /** The F6 test-prep set. No M5 unit writes it, so it is always null. */
  test_prep: null;
  xp: XpState;
  velocity: VelocityState;
  quiz: QuizState;
  pending_remediation: Remediation[];
  quiz_due: boolean;
  drill_due: boolean;
  frontier: number;
  due_reviews: number;
  nearly_due: number;
}

/** The mastery state of one topic (`core::event::TopicStatus`). */
export type TopicStatus = 'untouched' | 'frontier' | 'learning' | 'placed' | 'floor';

/**
 * One node of `GET /api/graph`.
 *
 * The 2.0 shape is NOT 1.0's. There is no `x`, no `y`, no `state`, no `memory`, no `core`
 * and no `drill`: the server computes no layout, and the node carries `status` and
 * `ability` instead (`session.rs:511-517`). The map view (S11) lays the graph out itself.
 */
export interface GraphNode {
  id: string;
  name: string | null;
  module: string;
  course: string | null;
  status: TopicStatus;
  ability: number;
}

/** One prerequisite edge. `from` and `to`, not 1.0's `source` and `target`. */
export interface GraphEdge {
  from: string;
  to: string;
}

/** `GET /api/graph` (`session.rs:526`). `?scope=` is a VIEW FILTER only. */
export interface GraphResponse {
  /** RFC 3339. */
  now: string;
  scope: string | null;
  courses: JourneyCourse[];
  modules: string[];
  counts: { nodes: number; edges: number; mastered: number };
  nodes: GraphNode[];
  edges: GraphEdge[];
}

/** `GET /api/modules`. `course` is an OBJECT; rendering it as a child throws. */
export interface ModulesResponse {
  course: CourseRef;
  modules: string[];
}

/** `POST /api/enroll`. */
export interface EnrollResponse {
  enrolled: string;
  /** Topic ids, sorted. */
  mastery_floor: string[];
  floor_size: number;
}

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

export type TaskType = 'lesson' | 'review' | 'quiz' | 'drill' | 'multi-step';

/** The client-safe topic of a plan task. No exemplar and no expected answer. */
export interface TopicRef {
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
  /**
   * Server-side completion. The session view filters the WHOLE task list on
   * `progress.done`: per-mount memory left a reload restarting at a closed task and
   * serving a `409 task_complete` the learner could not escape.
   */
  progress: { answered: number; done: boolean };
}

export interface PlanConstraints {
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
 * Seven keys, always all seven (`serve.rs` `serve_payload`). `expected` and
 * `solution_sketch` are named out of this payload on purpose (Hard Rule 1).
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

export type WorkQuality =
  | 'perfect'
  | 'nearly_perfect'
  | 'passable'
  | 'nearly_passable'
  | 'poor'
  | 'blowoff';

export type TaskStatus =
  | 'continue'
  | 'kp_advance'
  | 'task_passed'
  | 'task_failed'
  /** NOTHING was recorded twice: the attempt already stood. Never a normal advance. */
  | 'already_recorded';

/** The `diagnosis` field of a grade reply: no job row was written. */
export interface DiagnosisNotOffered {
  status: 'not_offered';
}

/** A pre-authored distractor diagnosis matched, in the grade transaction. No model call. */
export interface DiagnosisReady {
  status: 'ready';
  error_tags: string[];
  prose: string;
}

/** A `diagnosis_jobs` row was inserted. `id` is its primary key; subscribe or poll. */
export interface DiagnosisPending {
  id: string;
  status: 'pending';
}

/** The `diagnosis` field of a grade reply. Null for a quiz, which reveals nothing. */
export type DiagnosisField = DiagnosisNotOffered | DiagnosisReady | DiagnosisPending | null;

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

// ---------------------------------------------------------------------------
// The operator view (A6, T3)
// ---------------------------------------------------------------------------

/** One knowledge point's serving health (`operator.rs` `flag_json`). */
export interface OperatorFlag {
  kp_id: string;
  approved_templates: number;
  pool_depth: number;
  last_source: string | null;
  last_exemplar_at: string | null;
  needs_template: boolean;
  source_exhausted: boolean;
}

/** One gate run over an approved template body, asked for with `?kp=`. */
export interface OperatorGateNote {
  kp_id: string;
  digest: string;
  gated: boolean;
  reason?: string;
  exhaustive?: boolean;
  instances_checked?: number;
  notes: string[];
  rejected?: { code: string; message: string };
}

/** `GET /api/operator/flags` — admin only; a non-admin gets `403 forbidden`. */
export interface OperatorFlagsResponse {
  flags: OperatorFlag[];
  gate: OperatorGateNote[];
  gate_limit: number;
  gate_truncated: boolean;
}

// ---------------------------------------------------------------------------
// The client surface
// ---------------------------------------------------------------------------

/**
 * Every call the SPA can make.
 *
 * The demo client implements this interface too, which is what makes `?demo=1` a
 * one-object swap — and makes a missing demo method a BUILD error rather than the
 * load-time warning 1.0 emitted into a console nobody reads (F-F6-1).
 */
export interface ApiClient {
  readonly demo: boolean;

  // Probes.
  health(): Promise<HealthResponse>;
  ready(): Promise<ReadyResponse>;

  // Auth. Every one of these bypasses the central call wrapper, so
  // `invalid_credentials` never trips the session-expired path (AUTH-inline).
  me(): Promise<MeResponse>;
  login(email: string, password: string): Promise<SessionResponse>;
  signup(email: string, password: string): Promise<SignupResponse>;
  logout(): Promise<OkResponse>;
  logoutAll(): Promise<LogoutAllResponse>;
  changePassword(currentPassword: string, newPassword: string): Promise<OkResponse>;
  forgotPassword(email: string): Promise<OkResponse>;
  resetPassword(token: string, newPassword: string): Promise<OkResponse>;
  verifyEmail(token: string): Promise<SessionResponse>;
  resendVerification(email: string): Promise<OkResponse>;
  oauthProviders(): Promise<OauthProvidersResponse>;
  /** The URL the sign-in button navigates to. A GET, so no CSRF check reads it. */
  oauthStartUrl(provider: string, next?: string): string;

  // Dashboard and curriculum.
  getStatus(): Promise<StatusResponse>;
  getGraph(scope?: string): Promise<GraphResponse>;
  listModules(): Promise<ModulesResponse>;
  enroll(course: string): Promise<EnrollResponse>;

  // The session and the study loop.
  sessionStart(): Promise<SessionStartResponse>;
  sessionEnd(minutes?: number): Promise<SessionEndResponse>;
  getPlan(): Promise<SessionPlanResponse>;
  taskServe(taskId: string): Promise<ServedProblem>;
  taskTeach(taskId: string): Promise<TeachResponse>;
  taskHint(taskId: string, problemId: string): Promise<HintResponse>;
  taskAnswer(
    taskId: string,
    body: { problem_id: string; answer: string; work?: string; assisted?: boolean },
  ): Promise<TaskAnswerResponse>;

  // The async diagnosis (A4).
  getDiagnosis(diagnosisId: string): Promise<DiagnosisJob>;
  /** The URL an `EventSource` subscribes to. One connection per session, not per problem. */
  diagnosisStreamUrl(): string;

  // The operator view and the export.
  getOperatorFlags(kp?: string): Promise<OperatorFlagsResponse>;
  downloadExport(): Promise<void>;
}

// ---------------------------------------------------------------------------
// The route table
// ---------------------------------------------------------------------------

/** How the browser reaches a route. */
export type RouteVia =
  /** A typed `ApiClient` method issues a `fetch`. */
  | 'method'
  /** A full-page navigation. The client only builds the URL. */
  | 'navigation'
  /** An `EventSource` subscription. The client only builds the URL. */
  | 'stream'
  /** No client call at all: a scrape target or a provider redirect target. */
  | 'none';

export interface RouteRow {
  readonly method: 'GET' | 'POST';
  /** The axum route template, `{param}` and all. */
  readonly path: string;
  /** `S` = session required, `P` = public. */
  readonly auth: 'S' | 'P';
  readonly via: RouteVia;
  /** The `ApiClient` member that reaches it, or null when `via` is `none`. */
  readonly client: keyof ApiClient | null;
}

/**
 * Every route `crates/web/src/lib.rs` `create_app` mounts, in mount order.
 *
 * This list is the acceptance check of unit S2: each row with `via` other than `none`
 * names an `ApiClient` member, and the contract test proves the member exists on the live
 * client AND on the demo client. Add a route to the service, add it here, and the test
 * fails until a typed method reaches it.
 */
export const ROUTES: readonly RouteRow[] = [
  { method: 'GET', path: '/api/health', auth: 'P', via: 'method', client: 'health' },
  { method: 'GET', path: '/api/ready', auth: 'P', via: 'method', client: 'ready' },
  // Prometheus text for a scrape tool. No SPA screen reads it.
  { method: 'GET', path: '/metrics', auth: 'P', via: 'none', client: null },

  { method: 'GET', path: '/api/status', auth: 'S', via: 'method', client: 'getStatus' },
  { method: 'GET', path: '/api/graph', auth: 'S', via: 'method', client: 'getGraph' },
  { method: 'GET', path: '/api/modules', auth: 'S', via: 'method', client: 'listModules' },
  { method: 'GET', path: '/api/export', auth: 'S', via: 'method', client: 'downloadExport' },
  { method: 'POST', path: '/api/enroll', auth: 'S', via: 'method', client: 'enroll' },
  { method: 'POST', path: '/api/session/start', auth: 'S', via: 'method', client: 'sessionStart' },
  { method: 'POST', path: '/api/session/end', auth: 'S', via: 'method', client: 'sessionEnd' },
  { method: 'GET', path: '/api/session/plan', auth: 'S', via: 'method', client: 'getPlan' },

  { method: 'POST', path: '/api/task/{task_id}/serve', auth: 'S', via: 'method', client: 'taskServe' },
  { method: 'POST', path: '/api/task/{task_id}/teach', auth: 'S', via: 'method', client: 'taskTeach' },
  { method: 'POST', path: '/api/task/{task_id}/hint', auth: 'S', via: 'method', client: 'taskHint' },
  { method: 'POST', path: '/api/task/{task_id}/answer', auth: 'S', via: 'method', client: 'taskAnswer' },

  { method: 'POST', path: '/api/auth/signup', auth: 'P', via: 'method', client: 'signup' },
  { method: 'POST', path: '/api/auth/login', auth: 'P', via: 'method', client: 'login' },
  { method: 'POST', path: '/api/auth/logout', auth: 'S', via: 'method', client: 'logout' },
  { method: 'POST', path: '/api/auth/logout-all', auth: 'S', via: 'method', client: 'logoutAll' },
  { method: 'GET', path: '/api/auth/me', auth: 'S', via: 'method', client: 'me' },
  { method: 'POST', path: '/api/auth/password/change', auth: 'S', via: 'method', client: 'changePassword' },
  { method: 'POST', path: '/api/auth/password/forgot', auth: 'P', via: 'method', client: 'forgotPassword' },
  { method: 'POST', path: '/api/auth/password/reset', auth: 'P', via: 'method', client: 'resetPassword' },
  { method: 'POST', path: '/api/auth/verify-email', auth: 'P', via: 'method', client: 'verifyEmail' },
  { method: 'POST', path: '/api/auth/verify-email/resend', auth: 'P', via: 'method', client: 'resendVerification' },
  { method: 'GET', path: '/api/auth/oauth/providers', auth: 'P', via: 'method', client: 'oauthProviders' },
  { method: 'GET', path: '/api/auth/oauth/{provider}/start', auth: 'P', via: 'navigation', client: 'oauthStartUrl' },
  // The provider navigates the browser here. The SPA never calls it.
  { method: 'GET', path: '/api/auth/oauth/{provider}/callback', auth: 'P', via: 'none', client: null },

  { method: 'GET', path: '/api/diagnosis/stream', auth: 'S', via: 'stream', client: 'diagnosisStreamUrl' },
  { method: 'GET', path: '/api/diagnosis/{id}', auth: 'S', via: 'method', client: 'getDiagnosis' },

  { method: 'GET', path: '/api/operator/flags', auth: 'S', via: 'method', client: 'getOperatorFlags' },
];

/**
 * Rows of the spec table (section 2) that `create_app` does not mount.
 *
 * M5 built neither, so no typed method reaches them and none is written: a client method
 * for a path that answers `404 not_found` would let a screen be built against a route that
 * does not exist. The screens that need them (S8's exit path, S10's placement) are blocked
 * until a Rust unit adds the routes.
 */
export const SPEC_ROUTES_ABSENT: readonly { method: string; path: string; needed_by: string }[] = [
  { method: 'POST', path: '/api/task/{task_id}/abort', needed_by: 'S8 (the exit paths)' },
  { method: 'POST', path: '/api/diag/start', needed_by: 'S10 (diagnostic placement)' },
  { method: 'POST', path: '/api/diag/answer', needed_by: 'S10 (diagnostic placement)' },
  { method: 'POST', path: '/api/diag/finish', needed_by: 'S10 (diagnostic placement)' },
];
