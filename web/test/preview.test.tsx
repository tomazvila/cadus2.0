/** The answer preview, the input hint by contract, the mixed-number key and the notation hint. */
import { act } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { AnswerField, inputHint, PREVIEW_DEBOUNCE_MS } from '@/components/AnswerField';
import { notationHint, Feedback } from '@/views/session/Feedback';
import { parsePreview, signature } from '@/lib/preview';
import { mountRoot } from './helpers/react';
import type { AnswerResponse } from '@/api/types';

const roots: Array<() => void> = [];
afterEach(() => { roots.splice(0).forEach((fn) => { fn(); }); vi.useRealTimers(); });

const sig = (t: string) => { const n = parsePreview(t); return n ? signature(n) : null; };

describe('parsePreview', () => {
  it('reads fractions, mixed numbers and the mixed joiners', () => {
    expect(sig('3/4')).toBe('frac(3,4)');
    expect(sig('4 2/5')).toBe('mixed(4,2,5)');
    expect(sig('4 and 2/5')).toBe('mixed(4,2,5)');
    expect(sig('4_2/5')).toBe('mixed(4,2,5)');
    expect(sig('4+2/5')).toBe('4 + frac(2,5)');
  });
  it('shows * as times, sqrt as a root and ^ as a power', () => {
    expect(sig('4 * 2/5')).toBe('4 × frac(2,5)');
    expect(sig('sqrt(2)')).toBe('√2');
    expect(sig('x^2 - 3')).toBe('pow(x,2) − 3');
    expect(sig('2x+1')).toBe('2 x + 1');
    expect(sig('6 ÷ 3')).toBe('6 ÷ 3');
  });
  it('returns null when it cannot read the text', () => {
    expect(parsePreview('')).toBeNull();
    expect(parsePreview('(1+')).toBeNull();
    expect(parsePreview('3 # 4')).toBeNull();
    expect(parsePreview('4 and x')).toBeNull();
  });
});

describe('the answer preview in the field', () => {
  it('draws after 150 ms, with a stacked fraction, and shows raw text when unreadable', () => {
    vi.useFakeTimers();
    const m = mountRoot(<AnswerField />, roots);
    const input = m.find<HTMLInputElement>('.answer-input');
    input.value = '4 2/5';
    act(() => { input.dispatchEvent(new Event('input', { bubbles: true })); });
    expect(m.all('.answer-preview')).toHaveLength(0);
    act(() => { vi.advanceTimersByTime(PREVIEW_DEBOUNCE_MS); });
    expect(m.find('.pv-mixed .pv-frac .pv-den').textContent).toBe('5');
    input.value = '3 # 4';
    act(() => { input.dispatchEvent(new Event('input', { bubbles: true })); vi.advanceTimersByTime(PREVIEW_DEBOUNCE_MS); });
    expect(m.find('.pv-raw').textContent).toBe('3 # 4');
  });
});

describe('the input hint by contract', () => {
  it('names the mixed-number way, the fraction way, or the generic examples', () => {
    expect(inputHint({ kind: 'required_form', form: 'mixed_number' })).toBe(
      'Write a mixed number as 4 2/5: the whole number, a space, then the fraction.');
    expect(inputHint({ kind: 'required_form', form: 'reduced_fraction' })).toContain('3/4');
    expect(inputHint({ kind: 'list', ordered: true })).toBe(
      'Separate the numbers with commas or <, for example 5136, 5316, 5361');
    expect(inputHint({ kind: 'list', ordered: false })).toBe('answers like 3/4, 2x+1, sqrt(2) are fine');
    expect(inputHint(undefined)).toBe('answers like 3/4, 2x+1, sqrt(2) are fine');
    const m = mountRoot(<AnswerField contract={{ kind: 'required_form', form: 'mixed_number' }} />, roots);
    expect(m.find('.field-hint').textContent).toContain('4 2/5');
  });
});

describe('the a b/c symbol key', () => {
  it('inserts the template with the caret on the whole number', () => {
    const m = mountRoot(<AnswerField />, roots);
    const input = m.find<HTMLInputElement>('.answer-input');
    const key = m.all('.sym-key').find((k) => k.textContent === 'a b/c')!;
    act(() => { key.click(); });
    expect(input.value).toBe('  /');
    expect(input.selectionStart).toBe(0);
  });
});

describe('the notation hint', () => {
  const base = { outcome: 'incorrect', correct: false, work_quality: 'passable', error_tags: ['notation'], secs: 1,
    task_status: 'continue', remediation: [], next: null, diagnosis: null, attempt_id: 'a',
    reason: '4 * 2/5 means 4 times 2/5. For the mixed number 4⅖ write 4 2/5.' } as AnswerResponse;
  it('reads from the reason of a notation-tagged reply and shows it prominently', () => {
    expect(notationHint(base)).toContain('write 4 2/5');
    render(<Feedback res={base} hasNext onContinue={() => undefined} onEnd={() => undefined} />);
    const el = screen.getByRole('status');
    expect(el.className).toBe('feedback-notation');
    expect(document.querySelectorAll('.feedback-reason')).toHaveLength(0);
  });
  it('is absent when the reply carries no notation', () => {
    expect(notationHint({ ...base, error_tags: [] })).toBeNull();
  });
});
