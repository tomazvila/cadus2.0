/**
 * The S2 acceptance check, part 2: the fetch client.
 *
 * Four properties are load-bearing and each has a test that fails when it is removed:
 * the error envelope becomes an `ApiError`, a 2xx body that carries `error` throws anyway,
 * a 401 surfaces distinctly from every other failure, and no credential reaches
 * `localStorage` (SEC-cookie).
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError, NETWORK_MESSAGE, api, dispositionFilename, downloadFile, request } from '@/api';
import { downloads, objectUrls } from './setup';

/** One stubbed `fetch` answer. The body is text, the way a real one is. */
function answer(status: number, body: string, headers: Record<string, string> = {}): Response {
  return new Response(body, {
    status,
    headers: { 'Content-Type': 'application/json', ...headers },
  });
}

function stubFetch(...responses: Response[]): ReturnType<typeof vi.fn> {
  const fetchMock = vi.fn();
  for (const res of responses) fetchMock.mockResolvedValueOnce(res);
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

/** The rejection of a promise, as the error it carried. */
async function rejection<T>(promise: Promise<T>): Promise<ApiError> {
  try {
    await promise;
  } catch (err) {
    return err as ApiError;
  }
  throw new Error('the call resolved; it was expected to throw');
}

describe('the request wrapper', () => {
  it('sends the cookie, the method, and a JSON body', async () => {
    const fetchMock = stubFetch(answer(200, '{"ok":true}'));
    await request('POST', '/auth/logout', {});
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, opts] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/auth/logout');
    expect(opts.method).toBe('POST');
    expect(opts.credentials).toBe('same-origin');
    expect((opts.headers as Record<string, string>)['Content-Type']).toBe('application/json');
    expect(opts.body).toBe('{}');
  });

  it('sends no Content-Type and no body on a GET', async () => {
    const fetchMock = stubFetch(answer(200, '{"ok":true}'));
    await api.health();
    const [url, opts] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/health');
    expect(opts.body).toBeUndefined();
    expect(Object.keys(opts.headers as Record<string, string>)).toEqual([]);
  });

  it('resolves the parsed body of a 200', async () => {
    stubFetch(answer(200, '{"ok":true,"db":"ok","worker":{"claim_age_secs":null,"stale":false}}'));
    await expect(api.ready()).resolves.toEqual({
      ok: true,
      db: 'ok',
      worker: { claim_age_secs: null, stale: false },
    });
  });

  it('resolves null for an empty 200 body', async () => {
    stubFetch(answer(200, ''));
    await expect(request('GET', '/health')).resolves.toBeNull();
  });
});

describe('the error envelope', () => {
  it('turns {"error":{"code","message"}} into an ApiError', async () => {
    stubFetch(answer(404, '{"error":{"code":"unknown_task","message":"No such task."}}'));
    const err = await rejection(api.taskServe('t-1'));
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(404);
    expect(err.code).toBe('unknown_task');
    expect(err.message).toBe('No such task.');
  });

  it('falls back to a status message when the body is not JSON', async () => {
    stubFetch(answer(502, '<html>bad gateway</html>', { 'Content-Type': 'text/html' }));
    const err = await rejection(api.getStatus());
    expect(err.status).toBe(502);
    expect(err.code).toBeUndefined();
    expect(err.message).toBe('Request failed (502).');
  });

  it('reports a fetch that never reached the service as code network', async () => {
    const fetchMock = vi.fn().mockRejectedValue(new TypeError('failed to fetch'));
    vi.stubGlobal('fetch', fetchMock);
    const err = await rejection(api.getPlan());
    expect(err.status).toBe(0);
    expect(err.code).toBe('network');
    expect(err.message).toBe(NETWORK_MESSAGE);
  });
});

describe('the 2xx-with-error asymmetry', () => {
  it('throws on a 200 whose body still carries error', async () => {
    stubFetch(answer(200, '{"error":{"code":"state_unavailable","message":"State is gone."}}'));
    const err = await rejection(api.getPlan());
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(200);
    expect(err.code).toBe('state_unavailable');
    expect(err.message).toBe('State is gone.');
  });

  it('does not throw on a 200 whose body merely holds the word error', async () => {
    stubFetch(answer(200, '{"hint":"a sign error is common here","hint_number":1}'));
    await expect(api.taskHint('t-1', 'p-1')).resolves.toEqual({
      hint: 'a sign error is common here',
      hint_number: 1,
    });
  });

  it('does NOT apply the asymmetry to a download: a 2xx blob is never parsed', async () => {
    // A blob that happens to spell an envelope is still a file. Parsing it would refuse a
    // legitimate export whose first line is an event about an error.
    stubFetch(
      answer(200, '{"error":{"code":"x"}}\n', {
        'Content-Type': 'application/x-ndjson',
      }),
    );
    await expect(downloadFile('/export', 'fallback.jsonl')).resolves.toBeUndefined();
    expect(downloads).toHaveLength(1);
  });
});

describe('a 401 surfaces distinctly', () => {
  it('marks only a 401 as sessionExpired', async () => {
    stubFetch(answer(401, '{"error":{"code":"unauthorized","message":"Sign in."}}'));
    const gone = await rejection(api.getStatus());
    expect(gone.status).toBe(401);
    expect(gone.code).toBe('unauthorized');
    expect(gone.sessionExpired).toBe(true);
  });

  it('leaves a 403, a 404 and a 409 as ordinary failures', async () => {
    stubFetch(
      answer(403, '{"error":{"code":"cross_origin_rejected","message":"No."}}'),
      answer(404, '{"error":{"code":"unknown_problem","message":"Gone."}}'),
      answer(409, '{"error":{"code":"task_complete","message":"Done."}}'),
    );
    for (const call of [api.getStatus(), api.getStatus(), api.getStatus()]) {
      expect((await rejection(call)).sessionExpired).toBe(false);
    }
  });

  it('keeps invalid_credentials on the auth code, not on the session-expired path', async () => {
    // AUTH-inline: the sign-in form reads the CODE. A 401 from login is not a lost
    // session, and routing it to session-expired would throw the user off the page they
    // are already on.
    stubFetch(answer(401, '{"error":{"code":"invalid_credentials","message":"Invalid email or password."}}'));
    const err = await rejection(api.login('a@b.test', 'wrong-password'));
    expect(err.code).toBe('invalid_credentials');
    expect(err.message).toBe('Invalid email or password.');
  });
});

describe('SEC-cookie: no credential reaches localStorage', () => {
  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
  });

  it('SEC-cookie: a login writes nothing to localStorage or sessionStorage', async () => {
    const setLocal = vi.spyOn(Storage.prototype, 'setItem');
    stubFetch(
      answer(200, '{"user":{"id":"u1","email":"a@b.test","email_verified":true,'
        + '"created_at":"2026-01-01T00:00:00+00:00"},"session_token":"leak-me"}'),
    );
    const out = await api.login('a@b.test', 'correct-horse');
    expect(out.user.email).toBe('a@b.test');
    expect(setLocal).not.toHaveBeenCalled();
    expect(localStorage.length).toBe(0);
    expect(sessionStorage.length).toBe(0);
  });

  it('SEC-cookie: no request carries an Authorization or a session-token header', async () => {
    const fetchMock = stubFetch(answer(200, '{"ok":true}'), answer(200, '{"ok":true}'));
    await api.logout();
    await api.getStatus();
    for (const [, opts] of fetchMock.mock.calls as Array<[string, RequestInit]>) {
      const names = Object.keys(opts.headers as Record<string, string>).map((n) => n.toLowerCase());
      expect(names).not.toContain('authorization');
      expect(names).not.toContain('accept-session-token');
      expect(opts.credentials).toBe('same-origin');
    }
  });

  it('SEC-cookie: the export downloads through the cookie, with no token in the URL', async () => {
    const fetchMock = stubFetch(
      answer(200, '{"event":"attempt"}\n', {
        'Content-Disposition': 'attachment; filename="cadus-export-u1.jsonl"',
      }),
    );
    await api.downloadExport();
    const [url, opts] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/export');
    expect(opts.credentials).toBe('same-origin');
    expect(localStorage.length).toBe(0);
  });
});

describe('the download helper', () => {
  it('names the file from Content-Disposition and revokes the object URL', async () => {
    vi.useFakeTimers();
    stubFetch(
      answer(200, 'line\n', {
        'Content-Disposition': 'attachment; filename="cadus-export-u1.jsonl"',
      }),
    );
    await downloadFile('/export', 'fallback.jsonl');
    expect(downloads).toEqual([
      { href: objectUrls[0], download: 'cadus-export-u1.jsonl' },
    ]);
    expect(URL.revokeObjectURL).not.toHaveBeenCalled();
    vi.runAllTimers();
    expect(URL.revokeObjectURL).toHaveBeenCalledWith(objectUrls[0]);
  });

  it('falls back to the caller name when the service sends no disposition', async () => {
    stubFetch(answer(200, 'line\n'));
    await downloadFile('/export', 'cadus-export.jsonl');
    expect(downloads[0].download).toBe('cadus-export.jsonl');
  });

  it('throws the envelope of a failed download instead of saving it', async () => {
    stubFetch(answer(403, '{"error":{"code":"forbidden","message":"Admin only."}}'));
    const err = await rejection(downloadFile('/export', 'x.jsonl'));
    expect(err.status).toBe(403);
    expect(err.code).toBe('forbidden');
    expect(downloads).toHaveLength(0);
  });

  it('parses a filename and refuses a path separator inside one', () => {
    expect(dispositionFilename('attachment; filename="a.jsonl"')).toBe('a.jsonl');
    expect(dispositionFilename("attachment; filename*=UTF-8''b.jsonl")).toBe('b.jsonl');
    expect(dispositionFilename('attachment; filename="../../etc/passwd"')).toBe('.._.._etc_passwd');
    expect(dispositionFilename(null)).toBeNull();
    expect(dispositionFilename('attachment')).toBeNull();
  });
});

describe('the request bodies the service is strict about', () => {
  it('omits work and assisted rather than sending them as undefined', async () => {
    const fetchMock = stubFetch(answer(200, '{}'), answer(200, '{}'));
    await api.taskAnswer('t-1', { problem_id: 'p-1', answer: '3/4' });
    expect((fetchMock.mock.calls[0] as [string, RequestInit])[1].body)
      .toBe('{"problem_id":"p-1","answer":"3/4"}');
    await api.taskAnswer('t-1', { problem_id: 'p-1', answer: '3/4', work: 'w', assisted: true });
    expect((fetchMock.mock.calls[1] as [string, RequestInit])[1].body)
      .toBe('{"problem_id":"p-1","answer":"3/4","work":"w","assisted":true}');
  });

  it('sends no minutes on session/end by default', async () => {
    const fetchMock = stubFetch(answer(200, '{}'), answer(200, '{}'));
    await api.sessionEnd();
    expect((fetchMock.mock.calls[0] as [string, RequestInit])[1].body).toBe('{}');
    await api.sessionEnd(12);
    expect((fetchMock.mock.calls[1] as [string, RequestInit])[1].body).toBe('{"minutes":12}');
  });

  it('escapes a task id and a provider name into the path', async () => {
    const fetchMock = stubFetch(answer(200, '{}'));
    await api.taskServe('t/../admin');
    expect((fetchMock.mock.calls[0] as [string, RequestInit])[0])
      .toBe('/api/task/t%2F..%2Fadmin/serve');
    expect(api.oauthStartUrl('goo gle')).toBe('/api/auth/oauth/goo%20gle/start');
    expect(api.oauthStartUrl('google', '/session')).toBe(
      '/api/auth/oauth/google/start?next=%2Fsession',
    );
  });

  it('builds the query of getGraph and getOperatorFlags only when asked', async () => {
    const fetchMock = stubFetch(
      answer(200, '{}'), answer(200, '{}'), answer(200, '{}'), answer(200, '{}'),
    );
    await api.getGraph();
    await api.getGraph('all');
    await api.getOperatorFlags();
    await api.getOperatorFlags('kp-1');
    const urls = (fetchMock.mock.calls as Array<[string, RequestInit]>).map(([u]) => u);
    expect(urls).toEqual([
      '/api/graph',
      '/api/graph?scope=all',
      '/api/operator/flags',
      '/api/operator/flags?kp=kp-1',
    ]);
  });

  it('points the diagnosis stream at the one per-session subscription URL', () => {
    expect(api.diagnosisStreamUrl()).toBe('/api/diagnosis/stream');
  });
});
