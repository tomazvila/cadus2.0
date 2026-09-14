/**
 * The 2.0 HTTP contract, part 4: the client surface and the route table.
 *
 * `types.ts` carries the conventions and the source-of-truth order. Every rule there holds
 * here.
 */
import type { ProblemReportReceipt, ProblemReportSubmission } from './types-report';
import type {
  EnrollResponse,
  GraphResponse,
  HealthResponse,
  LogoutAllResponse,
  MeResponse,
  ModulesResponse,
  OauthProvidersResponse,
  OkResponse,
  ReadyResponse,
  SessionResponse,
  SignupResponse,
  StatusResponse,
} from './types';
import type {
  DiagAnswerResponse,
  DiagFinishResponse,
  DiagStartResponse,
  DiagnosisJob,
  HintResponse,
  ServedProblem,
  SessionEndResponse,
  SessionPlanResponse,
  SessionStartResponse,
  TaskAnswerResponse,
  QuizResultResponse,
  TeachResponse,
} from './types-study';
import type {
  ApproveResponse,
  ContentFilter,
  OperatorFlagsResponse,
  RegradeResponse,
  RejectResponse,
  RetentionReportResponse,
  ReviewDocument,
  ReviewListResponse,
  UngradedListResponse,
} from './types-review';
import type {
  IntegratedGrade,
  IntegratedHintResponse,
  IntegratedProblem,
  IntegratedSubmission,
} from './types-integrated';

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
  /** The delayed-retention report of D-F11. */
  getRetentionReport(): Promise<RetentionReportResponse>;
  getGraph(scope?: string): Promise<GraphResponse>;
  listModules(): Promise<ModulesResponse>;
  enroll(course: string): Promise<EnrollResponse>;

  // The session and the study loop.
  sessionStart(): Promise<SessionStartResponse>;
  sessionEnd(minutes?: number): Promise<SessionEndResponse>;
  getPlan(): Promise<SessionPlanResponse>;
  taskReport(taskId: string, body: ProblemReportSubmission, signal?: AbortSignal): Promise<ProblemReportReceipt>;
  getProblemReport(reportId: string, signal?: AbortSignal): Promise<ProblemReportReceipt>;
  taskServe(taskId: string): Promise<ServedProblem>;
  taskQuizResult(taskId: string, practice?: boolean): Promise<QuizResultResponse>;
  taskTeach(taskId: string): Promise<TeachResponse>;
  taskHint(taskId: string, problemId: string): Promise<HintResponse>;
  taskAnswer(
    taskId: string,
    body: { problem_id: string; answer: string; work?: string; assisted?: boolean },
  ): Promise<TaskAnswerResponse>;

  // The integrated task of D-F10. The serve carries no answer, the hint carries one rung,
  // and the answer route grades every step and the final answer in ONE submission.
  taskIntegrated(taskId: string): Promise<IntegratedProblem>;
  taskIntegratedHint(
    taskId: string,
    body: { field: string; index: number },
  ): Promise<IntegratedHintResponse>;
  taskIntegratedAnswer(taskId: string, body: IntegratedSubmission): Promise<IntegratedGrade>;

  // The placement diagnostic (spec section 2). The verdict is deterministic and the
  // reply carries no expected answer, so a screen cannot leak one.
  diagStart(course?: string): Promise<DiagStartResponse>;
  diagAnswer(body: { problem_id: string; answer: string }): Promise<DiagAnswerResponse>;
  diagFinish(): Promise<DiagFinishResponse>;

  // The async diagnosis (A4).
  getDiagnosis(diagnosisId: string): Promise<DiagnosisJob>;
  /** The URL an `EventSource` subscribes to. One connection per session, not per problem. */
  diagnosisStreamUrl(): string;

  // The operator view and the export.
  getOperatorFlags(kp?: string): Promise<OperatorFlagsResponse>;
  downloadExport(): Promise<void>;

  // The review surface (C6). All four refuse a non-admin session with `403 forbidden`,
  // and that refusal is what the two admin screens render (REVIEW-admin).
  listContent(filter?: ContentFilter): Promise<ReviewListResponse>;
  getContent(digest: string): Promise<ReviewDocument>;
  approveContent(
    digest: string,
    policyDigest: string | null,
    templateContextDigest: string | null,
    curriculumDigest: string,
    reviewEngineDigest: string,
  ): Promise<ApproveResponse>;
  /** The reason is required by the service and by the screen (REVIEW-reason). */
  rejectContent(digest: string, reason: string): Promise<RejectResponse>;

  // The recovery path of the third outcome (D-F2). Admin only, like the four above.
  listUngraded(): Promise<UngradedListResponse>;
  regradeUngraded(attemptId: string, outcome: 'correct' | 'incorrect'): Promise<RegradeResponse>;
}

// ---------------------------------------------------------------------------
// The route table
// ---------------------------------------------------------------------------

/** How the browser reaches a route. */
type RouteVia =
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
  { method: 'GET', path: '/api/report/retention', auth: 'S', via: 'method', client: 'getRetentionReport' },
  { method: 'POST', path: '/api/enroll', auth: 'S', via: 'method', client: 'enroll' },
  { method: 'POST', path: '/api/session/start', auth: 'S', via: 'method', client: 'sessionStart' },
  { method: 'POST', path: '/api/session/end', auth: 'S', via: 'method', client: 'sessionEnd' },
  { method: 'GET', path: '/api/session/plan', auth: 'S', via: 'method', client: 'getPlan' },

  { method: 'POST', path: '/api/task/{task_id}/serve', auth: 'S', via: 'method', client: 'taskServe' },
  { method: 'POST', path: '/api/task/{task_id}/teach', auth: 'S', via: 'method', client: 'taskTeach' },
  { method: 'POST', path: '/api/task/{task_id}/hint', auth: 'S', via: 'method', client: 'taskHint' },
  { method: 'POST', path: '/api/task/{task_id}/answer', auth: 'S', via: 'method', client: 'taskAnswer' },
  { method: 'POST', path: '/api/task/{task_id}/report', auth: 'S', via: 'method', client: 'taskReport' },
  { method: 'GET', path: '/api/reports/{report_id}', auth: 'S', via: 'method', client: 'getProblemReport' },
  { method: 'POST', path: '/api/task/{task_id}/integrated', auth: 'S', via: 'method', client: 'taskIntegrated' },
  { method: 'POST', path: '/api/task/{task_id}/integrated/hint', auth: 'S', via: 'method', client: 'taskIntegratedHint' },
  { method: 'POST', path: '/api/task/{task_id}/integrated/answer', auth: 'S', via: 'method', client: 'taskIntegratedAnswer' },
  { method: 'POST', path: '/api/task/{task_id}/quiz-result', auth: 'S', via: 'method', client: 'taskQuizResult' },

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

  { method: 'POST', path: '/api/diag/start', auth: 'S', via: 'method', client: 'diagStart' },
  { method: 'POST', path: '/api/diag/answer', auth: 'S', via: 'method', client: 'diagAnswer' },
  { method: 'POST', path: '/api/diag/finish', auth: 'S', via: 'method', client: 'diagFinish' },

  { method: 'GET', path: '/api/diagnosis/stream', auth: 'S', via: 'stream', client: 'diagnosisStreamUrl' },
  { method: 'GET', path: '/api/diagnosis/{id}', auth: 'S', via: 'method', client: 'getDiagnosis' },

  { method: 'GET', path: '/api/operator/flags', auth: 'S', via: 'method', client: 'getOperatorFlags' },

  // The C6 review surface. Admin only, and the service is the gate: `users.is_admin` is
  // outside the runtime role's column grants, so no reply the SPA reads carries the flag
  // and no client-side check could stand in for these four `403`s.
  { method: 'GET', path: '/api/admin/content', auth: 'S', via: 'method', client: 'listContent' },
  { method: 'GET', path: '/api/admin/content/{digest}', auth: 'S', via: 'method', client: 'getContent' },
  { method: 'POST', path: '/api/admin/content/{digest}/approve', auth: 'S', via: 'method', client: 'approveContent' },
  { method: 'POST', path: '/api/admin/content/{digest}/reject', auth: 'S', via: 'method', client: 'rejectContent' },

  // f4-outcome: the recovery path of the third outcome (D-F2). Admin only.
  { method: 'GET', path: '/api/admin/ungraded', auth: 'S', via: 'method', client: 'listUngraded' },
  { method: 'POST', path: '/api/admin/ungraded/{attempt_id}/regrade', auth: 'S', via: 'method', client: 'regradeUngraded' },
];

/**
 * Rows of the spec table (section 2) that `create_app` does not mount.
 *
 * No typed method reaches one, and none is written: a client method for a path that
 * answers `404 not_found` would let a screen be built against a route that does not
 * exist. The screen that needs the row below (S8's exit path) is blocked until a Rust
 * unit adds the route.
 *
 * The three `/api/diag/*` rows left this list when the placement routes landed. They are
 * `ROUTES` members now, with a typed method each on both clients.
 */
export const SPEC_ROUTES_ABSENT: readonly { method: string; path: string; needed_by: string }[] = [
  { method: 'POST', path: '/api/task/{task_id}/abort', needed_by: 'S8 (the exit paths)' },
];
