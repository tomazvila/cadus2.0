/**
 * The three screens of the study loop that hold no problem: the summary, the empty plan,
 * and the header above a live problem.
 *
 * Each one is pure render over the props the loop hands it, so `Session.tsx` keeps the
 * four machines and none of the markup that surrounds them.
 */
import '../../styles/fix-session.css';
import { Chip, Stat } from '@/components/primitives';
import { fmtClock, num, signed } from '@/lib/format';
import { taskKindText, taskReasonText } from '@/lib/stageCopy';
import type { PlanTask, ServedProblem, SessionEndResponse, SessionPlanResponse } from '@/api/types';

/** The one sentence a lesson with no approved teach page says (audit finding j). */
const NO_INSTRUCTION_MESSAGE = 'This lesson is not ready yet';

export interface SummaryProps {
  summary: SessionEndResponse | null;
  homeRef: React.Ref<HTMLButtonElement>;
  onExit: () => void;
}

/** The session-complete card. A null summary says the close failed and the work is safe. */
export function SessionSummary({ summary, homeRef, onExit }: SummaryProps) {
  return (
    <section className="view-session">
      <div className="card summary-card">
        <h2>Done for now</h2>
        {summary ? (
          <div className="stat-grid">
            <Stat value={signed(summary.xp_earned)} label="points earned" className="accent" />
            <Stat value={`${num(summary.minutes)}`} label="minutes" />
            <Stat value={`${num(summary.xp.streak_days)}`} label="day streak" />
          </div>
        ) : (
          <p className="muted">Your work is saved. The summary did not load.</p>
        )}
        <button ref={homeRef} type="button" className="btn btn-primary" onClick={onExit}>
          Back to dashboard
        </button>
      </div>
    </section>
  );
}

/** The one line an empty plan says, and it says which empty it is. */
export function emptyPlanMessage(plan: SessionPlanResponse, allDone: boolean): string {
  if (plan.course_complete) return 'Course complete. Join your next course to keep going.';
  // D-F5: the plan is empty because the content is not ready, and not because the
  // learner is done. The count says so rather than leaving a silent gap.
  if (!plan.tasks.length && plan.blocked.length) {
    return `${num(plan.blocked.length)} topic(s) wait on lessons that are not ready yet.`;
  }
  if (plan.frontier_blocked_until) {
    return `New lessons wait until ${plan.frontier_blocked_until}.`;
  }
  if (allDone) return 'Everything planned for today is done. Nice work.';
  return 'Nothing is due right now. Come back later.';
}

export interface EmptyPlanProps {
  message: string;
  onDiagnostic: () => void;
  onExit: () => void;
}

/** No dead end: an empty plan offers the placement and the way home. */
export function EmptyPlan({ message, onDiagnostic, onExit }: EmptyPlanProps) {
  return (
    <section className="view-session">
      <div className="empty">
        <p>{message}</p>
        <button type="button" className="btn" onClick={onDiagnostic}>
          Take the starting questions
        </button>
        <button type="button" className="btn btn-primary" onClick={onExit}>
          Back to dashboard
        </button>
      </div>
    </section>
  );
}

export interface NoInstructionProps {
  task: PlanTask;
  onSkip: () => void;
  onExit: () => void;
}

/**
 * The lesson the service cannot teach (audit finding j).
 *
 * The service serves a worked example before it serves practice. With no
 * approved teach page it has none, so this card stands in place of the practice
 * the older build served here. There is one way on: the next task.
 */
export function NoInstruction({ task, onSkip, onExit }: NoInstructionProps) {
  const topic = task.topic;
  return (
    <div className="card no-instruction-card">
      <h2>{NO_INSTRUCTION_MESSAGE}</h2>
      <p className="muted">
        {`The worked example for ${topic?.name || topic?.id || 'this topic'} is not ready yet. `}
        Practice starts after the lesson is ready.
      </p>
      <div className="actions">
        <button type="button" className="btn btn-primary" onClick={onSkip}>
          Skip this lesson for now
        </button>
        <button type="button" className="btn btn-ghost" onClick={onExit}>
          Exit
        </button>
      </div>
    </div>
  );
}

export interface ProblemHeaderProps {
  task: PlanTask;
  problem: ServedProblem;
  /** The seconds on the display clock. */
  elapsed: number;
  /** True while the clock counts down: a drill with a budget, and not a re-solve. */
  countdown: boolean;
  onExit: () => void;
  /** True while a verdict panel is on screen. */
  hideExit?: boolean;
}

/** The topic a graded review answer names in its feedback; null on every other task. */
export function reviewTopic(task: PlanTask): string | null {
  return task.task_type === 'review' ? task.topic?.name || task.topic?.id || null : null;
}

/**
 * The progress line of a problem: corrective practice, the mixed block, or the task count.
 *
 * The service sends no `total` for a review: a retention probe is one question where a review
 * is four, so a per-task count would mark the probe before the answer (D-F11). A review with
 * no total shows the block's count, or none, and never a bare question number.
 */
function countOf(position: number, total: number): string {
  const left = Math.max(0, num(total) - num(position));
  return `${num(position)} / ${num(total)} · ${left === 0 ? 'last one' : `${num(left)} left`}`;
}

function progressOf(task: PlanTask, problem: ServedProblem): string {
  if (problem.feedback_practice) return 'Practice on your own';
  if (problem.mixed_review) {
    return countOf(problem.mixed_review.position, problem.mixed_review.total);
  }
  if (problem.total != null) return countOf(problem.index, problem.total);
  return task.task_type === 'review' ? 'Review' : `Question ${num(problem.index)}`;
}

export interface StudyHeaderProps {
  /** The chip: what the learner is doing. */
  kind: string;
  chipClass: string;
  /** The one `h1` of the screen. */
  title: string;
  module?: string | null | undefined;
  progress?: string;
  /** The clock, shown only for a countdown. */
  clock?: { seconds: number; urgent: boolean } | null;
  onExit?: (() => void) | undefined;
  /** True while a verdict panel is on screen: it holds its own way out. */
  hideExit?: boolean;
  why?: string | null;
}

/** The header shared by a lesson problem and a mixed problem: chip, title, count, way out. */
export function StudyHeader({
  kind, chipClass, title, module, progress, clock, onExit, hideExit = false, why,
}: StudyHeaderProps) {
  return (
    <div className="task-header">
      <div className="task-meta">
        <Chip className={chipClass}>{kind}</Chip>
        <h1 className="h-screen topic-name">{title}</h1>
        {module ? <span className="topic-module">{module}</span> : null}
      </div>
      <div className="task-right">
        {progress ? <span className="progress-count">{progress}</span> : null}
        {clock ? (
          <span className={`timer${clock.urgent ? ' urgent' : ''}`}>{fmtClock(clock.seconds)}</span>
        ) : null}
        {onExit && !hideExit ? (
          <span className="exit-group">
            <button type="button" className="btn btn-ghost" onClick={onExit}>Save &amp; exit</button>
            <span className="exit-note">Your work is saved.</span>
          </span>
        ) : null}
      </div>
      {why ? <div className="why-chip">{why}</div> : null}
    </div>
  );
}

/**
 * The task chip, the topic, the count, the clock and the way out.
 *
 * A REVIEW names no topic and no module before the answer: recognising which method a
 * problem needs is part of the review (interleaving, Math Academy Way ch. 19). The
 * feedback names the topic once the answer is graded.
 */
export function ProblemHeader({ task, problem, elapsed, countdown, onExit, hideExit = false }: ProblemHeaderProps) {
  const topic = task.task_type === 'review' ? null : task.topic;
  const title = task.task_type === 'review'
    ? (problem.mixed_review ? 'Mixed review' : 'Review')
    : topic?.name || topic?.id || 'Practice';
  // The plan keeps `why` verbatim, the selector re-parses it; the learner reads the mapped text.
  return (
    <StudyHeader
      kind={taskKindText(task.task_type, task.why)}
      chipClass={`chip-${task.task_type}`}
      title={title}
      module={topic?.module}
      progress={progressOf(task, problem)}
      clock={countdown ? { seconds: elapsed, urgent: elapsed <= 3 } : null}
      onExit={onExit}
      hideExit={hideExit}
      why={taskReasonText(task.why)}
    />
  );
}
