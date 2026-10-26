/**
 * The figure of a teach page (brief 09, part A): one checked figure above the worked example.
 *
 * The service sends `worked_example.figure` as `{kind, svg, text}` for a page that names a
 * figure. The key is absent otherwise, and then the page looks as it did before.
 */
import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Teach, WorkedFold } from '@/views/session/Teach';
import type { TeachResponse } from '@/api/types';
import { LESSON, TEACHING } from './helpers/session';

const FIGURE = {
  kind: 'fraction',
  svg:
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 480 200" class="cadus-visual cadus-visual-fraction" role="img">' +
    '<title>A bar of 6 parts.</title><rect x="36" y="36" width="68" height="90" class="cadus-visual-part-shaded"/></svg>',
  text: 'A fraction bar divided into 6 equal parts, with 4 of them shaded.',
};

const WITH_FIGURE: TeachResponse = {
  ...TEACHING,
  worked_example: { ...TEACHING.worked_example, figure: FIGURE },
};

const mountTeach = (instruction: TeachResponse) =>
  render(<Teach task={LESSON} instruction={instruction} onContinue={() => {}} />);

describe('the figure of a teach page', () => {
  it('draws one figure above the "Example" label', () => {
    const view = mountTeach(WITH_FIGURE);
    const figures = view.container.querySelectorAll('.math-visual');
    expect(figures.length).toBe(1);
    const example = view.container.querySelector('.teach-example')!;
    expect(example.firstElementChild!.classList.contains('math-visuals')).toBe(true);
    const label = screen.getByText('Example');
    expect(figures[0].compareDocumentPosition(label) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(view.container.querySelector('.math-visual svg')).not.toBeNull();
    expect(screen.getByText(FIGURE.text)).toBeTruthy();
  });

  it('draws no figure for a page without the key', () => {
    const view = mountTeach(TEACHING);
    expect(view.container.querySelector('.math-visual')).toBeNull();
    expect(view.container.querySelector('.math-visuals')).toBeNull();
  });

  it('keeps the text description when the sanitizer refuses the SVG', () => {
    const bad = { ...FIGURE, svg: '<svg xmlns="http://www.w3.org/2000/svg"><script>1</script></svg>' };
    const view = mountTeach({ ...TEACHING, worked_example: { ...TEACHING.worked_example, figure: bad } });
    expect(view.container.querySelector('.math-visual')).not.toBeNull();
    expect(view.container.querySelector('.math-visual svg')).toBeNull();
    expect(screen.getByText(FIGURE.text)).toBeTruthy();
  });

  it('shows the same figure in the fold above the answer field', () => {
    const view = render(<WorkedFold taskType="lesson" example={WITH_FIGURE.worked_example} />);
    expect(view.container.querySelectorAll('.worked-fold .math-visual').length).toBe(1);
    const none = render(<WorkedFold taskType="lesson" example={TEACHING.worked_example} />);
    expect(none.container.querySelector('.math-visual')).toBeNull();
  });
});
