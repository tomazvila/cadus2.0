/**
 * A deterministic browser fixture for the complete integrated-learning journey.
 *
 * The ordinary demo remains the 1.0 compatibility walk. This client keeps the same
 * public contract while exposing the 2.0 sequence that needs a real rendering engine:
 * instruction, integrated application, feedback, an unseen delayed assessment, and the
 * resulting retention report.
 */
import { ApiError } from './client';
import { createDemoApi } from './demo';
import type {
  ApiClient,
  IntegratedGrade,
  IntegratedProblem,
  PlanTask,
  RetentionReportResponse,
} from './types';

const FRESH_TASK = 'demo-integrated-application';
const DELAYED_TASK = 'demo-integrated-assessment';

function task(task_id: string, assessment: boolean): PlanTask {
  return {
    task_id,
    task_type: 'multi-step',
    topic: { id: 'unit-rates', name: 'Unit rates', module: 'Algebra' },
    kp: 'unit-rates/kp1',
    start_at_kp: 'unit-rates/kp1',
    n_problems: 1,
    mix: null,
    component_topics: ['unit-rates', 'integer-operations'],
    time_budget_secs: 600,
    difficulty_target: 0.7,
    why: assessment
      ? 'Seven-day application check on an unseen scenario.'
      : 'Apply the worked example across one complete scenario.',
    integrated_instruction_required: !assessment,
    integrated_assessment: assessment,
    progress: { answered: 0, done: false },
  };
}

function problem(assessment: boolean): IntegratedProblem {
  return {
    item_id: assessment ? 'integrated-food-bank-shift' : 'integrated-clinic-window',
    item_digest: assessment ? 'fedcba9876543210' : '0123456789abcdef',
    title: assessment ? 'Plan the food-bank packing shift' : 'Staff the vaccination window',
    topic: 'unit-rates',
    domain: 'workforce_capacity',
    scenario: assessment
      ? 'A food bank packs $120$ boxes. Each box takes $12$ person-minutes.'
      : 'A clinic books $96$ patients. Each visit takes $15$ person-minutes.',
    given: [
      { label: 'Shift length', value: '4 h' },
      { label: assessment ? 'Boxes' : 'Patients', value: assessment ? '120' : '96' },
    ],
    method: {
      prompt: 'Which method gives the number of workers?',
      options: [
        { id: 'person-minutes', label: 'Divide total person-minutes by one worker\'s minutes.' },
        { id: 'items-per-hour', label: 'Divide the item count by the shift hours.' },
      ],
    },
    steps: [
      {
        id: 'work',
        ask: {
          prompt: assessment
            ? 'Compute $120 \\times 12$ person-minutes.'
            : 'Compute $96 \\times 15$ person-minutes.',
          unit: 'person-minutes',
          hints_available: assessment ? 0 : 2,
        },
      },
      {
        id: 'worker-minutes',
        ask: { prompt: 'Convert $4$ hours to minutes.', unit: 'minutes', hints_available: 0 },
      },
    ],
    final_ask: {
      prompt: 'How many workers are required?',
      unit: 'workers',
      hints_available: assessment ? 0 : 1,
    },
    skills: ['unit-rates/kp1', 'integer-operations/kp2'],
  };
}

function grade(taskId: string, assisted: boolean): IntegratedGrade {
  const assessment = taskId === DELAYED_TASK;
  const item = problem(assessment);
  return {
    item_id: item.item_id,
    item_digest: item.item_digest,
    method: {
      chosen: 'person-minutes',
      correct: true,
      why: 'The quotient compares total work with one worker\'s available time.',
    },
    steps: item.steps.map((step) => ({
      id: step.id,
      answered: true,
      correct: true,
      ungraded: false,
      notation: false,
      assisted: assisted && step.id === 'work',
      skills: ['unit-rates/kp1'],
    })),
    final: {
      id: 'final', answered: true, correct: true, ungraded: false, notation: false,
      assisted: false, skills: ['unit-rates/kp1'],
    },
    correct_steps: 2,
    total_steps: 2,
    solved: true,
    assisted,
    ungraded: false,
    skills_credited: ['unit-rates/kp1'],
    interpretation: 'Six workers complete the work inside the four-hour window.',
    reasoning: { recorded: true, graded: false, note: 'I divided total work by one worker\'s time.' },
    recorded: true,
  };
}

function retention(completed: number): RetentionReportResponse {
  const row = {
    delay_days: 7,
    probes: completed >= 2 ? 1 : 0,
    retained_accuracy: completed >= 2 ? 1 : null,
    assistance_dependence: completed >= 2 ? 0 : null,
    mean_independent_secs: completed >= 2 ? 74 : null,
    sufficient: false,
    provenance: {
      independent: completed >= 2 ? 1 : 0,
      independent_correct: completed >= 2 ? 1 : 0,
      correct: completed >= 2 ? 1 : 0,
      assisted: 0,
      repeated: 0,
      unknown_exposure: 0,
      ungraded: 0,
    },
  };
  return {
    policy: {
      version: 1,
      label: 'v1 (uncalibrated)',
      calibrated: false,
      digest: 'journey000000001',
      probe_delays_days: [7, 30, 90],
      min_sample: 20,
    },
    retention: {
      by_delay: [row, { ...row, delay_days: 30, probes: 0, retained_accuracy: null,
        assistance_dependence: null, mean_independent_secs: null,
        provenance: { ...row.provenance, independent: 0, independent_correct: 0, correct: 0 } },
      { ...row, delay_days: 90, probes: 0, retained_accuracy: null,
        assistance_dependence: null, mean_independent_secs: null,
        provenance: { ...row.provenance, independent: 0, independent_correct: 0, correct: 0 } }],
      total: { ...row, delay_days: 0 },
    },
    placement: { failed_confirmation: [], awaiting_confirmation: [] },
    integrated: {
      served: completed,
      passed: completed,
      failed: 0,
      inconclusive: 0,
      open: 0,
      pass_rate: completed ? 1 : null,
    },
  };
}

/** Build one isolated 2.0 browser journey. */
export function createIntegratedDemoApi(): ApiClient {
  const base = createDemoApi();
  const completed = new Set<string>();
  const hintCounts = new Map<string, number>();
  const known = (taskId: string) => taskId === FRESH_TASK || taskId === DELAYED_TASK;
  const requireKnown = (taskId: string) => {
    if (!known(taskId)) throw new ApiError(404, 'unknown_task', 'The journey plans two tasks.');
  };

  return {
    ...base,
    getPlan: async () => ({
      session: 'demo-integrated-session',
      tasks: [task(FRESH_TASK, false), task(DELAYED_TASK, true)],
      quiz_due: false,
      constraints: {
        lesson_ratio_ok: true, lesson_ratio: 0.5, throttle_ok: true, reviews: 1, lessons: 1,
      },
      course_complete: false,
      blocked: [],
      frontier_blocked_until: null,
    }),
    taskTeach: async (taskId) => {
      requireKnown(taskId);
      return {
        kp: 'unit-rates/kp1',
        concept: 'Total work equals items $\\times$ minutes per item.',
        worked_example: {
          problem: 'For $40$ items at $6$ minutes each, find the total work.',
          steps: '$40 \\times 6 = 240$ person-minutes.',
        },
      };
    },
    taskIntegrated: async (taskId) => {
      requireKnown(taskId);
      return problem(taskId === DELAYED_TASK);
    },
    taskIntegratedHint: async (taskId, body) => {
      requireKnown(taskId);
      const key = `${taskId}:${body.field}`;
      const count = hintCounts.get(key) ?? 0;
      const hint = taskId === FRESH_TASK && body.field === 'work' && count === 0
        ? 'Multiply the number served by the minutes for one service.'
        : null;
      if (hint) hintCounts.set(key, count + 1);
      return {
        field: body.field,
        index: body.index,
        hint,
        hints_available: body.field === 'work' && taskId === FRESH_TASK ? 2 : 0,
        hints_used: hint ? count + 1 : count,
      };
    },
    taskIntegratedAnswer: async (taskId, body) => {
      requireKnown(taskId);
      completed.add(taskId);
      return grade(taskId, body.steps.some((field) => field.hints_used > 0));
    },
    getRetentionReport: async () => retention(completed.size),
  };
}
