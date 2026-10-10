/** Lesson-flow wording and layout rules of the UX fix wave (W4). */
import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { LESSON, P, TEACHING, graded, mount, planOf, press, stubApi, submitAnswer } from './helpers/session';
import { Integrated } from '@/views/session/Integrated';
import { PROBLEM } from './helpers/integrated';
import type { IntegratedApi, IntegratedProblem } from '@/api/types';

const STEPS = { ...TEACHING, worked_example: { problem: 'Simplify four sixths.', steps: ['Divide by 2.'] } };

describe('the lesson header', () => {
  it('has one h1 with the topic name, and a Save & exit button with its reassurance in text', async () => {
    await mount();
    expect(document.querySelectorAll('h1').length).toBe(1);
    expect(screen.getByRole('button', { name: 'Save & exit' })).toBeTruthy();
    expect(screen.getByText('Your work is saved.')).toBeTruthy();
  });

  it('offers How to type answers as one plain button', async () => {
    await mount();
    expect(screen.getAllByRole('button', { name: 'How to type answers' }).length).toBeGreaterThan(0);
    expect(document.querySelector('.header-more')).toBeNull();
  });
});

describe('the lesson practice', () => {
  it('folds the worked example above the answer field', async () => {
    await mount({ plan: planOf(LESSON), api: stubApi({ taskTeach: async () => STEPS, taskServe: async () => P(1) }) });
    await press('Start practice ▸');
    const fold = document.querySelector('details.worked-fold') as HTMLDetailsElement;
    expect(fold.open).toBe(false);
    expect(fold.querySelector('summary')!.textContent).toBe('Show the worked example');
    expect(fold.textContent).toContain('Divide by 2.');
    const field = screen.getByLabelText('Answer');
    expect(fold.compareDocumentPosition(field) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});

describe('the verdict panel', () => {
  it('hides the header exit, says which key continues, and keeps the stop note off a correct panel', async () => {
    await mount({ api: stubApi({ taskAnswer: async () => graded({ correct: true }) }) });
    await submitAnswer('3/4');
    expect(screen.queryByRole('button', { name: 'Save & exit' })).toBeNull();
    expect(screen.getByText('Press Enter to continue')).toBeTruthy();
    expect(screen.queryByText(/An unfinished lesson comes back next time/)).toBeNull();
  });

  it('shows the stop note and the label of the mistake kinds on a miss', async () => {
    await mount({ api: stubApi({ taskAnswer: async () => graded({ correct: false, error_tags: ['sign'] }) }) });
    await submitAnswer('3/4');
    expect(screen.getByText(/An unfinished lesson comes back next time/)).toBeTruthy();
    expect(screen.getByText('Kind of mistake:')).toBeTruthy();
  });

  it('never calls a correct answer after a hint perfect', async () => {
    const taskHint = async () => ({ hint: 'Look at the factors.', hint_number: 1 });
    await mount({ api: stubApi({ taskHint, taskAnswer: async () => graded({ correct: true, work_quality: 'perfect' }) }) });
    await press('Hint');
    await submitAnswer('3/4');
    expect(screen.getByText('Correct, with a hint')).toBeTruthy();
    expect(screen.queryByText('perfect')).toBeNull();
  });
});

describe('the mixed problem', () => {
  const api = {} as IntegratedApi;
  const many: IntegratedProblem = {
    ...PROBLEM,
    steps: ['a', 'b', 'c', 'd'].map((id) => ({ id, ask: { prompt: `Step ${id}?`, unit: null, hints_available: 0 } })),
  };

  it('uses the lesson header, one report link and a definition list of givens', () => {
    render(<Integrated api={api} reportApi={api as never} taskId="t" problem={PROBLEM} onExit={() => {}} />);
    expect(screen.getByText('Mixed problem')).toBeTruthy();
    expect(screen.getByRole('heading', { level: 1 }).textContent).toBe(PROBLEM.title);
    expect(screen.getAllByRole('button', { name: 'Report this question' }).length).toBe(1);
    expect(document.querySelectorAll('dl.integrated-given dt').length).toBe(PROBLEM.given.length);
  });

  it('shows one step at a time when there are more than three', () => {
    render(<Integrated api={api} taskId="t" problem={many} />);
    expect(screen.getByLabelText('Step 1')).toBeTruthy();
    expect(screen.queryByLabelText('Step 2')).toBeNull();
    expect(screen.queryByLabelText('Final answer')).toBeNull();
    for (let i = 0; i < 3; i += 1) fireEvent.click(screen.getByRole('button', { name: 'Next step' }));
    expect(screen.getByLabelText('Step 4')).toBeTruthy();
    expect(screen.getByLabelText('Final answer')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Next step' })).toBeNull();
  });
});
