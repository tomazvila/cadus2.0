/**
 * The KaTeX string idiom, against the REAL renderer.
 *
 * `setup.ts` installs a recording no-op as `window.renderMathInElement`, because most units
 * only need the call count. This file stubs the real KaTeX over it. The whole risk of the
 * rewrite is what the DOM does when a component that owns KaTeX-mutated nodes re-renders, and
 * a no-op stub shows none of that.
 *
 * A regression here does not degrade politely: the learner reads raw `$\dfrac{1}{2}$` in the
 * middle of a problem, or the page goes blank.
 */
import { useEffect, useState } from 'react';
import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import katex from 'katex';
import renderMathInElement from 'katex/contrib/auto-render';
import { MathBlock } from '@/components/MathBlock';
import { DELIMITERS, renderMathToHtml } from '@/lib/katex';
import { mountRoot } from './helpers/react';

declare global {
  // The flag the hostile markup below tries to set. Nothing declares it, so a read is the
  // proof that nothing ran.
  var __pwned: boolean | undefined;
}

const roots: Array<() => void> = [];

const mount = (node: React.ReactElement) => mountRoot(node, roots);

beforeEach(() => {
  // The real thing, in place of the recording stub of `setup.ts`.
  vi.stubGlobal('renderMathInElement', renderMathInElement);
  vi.stubGlobal('katex', katex);
});

afterEach(() => {
  roots.splice(0).forEach((fn) => { fn(); });
});

// ---------------------------------------------------------------------------
// The renderer itself.
// ---------------------------------------------------------------------------

describe('the KaTeX string idiom', () => {
  it('renders all four delimiter pairs', () => {
    for (const src of ['$x+1$', '$$x+1$$', '\\(x+1\\)', '\\[x+1\\]']) {
      const html = renderMathToHtml(src);
      expect(html, `${src} did not render`).toContain('katex');
      expect(html).not.toContain(src);
    }
  });

  it('tries $$ before $, because the reverse order eats a display block', () => {
    expect(DELIMITERS.map((d) => d.left)).toEqual(['$$', '\\[', '$', '\\(']);
    const html = renderMathToHtml('$$x+1$$');
    // A `$` open on a display block leaves the second `$` of the pair in the prose.
    expect(html).not.toContain('$');
  });

  it('leaves the prose around the math untouched', () => {
    const html = renderMathToHtml('Work out $\\dfrac{1}{2} + \\dfrac{1}{3}$ now.');
    expect(html).toContain('Work out');
    expect(html).toContain('now.');
    expect(html).toContain('katex');
  });

  it('leaves malformed LaTeX visible instead of throwing', () => {
    // throwOnError: false. A bad macro from the model must not blank the problem: the
    // learner still reads the raw text.
    expect(() => renderMathToHtml('$\\badmacro{')).not.toThrow();
    expect(renderMathToHtml('$\\badmacro{')).toBeTruthy();
  });

  it('degrades to escaped plain text when the vendored scripts are absent', () => {
    vi.stubGlobal('renderMathInElement', undefined);
    // The symptom of a failed vendor load is raw LaTeX, never an exception in a render.
    expect(renderMathToHtml('$x+1$')).toBe('$x+1$');
  });

  it('shows the escaped source when the renderer throws', () => {
    // A renderer that throws must not throw INTO a React render: the learner reads the raw
    // text instead of a blank problem.
    vi.stubGlobal('renderMathInElement', () => { throw new Error('katex exploded'); });
    expect(renderMathToHtml('<b>$x$</b>')).toBe('&lt;b&gt;$x$&lt;/b&gt;');
  });

  it('holds sixty-four renders and re-renders the oldest after that', () => {
    const spy = vi.fn(renderMathInElement);
    vi.stubGlobal('renderMathInElement', spy);
    for (let i = 0; i < 64; i += 1) renderMathToHtml(`$x_{${i}}$`);
    expect(spy).toHaveBeenCalledTimes(64);
    // Every one of the sixty-four is still cached.
    renderMathToHtml('$x_{0}$');
    expect(spy).toHaveBeenCalledTimes(64);
    // The sixty-fifth evicts the oldest, and the oldest renders again.
    renderMathToHtml('$x_{64}$');
    renderMathToHtml('$x_{0}$');
    expect(spy).toHaveBeenCalledTimes(66);
    // That render evicted the next oldest in turn; the one after it is still there.
    renderMathToHtml('$x_{2}$');
    expect(spy).toHaveBeenCalledTimes(66);
    renderMathToHtml('$x_{1}$');
    expect(spy).toHaveBeenCalledTimes(67);
  });

  it('renders one source string once, across a remount as well', () => {
    const spy = vi.fn(renderMathInElement);
    vi.stubGlobal('renderMathInElement', spy);

    const first = mount(<MathBlock>{'$a+b$'}</MathBlock>);
    expect(spy).toHaveBeenCalledTimes(1);
    first.unmount();

    // `key={problem_id}` remounts the problem subtree at every problem, and the feedback
    // panel repeats the statement. The module-scope cache of `lib/katex.ts` covers both.
    mount(<MathBlock>{'$a+b$'}</MathBlock>);
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it('renders again when the source text changes', () => {
    const spy = vi.fn(renderMathInElement);
    vi.stubGlobal('renderMathInElement', spy);
    const m = mount(<MathBlock>{'$a$'}</MathBlock>);
    const first = m.find('.problem-text')!.innerHTML;

    mount(<MathBlock>{'$b$'}</MathBlock>);
    // A memo that never invalidates is a defect, not an optimization: the next problem then
    // shows the previous problem's math.
    expect(spy).toHaveBeenCalledTimes(2);
    expect(m.find('.problem-text')!.innerHTML).toBe(first);
  });
});

// ---------------------------------------------------------------------------
// Acceptance 1 — the 1 Hz re-render.
// ---------------------------------------------------------------------------

describe('the problem card under a 1 Hz clock', () => {
  /** A card with a clock beside the math, exactly as the session and quiz views build one. */
  function Card({ text }: { text: string }) {
    const [secs, setSecs] = useState(0);
    useEffect(() => {
      const id = setInterval(() => { setSecs((n) => n + 1); }, 1000);
      return () => { clearInterval(id); };
    }, []);
    return (
      <div>
        <span className="clock">{String(secs)}</span>
        <MathBlock>{text}</MathBlock>
      </div>
    );
  }

  it('S5 acceptance: a 1 Hz re-render does not re-run KaTeX and does not flash raw LaTeX', () => {
    vi.useFakeTimers();
    const spy = vi.fn(renderMathInElement);
    vi.stubGlobal('renderMathInElement', spy);

    const m = mount(<Card text={'Work out $\\dfrac{1}{2} + \\dfrac{1}{3}$.'} />);
    const block = m.find('.problem-text')!;
    const before = block.innerHTML;
    expect(spy).toHaveBeenCalledTimes(1);
    expect(before).toContain('katex');

    // Watch the rendered subtree. The naive port re-applies `innerHTML` at every tick, and
    // the 1.0 port measured that as a childList mutation here once a second.
    const seen: MutationRecord[] = [];
    const observer = new MutationObserver((records) => { seen.push(...records); });
    observer.observe(block, { childList: true, characterData: true, subtree: true });

    act(() => { vi.advanceTimersByTime(10_000); });

    // The clock DID move, so nothing passes because nothing re-rendered.
    expect(m.find('.clock')!.textContent).toBe('10');

    seen.push(...observer.takeRecords());
    observer.disconnect();
    expect(seen).toHaveLength(0);
    expect(spy).toHaveBeenCalledTimes(1);
    expect(block.innerHTML).toBe(before);

    // NOT `textContent`. KaTeX keeps the source TeX in
    // `<annotation encoding="application/x-tex">`, so `\dfrac` legitimately stays in
    // `textContent` forever. What the learner reads is `.katex-html`.
    const annotation = block.querySelector('annotation[encoding="application/x-tex"]');
    expect(annotation?.textContent).toContain('\\dfrac');
    const visible = block.querySelector('.katex-html')?.textContent ?? '';
    expect(visible).not.toContain('\\dfrac');
    expect(visible).not.toContain('$');

    // One KaTeX root, not one per tick: a second auto-render pass over rendered output
    // re-processes the annotation and nests a second `.katex` inside the first.
    expect(block.querySelectorAll('.katex')).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// Acceptance 2 — the escaping property.
// ---------------------------------------------------------------------------

describe('problem text is model output, and is escaped', () => {
  it('S5 acceptance: <img src=x onerror=alert(1)> in problem text renders as visible text', () => {
    const hostile = '<img src=x onerror=alert(1)>';
    const m = mount(<MathBlock>{hostile}</MathBlock>);
    const block = m.find('.problem-text')!;

    // No element was created, so no handler exists to fire.
    expect(m.container.querySelector('img')).toBeNull();
    // The learner reads the tag as text, character for character.
    expect(block.textContent).toBe('<img src=x onerror=alert(1)>');
    expect(block.innerHTML).toContain('&lt;img src=x onerror=alert(1)&gt;');
  });

  it('runs no injected handler once the block is mounted', () => {
    const m = mount(
      <MathBlock>{'<img src=x onerror="globalThis.__pwned = true">'}</MathBlock>,
    );
    expect(m.container.querySelector('img')).toBeNull();
    expect(globalThis.__pwned).toBeUndefined();
  });

  it('escapes hostile markup that sits beside real math', () => {
    const html = renderMathToHtml('$x^2$ <b>bold</b> <script>alert(2)</script>');
    expect(html).toContain('katex');
    expect(html).not.toContain('<b>');
    expect(html).not.toContain('<script');
    expect(html).toContain('&lt;b&gt;');
    expect(html).toContain('&lt;script&gt;');
  });
});

// ---------------------------------------------------------------------------
// The cascade.
// ---------------------------------------------------------------------------

describe('MathBlock and the stylesheet', () => {
  it('puts the KaTeX output INSIDE the classed node', () => {
    const m = mount(<MathBlock>{'$x$'}</MathBlock>);
    // `app.css` targets `.problem-text .katex`. A wrapper between the two silently loses the
    // sizing override.
    expect(m.container.querySelector('.problem-text .katex')).toBeTruthy();
  });

  it('takes a custom class for the solution and the hint bodies', () => {
    const m = mount(<MathBlock className="solution-text">{'$x$'}</MathBlock>);
    expect(m.container.querySelector('.solution-text .katex')).toBeTruthy();
  });
});
