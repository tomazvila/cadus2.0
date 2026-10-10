/**
 * The answer field: contextual keys, the roving tab stop, the draft, the empty-answer line.
 */
import { act, createRef } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AnswerField, EMPTY_ANSWER_LINE, type AnswerFieldHandle } from '@/components/AnswerField';
import { TypedSubmit } from '@/components/AnswerInput';
import { contextKeys } from '@/lib/notation';
import { render, screen } from '@testing-library/react';
import { mountRoot } from './helpers/react';

const roots: Array<() => void> = [];
beforeEach(() => { window.localStorage.clear(); window.sessionStorage.clear(); });
afterEach(() => { roots.splice(0).forEach((fn) => { fn(); }); });

const outside = (m: ReturnType<typeof mountRoot>) =>
  m.all('.sym-area > .sym-palette .sym-key').map((k) => k.textContent);

describe('the keys in view', () => {
  it('a fraction problem shows no key outside More symbols', () => {
    const m = mountRoot(<AnswerField contract={{ kind: 'required_form', form: 'reduced_fraction' }} />, roots);
    expect(outside(m)).toEqual([]);
    expect(m.find('.sym-more summary').textContent).toBe('More symbols');
    expect(m.find('.sym-help').textContent).toBe('How to type answers');
  });

  it('a radical problem shows the root keys', () => {
    const m = mountRoot(<AnswerField contract={{ kind: 'required_simplest_radical', form: 'simplest_radical' }} />, roots);
    expect(outside(m)).toEqual(['√(', 'ⁿ√']);
    expect(m.all('.sym-area > .sym-palette .sym-select')).toHaveLength(1);
  });

  it('a trig problem shows π, θ and °', () => {
    const m = mountRoot(<AnswerField contract={{ kind: 'exact', form: 'trig_value' }} />, roots);
    expect(outside(m)).toEqual(['π', 'θ', '°']);
  });

  it('never gives more than five keys', () => {
    for (const kind of ['inequality_union', 'polynomial_relation', 'interval', 'trig', 'radical']) {
      expect(contextKeys({ kind }).length).toBeLessThanOrEqual(5);
    }
  });

  it('keeps the More symbols choice', () => {
    const m = mountRoot(<AnswerField />, roots);
    const details = m.find<HTMLDetailsElement>('.sym-more');
    act(() => { details.open = true; details.dispatchEvent(new Event('toggle')); });
    expect(window.localStorage.getItem('cadus.symbols.more')).toBe('1');
    const again = mountRoot(<AnswerField />, roots);
    expect(again.find('.sym-more').hasAttribute('open')).toBe(true);
  });
});

describe('the roving tab stop', () => {
  it('puts tabindex 0 on the first key and moves focus with the arrow keys', () => {
    const m = mountRoot(<AnswerField contract={{ kind: 'exact', form: 'trig_value' }} />, roots);
    const keys = m.all('.sym-area > .sym-palette .sym-key') as HTMLButtonElement[];
    expect(keys.map((k) => k.tabIndex)).toEqual([0, -1, -1]);
    const press = (el: HTMLElement, key: string) => act(() => {
      el.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
    });
    keys[0]!.focus();
    press(keys[0]!, 'ArrowRight');
    expect(document.activeElement).toBe(keys[1]);
    press(keys[1]!, 'End');
    expect(document.activeElement).toBe(keys[2]);
    press(keys[2]!, 'ArrowRight');
    expect(document.activeElement).toBe(keys[0]);
    press(keys[0]!, 'ArrowLeft');
    expect(document.activeElement).toBe(keys[2]);
    press(keys[2]!, 'Home');
    expect(document.activeElement).toBe(keys[0]);
    expect(m.find('.sym-palette').getAttribute('aria-label')).toBe('Symbol keys. Use the arrow keys to move.');
  });
});

describe('the field around the keys', () => {
  it('marks a plain-number answer as short and shows no hint for it', () => {
    const m = mountRoot(<AnswerField contract={{ kind: 'exact', form: 'integer' }} />, roots);
    expect(m.find('.answer-input').classList.contains('is-short')).toBe(true);
    expect(m.container.querySelector('.field-hint')).toBeNull();
  });

  it('shows the empty-answer line until the learner types', () => {
    const ref = createRef<AnswerFieldHandle>();
    const m = mountRoot(<AnswerField ref={ref} />, roots);
    act(() => { ref.current!.remindEmpty!(); });
    expect(m.find('.field-note').textContent).toBe(EMPTY_ANSWER_LINE);
    act(() => { m.find('.answer-input').dispatchEvent(new Event('input', { bubbles: true })); });
    expect(m.find('.field-note').textContent).toBe('');
  });

  it('keeps the typed text per problem and restores it', () => {
    const first = mountRoot(<AnswerField draftKey="p1" />, roots);
    const input = first.find<HTMLInputElement>('.answer-input');
    act(() => { input.value = '3/4'; input.dispatchEvent(new Event('input', { bubbles: true })); });
    const back = mountRoot(<AnswerField draftKey="p1" />, roots);
    expect(back.find<HTMLInputElement>('.answer-input').value).toBe('3/4');
    const other = mountRoot(<AnswerField draftKey="p2" />, roots);
    expect(other.find<HTMLInputElement>('.answer-input').value).toBe('');
  });

  it('shows Checking… with aria-busy on Submit while a grade runs', () => {
    render(<TypedSubmit choices={undefined} busy disabled onClick={vi.fn()}>Submit</TypedSubmit>);
    expect(screen.getByRole('button', { name: 'Checking…' }).getAttribute('aria-busy')).toBe('true');
  });
});
