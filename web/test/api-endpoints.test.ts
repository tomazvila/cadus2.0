/**
 * The S2 acceptance check, part 5: every typed method reaches its row of the route table
 * with the method, the path and the body the service reads.
 *
 * `api-client.test.ts` proves the wrapper; this file proves the thirty-odd one-liners over
 * it, because a route rename that breaks one of them must break a test and not a screen.
 */
import { describe, expect, it, vi } from 'vitest';
import { api } from '@/api';

/** One `200 {}` per call, and the `[method, url, body]` of every call made. */
function record() {
  const fetchMock = vi.fn<(url: string, init: RequestInit) => Promise<Response>>(
    async () => new Response('{}', { status: 200, headers: { 'Content-Type': 'application/json' } }),
  );
  vi.stubGlobal('fetch', fetchMock);
  return () => fetchMock.mock.calls.map(([url, init]) => [init.method, url, init.body ?? null]);
}

describe('the probes and the sign-in', () => {
  it('read the two probes and post the credentials', async () => {
    const calls = record();
    await api.health();
    await api.ready();
    await api.login('a@b.test', 'pw');
    await api.logout();
    expect(calls()).toEqual([
      ['GET', '/api/health', null],
      ['GET', '/api/ready', null],
      ['POST', '/api/auth/login', '{"email":"a@b.test","password":"pw"}'],
      ['POST', '/api/auth/logout', '{}'],
    ]);
  });
});

describe('the study loop', () => {
  it('reads the status, the graph and the plan, and serves, hints and grades', async () => {
    const calls = record();
    await api.getStatus();
    await api.getGraph();
    await api.getGraph('all');
    await api.getPlan();
    await api.taskServe('t1');
    await api.taskHint('t1', 'p 1');
    await api.taskAnswer('t1', { problem_id: 'p1', answer: '3/4' });
    await api.taskAnswer('t1', { problem_id: 'p1', answer: '3/4', work: 'w', assisted: false });
    await api.sessionEnd();
    await api.sessionEnd(0);
    await api.diagStart();
    await api.diagStart('foundations');
    await api.getOperatorFlags();
    await api.getOperatorFlags('a:b');
    expect(calls()).toEqual([
      ['GET', '/api/status', null],
      ['GET', '/api/graph', null],
      ['GET', '/api/graph?scope=all', null],
      ['GET', '/api/session/plan', null],
      ['POST', '/api/task/t1/serve', '{}'],
      ['POST', '/api/task/t1/hint', '{"problem_id":"p 1"}'],
      ['POST', '/api/task/t1/answer', '{"problem_id":"p1","answer":"3/4"}'],
      ['POST', '/api/task/t1/answer', '{"problem_id":"p1","answer":"3/4","work":"w","assisted":false}'],
      ['POST', '/api/session/end', '{}'],
      ['POST', '/api/session/end', '{"minutes":0}'],
      ['POST', '/api/diag/start', '{}'],
      ['POST', '/api/diag/start', '{"course":"foundations"}'],
      ['GET', '/api/operator/flags', null],
      ['GET', '/api/operator/flags?kp=a%3Ab', null],
    ]);
  });
});

describe('the auth routes', () => {
  it('post the credentials, the tokens and the addresses the service reads', async () => {
    const calls = record();
    await api.me();
    await api.signup('a@b.test', 'pw');
    await api.logoutAll();
    await api.changePassword('old', 'new');
    await api.forgotPassword('a@b.test');
    await api.resetPassword('tok', 'new');
    await api.verifyEmail('tok');
    await api.resendVerification('a@b.test');
    await api.oauthProviders();
    expect(calls()).toEqual([
      ['GET', '/api/auth/me', null],
      ['POST', '/api/auth/signup', '{"email":"a@b.test","password":"pw"}'],
      ['POST', '/api/auth/logout-all', '{}'],
      ['POST', '/api/auth/password/change', '{"current_password":"old","new_password":"new"}'],
      ['POST', '/api/auth/password/forgot', '{"email":"a@b.test"}'],
      ['POST', '/api/auth/password/reset', '{"token":"tok","new_password":"new"}'],
      ['POST', '/api/auth/verify-email', '{"token":"tok"}'],
      ['POST', '/api/auth/verify-email/resend', '{"email":"a@b.test"}'],
      ['GET', '/api/auth/oauth/providers', null],
    ]);
  });
});

describe('the curriculum and session routes', () => {
  it('read the modules, enroll, start, teach and read the diagnosis', async () => {
    const calls = record();
    await api.listModules();
    await api.enroll('proofs');
    await api.sessionStart();
    await api.taskTeach('t 1');
    await api.getDiagnosis('j/1');
    expect(calls()).toEqual([
      ['GET', '/api/modules', null],
      ['POST', '/api/enroll', '{"course":"proofs"}'],
      ['POST', '/api/session/start', '{}'],
      ['POST', '/api/task/t%201/teach', '{}'],
      ['GET', '/api/diagnosis/j%2F1', null],
    ]);
  });

  it('post the placement body the service reads', async () => {
    const calls = record();
    await api.diagAnswer({ problem_id: 'd1', answer: '5' });
    await api.diagFinish();
    expect(calls()).toEqual([
      ['POST', '/api/diag/answer', '{"problem_id":"d1","answer":"5"}'],
      ['POST', '/api/diag/finish', '{}'],
    ]);
  });
});

describe('the review routes', () => {
  it('send a filter key only when it has a value, and page 0 as no key', async () => {
    const calls = record();
    await api.listContent();
    await api.listContent({ status: 'pending', kind: 'teach', kp: 'a:b', page: 2 });
    await api.listContent({ status: '', page: 0 });
    expect(calls()).toEqual([
      ['GET', '/api/admin/content', null],
      ['GET', '/api/admin/content?status=pending&kind=teach&kp=a%3Ab&page=2', null],
      ['GET', '/api/admin/content', null],
    ]);
  });

  it('read one document and post the two decisions with an escaped digest', async () => {
    const calls = record();
    await api.getContent('d/1');
    await api.approveContent('d/1');
    await api.rejectContent('d/1', 'why');
    expect(calls()).toEqual([
      ['GET', '/api/admin/content/d%2F1', null],
      ['POST', '/api/admin/content/d%2F1/approve', '{}'],
      ['POST', '/api/admin/content/d%2F1/reject', '{"reason":"why"}'],
    ]);
  });
});
