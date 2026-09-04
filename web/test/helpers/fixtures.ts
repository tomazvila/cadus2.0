/** The account and the quiz task the router tests share. */
import type { PlanTask, User } from '@/api/types';

export const USER: User = {
  id: 'u1',
  email: 'learner@example.com',
  email_verified: true,
  created_at: '2026-08-30T00:00:00Z',
};

/** One quiz task of eight problems on a 480 s budget, under the id the test names. */
export const quizTask = (taskId: string): PlanTask => ({
  task_id: taskId,
  task_type: 'quiz',
  topic: { id: 'fractions', name: 'Fractions', module: 'Arithmetic' },
  kp: null,
  start_at_kp: null,
  n_problems: 8,
  mix: null,
  component_topics: null,
  time_budget_secs: 480,
  difficulty_target: 0.7,
  why: 'Quiz due.',
  progress: { answered: 0, done: false },
});
