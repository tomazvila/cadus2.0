/**
 * The async diagnosis panel (A4).
 *
 * It sits UNDER the verdict, the solution and the follow-up, and above the actions. The
 * position is a correctness choice, not a taste one: this content arrives seconds after the
 * rest of the panel painted, and a late insertion above the solution moves the solution down
 * the page while the learner is reading it.
 *
 * WHAT IT NEVER DOES. It does not gate the verdict, it does not disable the Continue button,
 * and it does not delay the auto-advance. The learner can leave at any point; the job dies
 * with the view.
 *
 * `not_offered` renders NOTHING. The state is real — a correct answer, a blank one, or an
 * undecidable kind with no template diagnosis — and the honest rendering of "no explanation
 * is owed" is no panel, not an empty box that says so.
 *
 * The tags here are the DIAGNOSIS's tags, from `content_store` or from the worker's
 * vocabulary filter. The tags above are the checker's deterministic ones. Two sources, two
 * rows, and neither is derived from the other (trap T3).
 */
import { MathBlock } from '@/components/MathBlock';
import { Chip } from '@/components/primitives';
import type { DiagnosisField } from '@/api/types';
import { useDiagnosisJob, type DiagnosisStore } from './useDiagnosis';

/** The line under the panel heading while the worker is still writing. */
export const DIAGNOSIS_WAIT = 'Working out what went wrong…';

/** The 30 s rule, said to the learner. The verdict above it stands either way. */
export const DIAGNOSIS_FAILED =
  'No explanation arrived this time. Your result and the solution above still stand.';

export interface DiagnosisProps {
  store: DiagnosisStore;
  /** The `diagnosis` field of the grade reply this panel belongs to. */
  field: DiagnosisField;
}

export function Diagnosis({ store, field }: DiagnosisProps) {
  const state = useDiagnosisJob(store, field);
  if (!state) return null;

  return (
    // The live region is the OUTER node, and it exists from the first paint. A region
    // inserted together with its own text announces nothing in most screen readers.
    <div className="diagnosis" data-status={state.status} aria-live="polite">
      <div className="solution-label">What went wrong</div>

      {state.status === 'pending' ? (
        <p className="diagnosis-note muted">{DIAGNOSIS_WAIT}</p>
      ) : null}

      {state.status === 'failed' ? (
        <p className="diagnosis-note muted">{DIAGNOSIS_FAILED}</p>
      ) : null}

      {state.status === 'ready' ? (
        <>
          {state.error_tags.length ? (
            <div className="error-tags">
              {state.error_tags.map((tag) => (
                <Chip key={tag} className="chip-tag">{tag}</Chip>
              ))}
            </div>
          ) : null}
          {/* Model-authored prose, through the same escape-then-KaTeX path as a problem. */}
          <MathBlock className="diagnosis-prose">{String(state.prose)}</MathBlock>
        </>
      ) : null}
    </div>
  );
}
