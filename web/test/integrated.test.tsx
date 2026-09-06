/**
 * The integrated task screen (D-F10): one scenario on one screen, a hint ladder that
 * fades, and a result that keeps the verdicts apart from the learner's prose.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import { Integrated } from '@/views/session/Integrated';
import type {
  IntegratedApi,
  IntegratedGrade,
  IntegratedHintResponse,
  IntegratedProblem,
} from '@/api/types';

const TASK = 's_2026-01-01a-multi-step';

const PROBLEM: IntegratedProblem = {
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

function gradeReply(over: Partial<IntegratedGrade> = {}): IntegratedGrade {
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
    ...over,
  };
}

/** A client with the two integrated methods the screen calls. */
function stub(over: Partial<IntegratedApi> = {}): IntegratedApi {
  return {
    taskIntegratedHint: vi.fn(
      async (_task: string, body: { field: string; index: number }): Promise<IntegratedHintResponse> => ({
        field: body.field,
        index: body.index,
        hint: body.index === 0 ? 'Every patient needs the same time.' : null,
        hints_available: 2,
        hints_used: body.index === 0 ? 1 : body.index,
      }),
    ),
    taskIntegratedAnswer: vi.fn(async () => gradeReply()),
    ...over,
  };
}

function mount(api: IntegratedApi = stub()) {
  return render(<Integrated api={api} taskId={TASK} problem={PROBLEM} />);
}

const input = (label: string) => screen.getByLabelText(label) as HTMLInputElement;

async function type(label: string, value: string) {
  const field = input(label);
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!;
    setter.call(field, value);
    field.dispatchEvent(new Event('input', { bubbles: true }));
  });
}

async function press(name: string | RegExp) {
  const button = screen.getByRole('button', { name });
  await act(async () => { button.dispatchEvent(new MouseEvent('click', { bubbles: true })); });
}

describe('the integrated task screen', () => {
  it('shows one scenario with every step and the final question together', () => {
    mount();
    expect(screen.getByText('Staff the vaccination window')).toBeTruthy();
    expect(screen.getByText('workforce capacity')).toBeTruthy();
    expect(screen.getByText('A clinic opens a 4 hour window and books 96 patients.')).toBeTruthy();
    expect(screen.getByText('A longer visit needs more nurses.')).toBeTruthy();
    expect(screen.getByText('Which method gives the number of nurses?')).toBeTruthy();
    expect(input('Step 1')).toBeTruthy();
    expect(input('Step 2')).toBeTruthy();
    expect(input('Final answer')).toBeTruthy();
  });

  it('asks for one hint rung at a time and counts what it opened', async () => {
    const api = stub();
    mount(api);
    expect(screen.getByRole('button', { name: 'Hint (0/2)' })).toBeTruthy();
    await press('Hint (0/2)');
    expect(screen.getByText('Every patient needs the same time.')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Hint (1/2)' })).toBeTruthy();
    expect(api.taskIntegratedHint).toHaveBeenCalledWith(TASK, { field: 'work', index: 0 });
  });

  it('holds the count still when the ladder has no rung left', async () => {
    const api = stub({
      taskIntegratedHint: vi.fn(async () => ({
        field: 'final',
        index: 3,
        hint: null,
        hints_available: 1,
        hints_used: 3,
      })),
    });
    mount(api);
    await press('Hint (0/1)');
    expect(screen.getByRole('button', { name: 'Hint (0/1)' })).toBeTruthy();
  });

  it('submits every step, the final answer, the method and the opened hints in ONE call', async () => {
    const api = stub();
    mount(api);
    await press('Hint (0/2)');
    await type('Step 1', '1440');
    await type('Step 2', '220');
    await type('Final answer', '7');
    await press('Submit the whole task');
    expect(api.taskIntegratedAnswer).toHaveBeenCalledWith(TASK, {
      method: null,
      steps: [
        { id: 'work', answer: '1440', hints_used: 1 },
        { id: 'nurse-minutes', answer: '220', hints_used: 0 },
      ],
      final_answer: { id: 'final', answer: '7', hints_used: 0 },
    });
  });

  it('keeps the verdicts apart from the reasoning, and says the prose is not graded', async () => {
    mount();
    await press('Submit the whole task');
    expect(screen.getByText('Answers')).toBeTruthy();
    expect(screen.getByText('1 of 2 steps, and the final answer is correct.')).toBeTruthy();
    expect(screen.getAllByText('correct').length).toBeGreaterThan(0);
    expect(screen.getByText('not correct')).toBeTruthy();
    expect(screen.getByText('after a hint')).toBeTruthy();
    expect(screen.getByText('7 nurses clear the window; 6 leave 12 patients unseen.')).toBeTruthy();
    expect(
      screen.getByText(
        'The service does not grade reasoning. It stands here beside the verdicts, and it changes none of them.',
      ),
    ).toBeTruthy();
    expect(screen.getByText('I counted the work first.')).toBeTruthy();
  });

  it('names an ungraded field as one a human checks, and never as a miss', async () => {
    const api = stub({
      taskIntegratedAnswer: vi.fn(async () =>
        gradeReply({
          final: {
            id: 'final',
            answered: true,
            correct: false,
            ungraded: true,
            notation: false,
            assisted: false,
            skills: [],
          },
          solved: false,
          ungraded: true,
        }),
      ),
    });
    mount(api);
    await press('Submit the whole task');
    expect(screen.getByText('needs a human check')).toBeTruthy();
  });

  it('reports a submission that did not reach the service and keeps the answers', async () => {
    const api = stub({
      taskIntegratedAnswer: vi.fn(async () => { throw new Error('the network went away'); }),
    });
    mount(api);
    await type('Final answer', '7');
    await press('Submit the whole task');
    expect(screen.getByText('The submission did not reach the service. Try again.')).toBeTruthy();
    expect(input('Final answer').value).toBe('7');
  });
});
