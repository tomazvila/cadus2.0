/**
 * The timed quiz.
 *
 * The quiz is the one study screen that reveals NOTHING while it runs. An answer is
 * accepted silently — "N remaining" — and the service buffers it for the batch reveal
 * (`crates/web/src/grade.rs:859-876`). Three invariants live here, and each one shipped as
 * a defect in 1.0:
 *
 *   QUIZ-budget   The clock is the WHOLE-quiz budget of the plan task, never the
 *                 per-question value a serve carries. The per-question value is ONE topic's
 *                 raw expected time; using it as the whole-quiz clock expired mid-quiz and
 *                 blank-submitted the rest, which made quizzes unpassable. The budget times
 *                 the QUIZ, not the screen: the START is server state, so a re-mount AND a
 *                 page reload both resume the running clock (M6-review-2, V6).
 *   QUIZ-reveal   No correctness on screen before the last answer. The receipt carries no
 *                 verdict, and this screen renders none even when a payload carries one.
 *   QUIZ-timeout  On timeout with an answer in flight, SKIP that question. Re-posting the
 *                 same `problem_id` writes a second attempt to an append-only log and gives
 *                 the loser `404 unknown_problem`.
 *
 * The completed batch has a server-owned result and a separate reveal. Fresh practice
 * starts only after the learner has studied that batch.
 *
 * WHAT THIS UNIT DOES NOT OWN. There is no router yet, so navigation arrives as props.
 */
import { useEffect, useRef, useState } from 'react';
import { ApiError } from '@/api';
import { QuizResults } from './QuizResults';
import { QuestionReport } from './session/ProblemReport';
import { MathBlock } from '@/components/MathBlock';
import { AnswerField, type AnswerFieldHandle } from '@/components/AnswerField';
import { Chip, LoadingBlock } from '@/components/primitives';
import { releaseOnFail } from '@/hooks/screen';
import { useCall } from '@/hooks/useCall';
import { useLifetime } from '@/hooks/useLifetime';
import { usePhase } from '@/hooks/usePhase';
import { toast } from '@/app/toast';
import { fmtClock, num } from '@/lib/format';
import { isQuizReceipt } from '@/api/types';
import type { ApiClient, PlanTask, ServedProblem } from '@/api/types';

type Phase = 'loading' | 'ready' | 'submitting' | 'done';

/**
 * The deadline of every quiz this page load started, keyed by API client and task id.
 *
 * QUIZ-budget times the WHOLE quiz, so the deadline cannot live in component state. The
 * topbar offers the map from every signed-in screen (`app/Topbar.tsx`), the map's Done gives
 * the previous screen back (`app/Root.tsx`), and React unmounts and mounts the quiz across
 * that round trip. A mount effect that seeds the clock from the budget hands the learner the
 * whole budget again, once per trip, so the timed quiz has no end (M6-review-2, V6).
 *
 * The KEY is the API client: boot builds one per page load and every screen shares it, so an
 * entry lives exactly as long as the connection the quiz runs on.
 *
 * IT IS THE FALLBACK, NOT THE CLOCK. A page reload builds a new client, so this map is empty
 * and every deadline in it is gone. The clock itself is SERVER state: the D-S6 quiz buffer
 * holds the quiz start (`crates/web/src/state.rs` `QuizBuffer.started_at`) and the quiz serve
 * reports `quiz_elapsed_secs`. The map answers only for a serve that carries no count, which
 * is a task type with no quiz clock at all.
 */
const deadlines = new WeakMap<ApiClient, Map<string, number>>();

/**
 * The Unix time in milliseconds this quiz ends at.
 *
 * `elapsed` is the server's own count of the seconds the quiz has run, and it WINS: the
 * server stamps the start once per task, so its count survives a re-mount and a reload
 * alike. A count at or past the budget gives a deadline in the past, and `secsTo` then
 * reads 0, which runs the timeout path on the first render (M6-review-2, V6).
 *
 * With no count from the server the map answers instead: the first call of a task stamps the
 * deadline, and every later call gives that stamp back, so the clock RESUMES.
 */
function deadlineOf(
  api: ApiClient,
  taskId: string,
  budget: number,
  elapsed: number | null,
): number {
  let open = deadlines.get(api);
  if (!open) { open = new Map(); deadlines.set(api, open); }
  const end = elapsed === null
    ? open.get(taskId) ?? Date.now() + budget * 1000
    : Date.now() + (budget - elapsed) * 1000;
  open.set(taskId, end);
  return end;
}

/**
 * The server's count of the seconds this quiz has run, or null when it sent none.
 *
 * Null and zero are DIFFERENT answers, so `num` is the wrong reader here: it turns an absent
 * count into 0, and a deadline seeded from 0 is the whole budget over again — the defect
 * (V6). Only a finite number is a count.
 */
function elapsedOf(served: ServedProblem): number | null {
  // `null` and an absent field both read as NaN here, and NaN is not a count.
  const secs = served.quiz_elapsed_secs ?? Number.NaN;
  return Number.isFinite(secs) ? secs : null;
}

/** Whole seconds from now to `end`, never below zero. */
function secsTo(end: number): number {
  return Math.max(0, Math.round((end - Date.now()) / 1000));
}

/** The clock turns red in the last minute. The 1.0 quiz literal (`quiz.js:46`). */
const QUIZ_URGENT_SECS = 60;

/** The line under the answer field, for the whole quiz. */
export const QUIZ_SILENCE_NOTE = 'No feedback until the end.';

/** The line the time-out raises. It is the only mid-quiz message. */
export const QUIZ_TIMEOUT_MESSAGE = 'Time’s up — grading your answers.';

export interface QuizProps {
  api: ApiClient;
  /** The plan task. Its `time_budget_secs` is the WHOLE-quiz clock (QUIZ-budget). */
  task: PlanTask;
  /** Demo mode. A 401 then keeps the learner on the screen. */
  demo: boolean;
  onUnauthorized: () => void;
  /** Leave the quiz screen. The session, if there is one, continues behind it. */
  onDone: () => void;
  /** The wording of the way out: back into a session, or back to the dashboard. */
  fromSession?: boolean;
}

export function Quiz({
  api,
  task,
  demo,
  onUnauthorized,
  onDone,
  fromSession = false,
}: QuizProps) {
  const life = useLifetime();
  const call = useCall({ demo, onUnauthorized });
  const [phase, gate] = usePhase<Phase>('loading');

  const [problem, setProblem] = useState<ServedProblem | null>(null);
  const [total, setTotal] = useState(0);
  // Questions still owed. The first serve numbers it before a question is on screen.
  const [remaining, setRemaining] = useState(0);
  const [left, setLeft] = useState<number | null>(null);
  const [resumePractice, setResumePractice] = useState(false);

  const answerRef = useRef<AnswerFieldHandle>(null);
  const doneRef = useRef<HTMLButtonElement>(null);
  // The live question, read SYNCHRONOUSLY by a submit and by the timeout — neither can wait
  // for a render. Every `setProblem` site assigns this on the same line.
  const problemRef = useRef<ServedProblem | null>(null);
  const timedOutRef = useRef(false);
  const servedOnce = useRef(false);
  // The end of the whole-quiz clock, in Unix milliseconds. Null until the first serve, and
  // null again once the quiz closes.
  const deadlineRef = useRef<number | null>(null);

  /**
   * Close the quiz. Every caller is a continuation that already checked the view is alive.
   *
   * THE CLOCK STOPS HERE, not in the effect that armed it. The effect cleans up at the next
   * commit, and a tick between this call and that commit would write a number back over the
   * cleared clock and run the timeout path on a quiz that is over.
   */
  /**
   * The quiz is over. The done screen renders first, so nothing on the card needs clearing,
   * and the clock effect lets go of its interval when the phase moves.
   */
  const finish = (): void => {
    deadlines.get(api)?.delete(task.task_id);
    gate.enter('done');
  };

  /**
   * Blank-submit whatever is left, so the service closes the quiz.
   *
   * A continuation CHAIN, not an await loop: each step's Retry resumes the fill where it
   * stopped instead of abandoning the quiz half-answered. It recurses through a ref, so the
   * callback does not have to list itself as its own dependency.
   */
  const fillBlanksRef = useRef<(guard?: number) => void>(() => {});

  const fillBlanks = (guard = 0): void => {
    // The bound: a service that serves past its own count is a defect, not a loop.
    if (guard > total + 1) return;
    // Every caller stands on a live question: the timeout fires only after the first serve,
    // and a continuation resumes the fill with the question it just served.
    const current = problemRef.current!;
    gate.enter('submitting');

    void call(
      () => api.taskAnswer(task.task_id, { problem_id: current.problem_id, answer: '' }),
      (res) => {
        // A view that left serves nothing more.
        if (!life.alive()) return undefined;
        if (!isQuizReceipt(res) || res.quiz_complete) { finish(); return undefined; }
        return call(() => api.taskServe(task.task_id), (next) => {
          // The ref only, NO `setProblem`: the end screen is next, and repainting each
          // intermediate question flashes them past the learner behind a frozen count and a
          // 0:00 clock. 1.0 makes the same choice explicitly (`quiz.js:134`).
          // A view that left fills nothing more.
          if (!life.alive()) return;
          problemRef.current = next;
          fillBlanksRef.current(guard + 1);
        });
      },
      {
        // THE RETRY GATE (F-37-1c), for the time-up path. The fill owns `submitting` for
        // its whole chain, so its Retry RESUMES the fill and must NOT re-take a `ready`
        // lock the way a submit does. A failed fill leaves the card locked, so nothing but
        // this Retry moves the question: the one thing the gate tests is that the view is
        // still on screen (F-37-1b).
        retryGate: () => life.alive(),
      },
    );
  };

  useEffect(() => { fillBlanksRef.current = fillBlanks; });

  /**
   * The clock ran out. Once per quiz: the effect below fires when `left` BECOMES zero, and
   * a clock read off the deadline never leaves zero again.
   */
  // Built ONCE: it reads the gate and two refs, and the effect that fires it holds it.
  const [timeUp] = useState(() => (): void => {
    timedOutRef.current = true;
    toast(QUIZ_TIMEOUT_MESSAGE, { kind: 'info' });
    // QUIZ-timeout. An answer for this question is already in flight: the service has it,
    // and a second post of the same `problem_id` writes a second attempt to an append-only
    // log. Skip it. The in-flight continuation resumes the fill as soon as the next
    // question is served, so the quiz still closes.
    if (gate.is('submitting')) return;
    fillBlanksRef.current();
  });

  // The first question. NO-2BILL: StrictMode runs a mount effect twice in development, and
  // `taskServe` is a write, so an unguarded serve is two writes on one mount.
  useEffect(() => {
    if (servedOnce.current) return;
    servedOnce.current = true;
    void call(() => api.taskServe(task.task_id).catch((error: Error) => {
      if (error instanceof ApiError && error.code === 'task_complete') return null;
      throw error;
    }), (s) => {
      if (!s || s.feedback_practice) {
        setResumePractice(s?.feedback_practice === true);
        finish();
        return;
      }
      setTotal(num(s.total));
      // The count is SERVER state. A quiz serve numbers the live question `answered + 1`
      // (`crates/web/src/serve.rs:159-164`), so the answers already in survive a re-mount
      // instead of resetting to the full quiz (V6).
      const answered = Math.max(0, num(s.index) - 1);
      setRemaining(Math.max(0, num(s.total) - answered));
      // QUIZ-budget. The plan task's budget times the WHOLE quiz; the serve value times one
      // question. The serve value is the fallback and nothing more.
      const budget = num(task.time_budget_secs) || num(s.time_budget_secs);
      if (budget > 0) {
        // The deadline of THIS quiz. The server's own count of the seconds gone comes
        // first, so a page RELOAD resumes the running clock; the module-scope map is the
        // fallback. At or under zero the effect below runs the timeout path at once.
        const end = deadlineOf(api, task.task_id, budget, elapsedOf(s));
        deadlineRef.current = end;
        setLeft(secsTo(end));
      }
      setProblem(s);
      problemRef.current = s;
      gate.enter('ready');
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once per mount, by design.
  }, []);

  // The clock. One interval for the whole quiz, registered in the lifetime.
  //
  // The updater is PURE. React runs a state updater during the RENDER phase whenever the
  // eager path is unavailable, so calling `timeUp()` from inside it issues a POST, a phase
  // write and a toast mid-render. Zero is detected in the effect below instead.
  // The clock ticks while a budget is set and the quiz is not over. The deadline is read
  // off the ref, so the interval survives every re-render in between.
  const ticking = left !== null && phase !== 'done';
  useEffect(() => {
    if (!ticking) return undefined;
    // The clock has a value, so the deadline behind it is set: the serve wrote both.
    const end = deadlineRef.current!;
    const id = life.setInterval(() => {
      // Read the DEADLINE, not the last value: a browser throttles the interval of a hidden
      // tab, and a clock that counts ticks gives that throttled time back to the learner.
      setLeft(secsTo(end));
    }, 1000);
    return () => life.clearTimer(id);
  }, [ticking, life]);

  useEffect(() => { if (left === 0) timeUp(); }, [left, timeUp]);

  // Focus moves on every transition (spec section 4.5).
  // Each control is on screen in the phase that focuses it, so the refs name them.
  useEffect(() => {
    if (phase === 'ready') answerRef.current!.focus();
    else if (phase === 'done') doneRef.current!.focus();
  }, [phase, problem]);

  const submit = (): void => {
    // Submit and Enter render beside a question, so the ref names one and the field is up.
    const current = problemRef.current!;
    const field = answerRef.current!;
    const answer = field.value();
    // THE gate (F-37-1c). Synchronous, before the first await, so the Submit button and an
    // Enter inside the grading window cannot both post this `problem_id`.
    if (!gate.tryEnter('ready', 'submitting')) return;
    if (!answer) { gate.enter('ready'); field.focus(); return; }

    void call(
      () => api.taskAnswer(task.task_id, { problem_id: current.problem_id, answer }),
      (res) => {
        if (!life.alive()) return undefined;
        // QUIZ-reveal, structurally. A quiz reply is a receipt; anything else is a service
        // defect, and the honest response to it is to end the quiz revealing NOTHING —
        // never to paint a verdict this screen is not allowed to show.
        if (!isQuizReceipt(res)) { finish(); return undefined; }
        // The done screen shows no count, so none is set here.
        if (res.quiz_complete) { finish(); return undefined; }
        setRemaining(num(res.remaining));
        // A nested call, so this serve owns its own Retry. A failure here leaves the phase
        // at `submitting` — correct: there is nothing to submit until a question is up.
        return call(() => api.taskServe(task.task_id), (next) => {
          // A view that left fills nothing more.
          if (!life.alive()) return;
          setProblem(next);
          problemRef.current = next;
          gate.enter('ready');
          // The clock ran out while this answer was in flight. Resume the fill now.
          if (timedOutRef.current) fillBlanks();
        });
      },
      {
        // THE RETRY RE-ENTERS THE GATE (F-37-1c). `useCall` holds no view state, and this
        // toast never expires (F-36-1b), so a Retry pressed after the learner answered
        // again re-posts a `problem_id` the service already spent. A service that refuses
        // it answers `404 unknown_problem` and arms yet another Retry; a service that
        // accepts it is worse, because the stale continuation then runs `taskServe` and
        // replaces the question on screen UNANSWERED. The gate refuses the retry, and the
        // refusal toast expires.
        //
        // Two more terms sit in the gate. `life.alive()` comes first: the toast outlives
        // the view — the store is module-scope — so a learner who left the quiz can still
        // press this Retry, and a post from a dead screen is a write nobody is on
        // (F-37-1b). `timedOutRef` comes last: past the deadline the blank fill owns every
        // remaining post (QUIZ-timeout), and a retried answer is a second attempt.
        retryGate: () => life.alive()
          && problemRef.current!.problem_id === current.problem_id
          && !timedOutRef.current
          && gate.tryEnter('ready', 'submitting'),
        onFail: releaseOnFail(gate, 'submitting', 'ready'),
      },
    );
  };

  if (phase === 'done') {
    return (
      <section className="view-quiz">
        <div className="card summary-card">
          <h2>Quiz complete</h2>
          <p className="quiz-recorded">
            {total > 0
              ? `All ${total} answers are recorded.`
              : 'Your answers are recorded.'}
          </p>
          {/* No score and no per-question breakdown: the receipt carries neither, and a
              number invented here would be a lie about a recorded attempt. */}
          <QuizResults api={api} taskId={task.task_id} onUnauthorized={onUnauthorized} resumePractice={resumePractice} />
          <button ref={doneRef} type="button" className="btn btn-primary" onClick={onDone}>
            {fromSession ? 'Continue session' : 'Back to dashboard'}
          </button>
        </div>
      </section>
    );
  }

  if (phase === 'loading') {
    return <section className="view-quiz"><LoadingBlock label="Loading the quiz…" /></section>;
  }
  // Every phase after `loading` has a question on screen.
  const question = problem!;

  return (
    // KEYED PER QUESTION. Without the key React reuses the input and the previous answer
    // pre-fills the next question.
    <section className="view-quiz" key={question.problem_id}>
      <div className="task-header">
        <div className="task-meta">
          <Chip className="chip-quiz">quiz</Chip>
          <span className="topic-name">Timed quiz</span>
        </div>
        <div className="task-right">
          <span className="progress-count">
            {`${num(question.index)} / ${num(question.total) || total}`}
          </span>
          {left !== null ? (
            <span className={`timer${left <= QUIZ_URGENT_SECS ? ' urgent' : ''}`}>
              {fmtClock(left)}
            </span>
          ) : null}
        </div>
      </div>

      <div className="card problem-card">
        <MathBlock>{question.text}</MathBlock>

        {/* No hint control: a hint inside a quiz is `409 no_hints_in_quiz`. */}
        <AnswerField ref={answerRef} disabled={phase !== 'ready'} onSubmit={submit} />

        <div className="actions">
          <button
            type="button"
            className={`btn btn-primary${phase === 'submitting' ? ' is-busy' : ''}`}
            disabled={phase !== 'ready'}
            onClick={submit}
          >
            Submit answer
          </button>
        </div>

        {/* QUIZ-reveal: a running count and an explicit promise. NOTHING about whether the
            last answer was right. */}
        <QuestionReport key={question.problem_id} api={api} hideResult context={{
          task_id: task.task_id, problem_id: question.problem_id, report_kind: 'served',
          problem_text: question.text, answer: '', work: '',
        }} />
        <div className="quiz-note">
          <span className="remaining">{`${remaining} remaining`}</span>
          <span className="muted">{QUIZ_SILENCE_NOTE}</span>
        </div>
      </div>
    </section>
  );
}
