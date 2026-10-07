/**
 * The three screens of the study loop that hold no problem: the summary, the empty plan,
 * and the header above a live problem.
 *
 * Each one is pure render over the props the loop hands it, so `Session.tsx` keeps the
 * four machines and none of the markup that surrounds them.
 */
import { Chip, Stat } from '@/components/primitives';
import { fmtClock, num, signed } from '@/lib/format';
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
            <Stat value={signed(summary.xp_earned)} label="XP earned" className="accent" />
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
function progressOf(task: PlanTask, problem: ServedProblem): string {
  if (problem.feedback_practice) return 'Practice on your own';
  if (problem.mixed_review) {
    return `${num(problem.mixed_review.position)} / ${num(problem.mixed_review.total)}`;
  }
  if (problem.total != null) return `${num(problem.index)} / ${num(problem.total)}`;
  return task.task_type === 'review' ? '' : `${num(problem.index)}`;
}

/**
 * The task chip, the topic, the count, the clock and the way out.
 *
 * A REVIEW names no topic and no module before the answer: recognising which method a
 * problem needs is part of the review (interleaving, Math Academy Way ch. 19). The
 * feedback names the topic once the answer is graded.
 */
export function ProblemHeader({ task, problem, elapsed, countdown, onExit }: ProblemHeaderProps) {
  const topic = task.task_type === 'review' ? null : task.topic;
  return (
    <div className="task-header">
      <div className="task-meta">
        <Chip className={`chip-${task.task_type}`}>{task.task_type}</Chip>
        {task.task_type === 'review'
          ? <span className="topic-name">{problem.mixed_review ? 'Mixed review' : 'Review'}</span>
          : <span className="topic-name">{topic?.name || topic?.id || 'Practice'}</span>}
        {topic?.module ? <span className="topic-module">{topic.module}</span> : null}
      </div>
      <div className="task-right">
        <span className="progress-count">{progressOf(task, problem)}</span>
        <span className={`timer${countdown && elapsed <= 3 ? ' urgent' : ''}`}>
          {fmtClock(elapsed)}
        </span>
        <button
          type="button"
          className="btn btn-ghost btn-exit"
          title="Your work is saved. An unfinished lesson comes back next time."
          onClick={onExit}
        >
          Exit
        </button>
      </div>
      {/* VERBATIM. The selector re-parses substrings of this prose to decide whether a
          task may be re-served, so a reword changes which tasks survive. */}
      {task.why ? <div className="why-chip">{task.why}</div> : null}
    </div>
  );
}
