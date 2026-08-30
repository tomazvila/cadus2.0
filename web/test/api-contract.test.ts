/**
 * The S2 acceptance check, part 1: the route table against the client surface.
 *
 * The point is mechanical coverage. `ROUTES` mirrors `crates/web/src/lib.rs` `create_app`,
 * row by row; every row that the browser reaches at all names an `ApiClient` member, and
 * this file proves the member exists on the live client AND on the demo client. Add a
 * route to the service and forget the client, and the count assertion fails.
 */
import { describe, expect, it } from 'vitest';
import { ROUTES, SPEC_ROUTES_ABSENT, api, createDemoApi } from '@/api';
import type { RouteRow } from '@/api';

const reachable = ROUTES.filter((row) => row.via !== 'none');
const clients: Array<[string, Record<string, unknown>]> = [
  ['the live client', api as unknown as Record<string, unknown>],
  ['the demo client', createDemoApi() as unknown as Record<string, unknown>],
];

describe('the route table mirrors create_app', () => {
  it('holds all 31 routes of create_app', () => {
    expect(ROUTES).toHaveLength(31);
    expect(ROUTES.filter((r) => r.method === 'GET')).toHaveLength(15);
    expect(ROUTES.filter((r) => r.method === 'POST')).toHaveLength(16);
  });

  it('names every path exactly once', () => {
    const keys = ROUTES.map((r) => `${r.method} ${r.path}`);
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
  it.each(reachable.map((row): [string, RouteRow] => [`${row.method} ${row.path}`, row]))(
    '%s',
    (_label, row) => {
      expect(row.client).not.toBeNull();
      for (const [name, client] of clients) {
        const member = String(row.client);
        expect(typeof client[member], `${name} lacks ${member}`).toBe('function');
      }
    },
  );

  it('leaves no client member unreached by the table', () => {
    const named = new Set(reachable.map((r) => String(r.client)));
    const members = Object.keys(api).filter((key) => key !== 'demo');
    expect(members.filter((m) => !named.has(m))).toEqual([]);
    expect(members).toHaveLength(reachable.length);
  });
});

describe('the spec rows M5 did not build', () => {
  it('records the four absent paths and reaches none of them', () => {
    expect(SPEC_ROUTES_ABSENT.map((r) => r.path)).toEqual([
      '/api/task/{task_id}/abort',
      '/api/diag/start',
      '/api/diag/answer',
      '/api/diag/finish',
    ]);
    // No row of the live table names one, so no screen can be built against a 404.
    const mounted = new Set(ROUTES.map((r) => r.path));
    for (const absent of SPEC_ROUTES_ABSENT) {
      expect(mounted.has(absent.path)).toBe(false);
      expect(absent.needed_by.length).toBeGreaterThan(0);
    }
  });
});
