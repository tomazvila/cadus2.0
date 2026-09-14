/**
 * The placement diagnostic.
 *
 * THE MOST LEVERAGED INPUT IN THE SYSTEM, and not because it is complex. Placement decides
 * where the learner starts; every probe answer appends a row to the `events` table, and
 * `cadus_app` holds no UPDATE and no DELETE on it. A defect here places the learner at the
 * wrong frontier, permanently. Four invariants live here:
 *
 *   P3          Three ground rules BEFORE probe 1, and an honest-skip control beside
 *               Submit. The rules are on their own screen so that reading them costs no
 *               probe time: `diagStart` is issued by the Begin button, never by a mount
 *               effect, or 60-120 s of reading lands in probe 1's `secs` and places the
 *               learner low.
 *   R15         The intro focuses the CARD, never the CTA. A held Enter carried over from
 *               the previous screen auto-repeats onto a focused button and skips the rules;
 *               a non-interactive container with `tabindex="-1"` swallows that keydown.
 *   DIAG-750    The 750 ms post-answer beat is registered in the view lifetime and cleared
 *               on cleanup, which is what makes "Save & exit" honest: leaving cancels the
 *               beat, so an exit can never commit placement behind the learner's back.
 *   DIAG-nosol  Placement feedback is a tick or a cross and a title. NOTHING else — no
 *               solution and no expected answer, even if a payload carried them. A revealed
 *               answer turns the next probe into a copy exercise and corrupts the
 *               plus-minus balance placement is built from.
 *
 * THE TRANSPORT IS A PROP. The service mounts the three `/api/diag/*` routes, and the
 * live adapter posts to them — `api/diag.ts` states why the port stays a prop. The
 * `404 not_found` branch stays too: a deployment older than those routes answers it, and
 * a failed start returns to the intro with a stated line rather than leaving the learner
 * on a spinner.
 *
 * WHAT THIS UNIT DOES NOT OWN. There is no router yet, so navigation arrives as props.
 */
import { useEffect, useRef, useState } from 'react';
import { MathBlock } from '@/components/MathBlock';
import { AnswerField, type AnswerFieldHandle } from '@/components/AnswerField';
import { Chip, LoadingBlock } from '@/components/primitives';
import { closeWith, releaseOnFail } from '@/hooks/screen';
import { useCall } from '@/hooks/useCall';
import { useLifetime } from '@/hooks/useLifetime';
import { usePhase } from '@/hooks/usePhase';
import { num } from '@/lib/format';
import { ProblemReport, QuestionReport } from './session/ProblemReport';
import { useProblemReport, type ProblemReportApi } from './session/useProblemReport';
import {
  DIAG_START_FAILED,
  IntroCard,
  PlacementDone,
  ProbeFeedback,
  topicName,
  type ProbeResult,
} from './diagnostic/DiagnosticScreens';
import type { DiagFinishResponse, DiagProbe, DiagnosticApi } from '@/api/diag';

export { DIAG_START_FAILED };

type Phase = 'intro' | 'loading' | 'ready' | 'submitting' | 'feedback' | 'closing' | 'done';

/** The post-answer beat, in milliseconds. The 1.0 literal (`diagnostic.js:144`). */
export const DIAG_BEAT_MS = 750;

/** The probe cap when the service names none. */
export const DIAG_DEFAULT_CAP = 40;

/** The promise the learner reads on probe 1 (DIAG-nosol). */
export const DIAG_NO_SOLUTIONS_NOTE =
  'No solutions are shown during placement — just answer as best you can.';

export interface DiagnosticProps {
  /** The three placement calls. See `api/diag.ts` for why this is not on `ApiClient`. */
  diag: DiagnosticApi;
  reportApi?: ProblemReportApi;
  /** Demo mode. A 401 then keeps the learner on the screen. */
  demo: boolean;
  onUnauthorized: () => void;
  /** Leave the placement. It resumes at this probe next time. */
  onExit: () => void;
}

export function Diagnostic({ diag, reportApi, demo, onUnauthorized, onExit }: DiagnosticProps) {
  const life = useLifetime();
  const call = useCall({ demo, onUnauthorized });
  const [phase, gate] = usePhase<Phase>('intro');

  const [probe, setProbe] = useState<DiagProbe | null>(null);
  const [qNum, setQNum] = useState(1);
  const [cap, setCap] = useState(DIAG_DEFAULT_CAP);
  const [result, setResult] = useState<ProbeResult | null>(null);
  const [summary, setSummary] = useState<DiagFinishResponse | null>(null);
  const [startFailed, setStartFailed] = useState(false);
  const report = useProblemReport(reportApi ?? null, undefined, (_receipt, context) => {
    setResult((previous) => previous && probeRef.current?.problem_id === context.problem_id
      ? { ...previous, res: { ...previous.res, correct: true, outcome: 'correct' } } : previous);
  });

  const answerRef = useRef<AnswerFieldHandle>(null);
  const introRef = useRef<HTMLDivElement>(null);
  const homeRef = useRef<HTMLButtonElement>(null);
  // The live probe, read SYNCHRONOUSLY by a submit and by a Retry — neither can wait for a
  // render, and a Retry arrives renders after the closure that armed it. Every `setProbe`
  // site assigns this on the same line.
  const probeRef = useRef<DiagProbe | null>(null);

  // R15: the briefing CARD holds the focus, not the CTA.
  // Each control is on screen in the phase that focuses it, so the refs name them.
  useEffect(() => {
    if (phase === 'intro') introRef.current!.focus();
    else if (phase === 'ready') answerRef.current!.focus();
    else if (phase === 'done') homeRef.current!.focus();
  }, [phase, probe]);

  /** Commit the placement. `closing` renders the commit screen over the answered probe. */
  const finish = (): void => {
    gate.enter('closing');
    closeWith(call, gate, 'done', () => diag.diagFinish(), setSummary);
  };

  // `finish` is read through a ref so it stays OUT of the beat effect's dependency list. It
  // is a `useCallback` keyed on the transport, and a caller that rebuilds that object each
  // render would re-arm the beat on every re-render.
  const finishRef = useRef(finish);
  useEffect(() => { finishRef.current = finish; });

  const begin = (): void => {
    // A repeated Enter or click cannot start two placements.
    if (!gate.tryEnter('intro', 'loading')) return;
    // The line of a failed start stays as it is: the intro is off screen from here on.
    void call(() => diag.diagStart(), (s) => {
      // A view that left commits nothing.
      if (!life.alive()) return;
      // A restarted, exhausted placement answers `{"probe": null}`. Reading `.text` off
      // that blanks the screen; finishing is the honest response — there is nothing left
      // to ask.
      if (!s.probe) { finishRef.current(); return; }
      setProbe(s.probe);
      probeRef.current = s.probe;
      setQNum(num(s.asked) + 1);
      setCap(num(s.cap, DIAG_DEFAULT_CAP) || DIAG_DEFAULT_CAP);
      gate.enter('ready');
    }).then((s) => {
      // The failure `useCall` already toasted with a Retry. Say so on the card too, and go
      // back to `intro`: the alternative is a spinner with no way on, which is what a
      // route the service does not mount would leave behind.
      if (!s && life.alive()) { setStartFailed(true); gate.enter('intro'); }
    });
  };

  const send = (answer: string, skipped: boolean): void => {
    // THE REF, never the render value: a Retry re-enters this closure renders later, and a
    // captured probe is the stale one by then. Submit and Skip render beside a probe, so the
    // ref names one.
    const current = probeRef.current!;
    // THE gate, for every entry point — Submit, Skip, and the Enter key, which bypasses the
    // disabled button entirely. Synchronous, before the first await.
    if (!gate.tryEnter('ready', 'submitting')) return;

    void call(
      () => diag.diagAnswer({ problem_id: current.problem_id, answer }),
      (res) => {
        if (reportApi) report.remember({ task_id: 'diag', problem_id: current.problem_id, report_kind: 'diagnostic',
          problem_text: current.text, answer, work: '' });
        setResult({ res, skipped });
        gate.enter('feedback');
      },
      {
        // THE RETRY RE-ENTERS THE GATE (F-37-1c). `useCall` holds no view state, and this
        // toast never expires (F-36-1b), so a Retry pressed after the learner answered
        // again re-posts a spent `problem_id` to an append-only log with no DELETE. Its
        // continuation then runs the unconditional `gate.enter('feedback')` on top of the
        // LIVE probe, and the beat below advances the placement off the OLD reply's
        // `next_probe` — the probe on screen is skipped unanswered, and placement is the
        // most leveraged input in the system. The gate refuses the retry, and the refusal
        // toast expires.
        //
        // `life.alive()` FIRST. The toast outlives the view — the store is module-scope —
        // so a learner who pressed "Save & exit" can still press this Retry, and DIAG-750
        // says an exit commits nothing behind their back (F-37-1b).
        retryGate: () => life.alive()
          && probeRef.current!.problem_id === current.problem_id
          && gate.tryEnter('ready', 'submitting'),
        onFail: releaseOnFail(gate, 'submitting', 'ready'),
      },
    );
  };

  /**
   * DIAG-750. The beat is armed in an EFFECT, registered in the lifetime, and cleared on
   * cleanup.
   *
   * The cleanup is not decoration. Without it, every re-render inside the 750 ms window
   * armed one more timer: two probes advanced at once, or `diagFinish` posted twice — and
   * two concurrent commits both pass the service's "no placement yet" check and write two
   * placement rows to an append-only log with no DELETE.
   */
  useEffect(() => {
    if (phase !== 'feedback' || report.open) return;
    // A verdict is on screen in `feedback`, so the state holds one.
    const next = result!.res.next_probe;
    // `null`, `{ done: true }` and a probe with no id all say the placement is over.
    const isProbe = next != null && 'problem_id' in next && next.problem_id !== '';

    const timer = life.setTimeout(() => {
      if (isProbe) {
        setProbe(next);
        probeRef.current = next;
        setResult(null);
        setQNum((n) => n + 1);
        // No clear of the field: the card is keyed on the probe, so the next one mounts fresh.
        gate.enter('ready');
      } else {
        finishRef.current();
      }
    }, DIAG_BEAT_MS);
    return () => life.clearTimer(timer);
  }, [phase, result, life, gate, report.open]);

  const submitTyped = (): void => {
    const field = answerRef.current!;
    const answer = field.value();
    // A blank Submit only refocuses. The honest way past a probe is Skip.
    if (!answer) { field.focus(); return; }
    send(answer, false);
  };

  if (phase === 'intro') {
    return <IntroCard cardRef={introRef} startFailed={startFailed} onBegin={begin} onExit={onExit} />;
  }

  if (phase === 'done') {
    return <><PlacementDone summary={summary} homeRef={homeRef} onExit={onExit} />
      <ProblemReport report={report} /></>;
  }

  if (phase === 'loading') {
    return <section className="view-diagnostic"><LoadingBlock label="Starting the placement…" /></section>;
  }

  if (phase === 'closing') {
    return <section className="view-diagnostic"><LoadingBlock label="Working out your placement…" /></section>;
  }

  // Every phase after `loading` has a probe on screen.
  const question = probe!;

  const locked = phase !== 'ready';

  return (
    // Keyed on the probe, so each question gets a FRESH input subtree. Without it React
    // reuses the nodes and the previous probe's answer pre-fills the next one.
    <section className="view-diagnostic" key={question.problem_id}>
      <div className="task-header">
        <div className="task-meta">
          <Chip className="chip-accent">placement</Chip>
          <span className="topic-name">{topicName(question.topic)}</span>
        </div>
        <div className="task-right">
          <span className="progress-count">{`Question ${qNum} of up to ${cap}`}</span>
        </div>
      </div>

      <div className="progress-bar">
        <div className="progress-fill" style={{ width: `${Math.min(100, (qNum / cap) * 100)}%` }} />
      </div>

      <div className="card problem-card">
        <MathBlock>{question.text}</MathBlock>

        <AnswerField ref={answerRef} disabled={locked} onSubmit={submitTyped} />

        <div className="actions">
          <button
            type="button"
            className={`btn btn-primary${phase === 'submitting' ? ' is-busy' : ''}`}
            disabled={locked}
            onClick={submitTyped}
          >
            Submit
          </button>
          {/* P3: an honest skip needs a REAL control. The intro tells the learner not to
              guess, and a blank Submit is refused — so Skip posts an empty answer, which
              the checker always grades incorrect. */}
          <button
            type="button"
            className={`btn btn-ghost${phase === 'submitting' ? ' is-busy' : ''}`}
            title="Records an honest skip (counts as incorrect — no guessing)"
            disabled={locked}
            onClick={() => send('', true)}
          >
            Skip — I don’t know
          </button>
          <button
            type="button"
            className="btn btn-ghost btn-exit"
            title="Leave the placement — it resumes exactly here next time"
            onClick={onExit}
          >
            Save &amp; exit
          </button>
        </div>

        {reportApi ? <QuestionReport key={`served:${question.problem_id}`} api={reportApi} hideResult context={{
          task_id: 'diag', problem_id: question.problem_id, report_kind: 'served',
          problem_text: question.text, answer: '', work: '',
        }} /> : null}
        <ProblemReport report={report} hideResult />
        {result ? <ProbeFeedback result={result} /> : null}

        {qNum === 1 ? <p className="muted small">{DIAG_NO_SOLUTIONS_NOTE}</p> : null}
      </div>
    </section>
  );
}
