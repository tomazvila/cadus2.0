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
        <h2>Session complete</h2>
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
  if (plan.course_complete) return 'Course complete. Enroll in your next course to keep going.';
  if (plan.frontier_blocked_until) {
    return `New lessons are on a retry delay until ${plan.frontier_blocked_until}.`;
  }
  if (allDone) return 'Everything planned for this session is done. Nice work.';
  return 'Nothing is due right now — enjoy the break.';
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
          Take the placement diagnostic
        </button>
        <button type="button" className="btn btn-primary" onClick={onExit}>
          Back to dashboard
        </button>
      </div>
    </section>
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

/** The task chip, the topic, the count, the clock and the way out. */
export function ProblemHeader({ task, problem, elapsed, countdown, onExit }: ProblemHeaderProps) {
  const topic = task.topic;
  return (
    <div className="task-header">
      <div className="task-meta">
        <Chip className={`chip-${task.task_type}`}>{task.task_type}</Chip>
        <span className="topic-name">{topic?.name || topic?.id || 'Practice'}</span>
        {topic?.module ? <span className="topic-module">{topic.module}</span> : null}
      </div>
      <div className="task-right">
        <span className="progress-count">
          {problem.total != null
            ? `${num(problem.index)} / ${num(problem.total)}`
            : `${num(problem.index)}`}
        </span>
        <span className={`timer${countdown && elapsed <= 3 ? ' urgent' : ''}`}>
          {fmtClock(elapsed)}
        </span>
        <button
          type="button"
          className="btn btn-ghost btn-exit"
          title="Leave the session — your work is saved and unfinished tasks come back next time"
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
