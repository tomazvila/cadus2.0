/**
 * The placement transport (`POST /api/diag/start|answer|finish`).
 *
 * WHY THIS PORT STILL EXISTS. The three calls are `ApiClient` members now — the service
 * mounts the three routes, `ROUTES` in `types.ts` carries them, and both clients answer
 * them. The port stays because the placement screen takes ITS OWN transport as a prop:
 * one page load resolves it once and keeps it, and a test hands the screen a stub without
 * building a whole client. `diagApi` is the live client's three methods and nothing else.
 *
 * The payload types moved to `types.ts`, beside every other response type, and are
 * re-exported here for the screen that already imports them from this file.
 *
 * The payloads are the frozen contract of `docs/reference/web-service-1.0-spec.md`
 * (section 2, the three `/api/diag/*` rows).
 */
import { ApiError } from './client';
import { api } from './endpoints';
import type {
  DiagAnswerResponse,
  DiagFinishResponse,
  DiagProbe,
  DiagStartResponse,
} from './types';

export type { DiagAnswerResponse, DiagFinishResponse, DiagProbe, DiagStartResponse };

/** The three calls the placement screen makes. Nothing else reaches this route family. */
export interface DiagnosticApi {
  diagStart(course?: string): Promise<DiagStartResponse>;
  diagAnswer(body: { problem_id: string; answer: string }): Promise<DiagAnswerResponse>;
  diagFinish(): Promise<DiagFinishResponse>;
}

/**
 * The error code the service answers for a route it does not mount.
 *
 * `Diagnostic.tsx` still reads it: a deployment older than the placement routes answers
 * `404 not_found`, and the screen states "not available" rather than spinning.
 */
export const ROUTE_ABSENT_CODE = 'not_found';

/** The live adapter: the three client methods, same cookie and same envelope. */
export const diagApi: DiagnosticApi = {
  diagStart: (course) => api.diagStart(course),
  diagAnswer: (body) => api.diagAnswer(body),
  diagFinish: () => api.diagFinish(),
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
