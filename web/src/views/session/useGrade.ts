/**
 * The grade machine of the study loop: the submit, the hint, and the drill auto-submit.
 *
 * Three of the five rules `Session.tsx` states live here, and each one is claimed by a
 * named test.
 *
 *   F-37-1c — ONE PHASE GATE. Every submit path — the Submit button, the Enter key, the
 *   drill auto-submit — starts with `gate.tryEnter('ready', 'submitting')`, set
 *   SYNCHRONOUSLY before the first await. Enter bypasses the disabled button by design
 *   (a keydown on the input does not read the button), so the gate, not the attribute, is
 *   what stops a second post of one `problem_id`. The loser of that race gets
 *   `404 unknown_problem` against an append-only log.
 *
 *   DD-3/P1 — THE RE-SOLVE. An assisted answer that grades correct is NOT recorded. The
 *   service stashes it and keeps the problem live, so the view returns to `ready` with the
 *   solution revealed and the SAME submit sends the unaided re-solve. `feedback` is not
 *   terminal. The re-solve is UNTIMED: the return to `ready` stops and clears the drill
 *   countdown, because a leftover second that runs out posts a blank re-solve and the
 *   service then rewrites the stashed assisted pass into a permanent miss.
 *
 *   W-A4 — THE REFERENCE LESSON ONCE. Every hint after the third repeats the pointer; it is
 *   rendered once, however many hints follow.
 *
 * The hook takes the refs and the setters the view owns, and returns the two handlers. It
 * holds no state of its own: every value a submit reads goes through a ref, because a
 * submit cannot wait for a render and a Retry arrives renders after the closure that armed
 * it (the React rule of `useCall`).
 */
import { useEffect, type RefObject } from 'react';
import { isQuizReceipt, isRework } from '@/api/types';
import { releaseOnFail } from '@/hooks/screen';
import type { AnswerFieldHandle } from '@/components/AnswerField';
import type { WorkFieldHandle } from '@/components/WorkField';
import type { Call } from '@/hooks/useCall';
import type { Lifetime } from '@/hooks/useLifetime';
import type { Gate } from '@/hooks/usePhase';
import type { AnswerResponse, ApiClient, PlanTask, ReworkResponse, ServedProblem } from '@/api/types';
import type { SessionPlan } from './useSessionPlan';

export type SessionPhase = 'loading' | 'ready' | 'submitting' | 'feedback' | 'closing' | 'done';

export interface GradeDeps {
  api: ApiClient;
  call: Call;
  gate: Gate<SessionPhase>;
  life: Lifetime;
  session: SessionPlan;
  /** The live problem. Every setter of the view writes it on the same line. */
  problemRef: RefObject<ServedProblem | null>;
  taskRef: RefObject<PlanTask | null>;
  answerRef: RefObject<AnswerFieldHandle | null>;
  workRef: RefObject<WorkFieldHandle | null>;
  /** The problem the SERVICE ALREADY ANSWERED, whatever the answer said. */
  answeredForRef: RefObject<string | null>;
  /** The problem whose drill timeout already fired. */
  timedOutForRef: RefObject<string | null>;
  setResult: (result: AnswerResponse | null) => void;
  setRework: (rework: ReworkResponse | null) => void;
  setElapsed: (secs: number) => void;
  setHints: (update: (prev: string[]) => string[]) => void;
  setReferenceLesson: (update: (prev: string | null) => string | null) => void;
  /** The three values the drill auto-submit watches. */
  countdown: boolean;
  elapsed: number;
  phase: SessionPhase;
}

export interface Grade {
  submit: (opts?: { timedOut?: boolean }) => void;
  requestHint: () => void;
}

export function useGrade({
  api, call, gate, life, session,
  problemRef, taskRef, answerRef, workRef, answeredForRef, timedOutForRef,
  setResult, setRework, setElapsed, setHints, setReferenceLesson,
  countdown, elapsed, phase,
}: GradeDeps): Grade {
  // Plain functions, rebuilt per render: `session` is a new object every render, so a memo
  // over them would hold nothing, and every consumer reads them at event time.
  const submit = (opts: { timedOut?: boolean } = {}): void => {
    // Every submit path starts from a problem on screen, so both refs name one, and the
    // two fields are mounted beside it.
    const current = problemRef.current!;
    const task = taskRef.current!;
    const field = answerRef.current!;

    const answer = field.value();
    // A timed-out drill submits whatever is there, blank included — an honest miss.
    // Otherwise an empty answer only refocuses.
    if (!answer && !opts.timedOut) { field.focus(); return; }

    // THE GATE (F-37-1c). Synchronous, before the first await, so two events inside the
    // grading window can never both post this `problem_id`.
    if (!gate.tryEnter('ready', 'submitting')) return;

    const work = workRef.current!.value();
    void call(
      () => api.taskAnswer(task.task_id, {
        problem_id: current.problem_id,
        answer,
        ...(work ? { work } : {}),
      }),
      (reply) => {
        // THE SERVICE ANSWERED THIS PROBLEM, whatever the reply says. The latch is set here,
        // before any branch: a branch that returns the view to `ready` leaves the leftover
        // seconds to run out, and the auto-submit then posts a BLANK second attempt for a
        // `problem_id` the service already holds an attempt for.
        answeredForRef.current = current.problem_id;
        if (isQuizReceipt(reply)) {
          // Unreachable by construction: a quiz task is handed to the quiz screen before it
          // is served, so this view posts no quiz answer. The branch exists because the
          // reply is a union, and a receipt reveals nothing to render.
          gate.enter('ready');
          return;
        }
        if (isRework(reply)) {
          // DD-3/P1: back to `ready`, deliberately. The next submit of this same problem is
          // the unaided re-solve, and only that locks the assisted pass in. The service
          // needs no flag from here — it counts the hints it served.
          //
          // THE COUNTDOWN STOPS AND CLEARS HERE. The timed attempt is made and the service
          // stashed it; the re-solve is untimed, and `countdown` reads false while `rework`
          // stands. Leaving the leftover seconds to run out fires a blank re-solve of this
          // same problem, and the service then rewrites the stashed assisted pass into a
          // permanent miss in an append-only log.
          setRework(reply);
          setElapsed(0);
          field.clear();
          gate.enter('ready');
          return;
        }
        setResult(reply);
        setRework(null);
        // Hard Rule 3: the core scheduled it, so ask the core for a new plan when this task
        // ends. The view re-orders nothing itself.
        if (reply.remediation.length) session.requestReplan();
        gate.enter('feedback');
      },
      {
        // THE RETRY RE-ENTERS THE GATE (F-37-1c). `useCall` holds no view state, so a Retry
        // pressed after a second submit graded this problem would post a `problem_id` the
        // service already spent: `404 unknown_problem` against an append-only log, and that
        // failure arms yet another Retry. The gate refuses it instead, and the refusal
        // toast expires.
        //
        // `life.alive()` comes FIRST (F-37-1b). The toast store is module-scope, so the
        // Retry outlives this view: a learner who left the session can still press it, and
        // a post from a dead screen is a write nobody is on. The refs of an unmounted view
        // still name the last problem, so no other term in this gate refuses that press
        // (M6-review-2, the C residual). The problem on screen cannot change before the
        // service answers it, so the answered latch alone tells a stale Retry apart.
        retryGate: () => life.alive()
          && answeredForRef.current !== current.problem_id
          && gate.tryEnter('ready', 'submitting'),
        onFail: releaseOnFail(gate, 'submitting', 'ready'),
      },
    );
  };

  // The drill auto-submit goes through the SAME gate, so it can only fire while the problem
  // is genuinely `ready` — never on top of an in-flight grade or a feedback panel. It runs
  // after every render; the latch makes a second pass a no-op.
  useEffect(() => {
    if (!countdown || elapsed !== 0 || phase !== 'ready') return;
    // A countdown runs for a problem on screen, so the ref names one.
    const id = problemRef.current!.problem_id;
    // Two latches, one rule each: the timeout of this problem already fired, or the service
    // already answered this problem and handed it back with the clock still running.
    if (timedOutForRef.current === id || answeredForRef.current === id) return;
    timedOutForRef.current = id;
    submit({ timedOut: true });
  });

  const requestHint = (): void => {
    // The same gate: no hint is fired at a problem already being graded. The `H` key reaches
    // here while the buttons are disabled.
    if (!gate.is('ready')) return;
    const current = problemRef.current!;
    const task = taskRef.current!;

    void call(() => api.taskHint(task.task_id, current.problem_id), (h) => {
      // The reply carries `hint` and nothing else that names the answer. Hard Rule 1 is
      // structural here: there is no `expected` on this route to leak.
      setHints((prev) => [...prev, h.hint]);
      // W-A4: shown ONCE, however many hints repeat the pointer.
      setReferenceLesson((prev) => prev ?? h.reference_lesson?.name ?? null);
    });
  };

  return { submit, requestHint };
}
