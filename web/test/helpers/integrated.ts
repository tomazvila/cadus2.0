/** Shared authored scenario and service receipt for integrated session tests. */
import type { IntegratedGrade, IntegratedProblem } from '@/api/types';

export const PROBLEM: IntegratedProblem = {
  item_id: 'integrated-clinic-window',
  item_digest: '0123456789abcdef',
  title: 'Staff the vaccination window',
  topic: 'unit-rates',
  domain: 'workforce_capacity',
  scenario: 'A clinic opens a 4 hour window and books 96 patients.',
  given: [
    { label: 'Service window', value: '4 h' },
    { label: 'Time for one patient', value: '15 min', note: 'A longer visit needs more nurses.' },
  ],
  method: {
    prompt: 'Which method gives the number of nurses?',
    options: [
      { id: 'person-minutes', label: 'Divide the person-minutes by the minutes of one nurse.' },
      { id: 'patients-per-hour', label: 'Divide the patients by the hours.' },
    ],
  },
  steps: [
    {
      id: 'work',
      ask: { prompt: 'How many person-minutes of work?', unit: 'person-minutes', hints_available: 2 },
    },
    { id: 'nurse-minutes', ask: { prompt: 'How many minutes does one nurse work?', unit: 'min', hints_available: 0 } },
  ],
  final_ask: { prompt: 'How many nurses does the window need?', unit: 'nurses', hints_available: 1 },
  skills: ['unit-rates/kp1', 'inequality-word-problems/kp2'],
};

export function gradeReply(over: Partial<IntegratedGrade> = {}): IntegratedGrade {
  return {
    item_id: PROBLEM.item_id,
    item_digest: PROBLEM.item_digest,
    method: { chosen: 'person-minutes', correct: true, why: 'Every nurse works the same minutes.' },
    steps: [
      { id: 'work', answered: true, correct: true, ungraded: false, notation: false, assisted: true, skills: [] },
      {
        id: 'nurse-minutes',
        answered: true,
        correct: false,
        ungraded: false,
        notation: false,
        assisted: false,
        skills: [],
      },
    ],
    final: { id: 'final', answered: true, correct: true, ungraded: false, notation: false, assisted: false, skills: [] },
    correct_steps: 1,
    total_steps: 2,
    solved: true,
    assisted: true,
    ungraded: false,
    skills_credited: ['unit-rates/kp1'],
    interpretation: '7 nurses clear the window; 6 leave 12 patients unseen.',
    reasoning: { recorded: true, graded: false, note: 'I counted the work first.' },
    recorded: true,
    ...over,
  };
}
