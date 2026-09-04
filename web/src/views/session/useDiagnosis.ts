/**
 * The async diagnosis feed (A4) — one subscription per session, a 2 s poll behind it, and a
 * 30 s deadline in front of both.
 *
 * The grade reply returns the whole verdict from local CPU and hands the prose off
 * (`docs/reference/web-service-1.0-spec.md` section 2.1). So the study loop paints the
 * verdict, the solution and the re-solve instruction at once, and THIS file fills the
 * explanation in later. Three rules are load-bearing, and each one is claimed by a named
 * test.
 *
 *   DIAG-async — THE VERDICT NEVER WAITS. Nothing here can hold a render back: the panel is
 *   a separate node under the verdict, and it starts at `pending`. A diagnosis that never
 *   arrives costs prose and nothing else (L2/L3).
 *
 *   DIAG-poll — THE POLL IS THE FALLBACK, ARMED IN ADVANCE. A proxy that buffers
 *   server-sent events leaves the connection OPEN and mute, so stream health cannot be the
 *   trigger — there is no signal to read. Every watched job therefore arms a 2 s interval
 *   the moment the grade lands. In the healthy path the push arrives inside the service's
 *   250 ms notify-to-flush budget, the job leaves the watch list, and the interval is
 *   cleared BEFORE its first tick: the fallback costs zero requests when it is not needed.
 *
 *   DIAG-30s — NOTHING STAYS OPEN. A job still `pending` 30 s after the grade reads
 *   `failed`. The service applies the same rule to its own rows (`crates/web/src/diagnosis.rs`
 *   `PENDING_DEADLINE_SECS`), and the client applies it too, because a stalled worker and a
 *   silenced stream look identical from here.
 *
 * # Why the poll does not go through `useCall`
 *
 * `useCall` toasts every failure with a Retry (F-36-1). This poll fires up to 15 times per
 * job with no learner behind it, so that contract inverts: a service blip would raise a
 * toast every 2 s for prose the learner did not ask for. A failed poll is therefore
 * swallowed and the 30 s rule ends the job. Nothing is lost — a 401 reaches the learner on
 * the next serve or grade, which DOES go through `useCall`.
 *
 * # Why one connection, and why it is registered nowhere else
 *
 * The subscription is per SESSION, not per problem (spec section 4.1). The session view
 * opens it at mount and closes it at unmount; the timers go through the view `Lifetime`, so
 * they cannot outlive the view either (F-37-1b).
 */
import { useEffect, useMemo, useState, useSyncExternalStore } from 'react';
import type { ApiClient, DiagnosisField, DiagnosisJob } from '@/api/types';
import type { Lifetime } from '@/hooks/useLifetime';

/** The fallback poll interval, in milliseconds. The spec literal (section 2.1). */
export const DIAGNOSIS_POLL_MS = 2000;

/** A job still pending this long after the grade reads `failed`. The spec literal. */
export const DIAGNOSIS_DEADLINE_MS = 30_000;

/** The `event:` name of every frame the stream writes (`crates/web/src/diagnosis.rs`). */
const DIAGNOSIS_EVENT = 'diagnosis';

/** What the panel paints for one grade reply. */
export type DiagnosisPanel =
  | { status: 'pending' }
  | { status: 'ready'; error_tags: string[]; prose: string }
  | { status: 'failed' };

/**
 * The shared value for every job nobody has heard about yet.
 *
 * ONE frozen object, not a fresh literal per read. `useSyncExternalStore` compares snapshots
 * by identity and re-renders forever when the getter allocates.
 */
const PENDING: DiagnosisPanel = Object.freeze({ status: 'pending' });

const FAILED: DiagnosisPanel = Object.freeze({ status: 'failed' });

/** Read one wire job into a panel state. `capped` is a failure the learner cannot fix. */
function panelOf(job: DiagnosisJob): DiagnosisPanel {
  if (job.status === 'ready') {
    return { status: 'ready', error_tags: job.error_tags, prose: job.prose ?? '' };
  }
  if (job.status === 'pending') return PENDING;
  return FAILED;
}

interface Watch {
  /** How many panels are showing this job. The last one out clears the timers. */
  count: number;
  poll: number;
  deadline: number;
}

export interface DiagnosisDeps {
  api: ApiClient;
  life: Lifetime;
}

/**
 * The store behind the panel: the jobs it has landed and the timers still owed.
 *
 * An external store, for the reason `usePhase` gives: a stream frame and a poll answer both
 * arrive outside React, and `useSyncExternalStore` is the supported way to read a mutable
 * source without tearing.
 */
export class DiagnosisStore {
  private readonly deps: DiagnosisDeps;
  private readonly jobs = new Map<string, DiagnosisPanel>();
  private readonly watches = new Map<string, Watch>();
  private readonly listeners = new Set<() => void>();

  constructor(deps: DiagnosisDeps) { this.deps = deps; }

  readonly subscribe = (fn: () => void): (() => void) => {
    this.listeners.add(fn);
    return () => { this.listeners.delete(fn); };
  };

  /** The snapshot of one job. A null id is "no job is owed here". */
  readonly get = (id: string | null): DiagnosisPanel | null => {
    if (!id) return null;
    return this.jobs.get(id) ?? PENDING;
  };

  private emit(): void {
    for (const fn of [...this.listeners]) fn();
  }

  /**
   * Land one wire job — from a stream frame or from a poll answer.
   *
   * A TERMINAL STATE NEVER GOES BACK TO PENDING. The two sources race by design, and a poll
   * answer read before the push landed would otherwise wipe prose already on screen.
   */
  readonly land = (job: DiagnosisJob): void => {
    const id = job?.id;
    if (!id) return;
    const next = panelOf(job);
    const current = this.jobs.get(id);
    if (current && current.status !== 'pending') return;
    if (next.status === 'pending') return;
    this.jobs.set(id, next);
    this.release(id);
    this.emit();
  };

  /**
   * The 30 s rule. The job stops; the verdict on screen stands.
   *
   * A job that landed has no deadline left to fire: `land` releases the timers first.
   */
  private readonly expire = (id: string): void => {
    this.jobs.set(id, FAILED);
    this.release(id);
    this.emit();
  };

  /** Stop the timers of one job. Called on every terminal state, and on the last unwatch. */
  private release(id: string): void {
    const watch = this.watches.get(id);
    if (!watch) return;
    this.deps.life.clearTimer(watch.poll);
    this.deps.life.clearTimer(watch.deadline);
    this.watches.delete(id);
  }

  /**
   * Start following one pending job: arm the fallback poll and the deadline.
   *
   * Both timers go through the view `Lifetime`, so a learner who leaves the session inside
   * the window takes them with him (F-37-1b).
   */
  readonly open = (id: string): void => {
    const watch = this.watches.get(id);
    if (watch) { watch.count += 1; return; }
    // Already finished, or already expired. Nothing to arm.
    if (this.jobs.get(id)) return;
    const { life } = this.deps;
    this.watches.set(id, {
      count: 1,
      poll: life.setInterval(() => { void this.pollOnce(id); }, DIAGNOSIS_POLL_MS),
      deadline: life.setTimeout(() => { this.expire(id); }, DIAGNOSIS_DEADLINE_MS),
    });
  };

  /** The panel is gone. The last one out stops the work it started. */
  readonly close = (id: string): void => {
    const watch = this.watches.get(id);
    if (!watch) return;
    watch.count -= 1;
    if (watch.count <= 0) this.release(id);
  };

  /**
   * One fallback read. A failure is swallowed on purpose — see the module docstring.
   */
  private async pollOnce(id: string): Promise<void> {
    let job: DiagnosisJob;
    try {
      job = await this.deps.api.getDiagnosis(id);
    } catch {
      return;
    }
    if (!this.deps.life.alive()) return;
    this.land(job);
  }
}

/**
 * Open the one per-session subscription and give back the store that feeds every panel.
 *
 * The stream carries `event: diagnosis` frames whose data is the same body the poll route
 * answers, so ONE reader serves both paths. A frame that does not parse is dropped: the
 * poll behind it reads the row again 2 s later.
 */
export function useDiagnosisStream({ api, life, enabled }: DiagnosisDeps & { enabled: boolean }):
DiagnosisStore {
  // The lazy initializer, for the reason `usePhase` gives: React calls it twice in StrictMode
  // development and throws one result away, which is safe ONLY because the constructor is a
  // pure allocation. It arms no timer and opens no connection.
  const [store] = useState(() => new DiagnosisStore({ api, life }));

  useEffect(() => {
    // The demo runs no worker, so every demo grade reports `not_offered` and a connection
    // would retry against a route the demo client does not answer.
    if (!enabled || typeof EventSource === 'undefined') return undefined;
    const source = new EventSource(api.diagnosisStreamUrl());
    const onFrame = (event: MessageEvent<string>) => {
      let job: DiagnosisJob;
      try {
        job = JSON.parse(event.data) as DiagnosisJob;
      } catch {
        return;
      }
      store.land(job);
    };
    source.addEventListener(DIAGNOSIS_EVENT, onFrame as EventListener);
    // A drop needs no handler of its own. `EventSource` reconnects by itself, and the poll
    // that every watched job already armed is what makes the prose land meanwhile.
    return () => {
      source.removeEventListener(DIAGNOSIS_EVENT, onFrame as EventListener);
      source.close();
    };
  }, [api, enabled, store]);

  return store;
}

/**
 * Follow the `diagnosis` field of one grade reply.
 *
 * `ready` is read straight off the field: a pre-authored distractor matched in the grade
 * transaction, so there is nothing to wait for and no job row to poll. `not_offered` and a
 * null field give null, and the panel renders nothing at all.
 */
export function useDiagnosisJob(
  store: DiagnosisStore,
  field: DiagnosisField,
): DiagnosisPanel | null {
  const id = field && field.status === 'pending' ? field.id : null;

  const getSnapshot = useMemo(() => () => store.get(id), [store, id]);
  const live = useSyncExternalStore(store.subscribe, getSnapshot, getSnapshot);

  useEffect(() => {
    if (!id) return undefined;
    store.open(id);
    return () => { store.close(id); };
  }, [store, id]);

  if (field?.status === 'ready') {
    return { status: 'ready', error_tags: field.error_tags, prose: field.prose };
  }
  return live;
}
