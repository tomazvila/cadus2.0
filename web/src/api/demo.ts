/**
 * The demo backend behind `?demo=1`.
 *
 * F-F6-1, AND ITS RESOLUTION. The whole object is swapped in for `api`, so a method the
 * SPA calls and the demo lacks is a `not a function` TypeError in the one mode nobody runs
 * in CI. 1.0 guarded that with a `console.warn` at module load and shipped a demo that
 * crashed on every lesson. `createDemoApi(): ApiClient` deletes the failure mode: a
 * missing method is a BUILD error, and the type IS the check.
 *
 * Two rules this file keeps.
 *  * SERVE-idem (trap T13). `taskServe` re-serves the SAME problem until an answer commits
 *    it. A mock that advances a cursor per call makes the learner practise a problem the
 *    real server never served, and the click-through then passes against a fiction.
 *  * State is per CLIENT, not per module. A demo that held state in module scope could
 *    only be reset by reloading the page, and one test poisoned the next in its file.
 *
 * The demo answers no model prose, so every grade reports `diagnosis: not_offered` and
 * `getDiagnosis` refuses. That is the honest shape: the demo has no worker.
 */
import { ApiError } from './client';
import { createDemoDiagApi } from './diag';
import type {
  ApiClient,
  DiagnosisJob,
  PlanTask,
  ServedProblem,
  TaskAnswerResponse,
  User,
} from './types';

const DEMO_USER: User = {
  id: 'demo',
  email: 'demo@cadus.local',
  email_verified: true,
  created_at: '2026-01-01T00:00:00+00:00',
};

/** The stock re-solve instruction, shortened. The real text is a server constant. */
const DEMO_RE_SOLVE =
  'Study the worked solution above until you can see why each step follows, then solve it '
  + 'again without help.';

interface DemoProblem {
  problem_id: string;
  text: string;
  kp: string;
  expected: string;
  solution: string;
}

const DEMO_PROBLEMS: readonly DemoProblem[] = [
  {
    problem_id: 'demo-p1',
    text: 'Simplify $\\frac{6}{8}$.',
    kp: 'kp-fraction-simplify',
    expected: '3/4',
    solution: 'Divide the numerator and the denominator by 2: $\\frac{6}{8}=\\frac{3}{4}$.',
  },
  {
    problem_id: 'demo-p2',
    text: 'Solve $2x + 1 = 9$ for $x$.',
    kp: 'kp-linear-one-step',
    expected: '4',
    solution: 'Subtract 1 from both sides, then divide by 2: $x = 4$.',
  },
  {
    problem_id: 'demo-p3',
    text: 'What is $\\sqrt{49}$?',
    kp: 'kp-square-roots',
    expected: '7',
    solution: '$7 \\times 7 = 49$, so $\\sqrt{49} = 7$.',
  },
];

const DEMO_HINTS: readonly string[] = [
  'Name what the question asks for before you compute anything.',
  'Write the one operation that isolates the unknown.',
  'Do that operation on both sides, then simplify.',
];

const DEMO_TASK_ID = 'demo-lesson';

/** The demo's grader: whitespace-insensitive, and a leading `+` dropped. */
const norm = (value: string) => value.replace(/\s/g, '').replace(/^\+/, '');

/** A short delay, so a demo screen shows its loading state the way the real one does. */
const wait = (ms: number) => new Promise<void>((resolve) => { setTimeout(resolve, ms); });
async function reply<T>(value: T, ms = 120): Promise<T> {
  await wait(ms);
  return value;
}

/** The one task of the demo plan, with `answered` of its problems graded so far. */
function demoTask(answered: number): PlanTask {
  return {
    task_id: DEMO_TASK_ID,
    task_type: 'lesson',
    topic: { id: 'fractions', name: 'Fractions', module: 'Arithmetic' },
    kp: DEMO_PROBLEMS[0].kp,
    start_at_kp: DEMO_PROBLEMS[0].kp,
    n_problems: DEMO_PROBLEMS.length,
    mix: null,
    component_topics: null,
    time_budget_secs: 600,
    difficulty_target: 0.7,
    why: 'Frontier topic: fractions is ready to learn.',
    confirm: false,
    progress: { answered, done: answered >= DEMO_PROBLEMS.length },
  };
}

/** The message the four review routes refuse a demo caller with (C6). */
const DEMO_ADMIN_ONLY = 'This route serves an admin account only.';

/** A refusal the demo cannot honestly answer. Same shape as a served one. */
function refuse(status: number, code: string, message: string): never {
  throw new ApiError(status, code, message);
}

/**
 * Build one demo client with its own state.
 *
 * `cursor` moves only when an answer commits. `taskServe` reads it and never writes it,
 * which is the whole of SERVE-idem.
 */
export function createDemoApi(): ApiClient {
  let cursor = 0;
  let answered = 0;
  let hintCount = 0;
  let open: string | null = null;
  // The placement: the same three canned probes `api/diag.ts` walks for a screen that takes
  // the port on its own. One adapter per CLIENT, so one test never poisons the next, and
  // this pair keeps the client surface whole: every row of `ROUTES` has a method on both
  // clients.
  const placement = createDemoDiagApi();

  const served = (): ServedProblem => {
    const problem = DEMO_PROBLEMS[cursor];
    return {
      problem_id: problem.problem_id,
      index: cursor + 1,
      total: DEMO_PROBLEMS.length,
      text: problem.text,
      kp: problem.kp,
      time_budget_secs: 120,
      countdown: false,
    };
  };

  return {
    demo: true,

    health: () => reply({ ok: true }),
    ready: () =>
      reply({ ok: true, db: 'ok' as const, worker: { claim_age_secs: null, stale: false } }),

    me: () => reply({ user: DEMO_USER }),
    login: async () =>
      refuse(401, 'invalid_credentials', 'The demo signs nobody in. Leave demo mode to sign in.'),
    signup: () =>
      reply({
        status: 'verification_required' as const,
        message: 'The demo registers no account.',
      }),
    logout: () => reply({ ok: true as const }),
    logoutAll: () => reply({ ok: true as const, revoked: 0 }),
    changePassword: async () =>
      refuse(401, 'invalid_credentials', 'The demo holds no password to change.'),
    forgotPassword: () => reply({ ok: true as const }),
    resetPassword: async () => refuse(400, 'invalid_token', 'The demo mints no reset link.'),
    verifyEmail: async () => refuse(400, 'invalid_token', 'The demo mints no verification link.'),
    resendVerification: () => reply({ ok: true as const }),
    // No provider is configured, so the sign-in page renders no button (AUTH-7).
    oauthProviders: () => reply({ providers: [] }),
    oauthStartUrl: (provider) => `/api/auth/oauth/${encodeURIComponent(provider)}/start`,

    getStatus: () =>
      reply({
        course: { id: 'foundations', name: 'Foundations' },
        placed: true,
        courses: [{ id: 'foundations', name: 'Foundations', current: true }],
        test_prep: null,
        xp: { total: 340, today: 12, goal: 40, streak_days: 3 },
        velocity: {
          xp_per_day_28d: 21.5,
          topics_per_week_28d: 2.25,
          course_progress: 0.18,
          eta: '2026-11-04',
        },
        quiz: { last_at: '2026-08-24', xp_since: 120, retake_pending: false },
        pending_remediation: [],
        quiz_due: false,
        drill_due: false,
        frontier: 4,
        due_reviews: 2,
        nearly_due: 1,
        mastery: { practiced: 9, inferred: 6, total: 50, to_confirm: ['whole-numbers'] },
        // The demo grades every answer, so nothing waits for a human (D-F2).
        ungraded_attempts: {},
        ungraded: 0,
      }),

    getGraph: (scope) =>
      reply({
        now: '2026-08-30T09:00:00+00:00',
        scope: scope ?? null,
        courses: [{ id: 'foundations', name: 'Foundations', current: true }],
        modules: ['Arithmetic'],
        counts: { nodes: 2, edges: 1, mastered: 1 },
        nodes: [
          {
            id: 'whole-numbers',
            name: 'Whole numbers',
            module: 'Arithmetic',
            course: 'foundations',
            status: 'floor' as const,
            ability: 0.9,
          },
          {
            id: 'fractions',
            name: 'Fractions',
            module: 'Arithmetic',
            course: 'foundations',
            status: 'frontier' as const,
            ability: 0.1,
          },
        ],
        edges: [{ from: 'whole-numbers', to: 'fractions' }],
      }),

    listModules: () =>
      reply({ course: { id: 'foundations', name: 'Foundations' }, modules: ['Arithmetic'] }),

    enroll: (course) =>
      reply({ enrolled: course, mastery_floor: ['whole-numbers'], floor_size: 1 }),

    sessionStart: () => {
      open = 'demo-session';
      return reply({
        session: open,
        reopened: false,
        xp: { total: 340, today: 12, goal: 40, streak_days: 3 },
        frontier: 4,
        due_reviews: 2,
      });
    },

    sessionEnd: (minutes) => {
      const session = open ?? 'demo-session';
      open = null;
      return reply({
        session,
        xp_earned: 12,
        minutes: minutes ?? 8,
        xp: { total: 352, today: 24, goal: 40, streak_days: 3 },
        anki: { pending: 0 },
      });
    },

    getPlan: () =>
      reply({
        session: open ?? 'demo-session',
        tasks: [demoTask(answered)],
        quiz_due: false,
        constraints: {
          lesson_ratio_ok: true,
          lesson_ratio: 0.5,
          throttle_ok: true,
          reviews: 2,
          lessons: 1,
        },
        course_complete: false,
        blocked: [],
        frontier_blocked_until: null,
      }),

    // SERVE-idem: the SAME problem comes back until an answer commits it. `cursor` is read
    // here and written only by `taskAnswer`.
    taskServe: async (taskId) => {
      if (taskId !== DEMO_TASK_ID) refuse(404, 'unknown_task', 'The demo plans one task.');
      if (cursor >= DEMO_PROBLEMS.length) {
        refuse(409, 'task_complete', 'This task is finished.');
      }
      return reply(served());
    },

    taskTeach: async (taskId) => {
      if (taskId !== DEMO_TASK_ID) refuse(404, 'unknown_task', 'The demo plans one task.');
      return reply({
        kp: DEMO_PROBLEMS[cursor].kp,
        concept: 'A fraction names the same number when you divide both parts by the same factor.',
        worked_example: {
          problem: 'Simplify $\\frac{4}{6}$.',
          steps: 'Both parts divide by 2, so $\\frac{4}{6}=\\frac{2}{3}$.',
        },
      });
    },

    // A hint never contains the expected answer (Hard Rule 1).
    taskHint: async (taskId, problemId) => {
      if (taskId !== DEMO_TASK_ID) refuse(404, 'unknown_task', 'The demo plans one task.');
      if (problemId !== DEMO_PROBLEMS[cursor].problem_id) {
        refuse(404, 'unknown_problem', 'That problem is no longer live.');
      }
      hintCount = Math.min(hintCount + 1, DEMO_HINTS.length);
      return reply({ hint: DEMO_HINTS[hintCount - 1], hint_number: hintCount });
    },

    taskAnswer: async (taskId, body): Promise<TaskAnswerResponse> => {
      if (taskId !== DEMO_TASK_ID) refuse(404, 'unknown_task', 'The demo plans one task.');
      const problem = DEMO_PROBLEMS[cursor];
      if (body.problem_id !== problem.problem_id) {
        refuse(404, 'unknown_problem', 'That problem is no longer live.');
      }
      const correct = norm(body.answer) === norm(problem.expected);
      const attempt = `${taskId}-${answered + 1}`;
      // The cursor moves HERE, and only here.
      cursor += 1;
      answered += 1;
      hintCount = 0;
      const done = cursor >= DEMO_PROBLEMS.length;
      const reply_: TaskAnswerResponse = {
        attempt_id: attempt,
        outcome: correct ? 'correct' : 'incorrect',
        correct,
        work_quality: correct ? 'perfect' : 'nearly_passable',
        error_tags: [],
        secs: 30,
        task_status: done ? 'task_passed' : 'continue',
        remediation: [],
        next: done ? null : served(),
        // The demo runs no worker, so it offers no prose and writes no job row.
        diagnosis: { status: 'not_offered' },
        solution: problem.solution,
        ...(correct ? {} : { re_solve: DEMO_RE_SOLVE }),
        ...(correct ? { xp: 10 } : {}),
      };
      return reply(reply_);
    },

    diagStart: placement.diagStart,
    diagAnswer: placement.diagAnswer,
    diagFinish: placement.diagFinish,

    getDiagnosis: async (diagnosisId): Promise<DiagnosisJob> =>
      refuse(404, 'unknown_diagnosis', `The demo wrote no job ${diagnosisId}.`),
    // The demo serves no stream. An `EventSource` on this path fails at once, and the
    // panel falls back to the poll, which refuses — the same path a dropped SSE takes.
    diagnosisStreamUrl: () => '/api/diagnosis/stream',

    getOperatorFlags: async () =>
      refuse(403, 'forbidden', 'This route serves an admin account only.'),

    downloadExport: async () =>
      refuse(403, 'forbidden', 'The demo keeps no event log to export.'),

    // The review surface (C6). The demo account is NOT an admin, so every admin route
    // — the operator flags above and these six — answers the same `403 forbidden` the
    // service answers a signed-in learner. That is what makes `?demo=1` an honest
    // rehearsal of the non-admin path the two admin screens have to render (REVIEW-admin),
    // and it is why the demo defines no fixture queue: a demo that showed a review queue
    // would let a screen be built against a state no non-admin can reach.
    listContent: async () => refuse(403, 'forbidden', DEMO_ADMIN_ONLY),
    getContent: async () => refuse(403, 'forbidden', DEMO_ADMIN_ONLY),
    approveContent: async () => refuse(403, 'forbidden', DEMO_ADMIN_ONLY),
    rejectContent: async () => refuse(403, 'forbidden', DEMO_ADMIN_ONLY),
    // The recovery path of the third outcome is admin-only too (D-F2).
    listUngraded: async () => refuse(403, 'forbidden', DEMO_ADMIN_ONLY),
    regradeUngraded: async () => refuse(403, 'forbidden', DEMO_ADMIN_ONLY),
  };
}
