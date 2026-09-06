/**
 * The integrated task of D-F10: ONE scenario, its steps, and one final answer.
 *
 * WHY IT IS ONE SCREEN. A multi-step task of 1.0 showed one unrelated question after
 * another. This screen shows the scenario, the quantities, the method choice, every
 * intermediate step and the final question TOGETHER, because the learner has to carry one
 * situation through all of them. A screen that revealed one step at a time would turn the
 * task back into the drill it replaces.
 *
 * TWO PANELS, NEVER ONE. The reply splits what the service decided from what the learner
 * claimed:
 *   * "Answers" holds the verdicts. The checker decided every one of them from the
 *     authored answer and its contract.
 *   * "Your reasoning" holds the learner's own words. The service records them and marks
 *     them neither right nor wrong, and this screen says so in those words. No prose is
 *     corrected anywhere in this product.
 *
 * HINTS FADE. A ladder is asked for one rung at a time, and the count of opened rungs
 * rides with the submission: a field answered after a hint is `assisted` in the reply, so
 * the independent-application evidence stays honest.
 *
 * NO ANSWER IS ON THIS SCREEN before the submit. `IntegratedProblem` has no answer field
 * to render (Hard Rule 1), and the interpretation arrives with the grade.
 */
import { useState } from 'react';
import { MathBlock } from '@/components/MathBlock';
import { Chip } from '@/components/primitives';
import type {
  IntegratedApi,
  IntegratedFieldGrade,
  IntegratedGrade,
  IntegratedProblem,
} from '@/api/types';

/** The field id of the final answer. It is the server's literal (`FINAL_FIELD_ID`). */
const FINAL = 'final';

export interface IntegratedProps {
  api: IntegratedApi;
  taskId: string;
  problem: IntegratedProblem;
  /** Called with the grade after the service returns it. */
  onGraded?: (grade: IntegratedGrade) => void;
}

/** The answer text and the opened-hint count of one field. */
interface FieldState {
  answer: string;
  hintsUsed: number;
  hint: string | null;
}

const emptyField: FieldState = { answer: '', hintsUsed: 0, hint: null };

/** The domain label, in words the learner reads. */
function domainLabel(domain: string): string {
  return domain.replace(/_/g, ' ');
}

/** The verdict word of one graded field. An ungraded field is NOT a wrong field. */
function verdictOf(grade: IntegratedFieldGrade | undefined): string {
  if (!grade) return '';
  if (grade.ungraded) return 'needs a human check';
  if (!grade.answered) return 'not answered';
  return grade.correct ? 'correct' : 'not correct';
}

/** One row of the verdict panel. */
function VerdictRow({ label, grade }: { label: string; grade: IntegratedFieldGrade | undefined }) {
  return (
    <li className="integrated-verdict">
      <span className="integrated-verdict-label">{label}</span>
      <span className="integrated-verdict-value">{verdictOf(grade)}</span>
      {grade?.assisted ? <span className="integrated-verdict-note">after a hint</span> : null}
    </li>
  );
}

export function Integrated({ api, taskId, problem, onGraded }: IntegratedProps) {
  const [fields, setFields] = useState<Record<string, FieldState>>({});
  const [method, setMethod] = useState<string | null>(null);
  const [reasoning, setReasoning] = useState('');
  const [grade, setGrade] = useState<IntegratedGrade | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  const field = (id: string): FieldState => fields[id] ?? emptyField;
  const patch = (id: string, next: Partial<FieldState>) =>
    setFields((held) => ({ ...held, [id]: { ...(held[id] ?? emptyField), ...next } }));

  const askHint = async (id: string) => {
    const held = field(id);
    try {
      const reply = await api.taskIntegratedHint(taskId, { field: id, index: held.hintsUsed });
      // A ladder that ran out answers `hint: null`, and the count then stands still: the
      // learner is not marked assisted for a rung the item does not have.
      patch(id, {
        hint: reply.hint ?? held.hint,
        hintsUsed: reply.hint === null ? held.hintsUsed : reply.hints_used,
      });
    } catch {
      setFailure('The hint did not arrive. Try again.');
    }
  };

  const submit = async () => {
    setBusy(true);
    setFailure(null);
    try {
      const reply = await api.taskIntegratedAnswer(taskId, {
        method,
        steps: problem.steps.map((step) => ({
          id: step.id,
          answer: field(step.id).answer,
          hints_used: field(step.id).hintsUsed,
        })),
        final_answer: {
          id: FINAL,
          answer: field(FINAL).answer,
          hints_used: field(FINAL).hintsUsed,
        },
        ...(reasoning.trim() ? { reasoning } : {}),
      });
      setGrade(reply);
      onGraded?.(reply);
    } catch {
      setFailure('The submission did not reach the service. Try again.');
    } finally {
      setBusy(false);
    }
  };

  const answerBox = (id: string, ask: IntegratedProblem['final_ask'], label: string) => (
    <div className="integrated-field">
      <label className="integrated-prompt" htmlFor={`integrated-${id}`}>
        <MathBlock className="problem-text">{ask.prompt}</MathBlock>
      </label>
      <div className="integrated-entry">
        <input
          id={`integrated-${id}`}
          className="answer-input"
          value={field(id).answer}
          disabled={busy || grade !== null}
          aria-label={label}
          onChange={(event) => patch(id, { answer: event.target.value })}
        />
        {ask.unit ? <span className="integrated-unit">{ask.unit}</span> : null}
        {ask.hints_available > 0 ? (
          <button
            type="button"
            className="btn btn-quiet"
            disabled={busy || grade !== null}
            onClick={() => void askHint(id)}
          >
            Hint ({field(id).hintsUsed}/{ask.hints_available})
          </button>
        ) : null}
      </div>
      {field(id).hint ? <p className="integrated-hint">{field(id).hint}</p> : null}
    </div>
  );

  return (
    <section className="integrated-task">
      <div className="task-header">
        <div className="task-meta">
          <Chip className="chip-integrated">integrated</Chip>
          <span className="topic-name">{problem.title}</span>
          <span className="topic-module">{domainLabel(problem.domain)}</span>
        </div>
      </div>

      <MathBlock className="problem-text integrated-scenario">{problem.scenario}</MathBlock>

      <ul className="integrated-given">
        {problem.given.map((given) => (
          <li key={given.label}>
            <span className="integrated-given-label">{given.label}</span>
            <span className="integrated-given-value">{given.value}</span>
            {given.note ? <span className="integrated-given-note">{given.note}</span> : null}
          </li>
        ))}
      </ul>

      {problem.method ? (
        <fieldset className="integrated-method">
          <legend>{problem.method.prompt}</legend>
          {problem.method.options.map((option) => (
            <label key={option.id} className="integrated-option">
              <input
                type="radio"
                name="integrated-method"
                value={option.id}
                checked={method === option.id}
                disabled={busy || grade !== null}
                onChange={() => setMethod(option.id)}
              />
              {option.label}
            </label>
          ))}
        </fieldset>
      ) : null}

      <ol className="integrated-steps">
        {problem.steps.map((step, index) => (
          <li key={step.id}>{answerBox(step.id, step.ask, `Step ${index + 1}`)}</li>
        ))}
      </ol>

      {answerBox(FINAL, problem.final_ask, 'Final answer')}

      <div className="integrated-reasoning">
        <label htmlFor="integrated-reasoning">
          Your reasoning (optional). The service records it and never marks it right or
          wrong.
        </label>
        <textarea
          id="integrated-reasoning"
          value={reasoning}
          disabled={busy || grade !== null}
          onChange={(event) => setReasoning(event.target.value)}
        />
      </div>

      {grade === null ? (
        <button
          type="button"
          className={busy ? 'btn btn-primary is-busy' : 'btn btn-primary'}
          disabled={busy}
          onClick={() => void submit()}
        >
          Submit the whole task
        </button>
      ) : null}

      {failure ? <p className="integrated-failure">{failure}</p> : null}

      {grade ? (
        <div className="integrated-result">
          <h3>Answers</h3>
          <p className="integrated-score">
            {grade.correct_steps} of {grade.total_steps} steps, and the final answer is{' '}
            {verdictOf(grade.final)}.
          </p>
          <ul>
            {problem.steps.map((step, index) => (
              <VerdictRow
                key={step.id}
                label={`Step ${index + 1}`}
                grade={grade.steps.find((row) => row.id === step.id)}
              />
            ))}
            <VerdictRow label="Final answer" grade={grade.final} />
          </ul>
          {grade.method ? (
            <p className="integrated-method-verdict">
              Method: {grade.method.correct ? 'correct' : 'not correct'}
              {grade.method.why ? ` — ${grade.method.why}` : ''}
            </p>
          ) : null}
          <p className="integrated-interpretation">{grade.interpretation}</p>

          <h3>Your reasoning</h3>
          <p className="integrated-reasoning-note">
            The service does not grade reasoning. It stands here beside the verdicts, and
            it changes none of them.
          </p>
          <p className="integrated-reasoning-text">
            {grade.reasoning.recorded ? grade.reasoning.note : 'You wrote no note.'}
          </p>
        </div>
      ) : null}
    </section>
  );
}
