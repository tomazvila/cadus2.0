/**
 * The plan cursor of the study loop.
 *
 * The session view is four machines in one screen — the cursor, the serve, the grade, and
 * the clock. This is the first of them, kept apart so the view file holds one job less.
 *
 * THE TASK LIST IS FILTERED ON THE SERVER'S `progress.done`, never on this mount's memory
 * alone. Per-mount memory alone left a reload restarting at a task the service had already
 * closed: the serve answered `409 task_complete` and the learner could not get past it.
 *
 * The core owns the schedule (Hard Rule 3). Nothing here re-orders a plan or invents a
 * task; it walks the list the service sent and asks for a fresh one when a task ends.
 */
import { useRef, useState } from 'react';
import type { PlanTask, SessionPlanResponse } from '@/api/types';

export interface SessionPlan {
  /** The task the learner is on, or null when the plan is exhausted. */
  task: PlanTask | null;
  /** True when the plan carried tasks and every one of them was already finished. */
  allDone: boolean;
  /** The plan as it arrived, for the empty-state copy. */
  plan: SessionPlanResponse | null;
  /** Install a plan and start at its first open task. */
  start: (plan: SessionPlanResponse) => void;
  /**
   * Finish the current task and move on. Gives the task that becomes current, or null when
   * the plan is exhausted — so the caller ends the session THERE, and no effect has to
   * watch for exhaustion.
   */
  next: () => PlanTask | null;
  /** Ask for a re-plan when the current task ends. Remediation is served FIRST. */
  requestReplan: () => void;
  /** True when a re-plan is owed. */
  needsReplan: () => boolean;
  /**
   * Record the current task as finished and leave the cursor where it is. A re-plan resets
   * the index anyway, and a move first would start a task the re-plan then throws away.
   */
  markDone: () => void;
  /**
   * Install a fresh plan and drop what this mount already finished. False means nothing is
   * open, so the caller ends the session instead of stranding the view on a spinner.
   */
  replan: (plan: SessionPlanResponse) => boolean;
}

/** Tasks still owed: not done on the server, and not finished in this mount. */
function openTasks(plan: SessionPlanResponse, done: Set<string>): PlanTask[] {
  return plan.tasks.filter((t) => !t.progress.done && !done.has(t.task_id));
}

/** No plan yet. One array for every render before the first plan, so nothing rebuilds. */
const NO_TASKS: PlanTask[] = [];

export function useSessionPlan(): SessionPlan {
  const [plan, setPlan] = useState<SessionPlanResponse | null>(null);
  const [tasks, setTasks] = useState<PlanTask[]>(NO_TASKS);
  const [index, setIndex] = useState(0);

  // This mount's memory of what it finished. A ref, not state: `next()` reads it
  // synchronously and nothing renders from it.
  const doneIds = useRef(new Set<string>());
  const replanWanted = useRef(false);

  // The four moves that read refs and setters alone, built ONCE per mount.
  const [moves] = useState(() => {
    const install = (incoming: SessionPlanResponse): boolean => {
      const open = openTasks(incoming, doneIds.current);
      setPlan(incoming);
      setTasks(open);
      setIndex(0);
      return open.length > 0;
    };
    return {
      start: (incoming: SessionPlanResponse): void => { install(incoming); },
      requestReplan: (): void => { replanWanted.current = true; },
      needsReplan: (): boolean => replanWanted.current,
      replan: (incoming: SessionPlanResponse): boolean => {
        replanWanted.current = false;
        // A fresh plan carries remediation at its head, so the cursor restarts at the top
        // and the remedial task is served NEXT, not deferred to a future session.
        return install(incoming);
      },
    };
  });

  // The task the cursor stands on. The two moves below are called while it stands on one.
  const current = tasks[index] ?? null;

  return {
    ...moves,
    task: current,
    // "Nothing was due" and "everything planned is already finished" are two different
    // sentences to a learner, and a reload after the last answer lands on the second one.
    // Read only while no task is open, so a plan that named tasks has them all finished.
    allDone: plan !== null && plan.tasks.length > 0,
    plan,
    next: (): PlanTask | null => {
      doneIds.current.add(current!.task_id);
      setIndex(index + 1);
      return tasks[index + 1] ?? null;
    },
    markDone: (): void => { doneIds.current.add(current!.task_id); },
  };
}
