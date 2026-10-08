/**
 * LaTeX that a JSON decode damaged is restored on its way to the screen.
 *
 * The full path: the diagnosis prose arrives with U+000C where `\frac` stood, the panel
 * renders it with the real KaTeX, and the learner sees a fraction, not a box.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from '@testing-library/react';
import katex from 'katex';
import renderMathInElement from 'katex/contrib/auto-render';
import { MathBlock } from '@/components/MathBlock';
import { repairLatex } from '@/lib/latex';
import { resetMathCache } from '@/lib/katex';

beforeEach(() => {
  vi.stubGlobal('renderMathInElement', renderMathInElement);
  vi.stubGlobal('katex', katex);
  resetMathCache();
});
afterEach(() => { vi.restoreAllMocks(); });

const DAMAGED: Array<[string, string]> = [
  ['$\f' + 'rac{3}{4}$', '$\\frac{3}{4}$'],
  ['$2 \t' + 'imes 3$', '$2 \\times 3$'],
  ['$\t' + 'heta = 5$', '$\\theta = 5$'],
  ['$a \n' + 'eq b$', '$a \\neq b$'],
  ['$\t' + 'ext{cm}$', '$\\text{cm}$'],
  ['$\\left( x \r' + 'ight)$', '$\\left( x \\right)$'],
  ['$\b' + 'eta$', '$\\beta$'],
];

describe('repairLatex', () => {
  it.each(DAMAGED)('restores %j', (damaged, fixed) => {
    expect(repairLatex(damaged)).toBe(fixed);
  });

  it('logs each repair', () => {
    const seen: string[] = [];
    repairLatex('$\f' + 'rac{1}{2} \t' + 'imes 2$', ({ command }) => seen.push(command));
    expect(seen).toEqual(['\\frac', '\\times']);
  });

  it('leaves text outside math and unknown endings alone', () => {
    const outside = 'Line one\nneq is text\tand more';
    expect(repairLatex(outside)).toBe(outside);
    const unknown = '$x +\n' + 'umber$';
    expect(repairLatex(unknown)).toBe(unknown);
  });
});

describe('the explanation path', () => {
  it('draws a fraction, a product, an angle, a sign and a bracket from damaged prose', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const prose = DAMAGED.map(([damaged]) => damaged).join(' and ');
    const { container } = render(<MathBlock className="diagnosis-prose">{prose}</MathBlock>);
    const html = container.innerHTML;
    expect(html).toContain('katex');
    expect(html).not.toContain('katex-error');
    expect(html).not.toContain('\f');
    expect(container.textContent).not.toMatch(/[\f\b\r]/);
    const tex = [...container.querySelectorAll('annotation')].map((a) => a.textContent);
    expect(tex).toContain('\\frac{3}{4}');
    expect(tex).toContain('2 \\times 3');
    expect(tex).toContain('\\theta = 5');
    expect(tex).toContain('a \\neq b');
    expect(tex).toContain('\\text{cm}');
    expect(tex).toContain('\\left( x \\right)');
    expect(warn).toHaveBeenCalledTimes(DAMAGED.length);
  });
});
