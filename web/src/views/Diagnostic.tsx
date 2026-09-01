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
import { useCallback, useEffect, useRef, useState } from 'react';
import { MathBlock } from '@/components/MathBlock';
import { AnswerField, type AnswerFieldHandle } from '@/components/AnswerField';
import { Chip, Cross, LoadingBlock, Stat, Tick } from '@/components/primitives';
import { useCall } from '@/hooks/useCall';
import { useLifetime } from '@/hooks/useLifetime';
import { usePhase } from '@/hooks/usePhase';
import { num } from '@/lib/format';
import type {
  DiagAnswerResponse,
  DiagFinishResponse,
  DiagProbe,
  DiagnosticApi,
} from '@/api/diag';

type Phase = 'intro' | 'loading' | 'ready' | 'submitting' | 'feedback' | 'done';

/** The post-answer beat, in milliseconds. The 1.0 literal (`diagnostic.js:144`). */
export const DIAG_BEAT_MS = 750;

/** The probe cap when the service names none. */
export const DIAG_DEFAULT_CAP = 40;

/** The line a failed start leaves on the intro card. */
export const DIAG_START_FAILED = 'The placement did not start. Try again in a moment.';

/** The promise the learner reads on probe 1 (DIAG-nosol). */
export const DIAG_NO_SOLUTIONS_NOTE =
  'No solutions are shown during placement — just answer as best you can.';

/** The three ground rules, in order (P3). */
const GROUND_RULES: readonly { head: string; body: string }[] = [
  {
    head: 'Don’t guess — skip instead.',
    body: 'If you cannot see how to start within a couple of minutes, press “Skip — I don’t'
      + ' know”. A lucky guess places you too high and gets you over-challenged; an honest'
      + ' skip places you a little lower.',
  },
  {
    head: 'No external resources.',
    body: 'No notes, no textbooks, and no looking things up (a calculator only if the problem'
      + ' itself calls for one). This measures what you recall, not what you can find.',
  },
  {
    head: 'Answer honestly.',
    body: 'This is not a test you can fail — it only finds the right starting point.'
      + ' Overstating or understating what you know wastes your own time later.',
  },
];

const topicName = (t: DiagProbe['topic']): string =>
  (t && typeof t === 'object' ? (t.name ?? t.id) : t) ?? 'Placement';

export interface DiagnosticProps {
  /** The three placement calls. See `api/diag.ts` for why this is not on `ApiClient`. */
  diag: DiagnosticApi;
  /** Demo mode. A 401 then keeps the learner on the screen. */
  demo?: boolean;
  onUnauthorized: () => void;
  /** Leave the placement. It resumes at this probe next time. */
  onExit: () => void;
}

export function Diagnostic({ diag, demo = false, onUnauthorized, onExit }: DiagnosticProps) {
  const life = useLifetime();
  const call = useCall({ demo, onUnauthorized });
  const [phase, gate] = usePhase<Phase>('intro');

  const [probe, setProbe] = useState<DiagProbe | null>(null);
  const [qNum, setQNum] = useState(1);
  const [cap, setCap] = useState(DIAG_DEFAULT_CAP);
  const [result, setResult] = useState<{ res: DiagAnswerResponse; skipped: boolean } | null>(null);
  const [summary, setSummary] = useState<DiagFinishResponse | null>(null);
  const [startFailed, setStartFailed] = useState(false);
  // Tells the two `loading` moments apart. `qNum` cannot: a placement whose FIRST probe is
  // also its last reaches the commit with `qNum` still 1.
  const [finishing, setFinishing] = useState(false);

  const answerRef = useRef<AnswerFieldHandle>(null);
  const introRef = useRef<HTMLDivElement>(null);
  const homeRef = useRef<HTMLButtonElement>(null);
  // The live probe, read SYNCHRONOUSLY by a submit and by a Retry — neither can wait for a
  // render, and a Retry arrives renders after the closure that armed it. Every `setProbe`
  // site assigns this on the same line.
  const probeRef = useRef<DiagProbe | null>(null);

  // R15: the briefing CARD holds the focus, not the CTA.
  useEffect(() => { if (phase === 'intro') introRef.current?.focus(); }, [phase]);
  useEffect(() => { if (phase === 'ready') answerRef.current?.focus(); }, [phase, probe]);
  useEffect(() => { if (phase === 'done') homeRef.current?.focus(); }, [phase]);

  const finish = useCallback(() => {
    gate.enter('loading');
    // Clear the probe, or the loading branch below (guarded on `!probe`) never runs and the
    // learner sits on the answered question — with its tick and a live "Save & exit" —
    // through a placement commit that takes seconds.
    setProbe(null);
    probeRef.current = null;
    setResult(null);
    setFinishing(true);
    void call(() => diag.diagFinish(), (s) => {
      if (life.alive()) { setSummary(s); gate.enter('done'); }
    }).then((s) => {
      // A failed commit still ends the screen: the summary is a receipt, not the record.
      if (!s && life.alive()) { setSummary(null); gate.enter('done'); }
    });
  }, [call, diag, gate, life]);

  // `finish` is read through a ref so it stays OUT of the beat effect's dependency list. It
  // is a `useCallback` keyed on the transport, and a caller that rebuilds that object each
  // render would re-arm the beat on every re-render.
  const finishRef = useRef(finish);
  useEffect(() => { finishRef.current = finish; }, [finish]);

  const begin = useCallback(() => {
    // A repeated Enter or click cannot start two placements.
    if (!gate.tryEnter('intro', 'loading')) return;
    setStartFailed(false);
    void call(() => diag.diagStart(), (s) => {
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
  }, [call, diag, gate, life]);

  const send = useCallback((answer: string, skipped: boolean) => {
    // THE REF, never the render value: a Retry re-enters this closure renders later, and a
    // captured probe is the stale one by then.
    const current = probeRef.current;
    if (!current) return;
    // THE gate, for every entry point — Submit, Skip, and the Enter key, which bypasses the
    // disabled button entirely. Synchronous, before the first await.
    if (!gate.tryEnter('ready', 'submitting')) return;

    void call(
      () => diag.diagAnswer({ problem_id: current.problem_id, answer }),
      (res) => {
        if (!life.alive()) return;
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
          && probeRef.current?.problem_id === current.problem_id
          && gate.tryEnter('ready', 'submitting'),
        // A failed answer returns the probe to the learner — the first attempt and every
        // retried one alike. Without this the view sits at `submitting` with every control
        // disabled and no way on.
        onFail: () => { if (life.alive() && gate.is('submitting')) gate.enter('ready'); },
      },
    );
  }, [call, diag, gate, life]);

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
    if (phase !== 'feedback' || !result) return undefined;
    const next = result.res.next_probe;
    const isProbe = !!next && 'problem_id' in next && !!next.problem_id;

    const id = life.setTimeout(() => {
      if (isProbe) {
        setProbe(next as DiagProbe);
        probeRef.current = next as DiagProbe;
        setResult(null);
        setQNum((n) => n + 1);
        answerRef.current?.clear();
        gate.enter('ready');
      } else {
        finishRef.current();
      }
    }, DIAG_BEAT_MS);
    return () => life.clearTimer(id);
  }, [phase, result, life, gate]);

  const submitTyped = useCallback(() => {
    const answer = answerRef.current?.value() ?? '';
    // A blank Submit only refocuses. The honest way past a probe is Skip.
    if (!answer) { answerRef.current?.focus(); return; }
    send(answer, false);
  }, [send]);

  if (phase === 'intro') {
    return (
      <section className="view-diagnostic">
        {/* `tabindex="-1"` so the card holds focus without being interactive (R15). */}
        <div className="card intro-card" tabIndex={-1} ref={introRef}>
          <h2>Before we start</h2>
          <p className="muted">
            This short placement finds where you should start. It takes a few minutes, and
            there is no way to fail it. Three ground rules keep it accurate:
          </p>
          <ul className="intro-rules">
            {GROUND_RULES.map((rule) => (
              <li key={rule.head}>
                <strong>{rule.head}</strong>
                <span>{` ${rule.body}`}</span>
              </li>
            ))}
          </ul>
          {startFailed ? <p className="intro-error">{DIAG_START_FAILED}</p> : null}
          <div className="actions">
            <button type="button" className="btn btn-primary" onClick={begin}>
              Begin placement
            </button>
            <button type="button" className="btn btn-ghost" onClick={onExit}>
              Not now
            </button>
          </div>
        </div>
      </section>
    );
  }

  if (phase === 'done') {
    if (!summary) {
      return (
        <section className="view-diagnostic">
          <div className="empty">
            <p>Placement finished.</p>
            <button ref={homeRef} type="button" className="btn btn-primary" onClick={onExit}>
              Back to dashboard
            </button>
          </div>
        </section>
      );
    }

    const frontier = summary.frontier ?? [];
    return (
      <section className="view-diagnostic">
        <div className="card summary-card">
          <h2>Placement complete</h2>
          <div className="stat-grid">
            <Stat value={String(summary.placed?.length ?? 0)} label="topics placed" className="accent" />
            <Stat value={String(summary.conditional?.length ?? 0)} label="conditional" />
            <Stat value={String(frontier.length)} label="frontier topics" />
          </div>
          {frontier.length ? (
            <div className="frontier-block">
              <div className="solution-label">Start here</div>
              <ul className="frontier-list">{frontier.map((t) => <li key={t}>{t}</li>)}</ul>
            </div>
          ) : null}
          <button ref={homeRef} type="button" className="btn btn-primary" onClick={onExit}>
            Back to dashboard
          </button>
        </div>
      </section>
    );
  }

  if (!probe) {
    return (
      <section className="view-diagnostic">
        <LoadingBlock label={finishing ? 'Working out your placement…' : 'Starting the placement…'} />
      </section>
    );
  }

  const locked = phase !== 'ready';

  return (
    // Keyed on the probe, so each question gets a FRESH input subtree. Without it React
    // reuses the nodes and the previous probe's answer pre-fills the next one.
    <section className="view-diagnostic" key={probe.problem_id}>
      <div className="task-header">
        <div className="task-meta">
          <Chip className="chip-accent">placement</Chip>
          <span className="topic-name">{topicName(probe.topic)}</span>
        </div>
        <div className="task-right">
          <span className="progress-count">{`Question ${qNum} of up to ${cap}`}</span>
        </div>
      </div>

      <div className="progress-bar">
        <div className="progress-fill" style={{ width: `${Math.min(100, (qNum / cap) * 100)}%` }} />
      </div>

      <div className="card problem-card">
        <MathBlock>{probe.text}</MathBlock>

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

        {result ? (
          <div
            className={`feedback feedback-${
              result.res.correct ? 'correct' : result.skipped ? 'skip' : 'incorrect'
            }`}
          >
            <span className="feedback-mark">{result.res.correct ? <Tick /> : <Cross />}</span>
            <span className="feedback-title">
              {result.res.correct ? 'Correct' : result.skipped ? 'Skipped' : 'Not this time'}
            </span>
            {/* DIAG-nosol: a mark and a title. NOTHING else. */}
          </div>
        ) : null}

        {qNum === 1 ? <p className="muted small">{DIAG_NO_SOLUTIONS_NOTE}</p> : null}
      </div>
    </section>
  );
}
