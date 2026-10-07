/**
 * The two panels a graded answer can produce.
 *
 * They render a solution, which is why NEITHER is shared with the placement screen (S10).
 * That screen uses the same class names and must never reveal a solution or an expected
 * answer. A shared `FeedbackPanel` is the obvious refactor, and it would leak the answer
 * into placement.
 *
 * Hard Rule 2: `correct` is mathematical correctness only. Partial credit lives in
 * `work_quality`, and this panel shows both and derives neither from the other. Hard Rule
 * 3: `remediation` is rendered as the core sent it, in the core's order.
 *
 * D-F2 adds the third state, "Not marked". The checker reached no verdict, so the panel
 * says so and names the service's own reason. It carries no red, no "wrong" wording, no
 * solution, and no AI explanation, because none of those is true of an attempt nobody
 * graded. The learner reads the reason and takes the next problem.
 *
 * Amendment K point 6: an ungraded WRITTEN PROOF is graded in the background. Its panel
 * reads "Checking your proof…" while the worker runs, then "Proof accepted" or "Needs
 * revision" with the grader's feedback, its checks, and the solution (`ProofGrading.tsx`).
 */
import { useEffect, useState } from 'react';
import { Chip, Cross, Question, Tick } from '@/components/primitives';
import { MathBlock } from '@/components/MathBlock';
import { signed } from '@/lib/format';
import { isUngraded } from '@/api/types';
import type { AnswerResponse, ApiClient, AttemptOutcome, EquivalencePoll, EquivalenceStep, Remediation, ReworkResponse } from '@/api/types';
import type { Lifetime } from '@/hooks/useLifetime';
import { PROOF_TITLE, ProofResult, useProofGrading, useSeen, type ProofState } from './ProofGrading';

/** The line under the actions: what "Stop for now" does. */
const STOP_NOTE = 'Your work is saved. An unfinished lesson comes back next time.';

/** The panel mood of each outcome. `ungraded` is neutral: it is not a miss (D-F2). */
const MOOD: Record<AttemptOutcome, string> = {
  correct: 'correct',
  incorrect: 'incorrect',
  ungraded: 'ungraded',
};

/** The heading of each outcome. No wording here calls an ungraded answer wrong. */
const TITLE: Record<AttemptOutcome, string> = {
  correct: 'Correct',
  incorrect: 'Not quite',
  ungraded: 'Not marked',
};

/** The glyph of each outcome. */
const MARK: Record<AttemptOutcome, React.ReactNode> = {
  correct: <Tick />,
  incorrect: <Cross />,
  ungraded: <Question />,
};

export interface FeedbackProps {
  res: AnswerResponse;
  /** True when another problem follows this one. */
  hasNext: boolean;
  onContinue: () => void;
  onEnd: () => void;
  onRefresh?: () => void;
  continueRef?: React.Ref<HTMLButtonElement> | undefined;
  /**
   * The async diagnosis panel (S9), rendered LAST and above the actions.
   *
   * It is a slot rather than a field of `res`, because its content arrives seconds after
   * this panel paints. Anything above it would move down the page as it lands, and the
   * solution is the one thing the learner is reading at that moment.
   */
  children?: React.ReactNode;
  /** The background grading of a written proof, when the reply carries one. */
  proof?: ProofState | null;
  /** The topic a review question came from, named only once the answer is graded. */
  revealTopic?: string | null;
}

export function Feedback(props: FeedbackProps) {
  const status = props.res.equivalence?.status;
  if (status === 'pending') return <CheckingFeedback continueRef={props.continueRef} steps={props.res.equivalence?.steps} />;
  if (status === 'failed') return <UnfinishedFeedback {...props} />;
  return <GradedFeedback {...props} />;
}

function GradedFeedback({
  res,
  hasNext,
  onContinue,
  onEnd,
  onRefresh,
  continueRef,
  children,
  proof,
  revealTopic,
}: FeedbackProps) {
  if (res.report_corrected) return <CorrectedFeedback res={res} onContinue={onRefresh ?? onContinue}
    onEnd={onEnd} continueRef={continueRef} />;
  const ungraded = isUngraded(res);
  const accepted = res.equivalence?.status === 'accepted';
  const rows = remediationLines(res.remediation);
  const head = headOf(res, ungraded ? proof ?? null : null);
  return (
    <div className={`feedback feedback-${head.mood}`}>
      <div className="feedback-head">
        <span className="feedback-mark">{head.mark}</span>
        <span className="feedback-title">{head.title}</span>
        {/* An ungraded attempt earned no tier and no XP, so neither chip appears. */}
        <HeadChips res={res} hidden={ungraded} accepted={accepted} />
      </div>

      {revealTopic ? <p className="feedback-topic muted">{`Topic: ${revealTopic}`}</p> : null}
      <AcceptedLines res={res} />
      <TaskStatusLines res={res} />
      {head.proof ? <ProofResult state={head.proof} /> : <Reason reason={res.reason} shown={notationHint(res) === null} />}
      <NotationHint text={notationHint(res)} />

      {/* Verbatim, never re-interpreted: the checker owns the vocabulary (trap T3). */}
      {res.error_tags.length ? (
        <div className="error-tags">
          {res.error_tags.map((tag) => <Chip key={tag} className="chip-tag">{tag}</Chip>)}
        </div>
      ) : null}

      {res.solution ? (
        <div className="solution">
          <div className="solution-label">Solution</div>
          <MathBlock className="solution-text">{String(res.solution)}</MathBlock>
        </div>
      ) : null}

      {/* Amendment K (note 114): a refused answer is also checked in the
          background; the model's one-line reason rides beside the solution. */}
      {res.equivalence_reason ? <p className="feedback-reason muted">Checked: {res.equivalence_reason}</p> : null}

      {res.re_solve ? <p className="re-solve muted">{res.re_solve}</p> : null}

      {rows.length ? (
        <div>
          <div className="solution-label">Follow-up</div>
          <ul className="remediation">
            {rows.map((text, i) => <li key={`${text}-${i}`}>{text}</li>)}
          </ul>
        </div>
      ) : null}

      {/* D-F4: no diagnosis fires on an ungraded attempt, so its slot stays empty. */}
      {ungraded || res.equivalence?.status === 'accepted' ? null : children}

      <div className="actions">
        <button ref={continueRef} type="button" className="btn btn-primary" onClick={onContinue}>
          {continueLabel(res, hasNext)}
        </button>
        {/* The way out from here is always safe: the attempt already stands, and an
            unfinished task is re-served next time. */}
        <button type="button" className="btn btn-ghost" onClick={onEnd}>Stop for now</button>
      </div>
      <p className="muted small">{STOP_NOTE}</p>
    </div>
  );
}

/** The label of the forward button. A corrected attempt always reads "Continue". */
function continueLabel(res: AnswerResponse, hasNext: boolean): string {
  if (res.equivalence?.status === 'accepted') return 'Continue →';
  if (res.feedback_blocked) return 'Get a fresh problem →';
  if (res.feedback_practice) return 'Try a fresh problem →';
  return hasNext ? 'Next problem →' : 'Continue →';
}

/** The lines of an answer the background check accepted. */
function AcceptedLines({ res }: { res: AnswerResponse }) {
  if (res.equivalence?.status !== 'accepted') return null;
  const form = res.equivalence.accepted_form;
  return (
    <>
      <p role="status">Correct. Your progress is updated.</p>
      {form ? <p role="status">{`Accepted. Shorter form: \`${form}\`.`}</p> : null}
    </>
  );
}

/** The quality and XP chips. An ungraded or re-graded reply earned no figures of its own. */
function HeadChips({ res, hidden, accepted }: { res: AnswerResponse; hidden: boolean; accepted: boolean }) {
  if (hidden) return null;
  // A corrected attempt shows the XP it earned and no quality chip left from the first pass.
  if (accepted) return res.xp != null ? <Chip className="chip-xp">{`${signed(res.xp)} XP`}</Chip> : null;
  return (
    <>
      <Chip className="chip-quality">{String(res.work_quality).replace(/_/g, ' ')}</Chip>
      {res.xp != null ? <Chip className="chip-xp">{`${signed(res.xp)} XP`}</Chip> : null}
    </>
  );
}

/** The learner text of each remediation kind the service can send. */
const REMEDIATION_TEXT: Record<string, string> = {
  lesson_fail: 'This lesson comes back next time',
  repeat_fail: 'Practice this skill again',
  quiz_miss: 'Review what the quiz missed',
  confirm_failed: 'A fresh question will check this skill again',
};

/** The follow-up lines of one reply. A row with no text and no targets shows nothing. */
export function remediationLines(rows: Remediation[]): string[] {
  const lines: string[] = [];
  for (const r of rows) {
    const text = REMEDIATION_TEXT[r.kind] ?? (r.kind.startsWith('review_confirmation') ? 'A fresh question will confirm this skill' : '');
    const targets = r.targets.join(', ');
    const line = text && targets ? `${text}: ${targets}` : text || targets;
    if (line) lines.push(line);
  }
  return lines;
}

/** The poll interval of a background equivalence check, in milliseconds. */
export const EQUIVALENCE_POLL_MS = 3000;

/**
 * The reply once the background equivalence check lands.
 *
 * While the check is pending the reply stands as sent and the panel is neutral. The poll
 * then replaces the whole reply with its final form, so no provisional field survives.
 */
export function settleEquivalence(res: AnswerResponse, poll: EquivalencePoll): AnswerResponse {
  const v = poll.verdict;
  const equivalence = {
    ...(res.equivalence ?? {}),
    status: v.status,
    ...(v.steps ? { steps: v.steps } : {}),
    ...(v.accepted_form ? { accepted_form: v.accepted_form } : {}),
  };
  if (v.status === 'pending') return { ...res, equivalence };
  if (v.status === 'accepted') {
    const rest = { ...res };
    delete rest.re_solve;
    delete rest.equivalence_reason;
    delete rest.xp;
    delete rest.feedback_blocked;
    delete rest.feedback_practice;
    return { ...rest, ...(v.xp != null ? { xp: v.xp } : {}), outcome: 'correct', correct: true, error_tags: [], remediation: [], equivalence };
  }
  return { ...res, equivalence, ...(v.status === 'refused' && v.reason ? { equivalence_reason: v.reason } : {}) };
}

function useEquivalence(api: ApiClient, life: Lifetime, res: AnswerResponse): AnswerResponse {
  const id = res.equivalence?.status === 'pending' ? res.equivalence.id ?? null : null;
  const [landed, setLanded] = useState<{ id: string; res: AnswerResponse } | null>(null);
  useEffect(() => {
    if (id === null) return undefined;
    let done = false;
    let interval = 0;
    const poll = async () => {
      let job: EquivalencePoll;
      try {
        job = await api.getEquivalence(id);
      } catch {
        return;
      }
      if (done || !life.alive()) return;
      if (job.verdict.status === 'pending') {
        if (job.verdict.steps) setLanded({ id, res: settleEquivalence(res, job) });
        return;
      }
      done = true;
      life.clearTimer(interval);
      setLanded({ id, res: settleEquivalence(res, job) });
    };
    void poll();
    interval = life.setInterval(() => { void poll(); }, EQUIVALENCE_POLL_MS);
    return () => { done = true; life.clearTimer(interval); };
  }, [api, life, id, res]);
  // The poll reply owns the panel from its first landing: pending with steps, then final.
  return id !== null && landed?.id === id ? landed.res : res;
}

/** The neutral panel of an answer whose background check has not landed. */
function CheckingFeedback({ continueRef, steps }: Pick<FeedbackProps, 'continueRef'> & { steps: EquivalenceStep[] | undefined }) {
  return (
    <div className="feedback feedback-pending" role="status">
      <div className="feedback-head"><span className="feedback-title">Checking your answer…</span></div>
      {steps?.length ? (
        <ol className="equivalence-steps">
          {steps.map((step, i) => <li key={`${i}-${step.text}`}>{step.text}</li>)}
        </ol>
      ) : null}
      <div className="actions">
        {/* aria-disabled, not disabled: a disabled button cannot take the focus the view gives it. */}
        <button ref={continueRef} type="button" className="btn btn-primary" aria-disabled="true" onClick={(e) => e.preventDefault()}>
          Waiting for the check…
        </button>
      </div>
    </div>
  );
}

/** The neutral panel of an answer whose background check ended without a verdict. */
function UnfinishedFeedback({ res, hasNext, onContinue, onEnd, continueRef }: FeedbackProps) {
  return (
    <div className="feedback feedback-pending">
      <div className="feedback-head"><span className="feedback-title">Not marked</span></div>
      <p role="status">The check did not finish. This answer counts as pending, not wrong.</p>
      {res.equivalence?.steps?.length ? (
        <ol className="equivalence-steps">
          {res.equivalence.steps.map((step, i) => <li key={`${i}-${step.text}`}>{step.text}</li>)}
        </ol>
      ) : null}
      <div className="actions">
        <button ref={continueRef} type="button" className="btn btn-primary" onClick={onContinue}>
          {hasNext ? 'Next problem →' : 'Continue →'}
        </button>
        <button type="button" className="btn btn-ghost" onClick={onEnd}>Stop for now</button>
      </div>
      <p className="muted small">{STOP_NOTE}</p>
    </div>
  );
}

/**
 * The mood, glyph and heading of one reply.
 *
 * A written proof under background grading speaks for itself: its state names the heading,
 * and a pass takes the correct mood. Every other reply reads its outcome.
 */
function headOf(res: AnswerResponse, proof: ProofState | null) {
  if (!proof) return { mood: MOOD[res.outcome], mark: MARK[res.outcome], title: TITLE[res.outcome], proof };
  const passed = proof.status === 'pass';
  return {
    mood: passed ? MOOD.correct : MOOD[res.outcome],
    mark: passed ? MARK.correct : MARK[res.outcome],
    title: PROOF_TITLE[proof.status],
    proof,
  };
}

/** The lines that say where the task stands after this answer. */
function TaskStatusLines({ res }: { res: AnswerResponse }) {
  return (
    <>
      {res.feedback_blocked ? <p role="status">Fresh practice is unavailable for this skill. Your answer is saved.</p> : null}
      {res.task_status === 'task_failed' && res.correct ? <p>Correct. Keep practising this skill: the lesson passes after two right in a row.</p> : null}
      {res.task_status === 'task_failed' && !res.correct && res.feedback_practice ? (
        <p role="status">The lesson did not pass this time. It comes back next time. Practice a fresh problem now.</p>
      ) : null}
      {res.task_status === 'task_passed' ? (
        <p role="status">Lesson passed. The next one starts when you continue.</p>
      ) : null}
      {res.task_status === 'task_inconclusive' ? (
        <p className="feedback-reason">This review needs confirmation. A fresh question will check each uncertain skill.</p>
      ) : null}
    </>
  );
}

/**
 * The notation hint of a wrong-form reply, or null.
 *
 * The grade reply sets `notation` (the hint text) or tags the miss `notation` and sends the hint
 * in `reason`. The learner reads it as the lesson of the reply, not as a muted side line.
 */
export function notationHint(res: AnswerResponse): string | null {
  if (typeof res.notation === 'string' && res.notation !== '') return res.notation;
  if (res.notation === true || res.error_tags.includes('notation')) return res.reason ?? null;
  return null;
}

function NotationHint({ text }: { text: string | null }) {
  return text ? <p className="feedback-notation" role="status">{text}</p> : null;
}

/** The service's own reason for a reply with no verdict. */
function Reason({ reason, shown = true }: { reason: string | undefined; shown?: boolean }) {
  return reason && shown ? <p className="feedback-reason muted">{reason}</p> : null;
}

/** The feedback panel of one grade reply, following its proof grading when it has one. */
export function ProofAwareFeedback({ api, life, ...props }: FeedbackProps & { api: ApiClient; life: Lifetime }) {
  const proof = useProofGrading(api, life, props.res.proof_grading);
  useSeen(api, props.res.proof_grading, proof);
  const res = useEquivalence(api, life, props.res);
  // The pending button takes no click, so the focus returns to the live one when it lands.
  const wasPending = props.res.equivalence?.status === 'pending' && res.equivalence?.status !== 'pending';
  useEffect(() => {
    if (wasPending && props.continueRef && 'current' in props.continueRef) props.continueRef.current?.focus();
  }, [wasPending, props.continueRef]);
  return <Feedback {...props} res={res} proof={proof} />;
}

/**
 * DD-3/P1 — the re-solve panel.
 *
 * An assisted correct answer is NOT recorded. The service stashes it, keeps the problem
 * live, and waits for the unaided re-solve; the SAME submit sends it. So this panel reveals
 * the solution to study, arms no auto-advance (the verdict always waits for the learner),
 * and is not terminal — the view returns to
 * `ready` behind it.
 *
 * The instruction is the SERVICE's `re_solve` string, not a sentence written here. One
 * wording, one owner.
 */
export function Rework({ res }: { res: ReworkResponse }) {
  return (
    <div className="feedback feedback-rework">
      <div className="feedback-head">
        <span className="feedback-title">Make it stick</span>
      </div>
      <MathBlock className="feedback-text">{String(res.re_solve)}</MathBlock>
      <div className="solution">
        <div className="solution-label">Solution</div>
        <MathBlock className="solution-text">{String(res.solution ?? res.expected)}</MathBlock>
      </div>
    </div>
  );
}

function CorrectedFeedback({ res, onContinue, onEnd, continueRef }: Pick<FeedbackProps, 'res' | 'onContinue' | 'onEnd' | 'continueRef'>) {
  return <div className="feedback feedback-correct">
    <div className="feedback-head"><span className="feedback-mark"><Tick /></span><span className="feedback-title">Correct</span></div>
    <p role="status">Grade corrected after verification.</p>
    {res.solution ? <div className="solution"><div className="solution-label">Solution</div><MathBlock className="solution-text">{res.solution}</MathBlock></div> : null}
    <div className="actions">
      <button ref={continueRef} type="button" className="btn btn-primary" onClick={onContinue}>Continue →</button>
      <button type="button" className="btn btn-ghost" onClick={onEnd}>Stop for now</button>
    </div>
    <p className="muted small">{STOP_NOTE}</p>
  </div>;
}
