/**
 * The mathematical visual component and its sanitizer (unit f9).
 *
 * Two risks live here. The first is accessibility: a picture with no text equivalent hides
 * the question from a learner who reads with a screen reader. The second is injection: the
 * SVG carries author text, and this document must never run it.
 */
import { afterEach, describe, expect, it } from 'vitest';
import { MathVisual, MathVisuals } from '@/components/MathVisual';
import type { RenderedVisual } from '@/lib/visual';
import { sanitizeVisualSvg } from '@/lib/visual';
import { mountRoot } from './helpers/react';

declare global {
  // The flag the hostile markup below tries to set. Nothing declares it, so a read is the
  // proof that nothing ran.
  var __pwned_visual: boolean | undefined;
}

const roots: Array<() => void> = [];

const mount = (node: React.ReactElement) => mountRoot(node, roots);

afterEach(() => {
  roots.splice(0).forEach((fn) => { fn(); });
  globalThis.__pwned_visual = undefined;
});

/** The bytes `cadus_core::visual::render` writes for a small number line. */
const LINE_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 480 200" ' +
  'class="cadus-visual cadus-visual-number_line" role="img" ' +
  'aria-labelledby="q1-title q1-desc">' +
  '<title id="q1-title">Plot 3</title>' +
  '<desc id="q1-desc">A number line from 0 to 5 with a tick every 1.</desc>' +
  '<line x1="36.00" y1="100.00" x2="444.00" y2="100.00" class="cadus-visual-axis"/>' +
  '<circle cx="280.80" cy="100.00" r="5.00" class="cadus-visual-point"/>' +
  '<text x="280.80" y="76.00" text-anchor="middle" class="cadus-visual-label">x</text>' +
  '</svg>';

const line: RenderedVisual = {
  kind: 'number_line',
  svg: LINE_SVG,
  text: 'A number line from 0 to 5 with a tick every 1. A filled point at 3, labeled x.',
};

describe('the sanitizer', () => {
  it('rebuilds a figure the server drew', () => {
    const figure = sanitizeVisualSvg(LINE_SVG);
    expect(figure).not.toBeNull();
    expect(figure?.tagName.toLowerCase()).toBe('svg');
    expect(figure?.getAttribute('role')).toBe('img');
    expect(figure?.getAttribute('aria-labelledby')).toBe('q1-title q1-desc');
    expect(figure?.querySelector('title')?.textContent).toBe('Plot 3');
    expect(figure?.querySelectorAll('line, circle, text')).toHaveLength(3);
    expect(figure?.ownerDocument).toBe(document);
  });

  it('drops the whole figure when it names an element outside the allowlist', () => {
    const hostile =
      '<svg xmlns="http://www.w3.org/2000/svg"><script>globalThis.__pwned_visual = true;' +
      '</script><line x1="0" y1="0" x2="1" y2="1"/></svg>';
    expect(sanitizeVisualSvg(hostile)).toBeNull();
    expect(globalThis.__pwned_visual).toBeUndefined();
  });

  it('drops the whole figure when it carries an attribute outside the allowlist', () => {
    const hostile =
      '<svg xmlns="http://www.w3.org/2000/svg">' +
      '<circle cx="1" cy="1" r="1" onload="globalThis.__pwned_visual = true"/></svg>';
    expect(sanitizeVisualSvg(hostile)).toBeNull();
    expect(globalThis.__pwned_visual).toBeUndefined();
  });

  it('drops markup that does not parse and markup whose root is not an svg', () => {
    expect(sanitizeVisualSvg('<svg><line')).toBeNull();
    expect(sanitizeVisualSvg('<html><body>hi</body></html>')).toBeNull();
    expect(sanitizeVisualSvg('')).toBeNull();
  });
});

describe('the visual component', () => {
  it('draws the figure and prints the text equivalent beside it', () => {
    const view = mount(<MathVisual visual={line} />);
    const svg = view.container.querySelector('svg');
    expect(svg).not.toBeNull();
    expect(svg?.getAttribute('role')).toBe('img');
    expect(view.find('figure').dataset.kind).toBe('number_line');
    expect(view.find('summary').textContent).toBe('Text description');
    expect(view.container.textContent).toContain('A filled point at 3, labeled x.');
  });

  it('keeps the text equivalent when the sanitizer refuses the figure', () => {
    const refused: RenderedVisual = {
      ...line,
      svg: '<svg xmlns="http://www.w3.org/2000/svg"><foreignObject/></svg>',
    };
    const view = mount(<MathVisual visual={refused} />);
    expect(view.container.querySelector('svg')).toBeNull();
    expect(view.container.textContent).toContain('A filled point at 3, labeled x.');
  });

  it('draws one figure per visual and nothing at all for an empty list', () => {
    const empty = mount(<MathVisuals visuals={[]} />);
    expect(empty.container.querySelector('.math-visuals')).toBeNull();

    const pair: RenderedVisual = { kind: 'fraction', svg: LINE_SVG, text: 'A fraction bar.' };
    const view = mount(<MathVisuals visuals={[line, pair]} />);
    expect(view.all('figure')).toHaveLength(2);
    expect(view.all('svg')).toHaveLength(2);
    expect(view.all('figure')[1].dataset.kind).toBe('fraction');
  });
});
