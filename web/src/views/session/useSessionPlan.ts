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
import { useCallback, useRef, useState } from 'react';
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
function openTasks(plan: SessionPlanResponse | null, done: Set<string>): PlanTask[] {
  return (plan?.tasks ?? []).filter((t) => !t.progress?.done && !done.has(t.task_id));
}

export function useSessionPlan(): SessionPlan {
  const [plan, setPlan] = useState<SessionPlanResponse | null>(null);
  const [tasks, setTasks] = useState<PlanTask[]>([]);
  const [index, setIndex] = useState(0);
  const [allDone, setAllDone] = useState(false);

  // This mount's memory of what it finished. A ref, not state: `next()` reads it
  // synchronously and nothing renders from it.
  const doneIds = useRef(new Set<string>());
  const replanWanted = useRef(false);

  const install = useCallback((incoming: SessionPlanResponse) => {
    const open = openTasks(incoming, doneIds.current);
    setPlan(incoming);
    setTasks(open);
    setIndex(0);
    // "Nothing was due" and "everything planned is already finished" are two different
    // sentences to a learner, and a reload after the last answer lands on the second one.
    setAllDone(open.length === 0 && (incoming.tasks ?? []).length > 0);
    return open.length > 0;
  }, []);

  const markDone = useCallback(() => {
    const finished = tasks[index];
    if (finished) doneIds.current.add(finished.task_id);
  }, [index, tasks]);

  return {
    task: tasks[index] ?? null,
    allDone,
    plan,
    start: useCallback((incoming: SessionPlanResponse) => { install(incoming); }, [install]),
    next: useCallback((): PlanTask | null => {
      const finished = tasks[index];
      if (finished) doneIds.current.add(finished.task_id);
      setIndex((i) => i + 1);
      return tasks[index + 1] ?? null;
    }, [index, tasks]),
    requestReplan: useCallback(() => { replanWanted.current = true; }, []),
    needsReplan: useCallback(() => replanWanted.current, []),
    markDone,
    replan: useCallback((incoming: SessionPlanResponse): boolean => {
      replanWanted.current = false;
      // A fresh plan carries remediation at its head, so the cursor restarts at the top and
      // the remedial task is served NEXT, not deferred to a future session.
      return install(incoming);
    }, [install]),
  };
}
