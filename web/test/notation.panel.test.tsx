/**
 * "How to type answers": the cheat sheet panel and its example rows.
 *
 * The rows come from `notation-examples.json`; a Rust test grades each one. These tests cover
 * the panel: both entries open it, the layout follows the width, the search filters, and a
 * chip writes into the answer field at the caret.
 */
import { act } from 'react';
import { createRef } from 'react';
import katex from 'katex';
import renderMathInElement from 'katex/contrib/auto-render';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AnswerField, type AnswerFieldHandle } from '@/components/AnswerField';
import { AnswerPreview } from '@/components/AnswerPreview';
import { NotationPanel } from '@/components/NotationPanel';
import { NOTATION_SECTIONS, filterSections, resetNotationStore, setNotationOpen } from '@/lib/notation';
import { parsePreview } from '@/lib/preview';
import { mountRoot } from './helpers/react';

const roots: Array<() => void> = [];

function setWidth(px: number): void {
  Object.defineProperty(window, 'innerWidth', { configurable: true, value: px });
  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    value: (q: string) => ({
      matches: px >= 1400 && q.includes('1400'),
      addEventListener: () => {}, removeEventListener: () => {},
    }),
  });
}

beforeEach(() => { window.localStorage.clear(); resetNotationStore(); setWidth(1440); });
afterEach(() => {
  roots.splice(0).forEach((fn) => { fn(); });
  act(() => { setNotationOpen(false); });
});

function mountAll() {
  const ref = createRef<AnswerFieldHandle>();
  const m = mountRoot(<><AnswerField ref={ref} /><NotationPanel /></>, roots);
  return { m, ref };
}

describe('the example rows', () => {
  it('hold at least ten sections of rows with a want, a typed and a LaTeX spelling', () => {
    expect(NOTATION_SECTIONS.length).toBeGreaterThanOrEqual(10);
    for (const s of NOTATION_SECTIONS) {
      expect(s.rows.length).toBeGreaterThan(0);
      for (const r of s.rows) {
        expect(r.want).not.toBe('');
        expect(r.type).not.toBe('');
        expect(r.latex).not.toBe('');
      }
    }
  });

  it('each typed spelling goes through the preview without an error', () => {
    for (const s of NOTATION_SECTIONS) {
      for (const r of s.rows) {
        expect(() => parsePreview(r.type)).not.toThrow();
        const m = mountRoot(<AnswerPreview text={r.type} />, roots);
        expect(m.find('[data-testid="answer-preview"]')).toBeTruthy();
        m.unmount();
      }
    }
  });

  it('each LaTeX spelling renders as math with no KaTeX error', () => {
    // The real renderer, in place of the recording stub of `setup.ts`.
    vi.stubGlobal('renderMathInElement', renderMathInElement);
    vi.stubGlobal('katex', katex);
    act(() => { setNotationOpen(true); });
    const m = mountRoot(<NotationPanel />, roots);
    expect(m.all('.notation-row').length).toBe(NOTATION_SECTIONS.reduce((n, s) => n + s.rows.length, 0));
    expect(m.all('.katex-error')).toHaveLength(0);
    expect(m.all('.notation-rendered .katex').length).toBe(m.all('.notation-row').length);
  });

  it('the search keeps the rows that match every word and drops the rest', () => {
    const hit = filterSections(NOTATION_SECTIONS, 'fifth root');
    expect(hit.flatMap((s) => s.rows).some((r) => r.type === 'root(5, y)')).toBe(true);
    expect(hit.flatMap((s) => s.rows).length).toBeLessThan(5);
    expect(filterSections(NOTATION_SECTIONS, 'zzzz')).toEqual([]);
    expect(filterSections(NOTATION_SECTIONS, '  ')).toBe(NOTATION_SECTIONS);
  });
});

describe('the panel', () => {
  it('is closed at first and opens from the How to type answers link', () => {
    const { m } = mountAll();
    expect(m.container.querySelector('.notation-panel')).toBeNull();
    act(() => { m.find<HTMLButtonElement>('.sym-help').click(); });
    expect(m.find('.notation-panel h2').textContent).toBe('How to type answers');
  });

  it('is a side panel on a wide viewport and leaves the page column alone', () => {
    act(() => { setNotationOpen(true); });
    const { m } = mountAll();
    expect(m.find('.notation-panel').classList.contains('notation-side')).toBe(true);
    expect(m.find('.notation-panel').getAttribute('role')).toBeNull();
    expect(document.body.className).not.toContain('notation-open');
  });

  it('is a sheet, not a squeeze, on a laptop whose margin cannot hold the panel', () => {
    setWidth(1280);
    act(() => { setNotationOpen(true); });
    const { m } = mountAll();
    expect(m.find('.notation-panel').classList.contains('notation-sheet')).toBe(true);
  });

  it('is a full-screen sheet with a close button on a narrow viewport', () => {
    setWidth(600);
    act(() => { setNotationOpen(true); });
    const { m } = mountAll();
    const panel = m.find('.notation-panel');
    expect(panel.classList.contains('notation-sheet')).toBe(true);
    expect(panel.getAttribute('role')).toBe('dialog');
    const close = Array.from(panel.querySelectorAll('button')).find((b) => b.textContent === 'Close')!;
    act(() => { close.click(); });
    expect(m.container.querySelector('.notation-panel')).toBeNull();
  });

  it('keeps its open state across a remount and in localStorage', () => {
    act(() => { setNotationOpen(true); });
    expect(window.localStorage.getItem('cadus.notation.open')).toBe('1');
    const a = mountAll();
    a.m.unmount();
    const b = mountAll();
    expect(b.m.container.querySelector('.notation-panel')).not.toBeNull();
  });

  it('filters the rows by the words in the search box', () => {
    act(() => { setNotationOpen(true); });
    const { m } = mountAll();
    const before = m.all('.notation-row').length;
    const box = m.find<HTMLInputElement>('.notation-search');
    act(() => {
      const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!;
      set.call(box, 'cube root');
      box.dispatchEvent(new Event('input', { bubbles: true }));
    });
    const after = m.all('.notation-row');
    expect(after.length).toBeGreaterThan(0);
    expect(after.length).toBeLessThan(before);
    expect(after.some((r) => r.textContent!.includes('root(3, 8)'))).toBe(true);
  });

  it('writes a tapped chip into the answer field at the caret', () => {
    act(() => { setNotationOpen(true); });
    const { m } = mountAll();
    const input = m.find<HTMLInputElement>('.answer-input');
    input.value = '2+3';
    input.setSelectionRange(2, 2);
    const chip = m.all('.notation-chip').find((c) => c.textContent === 'sqrt(2)')!;
    act(() => { chip.click(); });
    expect(input.value).toBe('2+sqrt(2)3');
    expect(input.selectionStart).toBe('2+sqrt(2)'.length);
    const latexChip = m.all('.notation-chip').find((c) => c.textContent === '\\sqrt{2}')!;
    act(() => { latexChip.click(); });
    expect(input.value).toBe('2+sqrt(2)\\sqrt{2}3');
  });
});
