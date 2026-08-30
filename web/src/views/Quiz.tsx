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
 *                 blank-submitted the rest, which made quizzes unpassable.
 *   QUIZ-reveal   No correctness on screen before the last answer. The receipt carries no
 *                 verdict, and this screen renders none even when a payload carries one.
 *   QUIZ-timeout  On timeout with an answer in flight, SKIP that question. Re-posting the
 *                 same `problem_id` writes a second attempt to an append-only log and gives
 *                 the loser `404 unknown_problem`.
 *
 * WHAT THE BATCH REVEAL SHOWS IN 2.0, and why it is short. 1.0 built a per-question
 * breakdown from the close payload (`api.py:1702-1797`). M5 mounts no close route —
 * `SPEC_ROUTES_ABSENT` in `api/types.ts` records `POST /api/task/{id}/abort` — and the
 * quiz receipt carries `accepted`, `remaining` and `quiz_complete` and nothing else. So the
 * end screen states what is true: every answer is recorded. The breakdown arrives with the
 * Rust close unit, and it belongs behind that route, not in a guess made here.
 *
 * WHAT THIS UNIT DOES NOT OWN. There is no router yet, so navigation arrives as props.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { MathBlock } from '@/components/MathBlock';
import { AnswerField, type AnswerFieldHandle } from '@/components/AnswerField';
import { Chip, LoadingBlock } from '@/components/primitives';
import { useCall } from '@/hooks/useCall';
import { useLifetime } from '@/hooks/useLifetime';
import { usePhase } from '@/hooks/usePhase';
import { toast } from '@/app/toast';
import { fmtClock, num } from '@/lib/format';
import { isQuizReceipt } from '@/api/types';
import type { ApiClient, PlanTask, ServedProblem } from '@/api/types';

type Phase = 'loading' | 'ready' | 'submitting' | 'done';

/** The clock turns red in the last minute. The 1.0 quiz literal (`quiz.js:46`). */
export const QUIZ_URGENT_SECS = 60;

/** The line under the answer field, for the whole quiz. */
export const QUIZ_SILENCE_NOTE = 'No feedback until the end.';

/** The line the time-out raises. It is the only mid-quiz message. */
export const QUIZ_TIMEOUT_MESSAGE = 'Time’s up — grading your answers.';

export interface QuizProps {
  api: ApiClient;
  /** The plan task. Its `time_budget_secs` is the WHOLE-quiz clock (QUIZ-budget). */
  task: PlanTask;
  /** Demo mode. A 401 then keeps the learner on the screen. */
  demo?: boolean;
  onUnauthorized: () => void;
  /** Leave the quiz screen. The session, if there is one, continues behind it. */
  onDone: () => void;
  /** The wording of the way out: back into a session, or back to the dashboard. */
  fromSession?: boolean;
}

export function Quiz({
  api,
  task,
  demo = false,
  onUnauthorized,
  onDone,
  fromSession = false,
}: QuizProps) {
  const life = useLifetime();
  const call = useCall({ demo, onUnauthorized });
  const [phase, gate] = usePhase<Phase>('loading');

  const [problem, setProblem] = useState<ServedProblem | null>(null);
  const [total, setTotal] = useState(0);
  const [remaining, setRemaining] = useState<number | null>(null);
  const [left, setLeft] = useState<number | null>(null);

  const answerRef = useRef<AnswerFieldHandle>(null);
  const doneRef = useRef<HTMLButtonElement>(null);
  // The live question, read SYNCHRONOUSLY by a submit and by the timeout — neither can wait
  // for a render. Every `setProblem` site assigns this on the same line.
  const problemRef = useRef<ServedProblem | null>(null);
  const timedOutRef = useRef(false);
  const servedOnce = useRef(false);

  const finish = useCallback(() => {
    if (!life.alive()) return;
    setLeft(null);
    setProblem(null);
    problemRef.current = null;
    gate.enter('done');
  }, [gate, life]);

  /**
   * Blank-submit whatever is left, so the service closes the quiz.
   *
   * A continuation CHAIN, not an await loop: each step's Retry resumes the fill where it
   * stopped instead of abandoning the quiz half-answered. It recurses through a ref, so the
   * callback does not have to list itself as its own dependency.
   */
  const fillBlanksRef = useRef<(guard?: number) => void>(() => {});

  const fillBlanks = useCallback((guard = 0) => {
    if (!life.alive() || gate.is('done') || guard > total + 1) return;
    const current = problemRef.current;
    // Before the phase moves: entering `submitting` and then bailing would lock the view.
    if (!current) return;
    gate.enter('submitting');

    void call(
      () => api.taskAnswer(task.task_id, { problem_id: current.problem_id, answer: '' }),
      (res) => {
        if (!life.alive()) return undefined;
        if (!isQuizReceipt(res) || res.quiz_complete) { finish(); return undefined; }
        return call(() => api.taskServe(task.task_id), (next) => {
          if (!life.alive()) return;
          // The ref only, NO `setProblem`: the end screen is next, and repainting each
          // intermediate question flashes them past the learner behind a frozen count and a
          // 0:00 clock. 1.0 makes the same choice explicitly (`quiz.js:134`).
          problemRef.current = next;
          fillBlanksRef.current(guard + 1);
        });
      },
    );
  }, [api, call, finish, gate, life, task.task_id, total]);

  useEffect(() => { fillBlanksRef.current = fillBlanks; }, [fillBlanks]);

  const timeUp = useCallback(() => {
    if (timedOutRef.current || gate.is('done') || !life.alive()) return;
    timedOutRef.current = true;
    setLeft(0);
    toast(QUIZ_TIMEOUT_MESSAGE, { kind: 'info' });
    // QUIZ-timeout. An answer for this question is already in flight: the service has it,
    // and a second post of the same `problem_id` writes a second attempt to an append-only
    // log. Skip it. The in-flight continuation resumes the fill as soon as the next
    // question is served, so the quiz still closes.
    if (gate.is('submitting')) return;
    fillBlanks();
  }, [fillBlanks, gate, life]);

  // The first question. NO-2BILL: StrictMode runs a mount effect twice in development, and
  // `taskServe` is a write, so an unguarded serve is two writes on one mount.
  useEffect(() => {
    if (servedOnce.current) return;
    servedOnce.current = true;
    void call(() => api.taskServe(task.task_id), (s) => {
      if (!life.alive()) return;
      setTotal(num(s.total));
      setRemaining(num(s.total));
      // QUIZ-budget. The plan task's budget times the WHOLE quiz; the serve value times one
      // question. The serve value is the fallback and nothing more.
      const budget = num(task.time_budget_secs) || num(s.time_budget_secs);
      if (budget > 0) setLeft(budget);
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
  useEffect(() => {
    if (left === null || phase === 'done') return undefined;
    const id = life.setInterval(() => {
      setLeft((v) => (v === null ? v : Math.max(0, v - 1)));
    }, 1000);
    return () => life.clearTimer(id);
    // Both deps are BOOLEANS, so a tick re-render recomputes the same values and React
    // skips the effect — the interval is armed when the clock starts, not on every second.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [left === null, phase === 'done']);

  useEffect(() => { if (left === 0) timeUp(); }, [left, timeUp]);

  // Focus moves on every transition (spec section 4.5).
  useEffect(() => { if (phase === 'ready') answerRef.current?.focus(); }, [phase, problem]);
  useEffect(() => { if (phase === 'done') doneRef.current?.focus(); }, [phase]);

  const submit = useCallback(() => {
    const current = problemRef.current;
    if (!current) return;
    const answer = answerRef.current?.value() ?? '';
    // THE gate (F-37-1c). Synchronous, before the first await, so the Submit button and an
    // Enter inside the grading window cannot both post this `problem_id`.
    if (!gate.tryEnter('ready', 'submitting')) return;
    if (!answer) { gate.enter('ready'); answerRef.current?.focus(); return; }

    void call(
      () => api.taskAnswer(task.task_id, { problem_id: current.problem_id, answer }),
      (res) => {
        if (!life.alive()) return undefined;
        // QUIZ-reveal, structurally. A quiz reply is a receipt; anything else is a service
        // defect, and the honest response to it is to end the quiz revealing NOTHING —
        // never to paint a verdict this screen is not allowed to show.
        if (!isQuizReceipt(res)) { finish(); return undefined; }
        if (res.quiz_complete) { setRemaining(0); finish(); return undefined; }
        setRemaining(num(res.remaining));
        // A nested call, so this serve owns its own Retry. A failure here leaves the phase
        // at `submitting` — correct: there is nothing to submit until a question is up.
        return call(() => api.taskServe(task.task_id), (next) => {
          if (!life.alive()) return;
          setProblem(next);
          problemRef.current = next;
          answerRef.current?.clear();
          gate.enter('ready');
          // The clock ran out while this answer was in flight. Resume the fill now.
          if (timedOutRef.current) fillBlanks();
        });
      },
    ).then((res) => {
      // A failed grade returns the question to the learner, rather than locking the card.
      if (!res && life.alive() && gate.is('submitting')) gate.enter('ready');
    });
  }, [api, call, fillBlanks, finish, gate, life, task.task_id]);

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
          <p className="muted small">Your progress is up to date on the dashboard.</p>
          <button ref={doneRef} type="button" className="btn btn-primary" onClick={onDone}>
            {fromSession ? 'Continue session' : 'Back to dashboard'}
          </button>
        </div>
      </section>
    );
  }

  if (!problem) {
    return <section className="view-quiz"><LoadingBlock label="Loading the quiz…" /></section>;
  }

  return (
    // KEYED PER QUESTION. Without the key React reuses the input and the previous answer
    // pre-fills the next question.
    <section className="view-quiz" key={problem.problem_id}>
      <div className="task-header">
        <div className="task-meta">
          <Chip className="chip-quiz">quiz</Chip>
          <span className="topic-name">Timed quiz</span>
        </div>
        <div className="task-right">
          <span className="progress-count">
            {`${num(problem.index)} / ${num(problem.total) || total}`}
          </span>
          {left !== null ? (
            <span className={`timer${left <= QUIZ_URGENT_SECS ? ' urgent' : ''}`}>
              {fmtClock(left)}
            </span>
          ) : null}
        </div>
      </div>

      <div className="card problem-card">
        <MathBlock>{problem.text}</MathBlock>

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
        <div className="quiz-note">
          <span className="remaining">{`${remaining ?? total} remaining`}</span>
          <span className="muted">{QUIZ_SILENCE_NOTE}</span>
        </div>
      </div>
    </section>
  );
}
