/**
 * The worked example of a lesson (L4), with its optional active parts (step 5a).
 *
 * PEDAGOGY: a lesson teaches BEFORE it practises. The learner reads this, then presses
 * "I've got it" — which SERVES the first problem. The serve happens on that press and not
 * before it, because `POST /serve` stamps `started_at`: a problem served behind this page
 * bills the reading time as solve time, and this view mount would issue two writes
 * (NO-2BILL).
 *
 * ACTIVE PARTS. Reading is passive, so a page may carry two authored acts:
 *
 *  - `try_first`: a motivating problem shown BEFORE the worked example. The learner answers;
 *    any outcome is fine. The check reply brings the answer and a short reveal, then the
 *    worked example appears.
 *  - `step_check`: one step of the example is highlighted with a question and options. The
 *    learner must pick before the practice button appears; the check reply brings
 *    right/wrong and one sentence of why.
 *
 * The payload carries neither the answer nor the why (Hard Rule 1); `onCheck` posts the act
 * and the service grades it. The service records nothing for either act, so they never touch
 * mastery, XP, the pass rule or the schedule. A failed check never strands the learner: the
 * page moves on as if the act had been answered.
 */
import { useEffect, useRef, useState } from 'react';
import { AnswerInput, TypedSubmit } from '@/components/AnswerInput';
import type { AnswerFieldHandle } from '@/components/AnswerField';
import { MathBlock } from '@/components/MathBlock';
import { Chip } from '@/components/primitives';
import type { PlanTask, TeachCheckRequest, TeachCheckResponse, TeachResponse } from '@/api/types';

export interface TeachProps {
  task: PlanTask;
  instruction: TeachResponse;
  onContinue: () => void;
  /** Post one active-example act. Absent, the active parts are skipped. */
  onCheck?: (body: TeachCheckRequest) => Promise<TeachCheckResponse>;
}

type Check = (body: TeachCheckRequest) => Promise<TeachCheckResponse>;
type TryReply = Extract<TeachCheckResponse, { part: 'try_first' }>;
type StepReply = Extract<TeachCheckResponse, { part: 'step_check' }>;
type TryFirst = NonNullable<TeachResponse['try_first']>;
type StepCheck = NonNullable<TeachResponse['step_check']>;

/** The step list of the page. An older page sends one string. */
function stepsOf(steps: string | string[]): string[] {
  return Array.isArray(steps) ? steps : [steps];
}

export function Teach({ task, instruction, onContinue, onCheck }: TeachProps) {
  const example = instruction.worked_example;
  const tryFirst = onCheck ? instruction.try_first : undefined;
  const stepCheck = onCheck ? instruction.step_check : undefined;

  const [tried, setTried] = useState(false);
  const [picked, setPicked] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);

  const showExample = !tryFirst || tried;
  const canPractise = showExample && (!stepCheck || picked);
  const active = Boolean(tryFirst || stepCheck);

  // Keyboard wayfinding without a scroll jump: a bare focus() on mount YANKS the browser
  // scroll to the button, pinning it against the topbar and cutting the lesson header off
  // above. Arrive at the top of the lesson and focus in place instead. A page with an active
  // part focuses its own control, and the button once it appears.
  useEffect(() => {
    if (!active) window.scrollTo(0, 0);
    if (canPractise) buttonRef.current?.focus({ preventScroll: true });
  }, [active, canPractise]);

  return (
    <>
      <TeachHeader task={task} badge={showExample ? 'Worked example' : 'Try first'} />

      <div className="card teach-card">
        {tryFirst && onCheck ? (
          <TryFirstPanel tryFirst={tryFirst} onCheck={onCheck} onDone={() => setTried(true)} />
        ) : null}
        {showExample ? (
          <>
            {/* NOT `.problem-text`: the concept is prose, and borrowing the problem class made
                a 1.0 test assert a problem card against a lesson that never reached one. */}
            <div className="teach-concept">
              <MathBlock className="teach-concept-text">{instruction.concept}</MathBlock>
            </div>
            <div className="teach-example">
              <div className="teach-label">Example</div>
              <MathBlock className="teach-problem">{example.problem}</MathBlock>
              <div className="teach-label">Solution</div>
              <StepList steps={stepsOf(example.steps)} checked={stepCheck?.step} />
            </div>
            {stepCheck && onCheck ? (
              <StepCheckPanel stepCheck={stepCheck} onCheck={onCheck} onDone={() => setPicked(true)} />
            ) : null}
            {canPractise ? (
              <button ref={buttonRef} type="button" className="btn btn-primary" onClick={onContinue}>
                I&apos;ve got it — practice ▸
              </button>
            ) : null}
          </>
        ) : null}
      </div>
    </>
  );
}

function TeachHeader({ task, badge }: { task: PlanTask; badge: string }) {
  const { topic } = task;
  return (
    <div className="task-header">
      <div className="task-meta">
        <Chip className="chip-lesson">{task.task_type === 'multi-step' ? 'application preparation' : 'lesson'}</Chip>
        <span className="topic-name">{topic?.name || topic?.id || 'Lesson'}</span>
        {topic?.module ? <span className="topic-module">{topic.module}</span> : null}
      </div>
      <div className="task-right">
        <span className="teach-badge">{badge}</span>
      </div>
    </div>
  );
}

/** The worked solution, one item per step, with the asked step marked. */
function StepList({ steps, checked }: { steps: string[]; checked: number | undefined }) {
  return (
    <ol className="teach-steps">
      {steps.map((step, index) => {
        const marked = checked === index + 1;
        return (
          <li
            // Steps are fixed for the life of the page, so the index is a stable key.
            key={index}
            className={`teach-step${marked ? ' is-checked' : ''}`}
            aria-current={marked ? 'step' : undefined}
          >
            <MathBlock className="teach-step-text">{step}</MathBlock>
          </li>
        );
      })}
    </ol>
  );
}

/** The try-first problem: one attempt, then the answer and the reveal. */
function TryFirstPanel({ tryFirst, onCheck, onDone }: { tryFirst: TryFirst; onCheck: Check; onDone: () => void }) {
  const [reply, setReply] = useState<TryReply | 'failed' | null>(null);
  const [busy, setBusy] = useState(false);
  const answerRef = useRef<AnswerFieldHandle>(null);

  useEffect(() => {
    window.scrollTo(0, 0);
    answerRef.current?.focus();
  }, []);

  const submit = async () => {
    if (busy || reply !== null) return;
    const answer = answerRef.current?.value() ?? '';
    if (answer === '') return;
    setBusy(true);
    try {
      const got = await onCheck({ part: 'try_first', answer });
      setReply(got.part === 'try_first' ? got : 'failed');
    } catch {
      setReply('failed');
    } finally {
      setBusy(false);
      onDone();
    }
  };

  return (
    <div className="teach-try">
      <div className="teach-label">Try this first</div>
      <MathBlock className="teach-problem">{tryFirst.problem}</MathBlock>
      {reply === null ? (
        <>
          <p className="teach-note muted">Give it your best shot. It does not count toward anything.</p>
          <AnswerInput ref={answerRef} choices={tryFirst.choices} disabled={busy} onSubmit={() => { void submit(); }} />
          <TypedSubmit choices={tryFirst.choices} busy={busy} disabled={busy} onClick={() => { void submit(); }}>
            Check my attempt
          </TypedSubmit>
        </>
      ) : (
        <TryResult reply={reply} />
      )}
    </div>
  );
}

function TryResult({ reply }: { reply: TryReply | 'failed' }) {
  if (reply === 'failed') {
    return <p className="teach-note muted" role="status">Your attempt could not be checked. Read on.</p>;
  }
  let verdict = 'Not this time — that is fine.';
  if (reply.correct) verdict = 'You got it.';
  else if (reply.outcome === 'ungraded') verdict = 'That answer could not be checked automatically.';
  return (
    <div className="teach-result" role="status">
      <p className={`teach-verdict ${reply.correct ? 'is-right' : 'is-other'}`}>{verdict}</p>
      <p className="teach-answer">
        <span className="teach-label-inline">Answer: </span>
        <MathBlock as="span" className="teach-answer-text">{reply.answer}</MathBlock>
      </p>
      <MathBlock className="teach-reveal">{reply.reveal}</MathBlock>
    </div>
  );
}

/** The question on one step: one pick, then right/wrong and why. */
function StepCheckPanel({ stepCheck, onCheck, onDone }: { stepCheck: StepCheck; onCheck: Check; onDone: () => void }) {
  const [picked, setPicked] = useState<{ choice: string; reply: StepReply | null } | null>(null);
  const [busy, setBusy] = useState(false);

  const pick = async (choice: string) => {
    if (busy || picked !== null) return;
    setBusy(true);
    try {
      const got = await onCheck({ part: 'step_check', choice });
      setPicked({ choice, reply: got.part === 'step_check' ? got : null });
    } catch {
      setPicked({ choice, reply: null });
    } finally {
      setBusy(false);
      onDone();
    }
  };

  const markOf = (option: string): string => {
    if (!picked?.reply) return '';
    if (picked.reply.answer === option) return ' is-right';
    return picked.choice === option ? ' is-wrong' : '';
  };

  return (
    <div className="teach-check" role="group" aria-label={`Question about step ${stepCheck.step}`}>
      <div className="teach-label">{`Step ${stepCheck.step}`}</div>
      <MathBlock className="teach-question">{stepCheck.question}</MathBlock>
      <div className="choice-buttons">
        {stepCheck.options.map((option, index) => {
          const chosen = picked?.choice === option;
          return (
            <button
              key={`${index}:${option}`}
              type="button"
              className={`btn choice-button${chosen ? ' is-selected' : ''}${markOf(option)}`}
              aria-pressed={picked ? chosen : undefined}
              disabled={busy || picked !== null}
              onClick={() => { void pick(option); }}
            >
              <MathBlock as="span" className="choice-text">{option}</MathBlock>
            </button>
          );
        })}
      </div>
      {picked ? <StepResult reply={picked.reply} /> : null}
    </div>
  );
}

function StepResult({ reply }: { reply: StepReply | null }) {
  if (!reply) {
    return <p className="teach-note muted" role="status">Your pick could not be checked. Carry on to practice.</p>;
  }
  return (
    <div className="teach-result" role="status">
      <p className={`teach-verdict ${reply.correct ? 'is-right' : 'is-other'}`}>{reply.correct ? 'Right.' : 'Not quite.'}</p>
      <MathBlock className="teach-why">{reply.why}</MathBlock>
    </div>
  );
}
