/**
 * One typed method per row of the route table (`types.ts` `ROUTES`).
 *
 * Every path here is a literal, so a route rename breaks this file and not a screen. The
 * table is the index; this is the implementation.
 */
import { downloadFile, request } from './client';
import type { ProblemReportReceipt } from './types-report';
import type {
  DiagAnswerResponse,
  DiagFinishResponse,
  DiagStartResponse,
  ApiClient,
  ApproveResponse,
  DiagnosisJob,
  EnrollResponse,
  GraphResponse,
  HealthResponse,
  HintResponse,
  LogoutAllResponse,
  MeResponse,
  ModulesResponse,
  OauthProvidersResponse,
  OkResponse,
  OperatorFlagsResponse,
  ReadyResponse,
  RegradeResponse,
  RejectResponse,
  RetentionReportResponse,
  ReviewDocument,
  ReviewListResponse,
  ServedProblem,
  SessionEndResponse,
  SessionPlanResponse,
  SessionResponse,
  SessionStartResponse,
  SignupResponse,
  StatusResponse,
  TaskAnswerResponse,
  QuizResultResponse,
  TeachResponse,
  UngradedListResponse,
} from './types';
import type {
  IntegratedGrade,
  IntegratedHintResponse,
  IntegratedProblem,
} from './types-integrated';

/** A path segment. A task id or a provider name reaches the URL escaped. */
const seg = (value: string) => encodeURIComponent(value);

/** The name the export falls back to when the service sends no `Content-Disposition`. */
const EXPORT_FALLBACK_NAME = 'cadus-export.jsonl';

export const api: ApiClient = {
  demo: false,

  // --- Probes -------------------------------------------------------------
  health: () => request<HealthResponse>('GET', '/health'),
  ready: () => request<ReadyResponse>('GET', '/ready'),

  // --- Auth ---------------------------------------------------------------
  me: () => request<MeResponse>('GET', '/auth/me'),
  login: (email, password) => request<SessionResponse>('POST', '/auth/login', { email, password }),
  signup: (email, password) =>
    request<SignupResponse>('POST', '/auth/signup', { email, password }),
  logout: () => request<OkResponse>('POST', '/auth/logout', {}),
  logoutAll: () => request<LogoutAllResponse>('POST', '/auth/logout-all', {}),
  changePassword: (currentPassword, newPassword) =>
    request<OkResponse>('POST', '/auth/password/change', {
      current_password: currentPassword,
      new_password: newPassword,
    }),
  // Anti-enumeration: this answers `ok` for a registered address and for an unknown one.
  forgotPassword: (email) => request<OkResponse>('POST', '/auth/password/forgot', { email }),
  resetPassword: (token, newPassword) =>
    request<OkResponse>('POST', '/auth/password/reset', { token, new_password: newPassword }),
  // Spending a verify token opens a session, so this answers a user and sets the cookie.
  verifyEmail: (token) => request<SessionResponse>('POST', '/auth/verify-email', { token }),
  resendVerification: (email) =>
    request<OkResponse>('POST', '/auth/verify-email/resend', { email }),
  oauthProviders: () => request<OauthProvidersResponse>('GET', '/auth/oauth/providers'),
  // A URL, not a fetch: the button navigates the whole page, because the provider answers
  // with a redirect no `fetch` may follow across origins.
  oauthStartUrl: (provider, next) => {
    const base = `/api/auth/oauth/${seg(provider)}/start`;
    return next ? `${base}?next=${encodeURIComponent(next)}` : base;
  },

  // --- Dashboard and curriculum -------------------------------------------
  getStatus: () => request<StatusResponse>('GET', '/status'),
  getRetentionReport: () => request<RetentionReportResponse>('GET', '/report/retention'),
  // `scope` is a VIEW FILTER only. The learner whose state is joined comes from the
  // session identity, never from this parameter.
  getGraph: (scope) =>
    request<GraphResponse>('GET', scope ? `/graph?scope=${encodeURIComponent(scope)}` : '/graph'),
  listModules: () => request<ModulesResponse>('GET', '/modules'),
  enroll: (course) => request<EnrollResponse>('POST', '/enroll', { course }),

  // --- The session and the study loop -------------------------------------
  sessionStart: () => request<SessionStartResponse>('POST', '/session/start', {}),
  // Sends NO `minutes` by default, and it must stay that way: the service measures session
  // time from its own accumulator, and that value prices the XP (trap T4).
  sessionEnd: (minutes) =>
    request<SessionEndResponse>('POST', '/session/end', minutes === undefined ? {} : { minutes }),
  getPlan: () => request<SessionPlanResponse>('GET', '/session/plan'),
  taskReport: (taskId, { problem_id, attempt_id, note, request_id, report_kind, item_digest, field_id }, signal) =>
    request<ProblemReportReceipt>('POST', `/task/${seg(taskId)}/report`, {
      problem_id, request_id, ...(attempt_id === undefined ? {} : { attempt_id }),
      ...(report_kind === undefined ? {} : { report_kind }),
      ...(item_digest === undefined ? {} : { item_digest }),
      ...(field_id === undefined ? {} : { field_id }),
      ...(note === undefined ? {} : { note }),
    }, signal),
  getProblemReport: (reportId, signal) =>
    request<ProblemReportReceipt>('GET', `/reports/${seg(reportId)}`, undefined, signal),
  taskServe: (taskId) => request<ServedProblem>('POST', `/task/${seg(taskId)}/serve`, {}),
  taskTeach: (taskId) => request<TeachResponse>('POST', `/task/${seg(taskId)}/teach`, {}),
  taskHint: (taskId, problemId) =>
    request<HintResponse>('POST', `/task/${seg(taskId)}/hint`, { problem_id: problemId }),
  // A WHITELIST, not a passthrough. An absent `work` or `assisted` is OMITTED, never sent
  // as `undefined`: those are different bytes on the wire, and the body reader refuses a
  // field whose type it does not expect. TypeScript's excess-property check fires on
  // object LITERALS only, so a caller passing a variable would put extra keys on the wire.
  taskQuizResult: (taskId, practice = false) => request<QuizResultResponse>('POST', `/task/${seg(taskId)}/quiz-result`, { practice }),
  taskAnswer: (taskId, { problem_id, answer, work, assisted }) =>
    request<TaskAnswerResponse>('POST', `/task/${seg(taskId)}/answer`, {
      problem_id,
      answer,
      ...(work === undefined ? {} : { work }),
      ...(assisted === undefined ? {} : { assisted }),
    }),

  // The integrated task (D-F10). Every one of the three resolves the item from the TASK,
  // so no path here names an item id the learner could change.
  taskIntegrated: (taskId) =>
    request<IntegratedProblem>('POST', `/task/${seg(taskId)}/integrated`, {}),
  taskIntegratedHint: (taskId, { field, index }) =>
    request<IntegratedHintResponse>('POST', `/task/${seg(taskId)}/integrated/hint`, {
      field,
      index,
    }),
  // A WHITELIST, the same rule the graded answer above keeps: an absent method and an
  // absent note are OMITTED, never sent as `undefined`.
  taskIntegratedAnswer: (taskId, { method, steps, final_answer, reasoning }) =>
    request<IntegratedGrade>('POST', `/task/${seg(taskId)}/integrated/answer`, {
      ...(method === undefined || method === null ? {} : { method }),
      // The whitelist runs per field too: a step object is rebuilt key by key, so a
      // caller's extra property never reaches the wire.
      steps: steps.map(({ id, answer, hints_used }) => ({ id, answer, hints_used })),
      final_answer: {
        id: final_answer.id,
        answer: final_answer.answer,
        hints_used: final_answer.hints_used,
      },
      ...(reasoning === undefined ? {} : { reasoning }),
    }),

  // --- The placement diagnostic (spec section 2) ---------------------------
  // `start` takes the course only when the caller names one: an empty body makes the
  // service read the enrolled course, and a first-run learner gets the entry course.
  diagStart: (course) =>
    request<DiagStartResponse>('POST', '/diag/start', course ? { course } : {}),
  diagAnswer: (body) => request<DiagAnswerResponse>('POST', '/diag/answer', body),
  diagFinish: () => request<DiagFinishResponse>('POST', '/diag/finish', {}),

  // --- The async diagnosis (A4) -------------------------------------------
  getDiagnosis: (diagnosisId) =>
    request<DiagnosisJob>('GET', `/diagnosis/${seg(diagnosisId)}`),
  // One subscription per session, not per problem. `EventSource` sends the cookie on a
  // same-origin URL, so this carries no credential either.
  diagnosisStreamUrl: () => '/api/diagnosis/stream',

  // --- The operator view and the export -----------------------------------
  getOperatorFlags: (kp) =>
    request<OperatorFlagsResponse>(
      'GET',
      kp ? `/operator/flags?kp=${encodeURIComponent(kp)}` : '/operator/flags',
    ),
  // Through the cookie, never a token in the URL (SEC-cookie, DEP-3).
  downloadExport: () => downloadFile('/export', EXPORT_FALLBACK_NAME),

  // --- The review surface (C6) --------------------------------------------
  // A filter key with no value is OMITTED, never sent empty: `?status=` reaches the
  // handler as the empty string, and `ReviewFilter { status: Some("") }` matches no row.
  // The screen's "all" choice must therefore send no key at all.
  listContent: (filter = {}) => {
    const query = new URLSearchParams();
    if (filter.status) query.set('status', filter.status);
    if (filter.kind) query.set('kind', filter.kind);
    if (filter.kp) query.set('kp', filter.kp);
    // Page 0 sends no key: the route reads an absent `page` as the first page.
    if (filter.page) query.set('page', String(filter.page));
    const suffix = query.toString();
    return request<ReviewListResponse>(
      'GET',
      suffix ? `/admin/content?${suffix}` : '/admin/content',
    );
  },
  getContent: (digest) => request<ReviewDocument>('GET', `/admin/content/${seg(digest)}`),
  // The body is ignored by the handler, and `{}` is sent anyway: a POST with no body
  // carries no `Content-Type`, and the CSRF layer reads a simple request differently.
  approveContent: (
    digest, policyDigest, templateContextDigest, curriculumDigest, reviewEngineDigest,
  ) =>
    request<ApproveResponse>('POST', `/admin/content/${seg(digest)}/approve`,
      {
        policy_digest: policyDigest,
        template_context_digest: templateContextDigest,
        curriculum_digest: curriculumDigest,
        review_engine_digest: reviewEngineDigest,
      }),
  rejectContent: (digest, reason) =>
    request<RejectResponse>('POST', `/admin/content/${seg(digest)}/reject`, { reason }),
  listUngraded: () => request<UngradedListResponse>('GET', '/admin/ungraded'),
  regradeUngraded: (attemptId, outcome) =>
    request<RegradeResponse>('POST', `/admin/ungraded/${seg(attemptId)}/regrade`, { outcome }),
};
