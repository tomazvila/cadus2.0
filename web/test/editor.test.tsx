/**
 * The structural editor behind the plain answer field.
 *
 * The plain input is the first control. The vendored MathLive module loads after the mount;
 * `test/mocks/mathlive.ts` stands in for it. Every other test file runs with the rejecting
 * importer of `test/setup.ts`, so only this file sees the editor.
 */
import { act, createRef } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { axe } from 'vitest-axe';
import { AnswerField, EMPTY_ANSWER_LINE, EMPTY_SLOT_LINE, type AnswerFieldHandle } from '@/components/AnswerField';
import { NotationPanel } from '@/components/NotationPanel';
import { resetMathEditorLoader, type MathEditorModule } from '@/lib/math-editor-loader';
import { draftStorageKey, resetNotationStore, setNotationOpen } from '@/lib/notation';
import { AXE_IN_JSDOM } from './axe';
import { MathfieldElement, installKeyboard, mathVirtualKeyboard } from './mocks/mathlive';
import { allowConsoleError } from './setup';
import { mountRoot } from './helpers/react';

const roots: Array<() => void> = [];

beforeEach(() => {
  window.localStorage.clear();
  window.sessionStorage.clear();
  resetNotationStore();
  installKeyboard();
});
afterEach(() => { roots.splice(0).forEach((fn) => { fn(); }); });

interface FieldDouble extends HTMLElement {
  latex: string;
  readOnly: boolean;
  type: (latex: string) => void;
}

/** An importer that waits for the test to resolve it. */
function heldImport() {
  let release!: () => void;
  const hold = new Promise<void>((resolve) => { release = resolve; });
  const importer = vi.fn(async (): Promise<MathEditorModule> => {
    await hold;
    return { MathfieldElement };
  });
  return { importer, release };
}

/** Let the resolved import reach the component. */
async function flush(): Promise<void> {
  await act(async () => { await Promise.resolve(); await Promise.resolve(); });
}

const editorOf = (m: ReturnType<typeof mountRoot>) => m.find<FieldDouble>('math-field');

function mountField(props: Partial<React.ComponentProps<typeof AnswerField>> = {}) {
  const ref = createRef<AnswerFieldHandle>();
  const m = mountRoot(<AnswerField ref={ref} {...props} />, roots);
  return { m, ref };
}

describe('while the module loads', () => {
  it('shows the plain input, and Enter in it submits once', async () => {
    const held = heldImport();
    resetMathEditorLoader(held.importer);
    const onSubmit = vi.fn();
    const { m } = mountField({ onSubmit });
    const input = m.find<HTMLInputElement>('.answer-input');
    expect(input).not.toBeNull();
    expect(m.all('math-field')).toHaveLength(0);
    act(() => { input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); });
    expect(onSubmit).toHaveBeenCalledTimes(1);
    held.release();
    await flush();
  });

  it('moves the text typed before the resolve into the editor', async () => {
    const held = heldImport();
    resetMathEditorLoader(held.importer);
    const { m, ref } = mountField();
    const input = m.find<HTMLInputElement>('.answer-input');
    act(() => { input.value = '3/4'; input.dispatchEvent(new Event('input', { bubbles: true })); });
    expect(ref.current!.value()).toBe('3/4');
    held.release();
    await flush();
    expect(m.all('.answer-input')).toHaveLength(0);
    expect(editorOf(m).latex).toBe('3/4');
    expect(ref.current!.value()).toBe('3/4');
  });
});

describe('after the module loads', () => {
  beforeEach(() => { resetMathEditorLoader(async () => ({ MathfieldElement })); });

  it('sets the three statics of the library on the first resolve', async () => {
    mountField();
    await flush();
    expect(MathfieldElement.fontsDirectory).toBe('/vendor/mathlive/fonts');
    expect(MathfieldElement.soundsDirectory).toBeNull();
    expect(MathfieldElement.keypressVibration).toBe(false);
  });

  it('reads the LaTeX of the field through the handle', async () => {
    const { m, ref } = mountField();
    await flush();
    act(() => { editorOf(m).type('\\frac{3}{4}'); });
    expect(ref.current!.value()).toBe('\\frac{3}{4}');
  });

  it('writes the class and the name that reach the DOM', async () => {
    const { m } = mountField();
    await flush();
    const field = editorOf(m);
    expect(field.getAttribute('class')).toBe('answer-editor');
    expect(field.getAttribute('aria-label')).toBe('Answer');
    expect(field.getAttribute('math-virtual-keyboard-policy')).toBe('manual');
  });

  it('reads an empty box as an empty answer and asks for the box', async () => {
    const { m, ref } = mountField();
    await flush();
    act(() => { editorOf(m).type('4\\frac{\\placeholder{}}{\\placeholder{}}'); });
    expect(ref.current!.value()).toBe('');
    act(() => { ref.current!.remindEmpty!(); });
    expect(m.find('.field-note').textContent).toBe(EMPTY_SLOT_LINE);
  });

  it('asks for an answer when the editor is empty', async () => {
    const { m, ref } = mountField();
    await flush();
    act(() => { ref.current!.remindEmpty!(); });
    expect(m.find('.field-note').textContent).toBe(EMPTY_ANSWER_LINE);
  });

  it('submits on Enter and clears on clear()', async () => {
    const onSubmit = vi.fn();
    const { m, ref } = mountField({ onSubmit });
    await flush();
    act(() => { editorOf(m).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); });
    expect(onSubmit).toHaveBeenCalledTimes(1);
    act(() => { editorOf(m).type('2'); });
    act(() => { ref.current!.clear(); });
    expect(ref.current!.value()).toBe('');
  });

  it('keeps the field read-only while disabled and does not submit', async () => {
    const onSubmit = vi.fn();
    const { m } = mountField({ onSubmit, disabled: true });
    await flush();
    expect(editorOf(m).readOnly).toBe(true);
    act(() => { editorOf(m).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })); });
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('saves the draft as LaTeX and restores it after a remount', async () => {
    const first = mountField({ draftKey: 'p-1' });
    await flush();
    act(() => { editorOf(first.m).type('\\sqrt{2}'); });
    expect(window.sessionStorage.getItem(draftStorageKey('p-1'))).toBe('\\sqrt{2}');
    first.m.unmount();
    const second = mountField({ draftKey: 'p-1' });
    await flush();
    expect(editorOf(second.m).latex).toBe('\\sqrt{2}');
    expect(second.ref.current!.value()).toBe('\\sqrt{2}');
  });

  it('inserts the LaTeX spelling when a Type chip is tapped', async () => {
    act(() => { setNotationOpen(true); });
    const ref = createRef<AnswerFieldHandle>();
    const m = mountRoot(<><AnswerField ref={ref} /><NotationPanel /></>, roots);
    await flush();
    const chip = m.all('.notation-chip').find((c) => c.textContent === 'sqrt(2)')!;
    act(() => { chip.click(); });
    expect(ref.current!.value()).toBe('\\sqrt{2}');
  });

  it('shows neither the preview nor the symbol palette', async () => {
    const { m } = mountField({ contract: { kind: 'exact', form: 'trig_value' } });
    expect(m.all('.sym-key').length).toBeGreaterThan(0);
    await flush();
    act(() => { editorOf(m).type('3/4'); });
    expect(m.all('.answer-preview')).toHaveLength(0);
    expect(m.all('.sym-key')).toHaveLength(0);
    expect(m.all('.sym-more')).toHaveLength(0);
    expect(m.all('.sym-help')).toHaveLength(1);
  });

  it('keeps the hint line in both modes', async () => {
    const { m } = mountField({ contract: { kind: 'required_form', form: 'mixed_number' } });
    expect(m.find('.field-hint').textContent).toContain('mixed number');
    await flush();
    expect(m.find('.field-hint').textContent).toContain('mixed number');
  });

  it('opens the keypad on a tap, never on focus, and closes it on blur', async () => {
    const { m } = mountField({ contract: { kind: 'required_form', form: 'radical' } });
    await flush();
    const field = editorOf(m);
    act(() => { field.dispatchEvent(new FocusEvent('focusin', { bubbles: true })); });
    expect(mathVirtualKeyboard.shows).toBe(0);
    act(() => { field.dispatchEvent(new Event('pointerup', { bubbles: true })); });
    expect(mathVirtualKeyboard.shows).toBe(1);
    const names = mathVirtualKeyboard.layouts;
    expect(names).toHaveLength(2);
    expect(names[1]).toBe('alphabetic');
    act(() => { field.dispatchEvent(new FocusEvent('focusout', { bubbles: true })); });
    expect(mathVirtualKeyboard.visible).toBe(false);
  });

  it('has no axe violation', async () => {
    const { m } = mountField();
    await flush();
    expect(await axe(m.container, AXE_IN_JSDOM)).toHaveNoViolations();
  });
});

describe('when the module fails to load', () => {
  it('keeps the plain input, logs one error, and tries again on the next mount', async () => {
    allowConsoleError(/the editor file is missing/);
    const importer = vi.fn(async (): Promise<MathEditorModule> => { throw new Error('the editor file is missing'); });
    resetMathEditorLoader(importer);
    const first = mountField();
    await flush();
    expect(first.m.all('.answer-input')).toHaveLength(1);
    expect(first.m.all('math-field')).toHaveLength(0);
    const logged = vi.mocked(console.error).mock.calls.filter((c) => c.map(String).join(' ').includes('the editor file is missing'));
    expect(logged).toHaveLength(1);
    expect(importer).toHaveBeenCalledTimes(1);
    first.m.unmount();
    const second = mountField();
    await flush();
    expect(importer).toHaveBeenCalledTimes(2);
    expect(second.m.all('.answer-input')).toHaveLength(1);
  });
});
