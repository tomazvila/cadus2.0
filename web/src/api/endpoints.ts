/**
 * One typed method per row of the route table (`types.ts` `ROUTES`).
 *
 * Every path here is a literal, so a route rename breaks this file and not a screen. The
 * table is the index; this is the implementation.
 */
import { downloadFile, request } from './client';
import type {
  ApiClient,
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
  ServedProblem,
  SessionEndResponse,
  SessionPlanResponse,
  SessionResponse,
  SessionStartResponse,
  SignupResponse,
  StatusResponse,
  TaskAnswerResponse,
  TeachResponse,
} from './types';

/** A path segment. A task id or a provider name reaches the URL escaped. */
const seg = (value: string) => encodeURIComponent(value);

/** The name the export falls back to when the service sends no `Content-Disposition`. */
export const EXPORT_FALLBACK_NAME = 'cadus-export.jsonl';

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
    request<SessionEndResponse>('POST', '/session/end', minutes != null ? { minutes } : {}),
  getPlan: () => request<SessionPlanResponse>('GET', '/session/plan'),
  taskServe: (taskId) => request<ServedProblem>('POST', `/task/${seg(taskId)}/serve`, {}),
  taskTeach: (taskId) => request<TeachResponse>('POST', `/task/${seg(taskId)}/teach`, {}),
  taskHint: (taskId, problemId) =>
    request<HintResponse>('POST', `/task/${seg(taskId)}/hint`, { problem_id: problemId }),
  // A WHITELIST, not a passthrough. An absent `work` or `assisted` is OMITTED, never sent
  // as `undefined`: those are different bytes on the wire, and the body reader refuses a
  // field whose type it does not expect. TypeScript's excess-property check fires on
  // object LITERALS only, so a caller passing a variable would put extra keys on the wire.
  taskAnswer: (taskId, { problem_id, answer, work, assisted }) => {
    const body: Record<string, unknown> = { problem_id, answer };
    if (work != null) body.work = work;
    if (assisted != null) body.assisted = assisted;
    return request<TaskAnswerResponse>('POST', `/task/${seg(taskId)}/answer`, body);
  },

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
};
