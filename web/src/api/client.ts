/**
 * The fetch wrapper. Every request the SPA makes goes through here.
 *
 * SEC-cookie. The SPA authenticates by the HttpOnly `__Host-cadus_session` cookie the
 * service sets on login, on `verify-email`, and on the OAuth callback. The browser
 * attaches it to every same-origin request on its own, so there is NO credential in JS,
 * none in `localStorage`, and none in `sessionStorage` to read or to leak. This module
 * touches no storage API at all, and the contract test proves it.
 *
 * `credentials: 'same-origin'` is explicit for clarity. A non-GET request carries
 * `Sec-Fetch-Site: same-origin` from the browser, which is what the CSRF origin layer
 * accepts; a cross-site write is refused with `403 cross_origin_rejected` before the
 * handler runs.
 *
 * The service answers a session token in the body ONLY when the caller sends
 * `Accept-Session-Token: true` (D-M5-5). This client never sends that header, so the
 * service never hands it one.
 */
import type { ApiErrorBody, JsonBody } from './types';

/** The prefix every route of the table carries, except `/metrics`. */
const API = '/api';

/** The message of a `fetch` that never reached the service. */
export const NETWORK_MESSAGE = 'Network error — check your connection and try again.';

/**
 * One failed call.
 *
 * `status` is the HTTP status, or `0` when `fetch` itself rejected. `code` is the envelope
 * code when the body carried one. A caller branches on `code`; `status === 401` is the one
 * branch that must stay distinct, because it means the session is gone and every other
 * failure does not.
 */
export class ApiError extends Error {
  readonly status: number;
  readonly code: string | undefined;

  constructor(status: number, code?: string, message?: string) {
    super(message || code || `HTTP ${status}`);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
  }

  /**
   * Whether this failure means the session is gone.
   *
   * The session-expired path routes to sign-in and drops the view. Nothing else may take
   * it: a `403 cross_origin_rejected` and a `401 invalid_credentials` from the sign-in
   * form itself would both throw the learner out of a screen they are already on
   * (AUTH-inline), so the auth calls of S6 read the code and never this predicate.
   */
  get sessionExpired(): boolean {
    return this.status === 401;
  }
}

/**
 * The ONE place a failed `Response` becomes an `ApiError`.
 *
 * The envelope is `{"error":{"code","message"}}`, but a body can be empty, or HTML from a
 * proxy, so the parse is defensive and falls back to the call site's own message. A caller
 * that already read the body passes it: a `Response` body reads exactly once.
 */
async function failure(
  res: Response,
  fallback: string,
  data?: ApiErrorBody | null,
): Promise<ApiError> {
  const body = data === undefined ? await readJson<ApiErrorBody>(res) : data;
  const err = body?.error ?? {};
  return new ApiError(res.status, err.code, err.message || fallback);
}

/**
 * Read a body once and parse it, or give `null` for an empty or non-JSON one.
 *
 * `T` names the shape the caller expects. The parse itself checks nothing: the service
 * writes the contract of `types.ts`, and a reply outside it is a service defect.
 */
async function readJson<T>(res: Response): Promise<T | null> {
  const text = await res.text();
  if (!text) return null;
  try {
    const parsed: T = JSON.parse(text);
    return parsed;
  } catch {
    return null;
  }
}

/**
 * Issue one JSON request and resolve its parsed body.
 *
 * THE ASYMMETRY WITH `downloadFile`. Here a 2xx body that still carries `error` is a
 * failure: a proxy, or a handler that writes the envelope after its status line is
 * already out, can answer `200` over an envelope, and a caller that trusted the status
 * would render an error object as data. A download body is a blob, never JSON, so it gets
 * no such treatment — see `downloadFile`.
 */
export async function request<T>(method: string, path: string, body?: JsonBody): Promise<T> {
  const headers: Record<string, string> = {};
  const opts: RequestInit = { method, credentials: 'same-origin', headers };
  if (body !== undefined) {
    headers['Content-Type'] = 'application/json';
    opts.body = JSON.stringify(body);
  }

  let res: Response;
  try {
    res = await fetch(`${API}${path}`, opts);
  } catch {
    throw new ApiError(0, 'network', NETWORK_MESSAGE);
  }

  const data = await readJson<T>(res);
  const envelope = data as ApiErrorBody | null;
  if (!res.ok || envelope?.error) {
    throw await failure(res, `Request failed (${res.status}).`, envelope);
  }
  return data as T;
}

/** The `filename="…"` of a `Content-Disposition`, or null when there is none. */
export function dispositionFilename(header: string | null): string | null {
  if (!header) return null;
  const match = /filename\*?=(?:UTF-8'')?"?([^";]+)"?/i.exec(header);
  const name = match?.[1]?.trim();
  if (!name) return null;
  // A server-named path separator would write outside the download directory.
  return name.replace(/[/\\]/g, '_');
}

/**
 * Download an `/api` file to disk.
 *
 * It goes through `fetch`, so the HttpOnly cookie rides along on its own and no token is
 * ever put in a URL or in JS (SEC-cookie). A plain `<a href>` would work for the cookie
 * too, but it cannot read a `4xx` envelope, so a failed export would open a page of JSON
 * instead of raising a toast.
 *
 * The server names the file in `Content-Disposition`; `fallback` is used only when it does
 * not. The object URL is revoked on the next tick, once the browser has claimed the blob.
 */
export async function downloadFile(path: string, fallback: string): Promise<void> {
  let res: Response;
  try {
    res = await fetch(`${API}${path}`, { method: 'GET', credentials: 'same-origin' });
  } catch {
    throw new ApiError(0, 'network', NETWORK_MESSAGE);
  }
  if (!res.ok) throw await failure(res, `Download failed (${res.status}).`);

  const name = dispositionFilename(res.headers.get('Content-Disposition')) ?? fallback;
  const blob = await res.blob();
  const objUrl = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = objUrl;
  anchor.download = name;
  anchor.style.display = 'none';
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(objUrl), 0);
}
