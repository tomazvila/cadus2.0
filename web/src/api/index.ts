/**
 * The one import point of the API layer.
 *
 * A screen imports `resolveApi` (or the types) from here and never reaches into
 * `endpoints` or `demo` directly, so `?demo=1` stays a one-object swap.
 */
export { ApiError, NETWORK_MESSAGE, dispositionFilename, downloadFile, request } from './client';
export { EXPORT_FALLBACK_NAME, api } from './endpoints';
export { createDemoApi } from './demo';
export * from './types';

import { api } from './endpoints';
import { createDemoApi } from './demo';
import type { ApiClient } from './types';

/**
 * Pick the client for one page load.
 *
 * `?demo=1` selects the demo backend. The read is PURE and takes the query string as an
 * argument, so boot can call it before `createRoot` and a test can call it with a literal
 * — reading `location.search` inside would make every caller depend on a global.
 */
export function resolveApi(search: string): ApiClient {
  return new URLSearchParams(search).get('demo') === '1' ? createDemoApi() : api;
}
