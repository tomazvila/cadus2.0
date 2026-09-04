/**
 * The screens of the placement that hold no live probe: the briefing, the summary, and the
 * feedback mark under an answered probe.
 *
 * Each one is pure render over the props the placement hands it, so `Diagnostic.tsx` keeps
 * the phase machine and the beat, and none of the markup around them.
 */
import { Cross, Stat, Tick } from '@/components/primitives';
import type { DiagAnswerResponse, DiagFinishResponse, DiagProbe } from '@/api/diag';

/** The line a failed start leaves on the intro card. */
export const DIAG_START_FAILED = 'The placement did not start. Try again in a moment.';

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

/** The name a probe shows: the record's name or id in 2.0, the bare string in 1.0. */
export function topicName(t: DiagProbe['topic']): string {
  return (t && typeof t === 'object' ? (t.name ?? t.id) : t) ?? 'Placement';
}

export interface IntroCardProps {
  /** The card holds the focus (R15), so the placement hands it the ref. */
  cardRef: React.Ref<HTMLDivElement>;
  startFailed: boolean;
  onBegin: () => void;
  onExit: () => void;
}

/** The briefing: three ground rules BEFORE probe 1 (P3), on a card that holds focus (R15). */
export function IntroCard({ cardRef, startFailed, onBegin, onExit }: IntroCardProps) {
  return (
    <section className="view-diagnostic">
      {/* `tabindex="-1"` so the card holds focus without being interactive (R15). */}
      <div className="card intro-card" tabIndex={-1} ref={cardRef}>
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
          <button type="button" className="btn btn-primary" onClick={onBegin}>
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

export interface PlacementDoneProps {
  /** The commit's reply, or null when the commit failed: the summary is a receipt. */
  summary: DiagFinishResponse | null;
  homeRef: React.Ref<HTMLButtonElement>;
  onExit: () => void;
}

/** The end of the placement: the counts and the frontier, or the bare fact that it ended. */
export function PlacementDone({ summary, homeRef, onExit }: PlacementDoneProps) {
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

  const { frontier } = summary;
  return (
    <section className="view-diagnostic">
      <div className="card summary-card">
        <h2>Placement complete</h2>
        <div className="stat-grid">
          <Stat value={String(summary.placed.length)} label="topics placed" className="accent" />
          <Stat value={String(summary.conditional.length)} label="conditional" />
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

export interface ProbeResult {
  res: DiagAnswerResponse;
  skipped: boolean;
}

/** The three verdicts a probe shows: a class suffix and a title, and nothing else. */
function verdictOf({ res, skipped }: ProbeResult): { kind: string; title: string } {
  if (res.correct) return { kind: 'correct', title: 'Correct' };
  if (skipped) return { kind: 'skip', title: 'Skipped' };
  return { kind: 'incorrect', title: 'Not this time' };
}

/** DIAG-nosol: a mark and a title. NOTHING else — no solution and no expected answer. */
export function ProbeFeedback({ result }: { result: ProbeResult }) {
  const { kind, title } = verdictOf(result);
  return (
    <div className={`feedback feedback-${kind}`}>
      <span className="feedback-mark">{result.res.correct ? <Tick /> : <Cross />}</span>
      <span className="feedback-title">{title}</span>
    </div>
  );
}
