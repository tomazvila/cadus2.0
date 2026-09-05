/**
 * The S2 acceptance check, part 1: the route table against the service and against the
 * client surface.
 *
 * `ROUTES` in `src/api/types.ts` is a hand-written mirror of `crates/web/src/lib.rs`
 * `create_app`. Its ORACLE is `src/api/routes.generated.json`, a fixture that
 * `crates/web/tests/route_table.rs` dumps from the live router: that Rust test probes every
 * mounted path and fails when the committed fixture and the router disagree.
 *
 * This file closes the loop from the TypeScript side (FIX-M6-G, finding F17):
 *
 * 1. Every row of the fixture appears in `ROUTES`. A route added to `create_app` and not
 *    mirrored here now fails, first in the Rust test and then in this file.
 * 2. Every row of `ROUTES` appears in the fixture. A row for a path the service does not
 *    serve fails, so no screen is built against a `404`.
 * 3. Every reachable row names an `ApiClient` member, and the member exists on the live
 *    client AND on the demo client.
 *
 * Before the fixture existed, direction 1 had no check at all: nothing under `web/` read
 * `create_app`, and the count assertion compared `ROUTES` with a number written beside it.
 */
import { describe, expect, it } from 'vitest';
import { ROUTES, SPEC_ROUTES_ABSENT, api, createDemoApi } from '@/api';
import type { ApiClient, RouteRow } from '@/api';
import routeFixture from '@/api/routes.generated.json';

/** `METHOD path`, the key both tables are compared on. */
const key = (row: { method: string; path: string }) => `${row.method} ${row.path}`;

/** The routes `create_app` mounts, as the Rust dump recorded them. */
const MOUNTED = [...routeFixture.routes].map(key).sort();

const reachable = ROUTES.filter((row) => row.via !== 'none');
const clients: Array<[string, ApiClient]> = [
  ['the live client', api],
  ['the demo client', createDemoApi()],
];

describe('the route table mirrors create_app', () => {
  it('names every route the generated fixture holds, and no other', () => {
    expect([...ROUTES].map(key).sort()).toEqual(MOUNTED);
  });

  it('holds one row per mounted route, split into GET and POST', () => {
    expect(ROUTES).toHaveLength(routeFixture.routes.length);
    expect(ROUTES.filter((r) => r.method === 'GET')).toHaveLength(
      routeFixture.routes.filter((r) => r.method === 'GET').length,
    );
    expect(ROUTES.filter((r) => r.method === 'POST')).toHaveLength(
      routeFixture.routes.filter((r) => r.method === 'POST').length,
    );
  });

  it('reads a fixture that the Rust dump wrote', () => {
    // A fixture emptied by a broken dump would make the two comparisons above pass on
    // nothing at all, so this test pins the shape and one literal row.
    expect(routeFixture.note).toContain('crates/web/tests/route_table.rs');
    expect(MOUNTED).toContain('POST /api/task/{task_id}/answer');
    expect(routeFixture.routes.length).toBeGreaterThan(30);
  });

  it('names every path exactly once', () => {
    const keys = ROUTES.map(key);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it('pins the four public route families', () => {
    const publicPaths = ROUTES.filter((r) => r.auth === 'P').map((r) => r.path);
    expect(publicPaths).toEqual([
      '/api/health',
      '/api/ready',
      '/metrics',
      '/api/auth/signup',
      '/api/auth/login',
      '/api/auth/password/forgot',
      '/api/auth/password/reset',
      '/api/auth/verify-email',
      '/api/auth/verify-email/resend',
      '/api/auth/oauth/providers',
      '/api/auth/oauth/{provider}/start',
      '/api/auth/oauth/{provider}/callback',
    ]);
  });

  it('reaches only /metrics and the OAuth callback with no client member', () => {
    const none = ROUTES.filter((r) => r.via === 'none');
    expect(none.map((r) => r.path)).toEqual([
      '/metrics',
      '/api/auth/oauth/{provider}/callback',
    ]);
    expect(none.every((r) => r.client === null)).toBe(true);
  });
});

describe('every route of the table has a typed method', () => {
  it.each(reachable.map((row): [string, RouteRow] => [key(row), row]))('%s', (_label, row) => {
    const member = row.client;
    expect(member).not.toBeNull();
    for (const [name, client] of clients) {
      expect(typeof client[member!], `${name} lacks ${String(member)}`).toBe('function');
    }
  });

  it('leaves no client member unreached by the table', () => {
    const named = new Set(reachable.map((r) => String(r.client)));
    const members = Object.keys(api).filter((k) => k !== 'demo');
    expect(members.filter((m) => !named.has(m))).toEqual([]);
    expect(members).toHaveLength(reachable.length);
  });
});

describe('the spec rows no unit has built', () => {
  it('records the one absent path and reaches none of them', () => {
    // The three `/api/diag/*` rows left this list when the placement routes landed.
    expect(SPEC_ROUTES_ABSENT.map((r) => r.path)).toEqual(['/api/task/{task_id}/abort']);
    // The service mounts none of them either, so no screen can be built against a 404.
    const mounted = new Set(routeFixture.routes.map((r) => r.path));
    for (const absent of SPEC_ROUTES_ABSENT) {
      expect(mounted.has(absent.path)).toBe(false);
      expect(absent.needed_by.length).toBeGreaterThan(0);
    }
  });
});
