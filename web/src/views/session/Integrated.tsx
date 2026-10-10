/**
 * The integrated task of D-F10: ONE scenario, its steps, and one final answer.
 *
 * WHY IT IS ONE SCREEN. A multi-step task of 1.0 showed one unrelated question after
 * another. This screen shows the scenario, the quantities, the method choice, every
 * intermediate step and the final question TOGETHER, because the learner has to carry one
 * situation through all of them. A screen that revealed one step at a time would turn the
 * task back into the drill it replaces. The one exception is a problem with more than three
 * steps: it shows one step at a time, and keeps every answer until the single submit.
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
import { useEffect, useRef, useState } from 'react';
import { ApiError } from '@/api';
import { toast } from '@/app/toast';
import { ROLLED_OVER_MESSAGE, SESSION_ROLLED_OVER } from '@/hooks/useCall';
import { usePhase } from '@/hooks/usePhase';
import { useLifetime } from '@/hooks/useLifetime';
import { MathBlock } from '@/components/MathBlock';
import { taskKindText } from '@/lib/stageCopy';
import { StudyHeader } from './SessionScreens';
import { QuestionReport } from './ProblemReport';
import type { ProblemReportApi, ReportApplied } from './useProblemReport';
import type {
  IntegratedApi,
  IntegratedFieldGrade,
  IntegratedGrade,
  IntegratedProblem,
  IntegratedSubmission,
} from '@/api/types';

/** The field id of the final answer. It is the server's literal (`FINAL_FIELD_ID`). */
const FINAL = 'final';

export interface IntegratedProps {
  api: IntegratedApi;
  reportApi?: ProblemReportApi;
  taskId: string;
  problem: IntegratedProblem;
  /** Called with the grade after the service returns it. */
  onGraded?: (grade: IntegratedGrade) => void;
  onContinue?: () => void;
  onUnauthorized?: (() => void) | undefined;
  /**
   * The day rollover closed the session of this item (`409 session_rolled_over`). The
   * item cannot be sent again, so the view leaves it for the new session's plan.
   */
  onRolledOver?: () => void;
  /** Leave for the dashboard. Absent, the header shows no exit. */
  onExit?: () => void;
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
  if (grade.ungraded) return 'not marked yet';
  if (!grade.answered) return 'not answered';
  return grade.correct ? 'correct' : 'not correct';
}

/** One row of the verdict list: the label, then the verdict in its own column. */
function VerdictRow({ label, grade }: { label: string; grade: IntegratedFieldGrade | undefined }) {
  return (
    <div className="integrated-verdict">
      <dt className="integrated-verdict-label">{label}</dt>
      <dd className="integrated-verdict-value">
        {verdictOf(grade)}
        {grade?.assisted ? <span className="integrated-verdict-note">after a hint</span> : null}
      </dd>
    </div>
  );
}

/** Which steps show: all of them, or one at a time when the problem has many and is not graded. */
function stepView(steps: IntegratedProblem['steps'], at: number, graded: boolean) {
  const oneAtATime = steps.length > STEPS_AT_ONCE && !graded;
  return {
    oneAtATime,
    shownSteps: oneAtATime ? steps.slice(at, at + 1) : steps,
    onLastStep: !oneAtATime || at >= steps.length - 1,
  };
}

/** The step counter and the two buttons that move between steps. */
function Stepper({ at, total, onMove }: { at: number; total: number; onMove: (to: number) => void }) {
  return (
    <div className="integrated-stepper">
      <span className="muted">{`Step ${at + 1} of ${total}`}</span>
      {at > 0 ? <button type="button" className="btn btn-ghost" onClick={() => onMove(at - 1)}>Previous step</button> : null}
      {at < total - 1 ? <button type="button" className="btn" onClick={() => onMove(at + 1)}>Next step</button> : null}
    </div>
  );
}

/** A problem with more steps than this shows one step at a time. */
const STEPS_AT_ONCE = 3;

export function Integrated({ api, reportApi, taskId, problem, onGraded, onContinue, onUnauthorized, onRolledOver, onExit }: IntegratedProps) {
  const [fields, setFields] = useState<Record<string, FieldState>>(() => Object.fromEntries(
    Object.entries(problem.hints_used ?? {}).map(([id, hintsUsed]) => [id, { ...emptyField, hintsUsed }]),
  ));
  const [method, setMethod] = useState<string | null>(null);
  const [reasoning, setReasoning] = useState('');
  const [grade, setGrade] = useState<IntegratedGrade | null>(null);
  const [phase, gate] = usePhase<'ready' | 'hinting' | 'submitting' | 'graded'>('ready');
  const life = useLifetime();
  const busy = phase === 'hinting' || phase === 'submitting';
  const [failure, setFailure] = useState<string | null>(null);
  // The step on screen when the problem has many. Every answer stays in `fields`.
  const [stepAt, setStepAt] = useState(0);
  const { oneAtATime, shownSteps, onLastStep } = stepView(problem.steps, stepAt, grade !== null);
  const submittedBody = useRef<IntegratedSubmission | null>(null);
  const finalRef = useRef<HTMLInputElement>(null);
  const continueRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (grade) continueRef.current?.focus();
    else finalRef.current?.focus();
  }, [grade]);

  const field = (id: string): FieldState => fields[id] ?? emptyField;
  const patch = (id: string, next: Partial<FieldState>) =>
    setFields((held) => ({ ...held, [id]: { ...(held[id] ?? emptyField), ...next } }));

  /** The rollover answer: tell the learner once and hand the screen back. */
  const handBack = (): void => {
    toast(ROLLED_OVER_MESSAGE, { kind: 'info' });
    onRolledOver?.();
  };

  const askHint = async (id: string) => {
    if (!gate.tryEnter('ready', 'hinting')) return;
    const held = field(id);
    try {
      const reply = await api.taskIntegratedHint(taskId, { field: id, index: held.hintsUsed });
      if (!life.alive()) return;
      // A ladder that ran out answers `hint: null`, and the count then stands still: the
      // learner is not marked assisted for a rung the item does not have.
      patch(id, {
        hint: reply.hint ?? held.hint,
        hintsUsed: reply.hint === null ? held.hintsUsed : reply.hints_used,
      });
    } catch (error) {
      if (!life.alive()) return;
      if (error instanceof ApiError && error.code === SESSION_ROLLED_OVER) { handBack(); return; }
      if (error instanceof ApiError && error.sessionExpired) onUnauthorized?.();
      setFailure('The hint did not arrive. Try again.');
    } finally {
      if (life.alive()) gate.enter('ready');
    }
  };

  const submit = async () => {
    if (!gate.tryEnter('ready', 'submitting')) return;
    setFailure(null);
    let reply: IntegratedGrade;
    try {
      const body: IntegratedSubmission = {
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
      };
      submittedBody.current = body;
      reply = await api.taskIntegratedAnswer(taskId, body);
    } catch (error) {
      if (!life.alive()) return;
      if (error instanceof ApiError && error.code === SESSION_ROLLED_OVER) { handBack(); return; }
      if (error instanceof ApiError && error.sessionExpired) onUnauthorized?.();
      setFailure('Your answers did not send. Try again.');
      gate.enter('ready');
      return;
    }
    if (!life.alive()) return;
    gate.enter('graded');
    setGrade(reply);
    onGraded?.(reply);
  };

  const refreshCorrectedGrade = async () => {
    if (!submittedBody.current) return;
    try {
      const corrected = await api.taskIntegratedAnswer(taskId, submittedBody.current);
      if (life.alive()) setGrade(corrected);
    } catch (error) {
      if (!life.alive()) return;
      if (error instanceof ApiError && error.code === SESSION_ROLLED_OVER) { handBack(); return; }
      setFailure('The correction is saved. Reload this page to see the new result.');
    }
  };

  const answerBox = (id: string, ask: IntegratedProblem['final_ask'], label: string) => (
    <div className="integrated-field">
      <label className="integrated-prompt" htmlFor={`integrated-${id}`}>
        <MathBlock className="problem-text">{ask.prompt}</MathBlock>
      </label>
      <div className="integrated-entry">
        <input
          ref={id === FINAL ? finalRef : undefined}
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
            className="btn btn-ghost"
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
      <StudyHeader
        kind={taskKindText('multi-step', null)}
        chipClass="chip-multi-step"
        title={problem.title}
        module={domainLabel(problem.domain)}
        onExit={onExit}
      />

      <MathBlock className="problem-text integrated-scenario">{problem.scenario}</MathBlock>

      <dl className="integrated-given">
        {problem.given.map((given) => (
          <div key={given.label} className="integrated-given-row">
            <dt className="integrated-given-label">{given.label}</dt>
            <dd className="integrated-given-value">
              {given.value}
              {given.note ? <span className="integrated-given-note">{given.note}</span> : null}
            </dd>
          </div>
        ))}
      </dl>

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
        {shownSteps.map((step) => {
          const index = problem.steps.indexOf(step);
          return <li key={step.id} value={index + 1}>{answerBox(step.id, step.ask, `Step ${index + 1}`)}</li>;
        })}
      </ol>

      {oneAtATime ? <Stepper at={stepAt} total={problem.steps.length} onMove={setStepAt} /> : null}

      {onLastStep ? <>
      {answerBox(FINAL, problem.final_ask, 'Final answer')}

      <div className="integrated-reasoning">
        <label htmlFor="integrated-reasoning">
          Your reasoning (optional). It is saved and never marked right or wrong.
        </label>
        <textarea
          className="work-input"
          id="integrated-reasoning"
          value={reasoning}
          disabled={busy || grade !== null}
          onChange={(event) => setReasoning(event.target.value)}
        />
      </div>
      </> : null}

      {grade === null && onLastStep ? (
        <button
          type="button"
          className={busy ? 'btn btn-primary is-busy' : 'btn btn-primary'}
          disabled={busy}
          onClick={() => void submit()}
        >
          Submit all my answers
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
          <dl className="integrated-verdicts">
            {problem.steps.map((step, index) => (
              <VerdictRow
                key={step.id}
                label={`Step ${index + 1}`}
                grade={grade.steps.find((row) => row.id === step.id)}
              />
            ))}
            <VerdictRow label="Final answer" grade={grade.final} />
          </dl>
          {grade.method ? (
            <p className="integrated-method-verdict">
              Method: {grade.method.correct ? 'correct' : 'not correct'}
              {grade.method.why ? `: ${grade.method.why}` : ''}
            </p>
          ) : null}
          <p className="integrated-interpretation">{grade.interpretation}</p>

          {onContinue ? <button ref={continueRef} type="button" className="btn btn-primary" onClick={onContinue}>Continue</button> : null}
          <h3>Your reasoning</h3>
          <p className="integrated-reasoning-note">
            Your reasoning is not marked. It changes none of the results above.
          </p>
          <p className="integrated-reasoning-text">
            {grade.reasoning.recorded ? grade.reasoning.note : 'You wrote no note.'}
          </p>
        </div>
      ) : null}

      <IntegratedReports api={reportApi} taskId={taskId} problem={problem} grade={grade} field={field} reasoning={reasoning}
        onApplied={() => { void refreshCorrectedGrade(); }} />
    </section>
  );
}

function IntegratedReports({ api, taskId, problem, grade, field, reasoning, onApplied }: {
  api: ProblemReportApi | undefined; taskId: string; problem: IntegratedProblem;
  grade: IntegratedGrade | null; field: (id: string) => FieldState; reasoning: string; onApplied: ReportApplied;
}) {
  if (!api) return null;
  const submitted = grade !== null;
  // One report for the whole question: it names the final answer, and the scenario carries the rest.
  return <QuestionReport
    key={`${problem.item_digest}:${FINAL}:${submitted}`} api={api} hideResult={!submitted} onApplied={onApplied} context={{
      task_id: taskId, problem_id: problem.item_id, item_digest: problem.item_digest, field_id: FINAL,
      report_kind: submitted ? 'integrated' : 'served', submitted,
      problem_text: [problem.scenario, problem.final_ask.prompt].join('\n'),
      answer: submitted ? field(FINAL).answer : '', work: reasoning,
    }} />;
}
