import { StrictMode } from 'react';
import { render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { MathVisual } from '@/components/MathVisual';
import { sanitizeVisualSvg, type RenderedVisual } from '@/lib/visual';

const figure = (label: string): RenderedVisual => ({
  kind: 'number_line',
  svg: `<svg xmlns="http://www.w3.org/2000/svg"><title>${label}</title></svg>`,
  text: label,
});

it('replaces a changed figure once in StrictMode and clears it when its replacement is refused', () => {
  const view = render(<StrictMode><MathVisual visual={figure('Point at 2')} /></StrictMode>);
  expect(view.container.querySelectorAll('svg')).toHaveLength(1);
  expect(view.container.querySelector('title')?.textContent).toBe('Point at 2');

  view.rerender(<StrictMode><MathVisual visual={figure('Point at 5')} /></StrictMode>);
  expect(view.container.querySelectorAll('svg')).toHaveLength(1);
  expect(view.container.querySelector('title')?.textContent).toBe('Point at 5');
  expect(screen.queryByText('Point at 2')).toBeNull();

  const refused = { ...figure('Point at 7'), svg: '<svg><script/></svg>' };
  view.rerender(<StrictMode><MathVisual visual={refused} /></StrictMode>);
  expect(view.container.querySelector('svg')).toBeNull();
  expect(screen.getByText('Point at 7')).toBeTruthy();
  view.unmount();
  expect(view.container.childNodes).toHaveLength(0);
});

it('preserves decoded text, whitespace and nested labels as inert text nodes', () => {
  const markup = '<svg xmlns="http://www.w3.org/2000/svg"><text>  &lt;x&gt; &amp; '
    + '<tspan>y</tspan>  </text><title></title></svg>';
  const svg = sanitizeVisualSvg(markup);
  expect(svg?.querySelector('text')?.textContent).toBe('  <x> & y  ');
  expect(svg?.querySelector('tspan')?.textContent).toBe('y');
  expect(svg?.querySelector('title')?.textContent).toBe('');
  expect(svg?.querySelector('x')).toBeNull();
});

it.each(['<!-- author comment -->', '<![CDATA[label]]>', '<?label value?>'])(
  'rejects non-element, non-text child nodes: %s', (child) => {
    expect(sanitizeVisualSvg(`<svg xmlns="http://www.w3.org/2000/svg">${child}</svg>`)).toBeNull();
  },
);

it('keeps the text equivalent when the browser refuses to parse SVG', () => {
  vi.spyOn(DOMParser.prototype, 'parseFromString').mockImplementation(() => {
    throw new TypeError('Trusted Types policy refused the input');
  });
  const view = render(<MathVisual visual={figure('A point at 4')} />);
  expect(view.container.querySelector('svg')).toBeNull();
  expect(screen.getByText('A point at 4')).toBeTruthy();
  expect(screen.getByText('Text description')).toBeTruthy();
});
