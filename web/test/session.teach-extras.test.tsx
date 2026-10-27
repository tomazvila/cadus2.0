/**
 * The optional parts of a teach page (D-TM1, D-TD1): the word list, the picture, the limit
 * and the second example. A page without them reads as it did before.
 */
import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Teach } from '@/views/session/Teach';
import type { TeachResponse } from '@/api/types';
import { LESSON, TEACHING } from './helpers/session';

const STEPS: TeachResponse = {
  ...TEACHING,
  worked_example: { problem: 'Simplify $\\frac{4}{6}$.', steps: ['Both parts divide by 2.', 'So it is $\\frac{2}{3}$.'] },
};

const FULL: TeachResponse = {
  ...STEPS,
  terms: [{ term: 'Numerator', meaning: 'The top number of a fraction.' }],
  picture: 'Cut a pizza in six slices and eat four.',
  watch_out: 'The factor must divide both parts.',
  second_example: { problem: 'A bar shows $\\frac{2}{3}$.', steps: ['Two of three parts are shaded.'] },
};

const mountTeach = (instruction: TeachResponse) =>
  render(<Teach task={LESSON} instruction={instruction} onContinue={() => {}} />);

const follows = (a: Element, b: Element) => Boolean(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);

describe('the optional parts of a teach page', () => {
  it('prints the words under the solution and above the practice button', () => {
    const view = mountTeach(FULL);
    const label = screen.getByText('Words used here');
    expect(screen.getByText('Numerator')).toBeTruthy();
    const steps = view.container.querySelectorAll('.teach-step');
    expect(follows(steps[steps.length - 1], label)).toBe(true);
    expect(follows(label, screen.getByRole('button', { name: 'Start practice ▸' }))).toBe(true);
  });

  it('puts the picture above the concept and the limit under the steps', () => {
    const view = mountTeach(FULL);
    const picture = screen.getByText('An everyday picture');
    expect(follows(picture, screen.getByText(TEACHING.concept))).toBe(true);
    const watch = screen.getByText('Where the method fails');
    const steps = view.container.querySelectorAll('.teach-step');
    expect(follows(steps[1], watch)).toBe(true);
    expect(follows(watch, screen.getByRole('button', { name: 'Start practice ▸' }))).toBe(true);
  });

  it('keeps the second example in a closed fold', () => {
    const view = mountTeach(FULL);
    const folds = Array.from(view.container.querySelectorAll('details.teach-depth'));
    const second = folds.find((fold) => fold.textContent?.includes('A second example: what a result means'))!;
    expect(second.hasAttribute('open')).toBe(false);
  });

  it('prints none of the headings for a page without the parts', () => {
    mountTeach(STEPS);
    for (const label of ['Words used here', 'An everyday picture', 'Where the method fails', 'A second example: what a result means']) {
      expect(screen.queryByText(label)).toBeNull();
    }
  });

  it('prints no word list for an empty list', () => {
    mountTeach({ ...STEPS, terms: [] });
    expect(screen.queryByText('Words used here')).toBeNull();
  });

  it('starts the picture fold open inside a native details element', () => {
    const view = mountTeach(FULL);
    const fold = view.container.querySelector('details.teach-depth')!;
    expect(fold.hasAttribute('open')).toBe(true);
    expect(fold.querySelector(':scope > summary')?.textContent).toBe('An everyday picture');
  });
});
