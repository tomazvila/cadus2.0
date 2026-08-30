/**
 * The visual primitives (S4).
 *
 * The assertions are about SHAPE, not about looks. `app.css` selects on the element type in
 * three places (`.ring-label strong`, `.brand-mark svg`, `.spinner svg`), so a rewrite that
 * keeps every class name and swaps a `<strong>` for a `<span>` passes every other test in
 * the suite and silently loses the size of the number in the ring.
 *
 * The second theme is decoration: each of these is `aria-hidden`, and the text beside it
 * carries the meaning. A tick that announces itself as an image named "tick" tells a screen
 * reader nothing about whether the answer was right.
 */
import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { axe } from 'vitest-axe';
import {
  BrandMark, Chip, Cross, LoadingBlock, Ring, Spinner, Stat, Tick, clamp01, crossPath, tickPath,
} from '@/components/primitives';
import { AXE_IN_JSDOM } from './axe';

describe('BrandMark', () => {
  it('draws four strokes as an inline svg inside .brand-mark', () => {
    const { container } = render(<BrandMark id="m" />);
    expect(container.querySelector('.brand-mark svg')).not.toBeNull();
    expect(container.querySelectorAll('line')).toHaveLength(4);
  });

  it('paints the strokes with the accent gradient it declares', () => {
    const { container } = render(<BrandMark id="m" />);
    expect(container.querySelector('g')?.getAttribute('stroke')).toBe('url(#m)');
    const stops = Array.from(container.querySelectorAll('stop'));
    expect(stops.map((s) => s.getAttribute('style'))).toEqual([
      'stop-color: var(--accent);',
      'stop-color: var(--accent-2);',
    ]);
  });

  it('gives two marks on one page two different gradient ids', () => {
    // A shared id makes the second mark render with the first one's gradient — and on an
    // unmount of the first, with no gradient at all.
    const { container } = render(<><BrandMark /><BrandMark /></>);
    const ids = Array.from(container.querySelectorAll('linearGradient')).map((g) => g.id);
    expect(ids).toHaveLength(2);
    expect(ids[0]).not.toBe(ids[1]);
  });

  it('hides itself from assistive technology', () => {
    const { container } = render(<BrandMark id="m" />);
    expect(container.querySelector('.brand-mark')?.getAttribute('aria-hidden')).toBe('true');
  });
});

describe('Spinner and LoadingBlock', () => {
  it('keeps the spinner an svg, because the animation is on the element', () => {
    const { container } = render(<Spinner />);
    expect(container.querySelector('.spinner svg')).not.toBeNull();
  });

  it('announces the wait through its label, not through the spinner', () => {
    render(<LoadingBlock />);
    expect(screen.getByText('Loading…')).toBeTruthy();
  });

  it('takes a label for a longer wait', () => {
    render(<LoadingBlock label="Building the map…" />);
    expect(screen.getByText('Building the map…')).toBeTruthy();
  });
});

describe('Chip and Stat', () => {
  it('appends the variant class and keeps the base one', () => {
    const { container } = render(<Chip className="chip-good">review</Chip>);
    expect(container.querySelector('span')?.className).toBe('chip chip-good');
    expect(screen.getByText('review')).toBeTruthy();
  });

  it('leaves no trailing space when a chip takes no variant', () => {
    const { container } = render(<Chip>quiz</Chip>);
    expect(container.querySelector('span')?.className).toBe('chip');
  });

  it('splits the stat into a value node and a label node', () => {
    // `.stat.accent .stat-value` colors the NUMBER and not the label. Flattening the pair
    // into one node paints both.
    const { container } = render(<Stat value="12" label="today" className="accent" />);
    expect(container.querySelector('.stat')?.className).toBe('stat accent');
    expect(container.querySelector('.stat-value')?.textContent).toBe('12');
    expect(container.querySelector('.stat-label')?.textContent).toBe('today');
  });
});

describe('Ring', () => {
  /** 2 * PI * 54, to one decimal — the full circumference of the drawn circle. */
  const FULL = '339.3';

  it('keeps <strong> over <span> in the label', () => {
    const { container } = render(<Ring fraction={0.5} label="12" sub="today" />);
    expect(container.querySelector('.ring-label strong')?.textContent).toBe('12');
    expect(container.querySelector('.ring-label span')?.textContent).toBe('today');
  });

  it('draws nothing at zero and the whole circle at one', () => {
    const { container: empty } = render(<Ring fraction={0} />);
    expect(empty.querySelector('.ring-fg')?.getAttribute('stroke-dashoffset')).toBe(FULL);

    const { container: full } = render(<Ring fraction={1} />);
    expect(full.querySelector('.ring-fg')?.getAttribute('stroke-dashoffset')).toBe('0.0');
  });

  it('clamps a fraction outside 0..1 instead of drawing past the circle', () => {
    const { container: over } = render(<Ring fraction={2.5} />);
    expect(over.querySelector('.ring-fg')?.getAttribute('stroke-dashoffset')).toBe('0.0');

    const { container: under } = render(<Ring fraction={-1} />);
    expect(under.querySelector('.ring-fg')?.getAttribute('stroke-dashoffset')).toBe(FULL);
  });

  it('treats a non-finite fraction as zero', () => {
    // A goal of 0 gives `xp / goal = Infinity`, and the ring is the first thing on the
    // dashboard to see it.
    expect(clamp01(Number.NaN)).toBe(0);
    expect(clamp01(Number.POSITIVE_INFINITY)).toBe(0);
    expect(clamp01(0.25)).toBe(0.25);
  });

  it('sets the dash array to the full circumference', () => {
    const { container } = render(<Ring fraction={0.5} />);
    expect(container.querySelector('.ring-fg')?.getAttribute('stroke-dasharray')).toBe(FULL);
    // Half of it is drawn.
    expect(container.querySelector('.ring-fg')?.getAttribute('stroke-dashoffset')).toBe('169.6');
  });
});

describe('Tick and Cross', () => {
  it('keeps the two paths literal, so the marks cannot drift apart', () => {
    expect(tickPath).toBe('M20 6L9 17l-5-5');
    expect(crossPath).toBe('M18 6L6 18M6 6l12 12');
  });

  it('draws each mark in currentColor and hides it from assistive technology', () => {
    const { container } = render(<><Tick /><Cross /></>);
    const svgs = Array.from(container.querySelectorAll('svg'));
    expect(svgs).toHaveLength(2);
    for (const svg of svgs) expect(svg.getAttribute('aria-hidden')).toBe('true');
    expect(container.querySelectorAll('path[stroke="currentColor"]')).toHaveLength(2);
  });
});

describe('the primitives together', () => {
  it('report zero axe violations', async () => {
    const { container } = render(
      <div>
        <BrandMark id="m" />
        <LoadingBlock />
        <Chip className="chip-good">review</Chip>
        <Stat value="12" label="today" />
        <Ring fraction={0.4} label="12" sub="today" />
        <Tick />
        <Cross />
      </div>,
    );
    expect(await axe(container, AXE_IN_JSDOM)).toHaveNoViolations();
  });
});
