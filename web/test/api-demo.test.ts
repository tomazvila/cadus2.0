/**
 * The S2 acceptance check, part 3: the demo backend.
 *
 * Two invariants live here. F-F6-1 — the demo implements every method the SPA can call.
 * SERVE-idem — its serve re-serves the same problem, exactly as the real one does; a mock
 * that advanced a cursor per call would let the click-through pass against a fiction.
 */
import { describe, expect, it, vi } from 'vitest';
import { ApiError, ROUTES, api, createDemoApi, resolveApi } from '@/api';

describe('the demo client', () => {
  it('F-F6-1: implements every method the route table names', () => {
    const demo = createDemoApi() as unknown as Record<string, unknown>;
    const named = ROUTES.filter((r) => r.via !== 'none').map((r) => String(r.client));
    // 33 until the three `/api/diag/*` placement rows landed.
    expect(named.length).toBe(36);
    for (const member of named) {
      expect(typeof demo[member], `the demo lacks ${member}`).toBe('function');
    }
  });

  it('F-F6-1: carries the same member list as the live client, and flags itself', () => {
    const demo = createDemoApi();
    expect(Object.keys(demo).sort()).toEqual(Object.keys(api).sort());
    expect(demo.demo).toBe(true);
    expect(api.demo).toBe(false);
  });

  it('resolves from ?demo=1 and from nothing else', () => {
    expect(resolveApi('?demo=1').demo).toBe(true);
    expect(resolveApi('?demo=0').demo).toBe(false);
    expect(resolveApi('').demo).toBe(false);
    expect(resolveApi('?verify=abc').demo).toBe(false);
  });

  it('keeps its state per client, not per module', async () => {
    vi.useFakeTimers();
    const first = createDemoApi();
    const second = createDemoApi();
    const p = first.taskServe('demo-lesson');
    await vi.runAllTimersAsync();
    const served = await p;
    const q = first.taskAnswer('demo-lesson', { problem_id: served.problem_id, answer: '3/4' });
    await vi.runAllTimersAsync();
    await q;

    const r = second.taskServe('demo-lesson');
    await vi.runAllTimersAsync();
    // The second client never answered, so it still stands on problem one.
    expect((await r).problem_id).toBe('demo-p1');
  });
});

describe('the demo serve', () => {
  it('SERVE-idem: three serves in a row give the same problem', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    const seen: string[] = [];
    for (let i = 0; i < 3; i += 1) {
      const p = demo.taskServe('demo-lesson');
      await vi.runAllTimersAsync();
      const problem = await p;
      seen.push(problem.problem_id);
      expect(problem.index).toBe(1);
    }
    expect(seen).toEqual(['demo-p1', 'demo-p1', 'demo-p1']);
  });

  it('SERVE-idem: only a committed answer moves the cursor', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    // A teach and two hints are not commits.
    const t = demo.taskTeach('demo-lesson');
    await vi.runAllTimersAsync();
    await t;
    const h = demo.taskHint('demo-lesson', 'demo-p1');
    await vi.runAllTimersAsync();
    expect((await h).hint_number).toBe(1);

    const s1 = demo.taskServe('demo-lesson');
    await vi.runAllTimersAsync();
    expect((await s1).problem_id).toBe('demo-p1');

    const a = demo.taskAnswer('demo-lesson', { problem_id: 'demo-p1', answer: '3/4' });
    await vi.runAllTimersAsync();
    const graded = await a;
    expect('correct' in graded && graded.correct).toBe(true);

    const s2 = demo.taskServe('demo-lesson');
    await vi.runAllTimersAsync();
    expect((await s2).problem_id).toBe('demo-p2');
  });

  it('refuses a stale problem_id and an unknown task with the real codes', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    await expect(demo.taskServe('no-such-task')).rejects.toMatchObject({
      status: 404,
      code: 'unknown_task',
    });
    await expect(
      demo.taskAnswer('demo-lesson', { problem_id: 'demo-p3', answer: '7' }),
    ).rejects.toMatchObject({ status: 404, code: 'unknown_problem' });
  });

  it('offers no diagnosis and no prose, because it runs no worker', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    const a = demo.taskAnswer('demo-lesson', { problem_id: 'demo-p1', answer: 'wrong' });
    await vi.runAllTimersAsync();
    const graded = await a;
    expect('diagnosis' in graded && graded.diagnosis).toEqual({ status: 'not_offered' });
    expect('re_solve' in graded && typeof graded.re_solve).toBe('string');
    await expect(demo.getDiagnosis('any-id')).rejects.toBeInstanceOf(ApiError);
  });

  it('advertises no OAuth provider, so the sign-in page renders no button', async () => {
    vi.useFakeTimers();
    const demo = createDemoApi();
    const p = demo.oauthProviders();
    await vi.runAllTimersAsync();
    expect((await p).providers).toEqual([]);
  });
});
