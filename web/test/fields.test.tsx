/**
 * `AnswerField` — the keyboard, caret and handle contract.
 *
 * The caret tests are the ones that matter. A controlled React input breaks all three by
 * default, because the caret jumps to the end on every re-render, and nothing about that edit
 * looks wrong in review.
 *
 * The first-frame test guards the other half of the same design: the handlers sit in the JSX
 * and are live in the first commit. Installed from a passive effect, the learner's first Enter
 * reaches nothing.
 */
import { useEffect, useLayoutEffect, useRef, createRef } from 'react';
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { AnswerField, type AnswerFieldHandle } from '@/components/AnswerField';
import { mountRoot } from './helpers/react';

const roots: Array<() => void> = [];
afterEach(() => { roots.splice(0).forEach((fn) => { fn(); }); });

const mount = (node: React.ReactElement) => mountRoot(node, roots);

/** A raw keydown. user-event refuses to dispatch to a disabled element, so it proves nothing. */
function keydown(node: HTMLElement, key: string, init: KeyboardEventInit = {}): KeyboardEvent {
  const ev = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init });
  node.dispatchEvent(ev);
  return ev;
}

// ---------------------------------------------------------------------------
// Acceptance 3 — the first frame after mount.
// ---------------------------------------------------------------------------

describe('the answer field in the frame it mounts in', () => {
  it('S5 acceptance: Enter in the first frame after mount posts once, not zero times', () => {
    const onSubmit = vi.fn();
    const order: string[] = [];

    /**
     * A learner who hits Enter before the browser paints.
     *
     * The layout effect runs in the SAME commit as the mount and before every passive
     * effect, so this dispatch is the first frame. A view that installs its key handling from
     * a passive effect drops this Enter, and the count reads 0.
     */
    function FirstFrame() {
      const hostRef = useRef<HTMLDivElement>(null);
      useEffect(() => { order.push('passive effect'); }, []);
      useLayoutEffect(() => {
        order.push('Enter');
        const input = hostRef.current!.querySelector<HTMLInputElement>('.answer-input')!;
        keydown(input, 'Enter');
      }, []);
      return <div ref={hostRef}><AnswerField onSubmit={onSubmit} /></div>;
    }

    mount(<FirstFrame />);

    // The Enter really did land before the passive effects ran.
    expect(order).toEqual(['Enter', 'passive effect']);
    // Once. Not zero, which is a dropped first submit, and not twice, which is a double post
    // into an append-only log.
    expect(onSubmit).toHaveBeenCalledTimes(1);
  });
});

// ---------------------------------------------------------------------------
// The keyboard contract.
// ---------------------------------------------------------------------------

describe('AnswerField: the keyboard contract', () => {
  it('Enter submits, and prevents the default', () => {
    const onSubmit = vi.fn();
    const m = mount(<AnswerField onSubmit={onSubmit} />);
    const ev = keydown(m.find('.answer-input'), 'Enter');

    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(ev.defaultPrevented).toBe(true);
  });

  it('Enter is inert while the field is disabled or readOnly', () => {
    const onSubmit = vi.fn();
    const m = mount(<AnswerField onSubmit={onSubmit} />);
    const input = m.find<HTMLInputElement>('.answer-input');

    input.disabled = true;
    keydown(input, 'Enter');
    input.disabled = false;
    input.readOnly = true;
    keydown(input, 'Enter');

    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('the disabled PROP is the one owner of the disabled state', () => {
    // The handle offers no `setDisabled`. React never rewrites an unchanged prop, so an
    // imperative flip survives a re-render and the prop then lies.
    const m = mount(<AnswerField disabled />);
    expect(m.find<HTMLInputElement>('.answer-input').disabled).toBe(true);
  });

  it('a letter is a letter: h lands in the field and asks for nothing', () => {
    const m = mount(<AnswerField />);
    const input = m.find<HTMLInputElement>('.answer-input');
    // "Write an inequality for the height h": the answer starts with h.
    const lower = keydown(input, 'h');
    expect(lower.defaultPrevented).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// The caret contract.
// ---------------------------------------------------------------------------

describe('AnswerField: the field itself', () => {
  it('asks for an answer, and spells nothing for the learner', () => {
    const m = mount(<AnswerField />);
    const input = m.find<HTMLInputElement>('.answer-input');
    expect(input.placeholder).toBe('Your answer');
    expect(input.getAttribute('spellcheck')).toBe('false');
  });

  it('takes an Enter with nobody to tell', () => {
    const m = mount(<AnswerField />);
    expect(() => keydown(m.find('.answer-input'), 'Enter')).not.toThrow();
  });

  it('gives the input the focus back after a symbol key', () => {
    const m = mount(<AnswerField />);
    const input = m.find<HTMLInputElement>('.answer-input');
    document.body.focus();
    expect(document.activeElement).not.toBe(input);

    act(() => { m.find('.sym-key').click(); });
    expect(document.activeElement).toBe(input);
  });
});

describe('AnswerField: the caret contract', () => {
  it('inserts a symbol AT the caret and leaves the caret after it', () => {
    const m = mount(<AnswerField />);
    const input = m.find<HTMLInputElement>('.answer-input');

    input.value = '12';
    input.setSelectionRange(1, 1);
    const sqrt = m.all('.sym-key').find((k) => k.textContent === '√(')!;
    act(() => { sqrt.click(); });

    // `√(` carries its opening paren so the radicand lands inside it, so the caret has to
    // land after the paren. A controlled input puts it at the end of the value.
    expect(input.value).toBe('1√(2');
    expect(input.selectionStart).toBe(3);
    expect(input.selectionEnd).toBe(3);
  });

  it('the root key inserts root(n, with the chosen index and the caret inside the bracket', () => {
    const m = mount(<AnswerField />);
    const input = m.find<HTMLInputElement>('.answer-input');
    const select = m.find<HTMLSelectElement>('select[aria-label="Root index"]');
    act(() => {
      const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')!.set!;
      setter.call(select, '5');
      select.dispatchEvent(new Event('change', { bubbles: true }));
    });
    const key = m.all('.sym-key').find((k) => k.getAttribute('aria-label') === 'Insert indexed root')!;
    act(() => { key.click(); });
    expect(input.value).toBe('root(5, ');
    expect(input.selectionStart).toBe(8);
  });

  it('replaces the selection rather than inserting beside it', () => {
    const m = mount(<AnswerField />);
    const input = m.find<HTMLInputElement>('.answer-input');
    input.value = 'abc';
    input.setSelectionRange(0, 3);
    act(() => { m.all('.sym-key')[0].click(); });
    expect(input.value).toBe('∞');
  });

  it('appends at the end when the browser reports no caret', () => {
    // A host that answers null for the selection — the shape of an input type with no
    // selection API — still takes the symbol, at the end of the value.
    const m = mount(<AnswerField />);
    const input = m.find<HTMLInputElement>('.answer-input');
    input.value = '12';
    Object.defineProperty(input, 'selectionStart', { get: () => null });
    Object.defineProperty(input, 'selectionEnd', { get: () => null });
    act(() => { m.all('.sym-key')[0].click(); });
    expect(input.value).toBe('12∞');
  });

  it('suppresses mousedown so the caret survives the tap', () => {
    const m = mount(<AnswerField />);
    const ev = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
    act(() => { m.all('.sym-key')[0].dispatchEvent(ev); });

    // Without this the key takes focus before the click lands, and the recorded caret
    // position is gone by the time the insert reads it.
    expect(ev.defaultPrevented).toBe(true);
  });

  it('keeps the typed value and the caret across a parent re-render', () => {
    function Parent({ disabled }: { disabled: boolean }) {
      return <AnswerField disabled={disabled} />;
    }
    const container = document.createElement('div');
    document.body.append(container);
    const root = createRoot(container);
    act(() => { root.render(<Parent disabled={false} />); });
    roots.push(() => { act(() => { root.unmount(); }); container.remove(); });

    const input = container.querySelector<HTMLInputElement>('.answer-input')!;
    input.value = '5/6';
    input.setSelectionRange(1, 1);

    // The problem card re-renders about once a second while a clock ticks. A controlled
    // value driven by parent state resets here.
    act(() => { root.render(<Parent disabled={false} />); });

    expect(input.value).toBe('5/6');
    expect(input.selectionStart).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// The handle and the accessible names.
// ---------------------------------------------------------------------------

describe('AnswerField: the handle and accessibility', () => {
  it('reads the TRIMMED value synchronously, the way a submit does', () => {
    const ref = createRef<AnswerFieldHandle>();
    const m = mount(<AnswerField ref={ref} />);
    m.find<HTMLInputElement>('.answer-input').value = '  -12  ';

    // No await and no render in between: this is how a phase-gated submit reads it.
    expect(ref.current!.value()).toBe('-12');
    ref.current!.clear();
    expect(ref.current!.value()).toBe('');
  });

  it('reads an empty value once the field is gone', () => {
    // A continuation that kept the handle past the unmount reads nothing, not a throw.
    const ref = createRef<AnswerFieldHandle>();
    const m = mount(<AnswerField ref={ref} />);
    const handle = ref.current!;
    m.find<HTMLInputElement>('.answer-input').value = '5';
    m.unmount();
    expect(handle.value()).toBe('');
    expect(() => { handle.clear(); handle.focus(); }).not.toThrow();
  });

  it('focuses the input through the handle, for a fresh problem', () => {
    const ref = createRef<AnswerFieldHandle>();
    const m = mount(<AnswerField ref={ref} />);
    act(() => { ref.current!.focus(); });
    expect(document.activeElement).toBe(m.find('.answer-input'));
  });

  it('names the field, the toolbar and every symbol key', () => {
    const m = mount(<AnswerField />);
    expect(m.find('.answer-input').getAttribute('aria-label')).toBe('Answer');
    expect(m.find('.sym-palette').getAttribute('role')).toBe('toolbar');
    expect(m.find('.sym-palette').getAttribute('aria-label')).toBe('Symbol keys. Use the arrow keys to move.');
    for (const key of m.all('.sym-key')) {
      expect(key.getAttribute('aria-label')).toMatch(/^Insert /);
    }
    expect(m.find('.sym-help').textContent).toBe('How to type answers');
  });

  it('keeps all keys in order behind More symbols when the problem has no contract', () => {
    const m = mount(<AnswerField />);
    expect(m.all('.sym-key').map((k) => k.textContent))
      .toEqual(['∞', 'π', '√(', '^', '≤', '≥', '≠', '±', '×', '÷', '°', 'θ', 'a b/c', 'ⁿ√']);
    expect(m.find('.sym-more summary').textContent).toBe('More symbols');
    expect(m.find('.sym-more').hasAttribute('open')).toBe(false);
  });

  it('gives every symbol key type="button", so it never submits a form', () => {
    const m = mount(<AnswerField />);
    for (const key of m.all('.sym-key')) expect(key.getAttribute('type')).toBe('button');
  });
});

// ---------------------------------------------------------------------------
// The working area.
// ---------------------------------------------------------------------------

describe('the study-field stylesheet', () => {
  const css = readFileSync(resolve(process.cwd(), 'src/styles/app.css'), 'utf8')
    .replace(/\/\*[\s\S]*?\*\//g, ' ');

  it('sizes the KaTeX output through a DESCENDANT selector', () => {
    expect(css).toContain('.problem-text .katex { font-size: 1.15em; }');
  });

  it('gives every symbol key the 44px minimum target height', () => {
    const rule = /\.sym-key\s*\{([^}]*)\}/.exec(css);
    expect(rule).not.toBeNull();
    // Spec section 4.5. 1.0 shipped 26px keys, and a thumb hits the wrong symbol.
    expect(rule![1]).toContain('min-height: 44px');
  });

  it('never writes outline: none on a focused field', () => {
    // The accent border marks focus for a mouse user. Removing the outline with it takes the
    // ring away from a keyboard user, and no test of the border catches that.
    const focus = css.match(/:focus\s*\{[^}]*\}/g) ?? [];
    expect(focus.length).toBeGreaterThan(0);
    for (const rule of focus) expect(rule).not.toContain('outline: none');
  });
});
