/**
 * The S2 acceptance check, part 4: every method of the demo backend answers.
 *
 * F-F6-1 proves the members exist; this file proves what each one says. The demo runs no
 * worker, holds no password, mints no link and is no admin, so the refusals are as much of
 * the contract as the replies.
 */
import { describe, expect, it, vi } from 'vitest';
import { ApiError, createDemoApi } from '@/api';
import type { ApiClient } from '@/api';

/** Let the demo's reply delay elapse and hand back the value. */
async function settle<T>(promise: Promise<T>): Promise<T> {
  await vi.runAllTimersAsync();
  return promise;
}

/** The status and the code one demo refusal carries. */
async function refusal(promise: Promise<void | object | string>) {
  try {
    await promise;
  } catch (e) {
    const err = e as ApiError;
    return { status: err.status, code: err.code, message: err.message, isApiError: e instanceof ApiError };
  }
  throw new Error('the call resolved; it was expected to refuse');
}

describe('the demo probes and account', () => {
  it('answers the two probes and the demo account', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    expect(await settle(demo.health())).toEqual({ ok: true });
    expect(await settle(demo.ready())).toEqual({
      ok: true, db: 'ok', worker: { claim_age_secs: null, stale: false },
    });
    expect((await settle(demo.me())).user).toEqual({
      id: 'demo',
      email: 'demo@cadus.local',
      email_verified: true,
      created_at: '2026-01-01T00:00:00+00:00',
    });
  });

  it('signs nobody in and changes no password', async () => {
    const demo = createDemoApi();
    expect(await refusal(demo.login('a@b.test', 'x'))).toEqual({
      status: 401,
      code: 'invalid_credentials',
      message: 'The demo signs nobody in. Leave demo mode to sign in.',
      isApiError: true,
    });
    expect(await refusal(demo.changePassword('old', 'new'))).toMatchObject({
      status: 401, code: 'invalid_credentials', message: 'The demo holds no password to change.',
    });
  });

  it('registers no account but answers the non-enumerable receipt', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    expect(await settle(demo.signup('a@b.test', 'hunter2hunter2'))).toEqual({
      status: 'verification_required',
      message: 'The demo registers no account.',
    });
    expect(await settle(demo.resendVerification('a@b.test'))).toEqual({ ok: true });
    expect(await settle(demo.forgotPassword('a@b.test'))).toEqual({ ok: true });
    expect(await settle(demo.logout())).toEqual({ ok: true });
    expect(await settle(demo.logoutAll())).toEqual({ ok: true, revoked: 0 });
  });

  it('mints no reset link and no verification link', async () => {
    const demo = createDemoApi();
    expect(await refusal(demo.resetPassword('t', 'hunter2hunter2'))).toMatchObject({
      status: 400, code: 'invalid_token', message: 'The demo mints no reset link.',
    });
    expect(await refusal(demo.verifyEmail('t'))).toMatchObject({
      status: 400, code: 'invalid_token', message: 'The demo mints no verification link.',
    });
  });
});

describe('the demo curriculum and session', () => {
  it('lists the one module and its course as an object', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    expect(await settle(demo.listModules())).toEqual({
      course: { id: 'foundations', name: 'Foundations' },
      modules: ['Arithmetic'],
    });
    expect(await settle(demo.enroll('proofs'))).toEqual({
      enrolled: 'proofs', mastery_floor: ['whole-numbers'], floor_size: 1,
    });
  });

  it('closes the open session, and a session nobody opened, with the minutes given', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    // No session was started: the close still names the demo session and takes 8 minutes.
    expect(await settle(demo.sessionEnd())).toEqual({
      session: 'demo-session',
      xp_earned: 12,
      minutes: 8,
      xp: { total: 352, today: 24, goal: 40, streak_days: 3 },
      anki: { pending: 0 },
    });
    expect((await settle(demo.sessionStart())).session).toBe('demo-session');
    expect((await settle(demo.sessionEnd(3))).minutes).toBe(3);
    // The close dropped the session, so the plan names the fallback again.
    expect((await settle(demo.getPlan())).session).toBe('demo-session');
  });

  it('numbers the plan task by the answers given, and closes it after the third', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    const answers = ['3/4', '4', '7'];
    const statuses: string[] = [];
    for (const [i, answer] of answers.entries()) {
      const problem = await settle(demo.taskServe('demo-lesson'));
      expect(problem.index).toBe(i + 1);
      const graded = await settle(demo.taskAnswer('demo-lesson', { problem_id: problem.problem_id, answer }));
      if ('task_status' in graded) {
        statuses.push(graded.task_status);
        expect(graded.next === null).toBe(i === 2);
        expect(graded.attempt_id).toBe(`demo-lesson-${i + 1}`);
      }
    }
    expect(statuses).toEqual(['continue', 'continue', 'task_passed']);
    expect((await settle(demo.getPlan())).tasks[0].progress).toEqual({ answered: 3, done: true });
    expect(await refusal(demo.taskServe('demo-lesson'))).toMatchObject({
      status: 409, code: 'task_complete',
    });
  });

  it('refuses the wrong task on teach, hint and answer, and the wrong problem on hint', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    const unknownTask = { status: 404, code: 'unknown_task', message: 'The demo plans one task.' };
    expect(await refusal(demo.taskTeach('other'))).toMatchObject(unknownTask);
    expect(await refusal(demo.taskHint('other', 'demo-p1'))).toMatchObject(unknownTask);
    expect(await refusal(demo.taskAnswer('other', { problem_id: 'demo-p1', answer: '1' }))).toMatchObject(unknownTask);
    expect(await refusal(demo.taskHint('demo-lesson', 'demo-p2'))).toMatchObject({
      status: 404, code: 'unknown_problem',
    });
  });

  it('grades a miss with the re-solve line and no XP, and a hit with XP and no re-solve', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    const miss = await settle(demo.taskAnswer('demo-lesson', { problem_id: 'demo-p1', answer: '1/2' }));
    expect(miss).toMatchObject({ correct: false, work_quality: 'nearly_passable' });
    expect('xp' in miss).toBe(false);
    expect('re_solve' in miss && miss.re_solve).toContain('Study the worked solution');

    const hit = await settle(demo.taskAnswer('demo-lesson', { problem_id: 'demo-p2', answer: ' +4 ' }));
    expect(hit).toMatchObject({ correct: true, work_quality: 'perfect', xp: 10 });
    expect('re_solve' in hit).toBe(false);
  });

  it('walks the hint ladder to its last rung and resets it on an answer', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    const hints: number[] = [];
    for (let i = 0; i < 4; i += 1) {
      hints.push((await settle(demo.taskHint('demo-lesson', 'demo-p1'))).hint_number);
    }
    expect(hints).toEqual([1, 2, 3, 3]);
    await settle(demo.taskAnswer('demo-lesson', { problem_id: 'demo-p1', answer: '3/4' }));
    expect((await settle(demo.taskHint('demo-lesson', 'demo-p2'))).hint_number).toBe(1);
  });

  it('teaches the knowledge point of the live problem', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    expect((await settle(demo.taskTeach('demo-lesson'))).kp).toBe('kp-fraction-simplify');
  });
});

describe('the demo refusals of the admin routes', () => {
  it('refuses all five admin routes and the export as a non-admin', async () => {
    const demo = createDemoApi();
    const adminOnly = { status: 403, code: 'forbidden' };
    const calls: Array<() => Promise<object | void>> = [
      () => demo.getOperatorFlags(),
      () => demo.listContent(),
      () => demo.getContent('d1'),
      () => demo.approveContent('d1'),
      () => demo.rejectContent('d1', 'why'),
    ];
    for (const call of calls) {
      expect(await refusal(call())).toMatchObject({
        ...adminOnly, message: 'This route serves an admin account only.',
      });
    }
    expect(await refusal(demo.downloadExport())).toMatchObject({
      ...adminOnly, message: 'The demo keeps no event log to export.',
    });
  });

  it('builds the OAuth start URL and answers the graph of one scope', async () => {
    vi.useFakeTimers();
    const demo: ApiClient = createDemoApi();
    expect(demo.oauthStartUrl('goo gle')).toBe('/api/auth/oauth/goo%20gle/start');
    expect((await settle(demo.getGraph())).scope).toBeNull();
    expect((await settle(demo.getGraph('all'))).scope).toBe('all');
    expect((await settle(demo.getStatus())).course).toEqual({ id: 'foundations', name: 'Foundations' });
  });
});
