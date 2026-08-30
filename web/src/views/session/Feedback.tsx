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
 */
import { Chip, Cross, Tick } from '@/components/primitives';
import { MathBlock } from '@/components/MathBlock';
import { signed } from '@/lib/format';
import type { AnswerResponse, ReworkResponse } from '@/api/types';

export interface FeedbackProps {
  res: AnswerResponse;
  /** True when another problem follows this one. */
  hasNext: boolean;
  onContinue: () => void;
  onEnd: () => void;
  continueRef?: React.Ref<HTMLButtonElement>;
  /**
   * The async diagnosis panel (S9), rendered LAST and above the actions.
   *
   * It is a slot rather than a field of `res`, because its content arrives seconds after
   * this panel paints. Anything above it would move down the page as it lands, and the
   * solution is the one thing the learner is reading at that moment.
   */
  children?: React.ReactNode;
}

export function Feedback({
  res,
  hasNext,
  onContinue,
  onEnd,
  continueRef,
  children,
}: FeedbackProps) {
  return (
    <div className={`feedback feedback-${res.correct ? 'correct' : 'incorrect'}`}>
      <div className="feedback-head">
        <span className="feedback-mark">{res.correct ? <Tick /> : <Cross />}</span>
        <span className="feedback-title">{res.correct ? 'Correct' : 'Not quite'}</span>
        <Chip className="chip-quality">{String(res.work_quality).replace(/_/g, ' ')}</Chip>
        {res.xp != null ? <Chip className="chip-xp">{`${signed(res.xp)} XP`}</Chip> : null}
      </div>

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

      {children}

      <div className="actions">
        <button ref={continueRef} type="button" className="btn btn-primary" onClick={onContinue}>
          {hasNext ? 'Next problem →' : 'Continue →'}
        </button>
        {/* The way out from here is always safe: the attempt already stands, and an
            unfinished task is re-served next time. */}
        <button type="button" className="btn btn-ghost" onClick={onEnd}>End session</button>
      </div>
    </div>
  );
}

/**
 * DD-3/P1 — the re-solve panel.
 *
 * An assisted correct answer is NOT recorded. The service stashes it, keeps the problem
 * live, and waits for the unaided re-solve; the SAME submit sends it. So this panel reveals
 * the solution to study, arms no auto-advance, and is not terminal — the view returns to
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
