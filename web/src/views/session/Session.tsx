/**
 * Guided study loop. The synchronous phase gate owns every advance and submit.
 * Lessons teach before serving; assisted answers retain the untimed re-solve.
 * The server owns elapsed time and append-only progress. Integrated multi-step
 * tasks resolve before per-component serving, and complete through the same plan.
 */
import { useEffect, useRef, useState } from 'react';
import { MathBlock } from '@/components/MathBlock';
import { MathVisuals } from '@/components/MathVisual';
import type { AnswerFieldHandle } from '@/components/AnswerField';
import { AnswerInput, TypedSubmit } from '@/components/AnswerInput';
import { WorkField, type WorkFieldHandle } from '@/components/WorkField';
import { LoadingBlock } from '@/components/primitives';
import { closeWith } from '@/hooks/screen';
import { useCall } from '@/hooks/useCall';
import { useLifetime } from '@/hooks/useLifetime';
import { usePhase } from '@/hooks/usePhase';
import type {
  AnswerResponse,
  ApiClient,
  IntegratedProblem,
  PlanTask,
  ProofGradingField,
  ReworkResponse,
  ServedProblem,
  SessionEndResponse,
  SessionPlanResponse,
  TeachResponse,
} from '@/api/types';
import { useSessionPlan } from './useSessionPlan';
import { useGrade, type SessionPhase } from './useGrade';
import { clockStart, isDrill, useSessionClock } from './useSessionClock';
import {
  EmptyPlan, NoInstruction, ProblemHeader, SessionSummary, emptyPlanMessage, reviewTopic,
} from './SessionScreens';
import { Teach } from './Teach';
import { Integrated } from './Integrated';
import { serveIntegrated } from './serveIntegrated';
import { loadPlannedTask } from './loadPlannedTask';
import { ProofAwareFeedback, Rework } from './Feedback';
import { RestoredProofGrading } from './ProofGrading';
import { HintButton, HintPanel } from './Hints';
import { Diagnosis } from './Diagnosis';
import { useDiagnosisStream } from './useDiagnosis';
import { useProblemReport } from './useProblemReport';
import { ProblemReport, QuestionReport } from './ProblemReport';
import { applyReportCorrection } from './applyReportCorrection';

/** No hint yet. One array for every problem that starts, so nothing rebuilds. */
const NO_HINTS: string[] = [];

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

/** The drill rule, re-exported for the tests that pin it beside the loop. */
export { isDrill };

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
  const [phase, gate] = usePhase<SessionPhase>('loading');
  const session = useSessionPlan();
  // ONE connection for the whole session, never one per problem (spec section 4.1). It opens
  // here and closes when this view unmounts. The demo runs no worker and answers
  // `not_offered` to every grade, so it opens nothing.
  const diagnosis = useDiagnosisStream({ api, life, enabled: !demo });

  const [integrated, setIntegrated] = useState<IntegratedProblem | null>(null);
  const [problem, setProblem] = useState<ServedProblem | null>(null);
  const [teaching, setTeaching] = useState<TeachResponse | null>(null);
  const [hints, setHints] = useState<string[]>(NO_HINTS);
  const [referenceLesson, setReferenceLesson] = useState<string | null>(null);
  const [result, setResult] = useState<AnswerResponse | null>(null);
  const report = useProblemReport(api, undefined, (receipt, context) => {
    setResult((previous) => applyReportCorrection(previous, receipt, context));
  });
  const [rework, setRework] = useState<ReworkResponse | null>(null);
  const [summary, setSummary] = useState<SessionEndResponse | null>(null);
  // The proof grading the plan restores after a reload. It stands above the first problem
  // until the learner moves on from it.
  const [restoredProof, setRestoredProof] = useState<ProofGradingField>(null);

  // The display clock: display only, and the re-solve is untimed (trap T4, DD-3/P1).
  const { elapsed, setElapsed, countdown } = useSessionClock(
    life, session.task, problem, rework, phase,
  );

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

  // The task a mixed review block handed on to. The cursor moves to it with its problem
  // already in hand, so the start effect must not serve it a second time.
  const adopting = useRef<string | null>(null);

  useEffect(() => { taskRef.current = session.task; }, [session.task]);

  // The moves below are plain functions, rebuilt per render. Every consumer reads them at
  // event time, and the one effect that fires one later reads it through a ref.

  const setLive = (p: ServedProblem | null, startAt: number): void => {
    problemRef.current = p;
    setProblem(p);
    // Both latches belong to the problem that is live, so a fresh serve starts unlatched.
    timedOutFor.current = null;
    answeredFor.current = null;
    // A fresh serve also drops the previous problem's report context. Without this the
    // shared "Report submitted question" panel lingers into the new problem's `ready`
    // phase and renders a second report button beside the per-problem one. The re-solve
    // never calls `setLive`, so its report panel survives, as it should.
    report.clear();
    // The clock's starting value travels WITH the problem, so the ticking effect never
    // writes state synchronously to reset it.
    setElapsed(startAt);
  };

  /**
   * Everything a fresh problem clears. The re-solve keeps its panel and clears none of it.
   * The verdict and the re-solve panel are gone already: every path here runs after a
   * grade that replaced them, or after `advance` dropped the verdict.
   */
  const clearForProblem = (): void => {
    setHints(NO_HINTS);
    setReferenceLesson(null);
    answerRef.current?.clear();
  };

  // ---- serving -------------------------------------------------------------

  /**
   * Follow a problem of a mixed review block onto its own task. The block serves the
   * questions of its reviews interleaved, so a serve or a grade may hand back another
   * review's problem; it is answered on that task. False when the cached plan lacks the
   * task: the caller re-plans and the serve finds the problem again.
   */
  const follow = (p: ServedProblem): boolean => {
    if (!p.task_id || p.task_id === taskRef.current?.task_id) return true;
    const task = session.adopt(p.task_id);
    if (!task) return false;
    taskRef.current = task;
    adopting.current = task.task_id;
    return true;
  };

  /** The cached plan lacks a task the block handed on to: fetch the plan and start over. */
  const replanForBlock = (): void => {
    setLive(null, 0);
    void call(() => api.getPlan(), (fresh) => {
      if (!life.alive()) return;
      if (!session.replan(fresh)) endSession();
    });
  };

  /** Load only for the active task; recover stale plans before showing a question. */
  const loadTask = <T,>(
    request: () => Promise<T>,
    onLoaded: (value: T) => void,
    onFail?: () => void,
  ): void => {
    const taskId = taskRef.current!.task_id;
    const current = () => life.alive() && taskRef.current?.task_id === taskId;
    void call(() => loadPlannedTask(api, taskId, request), (loaded) => {
      if (!current() || !gate.is('loading')) return;
      if (loaded.kind === 'replanned') {
        setLive(null, 0);
        setTeaching(null);
        setIntegrated(null);
        setRework(null);
        session.replan(loaded.plan);
        return;
      }
      onLoaded(loaded.value);
    }, {
      retryGate: () => current()
        && (gate.is('loading') || gate.tryEnter('no-instruction', 'loading')),
      onFail: () => { if (current() && gate.is('loading')) onFail?.(); },
    });
  };

  /** Serve the task on screen. Every caller has the phase at `loading` already. */
  const serveThenShow = (): void => {
    // A serve is asked for by a task on screen, so the ref names one.
    const task = taskRef.current!;
    loadTask(() => api.taskServe(task.task_id), (served) => {
      if (!follow(served)) { replanForBlock(); return; }
      // Drop the worked example, or the teach branch keeps winning the render and the
      // lesson shows no answer field and no way on.
      setTeaching(null);
      setLive(served, clockStart(taskRef.current, served));
      clearForProblem();
      gate.enter('ready');
    });
  };

  const serveWholeItem = (): void => {
    const task = taskRef.current!;
    setLive(null, 0);
    loadTask(() => serveIntegrated(api, task.task_id), (item) => {
      if (!life.alive() || taskRef.current?.task_id !== task.task_id) return;
      if (!item) { serveThenShow(); return; }
      setTeaching(null);
      setIntegrated(item);
      gate.enter('ready');
    });
  };

  /** Start the task on screen. The phase is `loading` on every path that leads here. */
  const startTask = (): void => {
    // The ref is written before the effect that starts a task runs, so it names one.
    const task = taskRef.current!;
    taughtKp.current = null;

    // The quiz has its own screen, its own clock and its own reveal rules. Hand it over
    // BEFORE anything is served, so this view never posts a quiz answer.
    if (task.task_type === 'quiz') { onQuiz(task); return; }

    if (task.task_type === 'multi-step' && !task.integrated_instruction_required) {
      serveWholeItem();
      return;
    }

    if (task.task_type === 'lesson' || task.integrated_instruction_required) {
      // Teach FIRST, and teach ALONE (NO-2BILL). Every topic has knowledge points, so the
      // view needs no served problem to know a fresh lesson must teach.
      loadTask(() => api.taskTeach(task.task_id), (instruction) => {
        taughtKp.current = instruction.kp;
        setTeaching(instruction);
        gate.enter('teaching');
      }, () => {
        // AUDIT FINDING (j). A failed teach NEVER falls through to practice. The service
        // has no worked example for this knowledge point, so practising it hands the
        // learner a skill nobody taught. The card says so and offers the next task.
        gate.enter('no-instruction');
      });
      return;
    }

    serveThenShow();
  };

  /** The learner read the worked example. One press serves; a second in the same tick stops. */
  const practise = (): void => {
    if (!gate.tryEnter('teaching', 'loading')) return;
    if (taskRef.current?.task_type === 'multi-step') serveWholeItem();
    else serveThenShow();
  };

  /** Leave a lesson the service cannot teach. One press advances; a second stops. */
  const skipTask = (): void => {
    if (!gate.tryEnter('no-instruction', 'loading')) return;
    setTeaching(null);
    advanceTask();
  };

  // ---- the plan ------------------------------------------------------------

  const endSession = (): void => {
    // `closing` renders the wrap-up screen, and no submit path starts from it.
    gate.enter('closing');
    // NO ARGUMENTS. The on-screen clock is display only; the service measures the session
    // from its own accumulator and that value prices the XP (trap T4).
    closeWith(call, gate, 'done', () => api.sessionEnd(), setSummary);
  };

  /** The task is over. `advance` put the phase at `loading` before it came here. */
  const advanceTask = (): void => {
    setLive(null, 0);
    if (!session.needsReplan()) {
      // Decided HERE, not watched for in an effect: an effect that ends the session would
      // have to write state synchronously, and the decision belongs where the move happens.
      if (!session.next()) endSession();
      return;
    }
    // Fetch FIRST, then move. Advancing the cursor here starts the next planned task — a
    // problem the learner briefly sees — only for the re-plan to reset to index 0, because
    // remediation is served first.
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
  };

  // The first task. Guarded against the StrictMode double invoke: `/teach` and `/serve` are
  // both writes, and an unguarded start issues them twice (NO-2BILL).
  useEffect(() => {
    if (startedOnce.current) return;
    startedOnce.current = true;
    const begin = (first: SessionPlanResponse): void => {
      setRestoredProof(first.proof_grading ?? null);
      session.start(first);
    };
    if (initialPlan) { begin(initialPlan); return; }
    void call(() => api.getPlan(), begin);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per mount, by design.
  }, []);

  // Whenever the current task changes, start it.
  const currentTaskId = session.task?.task_id ?? null;
  useEffect(() => {
    if (currentTaskId && adopting.current === currentTaskId) {
      // A mixed review block handed on to this task with its problem in hand.
      adopting.current = null;
      return;
    }
    adopting.current = null;
    if (currentTaskId) startTask();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the task id is the trigger.
  }, [currentTaskId]);

  // ---- submitting ----------------------------------------------------------

  const { submit, requestHint } = useGrade({
    api, call, gate, life, session,
    problemRef, taskRef, answerRef, workRef,
    answeredForRef: answeredFor, timedOutForRef: timedOutFor,
    setResult, setRework, setElapsed, setHints, setReferenceLesson,
    onSubmitted: report.remember,
    countdown, elapsed,
  });

  /** The ONE way out of `feedback`. A second click stops at the already-shut gate. */
  const advance = (next?: ServedProblem | null, nextUnavailable = false): void => {
    if (!gate.tryEnter('feedback', 'loading')) return;
    // The verdict goes now, so no feedback panel stands over the next task's load.
    const restampAfterStudy = result?.feedback_practice;
    setResult(null);
    setRestoredProof(null);
    if (restampAfterStudy) { serveThenShow(); return; }

    // The attempt IS recorded and the task is NOT finished: the service could not draw the
    // next problem. Re-serve the SAME task. Falling through to `advanceTask` would skip the
    // problems still owed, which is the silent loss the flag exists to prevent.
    //
    // `serveThenShow`, never `startTask`: on a lesson, `startTask` re-teaches, re-renders
    // the example the learner already finished, and inverts the teach-then-practise order.
    if (!next && nextUnavailable) { serveThenShow(); return; }

    if (next) {
      // A mixed review block may hand on to another review: follow it onto its task.
      if (!follow(next)) { replanForBlock(); return; }
      // A new knowledge point inside a lesson is taught first — and then RE-SERVED, never
      // shown from this payload: that problem's clock started when the service drew it.
      // A verdict comes back to a task on screen, so the ref names one.
      if (taskRef.current!.task_type === 'lesson' && next.kp && next.kp !== taughtKp.current) {
        startTask();
        return;
      }
      setLive(next, clockStart(taskRef.current, next));
      clearForProblem();
      gate.enter('ready');
      return;
    }
    advanceTask();
  };

  // No auto-advance: the verdict stays on screen until the learner takes Continue (click or
  // Enter — the button holds focus). The spec's 1400 ms timer was removed by request: it
  // raced the learner off the solution before they finished reading it.

  // Focus moves on every transition (spec section 4.5). Each control is on screen in the
  // phase that focuses it, so the refs name them.
  useEffect(() => {
    if (phase === 'ready') answerRef.current?.focus();
    else if (phase === 'feedback') continueRef.current?.focus();
    else if (phase === 'done') homeRef.current!.focus();
  }, [phase, problem, result]);

  // ---- render --------------------------------------------------------------

  if (phase === 'done') {
    return <><SessionSummary summary={summary} homeRef={homeRef} onExit={onExit} /><ProblemReport report={report} /></>;
  }

  if (phase === 'closing') {
    return <section className="view-session"><LoadingBlock label="Wrapping up…" /></section>;
  }

  // No dead end: an empty plan offers the placement, and the wording says which empty it is.
  // A plan with no task to stand on: nothing open, or everything already finished.
  if (session.plan !== null && session.task === null) {
    return (
      <EmptyPlan
        message={emptyPlanMessage(session.plan, session.allDone)}
        onDiagnostic={onDiagnostic}
        onExit={onExit}
      />
    );
  }

  if (!session.task) {
    return <section className="view-session"><LoadingBlock label="Preparing your session…" /></section>;
  }

  // AUDIT FINDING (j): the lesson has no approved teach page. No practice is served
  // from here; the one control leads to the next task.
  if (phase === 'no-instruction') {
    return (
      <section className="view-session">
        <NoInstruction task={session.task} onSkip={skipTask} onExit={onExit} />
      </section>
    );
  }

  if (teaching) {
    const teachTaskId = session.task.task_id;
    return (
      <section className="view-session" aria-busy={phase === 'loading'}>
        <Teach
          // A new knowledge point is a new page: its active parts start unanswered.
          key={`${teachTaskId}/${teaching.kp}`}
          task={session.task}
          instruction={teaching}
          onContinue={practise}
          onCheck={(body) => api.taskTeachCheck(teachTaskId, body)}
        />
        <ProblemReport report={report} />
      </section>
    );
  }

  if (integrated) {
    return <section className="view-session">
      <p hidden={!session.task.integrated_assessment}>Delayed application assessment</p>
      <button type="button" className="btn btn-ghost" onClick={onExit}>Exit</button>
      <Integrated key={session.task.task_id} api={api} reportApi={api} taskId={session.task.task_id}
        problem={integrated} onUnauthorized={demo ? undefined : onUnauthorized}
        onGraded={() => gate.enter('feedback')}
        onContinue={() => {
          if (!gate.tryEnter('feedback', 'loading')) return;
          setIntegrated(null);
          advanceTask();
        }} />
    </section>;
  }

  if (!problem) {
    return <section className="view-session"><LoadingBlock label="Preparing your session…" /></section>;
  }

  const locked = phase !== 'ready';
  // H-3: the serve names whether this knowledge point holds an approved hint
  // ladder. Only an explicit `false` hides the affordance; a payload with no
  // flag keeps the historical behavior of asking and reading the refusal.
  const hintsAvailable = problem.hint_available !== false;

  return (
    // KEYED PER PROBLEM. Without the key React reuses the input and the work field, and the
    // previous problem's working posts with the next problem's answer — corrupt data in an
    // append-only log.
    <section className="view-session" key={problem.problem_id} aria-busy={phase === 'loading'}>
      <ProblemHeader
        task={session.task}
        problem={problem}
        elapsed={elapsed}
        countdown={countdown}
        onExit={onExit}
      />

      <RestoredProof api={api} field={restoredProof} />
      <div className="card problem-card">
        <MathBlock>{problem.text}</MathBlock>
        {/* The figures of the knowledge point, each with its text equivalent (unit f9). */}
        <MathVisuals visuals={problem.visuals} />

        <HintPanel hints={hints} referenceLesson={referenceLesson}
          hintsAvailable={hintsAvailable} taskType={session.task.task_type} />

        {/* Disabled only where the problem is over. A grade in flight leaves the field
            live: the phase gate, not the attribute, is what stops the second post. */}
        <AnswerInput
          ref={answerRef}
          choices={problem.choices}
          disabled={locked && phase !== 'submitting'}
          locked={locked}
          onSubmit={() => submit()}
          onHint={hintsAvailable ? requestHint : undefined}
        />
        <WorkField ref={workRef} onSubmit={() => submit()} />

        {/* W-C5: one primary here, and the quiet controls beside it. */}
        {phase === 'feedback' ? null : (
          <div className="actions">
            <TypedSubmit choices={problem.choices} busy={phase === 'submitting'} disabled={locked} onClick={() => submit()}>
              Submit
            </TypedSubmit>
            <HintButton hidden={!hintsAvailable} locked={locked} onClick={requestHint} />
          </div>
        )}

        <QuestionReport enabled={phase === 'ready'} key={`served:${problem.problem_id}`} api={api} hideResult context={{
          task_id: session.task.task_id, problem_id: problem.problem_id, report_kind: 'served',
          problem_text: problem.text, answer: '', work: '',
        }} />
        <ProblemReport report={report} />
        {rework ? <Rework res={rework} /> : null}

        {result ? (
          <ProofAwareFeedback
            key={`feedback:${result.attempt_id}`}
            api={api}
            life={life}
            res={result}
            hasNext={!!result.next || !!result.next_unavailable}
            onContinue={() => advance(result.next, result.next_unavailable)}
            onEnd={endSession}
            onRefresh={onExit}
            continueRef={continueRef}
            revealTopic={reviewTopic(session.task)}
          >
            {/* Keyed by the attempt, so a second grade of the same problem — the DD-3/P1
                re-solve — never shows the first attempt's explanation. */}
            <Diagnosis key={result.attempt_id} store={diagnosis} field={result.diagnosis} />
          </ProofAwareFeedback>
        ) : null}
      </div>
    </section>
  );
}

/** The restored proof grading, when the plan named one. */
function RestoredProof({ api, field }: { api: ApiClient; field: ProofGradingField }) {
  return field ? <RestoredProofGrading api={api} field={field} /> : null;
}
