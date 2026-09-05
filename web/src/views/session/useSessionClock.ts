/**
 * The display clock of the study loop (trap T4).
 *
 * DISPLAY ONLY. The service measures session time from its own accumulator and prices XP
 * with it, so this clock feeds nothing back: `sessionEnd()` is called with no arguments.
 *
 * It counts UP on every task but a drill, and DOWN on a drill with a budget. The starting
 * value travels with the problem — the view sets it on the same line it sets the problem —
 * so the ticking effect never writes state synchronously to reset it.
 *
 * A re-solve is UNTIMED (DD-3/P1). The timed attempt is already made and stashed, so the
 * budget of this problem is spent: the clock counts the re-solve up from zero, and the
 * drill auto-submit of `useGrade` — which fires only on a countdown — cannot fire at all.
 */
import { useEffect, useState } from 'react';
import { num } from '@/lib/format';
import type { Lifetime } from '@/hooks/useLifetime';
import type { PlanTask, ReworkResponse, ServedProblem } from '@/api/types';
import type { SessionPhase } from './useGrade';

/** A drill counts down only when all three hold. Nothing else has a countdown (trap T5). */
export function isDrill(task: PlanTask | null, problem: ServedProblem | null): boolean {
  return !!task && task.task_type === 'drill'
    && !!problem?.countdown && num(problem.time_budget_secs) > 0;
}

/** The starting value of the display clock: the budget on a drill, zero elsewhere. */
export function clockStart(task: PlanTask | null, problem: ServedProblem): number {
  return isDrill(task, problem) ? num(problem.time_budget_secs) : 0;
}

export interface SessionClock {
  /** The seconds on screen. */
  elapsed: number;
  /** Set the value a fresh problem starts from, or zero for the re-solve. */
  setElapsed: (secs: number) => void;
  /** True while the clock counts down: a drill with a budget, and not a re-solve. */
  countdown: boolean;
}

export function useSessionClock(
  life: Lifetime,
  task: PlanTask | null,
  problem: ServedProblem | null,
  rework: ReworkResponse | null,
  phase: SessionPhase,
): SessionClock {
  const [elapsed, setElapsed] = useState(0);
  const countdown = isDrill(task, problem) && !rework;
  // A problem is on screen in `ready`, and in no phase without one.
  const ticking = phase === 'ready';

  useEffect(() => {
    if (!ticking) return undefined;
    // Ticking only. The starting value arrived with the problem, so nothing here writes
    // state synchronously. Registered in the lifetime, so it dies with the view.
    const id = life.setInterval(() => {
      setElapsed((v) => (countdown ? Math.max(0, v - 1) : v + 1));
    }, 1000);
    return () => life.clearTimer(id);
  }, [ticking, countdown, life]);

  return { elapsed, setElapsed, countdown };
}
