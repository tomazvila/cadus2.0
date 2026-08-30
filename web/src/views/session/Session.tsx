/**
 * The guided study loop: plan, serve, teach, hint, answer, feedback, re-solve, advance.
 *
 * THE HIGHEST-STAKES SCREEN IN THE APP. Every answer it posts appends a row to the
 * `events` table, and `cadus_app` holds no UPDATE and no DELETE on it. A defect here writes
 * permanent corruption into the learner's real progress, so five rules are load-bearing and
 * each one is claimed by a named test.
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
 *   NO-2BILL — ONE WRITE PER MOUNT. A lesson mount posts `/teach` and nothing else; the
 *   serve waits for "I've got it". 1.0 fired a warm-up serve behind the worked example, and
 *   that is two writes on one mount plus a `started_at` stamped before the learner read a
 *   word.
 *
 *   W-A4 — THE REFERENCE LESSON ONCE. Every hint after the third repeats the pointer; it is
 *   rendered once, however many hints follow.
 *
 *   W-C5 — SUBMIT IS DOMINANT. One `.btn-primary` on the card. Hint is quiet, and Exit is
 *   quieter still. There is no "Give up" control, and its absence is a service fact, not a
 *   design choice: 1.0 abandoned a task through `POST /api/task/{id}/abort`, and the M5
 *   route list (`crates/web/src/lib.rs` `create_app`) has no such route. A button that
 *   posts nothing is worse than no button, so the tag's wording names Exit in its place.
 *
 * THE CLOCK IS DISPLAY ONLY (trap T4). The service measures session time from its own
 * accumulator and prices XP with it, so `sessionEnd()` is called with NO arguments and this
 * view keeps no cumulative counter.
 *
 * THE DIAGNOSIS IS A PASSENGER (S9). The verdict, the solution and the re-solve instruction
 * come from local CPU and paint at once; the `diagnosis` field of the same reply feeds a
 * panel that fills in later, from the one per-session subscription this view opens. Nothing
 * in the loop waits on it, and no exit is blocked by it — see `useDiagnosis.ts`.
 *
 * WHAT THIS UNIT DOES NOT OWN. There is no router yet, so navigation arrives as props.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { MathBlock } from '@/components/MathBlock';
import { AnswerField, type AnswerFieldHandle } from '@/components/AnswerField';
import { WorkField, type WorkFieldHandle } from '@/components/WorkField';
import { Chip, LoadingBlock, Stat } from '@/components/primitives';
import { useCall } from '@/hooks/useCall';
import { useLifetime } from '@/hooks/useLifetime';
import { usePhase } from '@/hooks/usePhase';
import { fmtClock, num, signed } from '@/lib/format';
import { isQuizReceipt, isRework } from '@/api/types';
import type {
  AnswerResponse,
  ApiClient,
  PlanTask,
  ReworkResponse,
  ServedProblem,
  SessionEndResponse,
  SessionPlanResponse,
  TeachResponse,
} from '@/api/types';
import { useSessionPlan } from './useSessionPlan';
import { Teach } from './Teach';
import { Feedback, Rework } from './Feedback';
import { Diagnosis } from './Diagnosis';
import { useDiagnosisStream } from './useDiagnosis';

type Phase = 'loading' | 'ready' | 'submitting' | 'feedback' | 'done';

/** The auto-advance window, in milliseconds. The 1.0 literal. */
export const AUTO_ADVANCE_MS = 1400;

export interface SessionProps {
  api: ApiClient;
  /** Demo mode. A 401 then keeps the learner on the screen. */
  demo?: boolean;
  /** A plan the caller already fetched. Absent, the view reads `GET /api/session/plan`. */
  plan?: SessionPlanResponse;
  /** The session-expired path of `useCall`. */
  onUnauthorized: () => void;
  /** Leave for the dashboard. The session stays open and unfinished tasks are re-served. */
  onExit: () => void;
  /** A quiz is not this loop: the quiz screen owns the whole-quiz clock (QUIZ-budget). */
  onQuiz: (task: PlanTask) => void;
  /** The placement, offered when the plan is empty. */
  onDiagnostic: () => void;
}

/** A drill counts down only when all three hold. Nothing else has a countdown (trap T5). */
export function isDrill(task: PlanTask | null, problem: ServedProblem | null): boolean {
  return !!task && task.task_type === 'drill'
    && !!problem?.countdown && num(problem.time_budget_secs) > 0;
}

export function Session({
  api,
  demo = false,
  plan: initialPlan,
  onUnauthorized,
  onExit,
  onQuiz,
  onDiagnostic,
}: SessionProps) {
  const life = useLifetime();
  const call = useCall({ demo, onUnauthorized });
  const [phase, gate] = usePhase<Phase>('loading');
  const session = useSessionPlan();
  // ONE connection for the whole session, never one per problem (spec section 4.1). It opens
  // here and closes when this view unmounts. The demo runs no worker and answers
  // `not_offered` to every grade, so it opens nothing.
  const diagnosis = useDiagnosisStream({ api, life, enabled: !demo });

  const [problem, setProblem] = useState<ServedProblem | null>(null);
  const [teaching, setTeaching] = useState<TeachResponse | null>(null);
  const [hints, setHints] = useState<string[]>([]);
  const [referenceLesson, setReferenceLesson] = useState<string | null>(null);
  const [result, setResult] = useState<AnswerResponse | null>(null);
  const [rework, setRework] = useState<ReworkResponse | null>(null);
  const [summary, setSummary] = useState<SessionEndResponse | null>(null);
  // "Wrapping up" and "nothing was due" both sit at `loading` with no problem, and offering
  // a placement in the middle of a close lets the learner abandon the close.
  const [wrappingUp, setWrappingUp] = useState(false);
  const [elapsed, setElapsed] = useState(0);

  const answerRef = useRef<AnswerFieldHandle>(null);
  const workRef = useRef<WorkFieldHandle>(null);
  const continueRef = useRef<HTMLButtonElement>(null);
  const homeRef = useRef<HTMLButtonElement>(null);
  // Both are read SYNCHRONOUSLY by a submit, which cannot wait for a render, so every
  // setter writes the ref on the same line.
  const problemRef = useRef<ServedProblem | null>(null);
  const taskRef = useRef<PlanTask | null>(null);
  const startedOnce = useRef(false);
  // Latches the drill timeout PER PROBLEM. A failed grade returns the phase to `ready` with
  // the clock still at zero, and an unlatched effect re-fires on every one of them. The
  // learner still has the field and the Submit button, so a timeout that failed to post is
  // not a dead end.
  const timedOutFor = useRef<string | null>(null);
  // Latches the problem the SERVICE ALREADY ANSWERED, whatever the answer said. Two rules
  // read it, and both are about a problem the view hands back to the learner (the DD-3/P1
  // re-solve): the countdown of that problem is over, so no blank auto-submit follows, and
  // the request that earned the reply is spent, so no stale Retry re-posts it.
  const answeredFor = useRef<string | null>(null);
  // The knowledge point whose worked example is on screen. A new one inside a lesson has to
  // be taught before it is practised.
  const taughtKp = useRef<string | null>(null);

  useEffect(() => { taskRef.current = session.task; }, [session.task]);

  const setLive = useCallback((p: ServedProblem | null, startAt: number) => {
    problemRef.current = p;
    setProblem(p);
    // Both latches belong to the problem that is live, so a fresh serve starts unlatched.
    timedOutFor.current = null;
    answeredFor.current = null;
    // The clock's starting value travels WITH the problem, so the ticking effect never
    // writes state synchronously to reset it.
    setElapsed(startAt);
  }, []);

  /** Everything a fresh problem clears. The re-solve keeps its panel and clears none of it. */
  const clearForProblem = useCallback(() => {
    setHints([]);
    setReferenceLesson(null);
    setResult(null);
    setRework(null);
    answerRef.current?.clear();
  }, []);

  // ---- serving -------------------------------------------------------------

  const serveThenShow = useCallback(() => {
    gate.enter('loading');
    const task = taskRef.current;
    if (!task) return;
    void call(() => api.taskServe(task.task_id), (served) => {
      if (!life.alive()) return;
      // Drop the worked example, or the teach branch keeps winning the render and the
      // lesson shows no answer field and no way on.
      setTeaching(null);
      setLive(served, isDrill(taskRef.current, served) ? num(served.time_budget_secs) : 0);
      clearForProblem();
      gate.enter('ready');
    });
  }, [api, call, clearForProblem, gate, life, setLive]);

  const startTask = useCallback(() => {
    const task = taskRef.current;
    if (!task) return;
    gate.enter('loading');
    setTeaching(null);
    taughtKp.current = null;

    // The quiz has its own screen, its own clock and its own reveal rules. Hand it over
    // BEFORE anything is served, so this view never posts a quiz answer.
    if (task.task_type === 'quiz') { onQuiz(task); return; }

    if (task.task_type === 'lesson') {
      // Teach FIRST, and teach ALONE (NO-2BILL). Every topic has knowledge points, so the
      // view needs no served problem to know a fresh lesson must teach.
      void call(() => api.taskTeach(task.task_id), (instruction) => {
        if (!life.alive()) return;
        taughtKp.current = instruction.kp;
        setTeaching(instruction);
        gate.enter('ready');
      }).then((instruction) => {
        // Teach failed and was toasted with a Retry. Practice is still servable, so fall
        // through rather than strand the task on a spinner.
        if (!instruction && life.alive()) serveThenShow();
      });
      return;
    }

    serveThenShow();
  }, [api, call, gate, life, onQuiz, serveThenShow]);

  // ---- the plan ------------------------------------------------------------

  const endSession = useCallback(() => {
    gate.enter('loading');
    setWrappingUp(true);
    setLive(null, 0);
    // NO ARGUMENTS. The on-screen clock is display only; the service measures the session
    // from its own accumulator and that value prices the XP (trap T4).
    void call(() => api.sessionEnd(), (s) => {
      if (life.alive()) { setSummary(s); gate.enter('done'); }
    }).then((s) => {
      // A failed close still ends the screen: the summary is a receipt, not the record.
      if (!s && life.alive()) { setSummary(null); gate.enter('done'); }
    });
  }, [api, call, gate, life, setLive]);

  const advanceTask = useCallback(() => {
    if (!session.needsReplan()) {
      // Decided HERE, not watched for in an effect: an effect that ends the session would
      // have to write state synchronously, and the decision belongs where the move happens.
      if (!session.next()) endSession();
      return;
    }
    // Fetch FIRST, then move. Advancing the cursor here starts the next planned task — a
    // problem the learner briefly sees — only for the re-plan to reset to index 0, because
    // remediation is served first.
    gate.enter('loading');
    session.markDone();
    void call(() => api.getPlan()).then((fresh) => {
      if (!life.alive()) return;
      // A FAILED FETCH IS NOT AN EMPTY PLAN. `call` resolves undefined on a failure, and
      // closing the session on a transient 500 would append `session_end` to the
      // append-only log and throw away every task still owed. Fall through instead.
      if (!fresh?.tasks) {
        if (!session.next()) endSession();
        return;
      }
      if (session.replan(fresh)) return;
      // A re-plan that really is empty ends the session, rather than leaving the view on a
      // spinner with the session never closed.
      endSession();
    });
  }, [api, call, endSession, gate, life, session]);

  // The first task. Guarded against the StrictMode double invoke: `/teach` and `/serve` are
  // both writes, and an unguarded start issues them twice (NO-2BILL).
  useEffect(() => {
    if (startedOnce.current) return;
    startedOnce.current = true;
    if (initialPlan) { session.start(initialPlan); return; }
    void call(() => api.getPlan(), (p) => { if (life.alive()) session.start(p); });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per mount, by design.
  }, []);

  // Whenever the current task changes, start it.
  const currentTaskId = session.task?.task_id ?? null;
  useEffect(() => {
    if (currentTaskId) startTask();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the task id is the trigger.
  }, [currentTaskId]);

  // ---- the display clock ---------------------------------------------------

  // A re-solve is UNTIMED (DD-3/P1). The timed attempt is already made and stashed, so the
  // budget of this problem is spent: the clock counts the re-solve up from zero, and the
  // auto-submit effect below — which fires only on a countdown — cannot fire at all.
  const countdown = isDrill(session.task, problem) && !rework;
  const ticking = phase === 'ready' && !!problem;
  const liveProblemId = problem?.problem_id ?? null;

  useEffect(() => {
    if (!ticking) return undefined;
    // Ticking only. The starting value arrived with the problem, so nothing here writes
    // state synchronously. Registered in the lifetime, so it dies with the view.
    const id = life.setInterval(() => {
      setElapsed((v) => (countdown ? Math.max(0, v - 1) : v + 1));
    }, 1000);
    return () => life.clearTimer(id);
  }, [ticking, liveProblemId, countdown, life]);

  // ---- submitting ----------------------------------------------------------

  const submit = useCallback((opts: { timedOut?: boolean } = {}) => {
    const current = problemRef.current;
    const task = taskRef.current;
    if (!current || !task) return;

    const answer = answerRef.current?.value() ?? '';
    // A timed-out drill submits whatever is there, blank included — an honest miss.
    // Otherwise an empty answer only refocuses.
    if (!answer && !opts.timedOut) { answerRef.current?.focus(); return; }

    // THE GATE (F-37-1c). Synchronous, before the first await, so two events inside the
    // grading window can never both post this `problem_id`.
    if (!gate.tryEnter('ready', 'submitting')) return;

    const work = workRef.current?.value() ?? '';
    void call(
      () => api.taskAnswer(task.task_id, {
        problem_id: current.problem_id,
        answer,
        ...(work ? { work } : {}),
      }),
      (reply) => {
        if (!life.alive()) return;
        // THE SERVICE ANSWERED THIS PROBLEM, whatever the reply says. The latch is set here,
        // before any branch: a branch that returns the view to `ready` leaves the leftover
        // seconds to run out, and the auto-submit then posts a BLANK second attempt for a
        // `problem_id` the service already holds an attempt for.
        answeredFor.current = current.problem_id;
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
          answerRef.current?.clear();
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
        retryGate: () => problemRef.current?.problem_id === current.problem_id
          && answeredFor.current !== current.problem_id
          && gate.tryEnter('ready', 'submitting'),
        // A failed grade returns the problem to the learner — the first attempt and every
        // retried one alike. Without this the view sits at `submitting` with every control
        // disabled and no way back.
        onFail: () => { if (life.alive() && gate.is('submitting')) gate.enter('ready'); },
      },
    );
  }, [api, call, gate, life, session]);

  // The drill auto-submit goes through the SAME gate, so it can only fire while the problem
  // is genuinely `ready` — never on top of an in-flight grade or a feedback panel.
  useEffect(() => {
    if (!countdown || elapsed !== 0 || phase !== 'ready') return;
    const id = problemRef.current?.problem_id;
    // Two latches, one rule each: the timeout of this problem already fired, or the service
    // already answered this problem and handed it back for the re-solve.
    if (!id || timedOutFor.current === id || answeredFor.current === id) return;
    timedOutFor.current = id;
    submit({ timedOut: true });
  }, [countdown, elapsed, phase, submit]);

  const requestHint = useCallback(() => {
    // The same gate: no hint is fired at a problem already being graded. The `H` key reaches
    // here while the buttons are disabled.
    if (!gate.is('ready')) return;
    const current = problemRef.current;
    const task = taskRef.current;
    if (!current || !task) return;

    void call(() => api.taskHint(task.task_id, current.problem_id), (h) => {
      if (!life.alive()) return;
      // The reply carries `hint` and nothing else that names the answer. Hard Rule 1 is
      // structural here: there is no `expected` on this route to leak.
      setHints((prev) => [...prev, h.hint]);
      // W-A4: shown ONCE, however many hints repeat the pointer.
      const name = h.reference_lesson?.name;
      if (name) setReferenceLesson((prev) => prev ?? name);
    });
  }, [api, call, gate, life]);

  /** The ONE way out of `feedback`. A second click, or an auto-advance racing it, stops. */
  const advance = useCallback((next?: ServedProblem | null, nextUnavailable = false) => {
    if (!gate.tryEnter('feedback', 'loading')) return;
    setResult(null);

    // The attempt IS recorded and the task is NOT finished: the service could not draw the
    // next problem. Re-serve the SAME task. Falling through to `advanceTask` would skip the
    // problems still owed, which is the silent loss the flag exists to prevent.
    //
    // `serveThenShow`, never `startTask`: on a lesson, `startTask` re-teaches, re-renders
    // the example the learner already finished, and inverts the teach-then-practise order.
    if (!next && nextUnavailable) { serveThenShow(); return; }

    if (next) {
      // A new knowledge point inside a lesson is taught first — and then RE-SERVED, never
      // shown from this payload: that problem's clock started when the service drew it.
      if (taskRef.current?.task_type === 'lesson' && next.kp && next.kp !== taughtKp.current) {
        startTask();
        return;
      }
      setLive(next, isDrill(taskRef.current, next) ? num(next.time_budget_secs) : 0);
      clearForProblem();
      gate.enter('ready');
      return;
    }
    advanceTask();
  }, [advanceTask, clearForProblem, gate, serveThenShow, setLive, startTask]);

  // Auto-advance: 1400 ms, correct answers only, and only when a next problem is already in
  // hand. `next_unavailable` needs a deliberate click, because it re-serves. The timer is
  // registered in the lifetime, so leaving the view inside the window cancels it, and the
  // cleanup cancels it when a click advances first.
  useEffect(() => {
    if (phase !== 'feedback' || !result?.correct || !result.next) return undefined;
    const id = life.setTimeout(() => advance(result.next), AUTO_ADVANCE_MS);
    return () => life.clearTimer(id);
  }, [phase, result, life, advance]);

  // Focus moves on every transition (spec section 4.5).
  useEffect(() => { if (phase === 'ready') answerRef.current?.focus(); }, [phase, problem]);
  useEffect(() => { if (phase === 'feedback') continueRef.current?.focus(); }, [phase, result]);
  useEffect(() => { if (phase === 'done') homeRef.current?.focus(); }, [phase]);

  // ---- render --------------------------------------------------------------

  if (phase === 'done') {
    return (
      <section className="view-session">
        <div className="card summary-card">
          <h2>Session complete</h2>
          {summary ? (
            <div className="stat-grid">
              <Stat value={signed(summary.xp_earned)} label="XP earned" className="accent" />
              <Stat value={`${num(summary.minutes)}`} label="minutes" />
              <Stat value={`${num(summary.xp.streak_days)}`} label="day streak" />
            </div>
          ) : (
            <p className="muted">Your work is saved. The summary did not load.</p>
          )}
          <button ref={homeRef} type="button" className="btn btn-primary" onClick={onExit}>
            Back to dashboard
          </button>
        </div>
      </section>
    );
  }

  if (wrappingUp) {
    return <section className="view-session"><LoadingBlock label="Wrapping up…" /></section>;
  }

  // No dead end: an empty plan offers the placement, and the wording says which empty it is.
  if (session.plan && !session.task && phase === 'loading' && !problem) {
    const p = session.plan;
    const message = p.course_complete
      ? 'Course complete. Enroll in your next course to keep going.'
      : p.frontier_blocked_until
        ? `New lessons are on a retry delay until ${p.frontier_blocked_until}.`
        : session.allDone
          ? 'Everything planned for this session is done. Nice work.'
          : 'Nothing is due right now — enjoy the break.';
    return (
      <section className="view-session">
        <div className="empty">
          <p>{message}</p>
          <button type="button" className="btn" onClick={onDiagnostic}>
            Take the placement diagnostic
          </button>
          <button type="button" className="btn btn-primary" onClick={onExit}>
            Back to dashboard
          </button>
        </div>
      </section>
    );
  }

  if (teaching && session.task) {
    return (
      <section className="view-session">
        <Teach task={session.task} instruction={teaching} onContinue={serveThenShow} />
      </section>
    );
  }

  if (!problem || !session.task) {
    return <section className="view-session"><LoadingBlock label="Preparing your session…" /></section>;
  }

  const task = session.task;
  const topic = task.topic;
  const locked = phase !== 'ready';

  return (
    // KEYED PER PROBLEM. Without the key React reuses the input and the work field, and the
    // previous problem's working posts with the next problem's answer — corrupt data in an
    // append-only log.
    <section className="view-session" key={problem.problem_id}>
      <div className="task-header">
        <div className="task-meta">
          <Chip className={`chip-${task.task_type}`}>{task.task_type}</Chip>
          <span className="topic-name">{topic?.name || topic?.id || 'Practice'}</span>
          {topic?.module ? <span className="topic-module">{topic.module}</span> : null}
        </div>
        <div className="task-right">
          <span className="progress-count">
            {problem.total != null
              ? `${num(problem.index)} / ${num(problem.total)}`
              : `${num(problem.index)}`}
          </span>
          <span className={`timer${countdown && elapsed <= 3 ? ' urgent' : ''}`}>
            {fmtClock(elapsed)}
          </span>
          <button
            type="button"
            className="btn btn-ghost btn-exit"
            title="Leave the session — your work is saved and unfinished tasks come back next time"
            onClick={onExit}
          >
            Exit
          </button>
        </div>
        {/* VERBATIM. The selector re-parses substrings of this prose to decide whether a
            task may be re-served, so a reword changes which tasks survive. */}
        {task.why ? <div className="why-chip">{task.why}</div> : null}
      </div>

      <div className="card problem-card">
        <MathBlock>{problem.text}</MathBlock>

        <div className="hint-list">
          {hints.map((h, i) => (
            <div key={`${i}-${h.slice(0, 24)}`} className="hint">
              <strong>{`Hint ${i + 1}: `}</strong>
              <MathBlock className="hint-text">{String(h)}</MathBlock>
            </div>
          ))}
          {referenceLesson ? (
            <div className="reference-lesson">
              {`Still stuck? This is a review — re-study the lesson “${referenceLesson}”, then answer as best you can.`}
            </div>
          ) : null}
        </div>

        {/* Disabled only where the problem is over. A grade in flight leaves the field
            live: the phase gate, not the attribute, is what stops the second post. */}
        <AnswerField
          ref={answerRef}
          disabled={phase !== 'ready' && phase !== 'submitting'}
          onSubmit={() => submit()}
          onHint={requestHint}
        />
        <WorkField ref={workRef} onSubmit={() => submit()} />

        {/* W-C5: one primary here, and the quiet controls beside it. */}
        {phase === 'feedback' ? null : (
          <div className="actions">
            <button
              type="button"
              className={`btn btn-primary${phase === 'submitting' ? ' is-busy' : ''}`}
              disabled={locked}
              onClick={() => submit()}
            >
              Submit
            </button>
            <button type="button" className="btn btn-ghost" disabled={locked} onClick={requestHint}>
              Hint
            </button>
          </div>
        )}

        {rework ? <Rework res={rework} /> : null}

        {result ? (
          <Feedback
            res={result}
            hasNext={!!result.next || !!result.next_unavailable}
            onContinue={() => advance(result.next, result.next_unavailable)}
            onEnd={endSession}
            continueRef={continueRef}
          >
            {/* Keyed by the attempt, so a second grade of the same problem — the DD-3/P1
                re-solve — never shows the first attempt's explanation. */}
            <Diagnosis key={result.attempt_id} store={diagnosis} field={result.diagnosis} />
          </Feedback>
        ) : null}
      </div>
    </section>
  );
}
