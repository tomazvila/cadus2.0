/**
 * Step 5a, active worked examples: the try-first flow, the step-check flow, and the page
 * without either part.
 *
 * The fixtures live in `test/helpers/session.tsx`.
 */
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, screen, waitFor } from '@testing-library/react';
import { act } from 'react';
import { LESSON, P, TEACHING, answerInput, mount, planOf, press, stubApi } from './helpers/session';
import type { ApiClient, TeachCheckResponse, TeachResponse } from '@/api/types';

const PRACTISE = "I've got it — practice ▸";

const STEPS: TeachResponse = {
  ...TEACHING,
  worked_example: { problem: 'Simplify $\\frac{4}{6}$.', steps: ['Both parts divide by 2.', 'So it is $\\frac{2}{3}$.'] },
};

const WITH_CHECK: TeachResponse = {
  ...STEPS,
  step_check: {
    step: 1,
    question: 'Why may both parts be divided by 2?',
    options: ['It is the same as multiplying by 1', 'Smaller numbers are always equal', 'Only the top changes'],
  },
};

const WITH_TRY: TeachResponse = { ...STEPS, try_first: { problem: 'Is $\\frac{3}{6}$ equal to $\\frac{1}{2}$?' } };

const STEP_REPLY: TeachCheckResponse = {
  part: 'step_check', correct: false, answer: 'It is the same as multiplying by 1',
  why: 'Dividing top and bottom by 2 divides the fraction by 2/2, which is 1.',
};

const TRY_REPLY: TeachCheckResponse = {
  part: 'try_first', outcome: 'incorrect', correct: false, answer: 'yes',
  reveal: 'Halving 6 parts gives 3 of them: the same amount, written two ways.',
};

describe('active worked examples', () => {
  it('without active parts the page and its practice button are as before', async () => {
    const taskTeachCheck = vi.fn<ApiClient['taskTeachCheck']>();
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    await mount({ plan: planOf(LESSON), api: stubApi({ taskTeach: async () => STEPS, taskTeachCheck, taskServe }) });

    expect(screen.getByText('Worked example')).toBeTruthy();
    expect(screen.getByText('Both parts divide by 2.')).toBeTruthy();
    expect(screen.queryByLabelText('Answer')).toBeNull();
    await press(PRACTISE);

    expect(taskTeachCheck).not.toHaveBeenCalled();
    expect(taskServe).toHaveBeenCalledTimes(1);
  });

  it('a step check withholds practice until the pick, then shows the verdict and why', async () => {
    const taskTeachCheck = vi.fn<ApiClient['taskTeachCheck']>(async () => STEP_REPLY);
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    await mount({ plan: planOf(LESSON), api: stubApi({ taskTeach: async () => WITH_CHECK, taskTeachCheck, taskServe }) });

    // The asked step is marked, and nothing on the page names the right option yet.
    const marked = document.querySelector('.teach-step.is-checked');
    expect(marked?.textContent).toBe('Both parts divide by 2.');
    expect(screen.getByText('Why may both parts be divided by 2?')).toBeTruthy();
    expect(screen.queryByRole('button', { name: PRACTISE })).toBeNull();
    expect(screen.queryByText(STEP_REPLY.why)).toBeNull();

    await press('Only the top changes');

    expect(taskTeachCheck).toHaveBeenCalledWith('t-lesson', { part: 'step_check', choice: 'Only the top changes' });
    await waitFor(() => expect(screen.getByText('Not quite.')).toBeTruthy());
    expect(screen.getByText(STEP_REPLY.why)).toBeTruthy();
    const right = screen.getByRole('button', { name: 'It is the same as multiplying by 1' });
    expect(right.className).toContain('is-right');
    expect((right as HTMLButtonElement).disabled).toBe(true);

    expect(taskServe).not.toHaveBeenCalled();
    await press(PRACTISE);
    expect(taskServe).toHaveBeenCalledTimes(1);
  });

  it('try first: the attempt comes before the example, and the reveal comes with it', async () => {
    const taskTeachCheck = vi.fn<ApiClient['taskTeachCheck']>(async () => TRY_REPLY);
    const taskServe = vi.fn<ApiClient['taskServe']>(async () => P(1));
    await mount({ plan: planOf(LESSON), api: stubApi({ taskTeach: async () => WITH_TRY, taskTeachCheck, taskServe }) });

    // Only the problem: no concept, no example, no practice button.
    expect(screen.getByText('Try first')).toBeTruthy();
    expect(screen.queryByText(TEACHING.concept)).toBeNull();
    expect(screen.queryByText('Both parts divide by 2.')).toBeNull();
    expect(screen.queryByRole('button', { name: PRACTISE })).toBeNull();

    fireEvent.change(answerInput(), { target: { value: 'no' } });
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Check my attempt' })); });

    expect(taskTeachCheck).toHaveBeenCalledWith('t-lesson', { part: 'try_first', answer: 'no' });
    await waitFor(() => expect(screen.getByText(TRY_REPLY.reveal)).toBeTruthy());
    expect(screen.getByText('Not this time — that is fine.')).toBeTruthy();
    expect(screen.getByText(TEACHING.concept)).toBeTruthy();
    expect(screen.getByText('Both parts divide by 2.')).toBeTruthy();

    // No practice was served by the attempt; the learner still takes the button.
    expect(taskServe).not.toHaveBeenCalled();
    await press(PRACTISE);
    expect(taskServe).toHaveBeenCalledTimes(1);
  });

  it('a failed check never strands the learner', async () => {
    const taskTeachCheck = vi.fn<ApiClient['taskTeachCheck']>(async () => { throw new Error('down'); });
    await mount({ plan: planOf(LESSON), api: stubApi({ taskTeach: async () => WITH_CHECK, taskTeachCheck }) });

    await press('Only the top changes');

    await waitFor(() => expect(screen.getByText('Your pick could not be checked. Carry on to practice.')).toBeTruthy());
    expect(screen.getByRole('button', { name: PRACTISE })).toBeTruthy();
  });
});
