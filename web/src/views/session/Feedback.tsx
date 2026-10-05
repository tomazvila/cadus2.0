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
import { Chip, Cross, Question, Tick } from '@/components/primitives';
import { MathBlock } from '@/components/MathBlock';
import { signed } from '@/lib/format';
import { isUngraded } from '@/api/types';
import type { AnswerResponse, ApiClient, AttemptOutcome, ReworkResponse } from '@/api/types';
import type { Lifetime } from '@/hooks/useLifetime';
import { PROOF_TITLE, ProofResult, useProofGrading, useSeen, type ProofState } from './ProofGrading';

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

export function Feedback({
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
  const head = headOf(res, ungraded ? proof ?? null : null);
  return (
    <div className={`feedback feedback-${head.mood}`}>
      <div className="feedback-head">
        <span className="feedback-mark">{head.mark}</span>
        <span className="feedback-title">{head.title}</span>
        {/* An ungraded attempt earned no tier and no XP, so neither chip appears. */}
        {ungraded ? null : (
          <Chip className="chip-quality">{String(res.work_quality).replace(/_/g, ' ')}</Chip>
        )}
        {res.xp != null ? <Chip className="chip-xp">{`${signed(res.xp)} XP`}</Chip> : null}
      </div>

      {revealTopic ? <p className="feedback-topic muted">{`Topic: ${revealTopic}`}</p> : null}
      <TaskStatusLines res={res} />
      {head.proof ? <ProofResult state={head.proof} /> : <Reason reason={res.reason} />}

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
      {res.equivalence?.status === 'pending' ? (
        <p className="feedback-reason muted" role="status">Your answer is being checked. The verdict on this page updates when the check lands.</p>
      ) : null}
      {res.equivalence_reason ? <p className="feedback-reason muted">Checked: {res.equivalence_reason}</p> : null}

      {res.re_solve ? <p className="re-solve muted">{res.re_solve}</p> : null}

      {res.remediation.length ? (
        <div>
          <div className="solution-label">Follow-up</div>
          <ul className="remediation">
            {res.remediation.map((r, i) => (
              <li key={`${r.kind}-${i}`}>{`${r.kind}: ${r.targets.join(', ')}`}</li>
            ))}
          </ul>
        </div>
      ) : null}

      {/* D-F4: no diagnosis fires on an ungraded attempt, so its slot stays empty. */}
      {ungraded ? null : children}

      <div className="actions">
        <button ref={continueRef} type="button" className="btn btn-primary" onClick={onContinue}>
          {res.feedback_blocked ? 'Check for fresh practice →' : res.feedback_practice ? 'Done studying — try a fresh problem →' : hasNext ? 'Next problem →' : 'Continue →'}
        </button>
        {/* The way out from here is always safe: the attempt already stands, and an
            unfinished task is re-served next time. */}
        <button type="button" className="btn btn-ghost" onClick={onEnd}>End session</button>
      </div>
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
      {res.task_status === 'task_failed' && res.correct ? <p>This practice answer is correct. The original assessment still needs more practice.</p> : null}
      {res.task_status === 'task_passed' ? (
        <p role="status">Task complete. The next task is up when you continue.</p>
      ) : null}
      {res.task_status === 'task_inconclusive' ? (
        <p className="feedback-reason">This review needs confirmation. A fresh question will check each uncertain skill.</p>
      ) : null}
    </>
  );
}

/** The service's own reason for a reply with no verdict. */
function Reason({ reason }: { reason: string | undefined }) {
  return reason ? <p className="feedback-reason muted">{reason}</p> : null;
}

/** The feedback panel of one grade reply, following its proof grading when it has one. */
export function ProofAwareFeedback({ api, life, ...props }: FeedbackProps & { api: ApiClient; life: Lifetime }) {
  const proof = useProofGrading(api, life, props.res.proof_grading);
  useSeen(api, props.res.proof_grading, proof);
  return <Feedback {...props} proof={proof} />;
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
      <button ref={continueRef} type="button" className="btn btn-primary" onClick={onContinue}>Continue with updated progress</button>
      <button type="button" className="btn btn-ghost" onClick={onEnd}>End session</button>
    </div>
  </div>;
}
