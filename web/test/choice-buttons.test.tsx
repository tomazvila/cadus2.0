/**
 * The answer buttons of a Label item, and the control that selects buttons or the typed field.
 *
 * The literals come from the freeze pack (`rust-api.md` section 5): the option order
 * `Step 3, Step 1, Step 4, Step 2` is the order of the service, and the view keeps it.
 */
import { createRef } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { axe } from 'vitest-axe';
import katex from 'katex';
import renderMathInElement from 'katex/contrib/auto-render';
import { ChoiceButtons } from '@/components/ChoiceButtons';
import { AnswerInput, TypedSubmit, hasChoices } from '@/components/AnswerInput';
import type { AnswerFieldHandle } from '@/components/AnswerField';
import { AXE_IN_JSDOM } from './axe';

const STEPS = ['Step 3', 'Step 1', 'Step 4', 'Step 2'];

const group = () => screen.getByRole('group', { name: 'Answer choices' });
const labels = () => within(group()).getAllByRole('button').map((b) => b.textContent);

afterEach(() => { vi.unstubAllGlobals(); });

describe('ChoiceButtons', () => {
  it('shows one button for each option, in the order of the payload', () => {
    render(<ChoiceButtons choices={STEPS} disabled={false} onSubmit={vi.fn()} />);
    expect(labels()).toEqual(['Step 3', 'Step 1', 'Step 4', 'Step 2']);
  });

  it('uses real buttons of type button, so a form around them does not submit', () => {
    render(<ChoiceButtons choices={STEPS} disabled={false} onSubmit={vi.fn()} />);
    const buttons = within(group()).getAllByRole('button');
    expect(buttons.map((b) => b.tagName)).toEqual(['BUTTON', 'BUTTON', 'BUTTON', 'BUTTON']);
    expect(buttons.map((b) => b.getAttribute('type'))).toEqual(['button', 'button', 'button', 'button']);
  });

  it('gives the exact option text to onSubmit one time for one tap', () => {
    const onSubmit = vi.fn();
    render(<ChoiceButtons choices={STEPS} disabled={false} onSubmit={onSubmit} />);
    fireEvent.click(screen.getByRole('button', { name: 'Step 1' }));
    expect(onSubmit.mock.calls).toEqual([['Step 1']]);
  });

  it('does not trim the text and does not change the letter case', () => {
    const onSubmit = vi.fn();
    const raw = '  The LIMIT is $\\infty$ ';
    render(<ChoiceButtons choices={[raw, 'b']} disabled={false} onSubmit={onSubmit} />);
    fireEvent.click(within(group()).getAllByRole('button')[0]);
    expect(onSubmit.mock.calls).toEqual([[raw]]);
  });

  it('keeps two options with the same text as two buttons', () => {
    render(<ChoiceButtons choices={['same', 'same']} disabled={false} onSubmit={vi.fn()} />);
    expect(labels()).toEqual(['same', 'same']);
  });

  it('disables each button and then a tap submits nothing', () => {
    const onSubmit = vi.fn();
    render(<ChoiceButtons choices={STEPS} disabled onSubmit={onSubmit} />);
    const buttons = within(group()).getAllByRole('button') as HTMLButtonElement[];
    expect(buttons.map((b) => b.disabled)).toEqual([true, true, true, true]);
    fireEvent.click(buttons[1]);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('renders the math of an option and submits the raw text with its dollar signs', () => {
    vi.stubGlobal('renderMathInElement', renderMathInElement);
    vi.stubGlobal('katex', katex);
    const onSubmit = vi.fn();
    const raw = 'The value is $\\frac{1}{2}$';
    render(<ChoiceButtons choices={[raw]} disabled={false} onSubmit={onSubmit} />);
    const button = within(group()).getByRole('button');
    expect(button.querySelector('.choice-text .katex')).not.toBeNull();
    fireEvent.click(button);
    expect(onSubmit.mock.calls).toEqual([[raw]]);
  });

  it('puts the option text in a span, because a div is not valid in a button', () => {
    render(<ChoiceButtons choices={STEPS} disabled={false} onSubmit={vi.fn()} />);
    const inner = within(group()).getAllByRole('button').map((b) => b.firstElementChild!);
    expect(inner.map((node) => `${node.tagName}.${node.className}`)).toEqual(
      ['SPAN.choice-text', 'SPAN.choice-text', 'SPAN.choice-text', 'SPAN.choice-text'],
    );
    expect(group().querySelector('div')).toBeNull();
  });

  it('marks the selected option on the locked buttons only, by the index', () => {
    const marks = () => within(group()).getAllByRole('button')
      .map((b) => `${b.className}|${b.getAttribute('aria-pressed')}`);
    const view = render(<ChoiceButtons choices={['same', 'same', 'c']} disabled selected={1} onSubmit={vi.fn()} />);
    expect(marks()).toEqual([
      'btn choice-button|null', 'btn choice-button is-selected|true', 'btn choice-button|null',
    ]);
    // An enabled button is not an answer yet, so it has no mark.
    view.rerender(<ChoiceButtons choices={['same', 'same', 'c']} disabled={false} selected={1} onSubmit={vi.fn()} />);
    expect(marks()).toEqual(['btn choice-button|null', 'btn choice-button|null', 'btn choice-button|null']);
    // No selection: no mark on the locked buttons.
    view.rerender(<ChoiceButtons choices={['same', 'same', 'c']} disabled onSubmit={vi.fn()} />);
    expect(marks()).toEqual(['btn choice-button|null', 'btn choice-button|null', 'btn choice-button|null']);
  });

  it('gives the index to onSelect before the text goes to onSubmit', () => {
    const order: string[] = [];
    render(<ChoiceButtons choices={STEPS} disabled={false}
      onSelect={(index) => { order.push(`select ${index}`); }}
      onSubmit={(answer) => { order.push(`submit ${answer}`); }} />);
    fireEvent.click(screen.getByRole('button', { name: 'Step 4' }));
    expect(order).toEqual(['select 2', 'submit Step 4']);
  });

  it('shows the markup of an option as text', () => {
    render(<ChoiceButtons choices={['<img src=x onerror=alert(1)>']} disabled={false} onSubmit={vi.fn()} />);
    expect(group().querySelector('img')).toBeNull();
    expect(labels()).toEqual(['<img src=x onerror=alert(1)>']);
  });

  it('has the focus order of the visual order', () => {
    render(<ChoiceButtons choices={STEPS} disabled={false} onSubmit={vi.fn()} />);
    const buttons = within(group()).getAllByRole('button');
    // No positive tabindex changes the order, so the DOM order is the focus order.
    expect(buttons.map((b) => b.getAttribute('tabindex'))).toEqual([null, null, null, null]);
    buttons[2].focus();
    expect(document.activeElement).toBe(buttons[2]);
  });

  it('passes axe in the enabled state and in the disabled state', async () => {
    const live = render(<ChoiceButtons choices={STEPS} disabled={false} onSubmit={vi.fn()} />);
    expect(await axe(live.container, AXE_IN_JSDOM)).toHaveNoViolations();
    live.unmount();
    const locked = render(<ChoiceButtons choices={STEPS} disabled onSubmit={vi.fn()} />);
    expect(await axe(locked.container, AXE_IN_JSDOM)).toHaveNoViolations();
  });
});

describe('hasChoices', () => {
  it('is true for one option or more, and false for an absent key and for an empty list', () => {
    expect(hasChoices(['a'])).toBe(true);
    expect(hasChoices(STEPS)).toBe(true);
    expect(hasChoices(undefined)).toBe(false);
    expect(hasChoices([])).toBe(false);
  });
});

describe('AnswerInput', () => {
  it('shows the typed field and no buttons for a payload with no choices key', () => {
    const ref = createRef<AnswerFieldHandle>();
    render(<AnswerInput ref={ref} choices={undefined} disabled={false} onSubmit={vi.fn()} />);
    expect(screen.getByLabelText('Answer')).toBeTruthy();
    expect(screen.queryByRole('group', { name: 'Answer choices' })).toBeNull();
    fireEvent.change(screen.getByLabelText('Answer'), { target: { value: ' 3/4 ' } });
    expect(ref.current!.value()).toBe('3/4');
  });

  it('shows the typed field for an empty choices list', () => {
    render(<AnswerInput choices={[]} disabled={false} onSubmit={vi.fn()} />);
    expect(screen.getByLabelText('Answer')).toBeTruthy();
    expect(screen.queryByRole('group', { name: 'Answer choices' })).toBeNull();
  });

  it('gives the typed field its disabled rule and its Enter submit', () => {
    const onSubmit = vi.fn();
    const view = render(<AnswerInput choices={undefined} disabled={false} onSubmit={onSubmit} />);
    fireEvent.keyDown(screen.getByLabelText('Answer'), { key: 'Enter' });
    expect(onSubmit).toHaveBeenCalledTimes(1);
    view.rerender(<AnswerInput choices={undefined} disabled onSubmit={onSubmit} />);
    expect((screen.getByLabelText('Answer') as HTMLInputElement).disabled).toBe(true);
  });

  it('shows the buttons and no typed field for a payload with choices', () => {
    render(<AnswerInput choices={STEPS} disabled={false} onSubmit={vi.fn()} />);
    expect(labels()).toEqual(['Step 3', 'Step 1', 'Step 4', 'Step 2']);
    expect(screen.queryByLabelText('Answer')).toBeNull();
    expect(screen.queryByRole('toolbar', { name: 'Math symbols' })).toBeNull();
  });

  it('has the option text in value() at the time of the onSubmit call', () => {
    const ref = createRef<AnswerFieldHandle>();
    const seen: string[] = [];
    render(<AnswerInput ref={ref} choices={STEPS} disabled={false}
      onSubmit={() => { seen.push(ref.current!.value()); }} />);
    expect(ref.current!.value()).toBe('');
    fireEvent.click(screen.getByRole('button', { name: 'Step 4' }));
    expect(seen).toEqual(['Step 4']);
  });

  it('removes the selection on clear(), so a re-solve starts with no answer', () => {
    const ref = createRef<AnswerFieldHandle>();
    render(<AnswerInput ref={ref} choices={STEPS} disabled={false} onSubmit={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Step 2' }));
    expect(ref.current!.value()).toBe('Step 2');
    // `clear()` writes state too, so the test calls it in `act`.
    act(() => { ref.current!.clear(); });
    expect(ref.current!.value()).toBe('');
  });

  it('puts the focus on the container, not on a button, so a held Enter selects nothing', () => {
    const ref = createRef<AnswerFieldHandle>();
    const onSubmit = vi.fn();
    render(<AnswerInput ref={ref} choices={STEPS} disabled={false} onSubmit={onSubmit} />);
    ref.current!.focus();
    const box = document.activeElement as HTMLElement;
    expect(box.className).toBe('choice-input');
    expect(box.getAttribute('tabindex')).toBe('-1');
    // The focus stop has a role and a name for a screen reader.
    expect(box).toBe(screen.getByRole('group', { name: 'Choose an answer' }));
    expect(box.contains(group())).toBe(true);
    fireEvent.keyDown(box, { key: 'Enter' });
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('marks the tapped option when the buttons lock, and clear() removes the mark', () => {
    const ref = createRef<AnswerFieldHandle>();
    const pressed = () => within(group()).getAllByRole('button')
      .filter((b) => b.getAttribute('aria-pressed') === 'true').map((b) => b.textContent);
    const view = render(<AnswerInput ref={ref} choices={STEPS} disabled={false} onSubmit={vi.fn()} />);
    fireEvent.click(screen.getByRole('button', { name: 'Step 4' }));
    expect(pressed()).toEqual([]);
    view.rerender(<AnswerInput ref={ref} choices={STEPS} disabled onSubmit={vi.fn()} />);
    expect(pressed()).toEqual(['Step 4']);
    expect(screen.getByRole('button', { name: 'Step 4' }).className).toBe('btn choice-button is-selected');
    act(() => { ref.current!.clear(); });
    expect(pressed()).toEqual([]);
    expect(ref.current!.value()).toBe('');
  });

  it('uses `locked` for the buttons and `disabled` for the typed field', () => {
    const view = render(<AnswerInput choices={STEPS} disabled={false} locked onSubmit={vi.fn()} />);
    expect((within(group()).getAllByRole('button') as HTMLButtonElement[]).every((b) => b.disabled)).toBe(true);
    view.rerender(<AnswerInput choices={STEPS} disabled locked={false} onSubmit={vi.fn()} />);
    expect((within(group()).getAllByRole('button') as HTMLButtonElement[]).some((b) => b.disabled)).toBe(false);
    // With no `locked`, the buttons follow `disabled`.
    view.rerender(<AnswerInput choices={STEPS} disabled onSubmit={vi.fn()} />);
    expect((within(group()).getAllByRole('button') as HTMLButtonElement[]).every((b) => b.disabled)).toBe(true);
  });

  it('passes axe in the choice mode', async () => {
    const view = render(<AnswerInput choices={STEPS} disabled={false} onSubmit={vi.fn()} />);
    expect(await axe(view.container, AXE_IN_JSDOM)).toHaveNoViolations();
  });
});

describe('TypedSubmit', () => {
  it('shows the Submit button for a typed problem and calls onClick', () => {
    const onClick = vi.fn();
    render(<TypedSubmit choices={undefined} busy={false} disabled={false} onClick={onClick}>Submit</TypedSubmit>);
    const button = screen.getByRole('button', { name: 'Submit' }) as HTMLButtonElement;
    expect(button.className).toBe('btn btn-primary');
    expect(button.getAttribute('type')).toBe('button');
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it('shows the busy class and the disabled state', () => {
    render(<TypedSubmit choices={[]} busy disabled onClick={vi.fn()}>Submit</TypedSubmit>);
    const button = screen.getByRole('button', { name: 'Checking…' }) as HTMLButtonElement;
    expect(button.className).toBe('btn btn-primary is-busy');
    expect(button.getAttribute('aria-busy')).toBe('true');
    expect(button.disabled).toBe(true);
  });

  it('shows nothing for a problem with choices', () => {
    const view = render(<TypedSubmit choices={STEPS} busy={false} disabled={false} onClick={vi.fn()}>Submit</TypedSubmit>);
    expect(view.container.innerHTML).toBe('');
  });
});
