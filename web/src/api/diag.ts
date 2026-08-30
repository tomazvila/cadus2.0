/**
 * The placement transport (`POST /api/diag/start|answer|finish`).
 *
 * WHY THIS IS A SEPARATE PORT AND NOT AN `ApiClient` MEMBER. `ROUTES` in `types.ts`
 * mirrors `crates/web/src/lib.rs` `create_app` row by row, and the S2 contract test proves
 * that every `ApiClient` member sits in that table. M5 mounts no `/api/diag/*` route:
 * `SPEC_ROUTES_ABSENT` records the three paths and names S10 as the unit that needs them.
 * Putting them on `ApiClient` would either break that proof or put a route in the table
 * that the service does not answer. So the placement screen takes ITS OWN typed port as a
 * prop, and the port has exactly three methods.
 *
 * WHAT THIS MEANS TODAY. `diagApi` posts to the literal spec paths and is correct the day
 * the Rust unit lands. Until then the service answers `404 not_found`, so a caller that
 * hands the live adapter to the screen gets the stated "not available" branch rather than
 * a spinner — `Diagnostic.tsx` owns that branch and one test pins it. `createDemoDiagApi`
 * walks a three-probe placement with no service at all, which is what `?demo=1` and the
 * S13 click-through use.
 *
 * The payloads are the frozen contract of `docs/reference/web-service-1.0-spec.md`
 * (section 2, the three `/api/diag/*` rows).
 */
import { ApiError, request } from './client';
import type { TopicRef } from './types';

/** One placement probe. `topic` is a bare name in 1.0 and a record in 2.0; both render. */
export interface DiagProbe {
  problem_id: string;
  topic?: TopicRef | string | null;
  text: string;
}

/**
 * `POST /api/diag/start`.
 *
 * `probe` is NULLABLE. A diagnostic whose probe list is exhausted answers `{"probe": null}`,
 * and a screen that reads `probe.text` off it goes blank.
 */
export interface DiagStartResponse {
  probe: DiagProbe | null;
  asked?: number;
  cap?: number;
}

/**
 * `POST /api/diag/answer`.
 *
 * The verdict is deterministic-only and the reply carries NO solution and NO expected
 * answer. The screen renders a tick or a cross from `correct` and nothing else, even if a
 * future payload grew a field (DIAG-nosol).
 */
export interface DiagAnswerResponse {
  correct?: boolean;
  next_probe?: DiagProbe | { done: true } | null;
}

/** `POST /api/diag/finish`. Three arrays of topic ids. */
export interface DiagFinishResponse {
  placed: string[];
  conditional: string[];
  frontier: string[];
}

/** The three calls the placement screen makes. Nothing else reaches this route family. */
export interface DiagnosticApi {
  diagStart(course?: string): Promise<DiagStartResponse>;
  diagAnswer(body: { problem_id: string; answer: string }): Promise<DiagAnswerResponse>;
  diagFinish(): Promise<DiagFinishResponse>;
}

/** The error code the service answers for a route it does not mount. */
export const ROUTE_ABSENT_CODE = 'not_found';

/** The live adapter. Same cookie, same envelope, same `request` as every other call. */
export const diagApi: DiagnosticApi = {
  diagStart: (course) => request<DiagStartResponse>('POST', '/diag/start', course ? { course } : {}),
  diagAnswer: (body) => request<DiagAnswerResponse>('POST', '/diag/answer', body),
  diagFinish: () => request<DiagFinishResponse>('POST', '/diag/finish', {}),
};

/** The three demo probes. Short, and each one names a different topic. */
const DEMO_PROBES: readonly DiagProbe[] = [
  { problem_id: 'demo-d1', topic: 'Adding integers', text: 'Work out $-7 + 12$.' },
  { problem_id: 'demo-d2', topic: 'Fractions', text: 'Simplify $\\frac{9}{12}$.' },
  { problem_id: 'demo-d3', topic: 'Linear equations', text: 'Solve $3x - 6 = 9$ for $x$.' },
];

/**
 * The demo placement, for `?demo=1` and the click-through.
 *
 * State is per CLIENT, not per module, for the reason `demo.ts` gives: module state can
 * only be reset by a reload, and one test then poisons the next.
 */
export function createDemoDiagApi(): DiagnosticApi {
  let asked = 0;
  let started = false;

  const probeAt = (i: number): DiagProbe | null => DEMO_PROBES[i] ?? null;

  return {
    diagStart: async () => {
      started = true;
      return { probe: probeAt(asked), asked, cap: DEMO_PROBES.length };
    },
    diagAnswer: async ({ answer }) => {
      if (!started) throw new ApiError(409, 'no_diagnostic', 'No diagnostic is open.');
      asked += 1;
      const next = probeAt(asked);
      return {
        // A blank answer is the honest skip, and it grades incorrect (P3).
        correct: answer.trim().length > 0,
        next_probe: next ?? { done: true },
      };
    },
    diagFinish: async () => {
      started = false;
      return {
        placed: ['integers', 'fractions'],
        conditional: ['linear-equations'],
        frontier: ['linear-equations'],
      };
    },
  };
}

/**
 * The placement port one page load gets.
 *
 * `?demo=1` walks the three canned probes; every other load posts to the service. The
 * function is PURE and takes the flag, for the reason `resolveApi` takes the query string:
 * a read of a global inside would make every caller depend on one.
 *
 * The router calls it ONCE per mount and keeps the answer, because both adapters hold
 * state — `createDemoDiagApi` counts the probes it asked — and a fresh port per render
 * restarts the placement at probe 1.
 */
export function resolveDiag(demo: boolean): DiagnosticApi {
  return demo ? createDemoDiagApi() : diagApi;
}
