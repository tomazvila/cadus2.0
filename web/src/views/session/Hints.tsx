/**
 * The hint panel and the hint button of the study loop. `Session.tsx` renders the two.
 */
import { MathBlock } from '@/components/MathBlock';
import type { PlanTask } from '@/api/types';

/**
 * The hints already given, and the review escalations (H-3, ISSUES.md).
 *
 * A review whose knowledge point holds NO approved ladder offers the same way
 * out the three-hint escalation does, up front: the ladder does not exist, so
 * `requestHint` would only buy a `409 no_hint_ladder`.
 */
export function HintPanel({ hints, referenceLesson, hintsAvailable, taskType, onRead }: {
  hints: string[];
  referenceLesson: { topic: string; name: string } | null;
  /** Open the lesson text of a topic. Absent, the note shows no button. */
  onRead?: (topic: string) => void;
  hintsAvailable: boolean;
  taskType: PlanTask['task_type'];
}) {
  return (
    <div className="hint-list">
      {hints.map((h, i) => (
        <div key={`${i}:${h}`} className="hint">
          <strong>{`Hint ${i + 1}: `}</strong>
          <MathBlock className="hint-text">{String(h)}</MathBlock>
        </div>
      ))}
      {referenceLesson ? (
        <div className="reference-lesson">
          {`Still stuck? Read the lesson “${referenceLesson.name}” again, then answer as well as you can.`}
          {onRead ? (
            <button type="button" className="btn btn-ghost" onClick={() => { onRead(referenceLesson.topic); }}>
              {`Open the lesson “${referenceLesson.name}”`}
            </button>
          ) : null}
        </div>
      ) : null}
      {!hintsAvailable && taskType === 'review' ? (
        <div className="reference-lesson">
          This problem has no hints yet. If you are stuck, read the lesson again, then answer
          as well as you can.
        </div>
      ) : null}
    </div>
  );
}

/**
 * The hint affordance (H-3, ISSUES.md).
 *
 * An affordance the service can only refuse is not an affordance: the flag
 * comes from the serve, so a knowledge point with no approved ladder hides the
 * button instead of spending the click on `409 no_hint_ladder`.
 */
export function HintButton({ hidden, locked, onClick }: {
  hidden: boolean;
  locked: boolean;
  onClick: () => void;
}) {
  if (hidden) return null;
  return (
    <button type="button" className="btn" disabled={locked} onClick={onClick}>
      Hint
    </button>
  );
}
