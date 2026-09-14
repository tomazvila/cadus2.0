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
// JSON
// ---------------------------------------------------------------------------

/** One JSON scalar. */
type JsonPrimitive = string | number | boolean | null;

/** One JSON value: the shape of every body the wire carries, before a type names it. */
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };

/** The body of one request. Every route of the table takes an object or nothing. */
export type JsonBody = { [key: string]: JsonValue };

// ---------------------------------------------------------------------------
// The error envelope
// ---------------------------------------------------------------------------

/** The envelope of every 4xx and 5xx: `{"error": {"code", "message"}}`. */
export interface ApiErrorBody {
  error: { code?: string; message?: string };
}

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
interface CourseRef {
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
interface VelocityState {
  xp_per_day_28d: number;
  topics_per_week_28d: number;
  course_progress: number;
  /** A local date, `YYYY-MM-DD`, or null when the pace projects no date. */
  eta: string | null;
}

/** The quiz cadence of the learner (`core::learner::QuizState`). */
interface QuizState {
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
  /**
   * The ungraded-attempt count of each topic that has one (D-F2).
   *
   * A topic with none is absent, so an empty object means nothing is waiting.
   */
  ungraded_attempts: Record<string, number>;
  /** The count of ungraded attempts the recovery list holds. */
  ungraded: number;
  /** D-F6: the practiced, inferred and to-confirm counts behind the progress bar. */
  mastery?: MasteryCounts;
}

/**
 * The three honest mastery numbers of `GET /api/status` (D-F6).
 *
 * `practiced` counts the topics the learner passed. `inferred` counts the placed and
 * floor topics that carry no direct answer. `to_confirm` lists the inferred topics the
 * next session confirms, which is at most `mastery.max_per_session` of them.
 */
interface MasteryCounts {
  practiced: number;
  inferred: number;
  total: number;
  to_confirm: string[];
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
// The other halves of the contract, kept in sibling files so no file passes 500 lines.
// ---------------------------------------------------------------------------

export * from './types-report';
export * from './types-study';
export * from './types-review';
export * from './types-integrated';
export * from './contract';
