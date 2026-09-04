/**
 * The store behind the diagnosis panel (S9), driven without a screen.
 *
 * `diagnosis.test.tsx` proves the three rules through the session view. This file pins the
 * edges of the store itself: a frame with no id, a frame for a job nobody watches yet, two
 * panels on one job, and a poll that lands after the view left.
 */
import { describe, expect, it, vi } from 'vitest';
import { renderHook } from '@testing-library/react';
import { createDemoApi } from '@/api';
import { createLifetime } from '@/hooks/useLifetime';
import { DiagnosisStore, useDiagnosisJob } from '@/views/session/useDiagnosis';
import type { ApiClient, DiagnosisJob } from '@/api/types';

const READY: DiagnosisJob = { id: 'j1', status: 'ready', error_tags: ['sign'], prose: 'Because.' };

function storeWith(over: Partial<ApiClient> = {}) {
  vi.useFakeTimers();
  const life = createLifetime();
  const api: ApiClient = { ...createDemoApi(), ...over };
  return { life, api, store: new DiagnosisStore({ api, life }) };
}

describe('DiagnosisStore', () => {
  it('reads a null id as no job, and a job nobody heard of as pending', () => {
    const { store } = storeWith();
    expect(store.get(null)).toBeNull();
    expect(store.get('j1')).toEqual({ status: 'pending' });
  });

  it('drops a frame that names no job', () => {
    const { store } = storeWith();
    const frame: Partial<DiagnosisJob> = { status: 'ready', error_tags: [] };
    store.land(frame as DiagnosisJob);
    expect(store.get('j1')).toEqual({ status: 'pending' });
  });

  it('lands a frame for a job nobody watches yet, and arms nothing when it is watched', () => {
    const { store } = storeWith();
    store.land(READY);
    expect(store.get('j1')).toEqual({ status: 'ready', error_tags: ['sign'], prose: 'Because.' });
    store.open('j1');
    expect(vi.getTimerCount()).toBe(0);
    // A ready job never goes back to pending, whatever a later poll says.
    store.land({ id: 'j1', status: 'pending', error_tags: [] });
    expect(store.get('j1')!.status).toBe('ready');
    store.close('j1');
  });

  it('reads a ready job with no prose as empty prose', () => {
    const { store } = storeWith();
    store.land({ id: 'j1', status: 'ready', error_tags: [] });
    expect(store.get('j1')).toEqual({ status: 'ready', error_tags: [], prose: '' });
  });

  it('keeps the timers while any panel still watches the job', () => {
    const { store } = storeWith();
    store.open('j1');
    store.open('j1');
    expect(vi.getTimerCount()).toBe(2);
    store.close('j1');
    expect(vi.getTimerCount()).toBe(2);
    store.close('j1');
    expect(vi.getTimerCount()).toBe(0);
    // A close with nothing open is a no-op.
    store.close('j1');
    expect(vi.getTimerCount()).toBe(0);
  });

  it('lands nothing from a poll that answers after the view left', async () => {
    let release!: (job: DiagnosisJob) => void;
    const getDiagnosis = vi.fn(() => new Promise<DiagnosisJob>((r) => { release = r; }));
    const { store, life } = storeWith({ getDiagnosis });
    store.open('j1');
    await vi.advanceTimersByTimeAsync(2000);
    expect(getDiagnosis).toHaveBeenCalledTimes(1);

    life.end();
    release(READY);
    await vi.advanceTimersByTimeAsync(0);
    expect(store.get('j1')).toEqual({ status: 'pending' });
  });

  it('serves two panels of one job from one watch', () => {
    const { store } = storeWith();
    const field = { id: 'j1', status: 'pending' as const };
    const a = renderHook(() => useDiagnosisJob(store, field));
    const b = renderHook(() => useDiagnosisJob(store, field));
    expect(a.result.current).toEqual({ status: 'pending' });
    expect(vi.getTimerCount()).toBe(2);
    a.unmount();
    expect(vi.getTimerCount()).toBe(2);
    b.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });
});
