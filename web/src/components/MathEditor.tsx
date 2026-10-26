/**
 * The structural math editor: a MathLive `<math-field>` with the keypad of the contract.
 *
 * `AnswerField` mounts this component after `loadMathEditor()` resolves. Until then, and
 * when the load fails, the plain input stays. The posted answer is the LaTeX of the field.
 *
 * The keypad opens on a tap on the field and never on focus: a focus handler would cover the
 * problem text each time the page puts the focus in the field. It closes on blur.
 *
 * The component does not render the field value. It is UNCONTROLLED, for the same reasons as
 * the plain input (`AnswerField.tsx`): the caller reads the LaTeX through the handle at
 * submit time.
 */
import { useEffect, useImperativeHandle, useRef, type KeyboardEvent, type Ref } from 'react';
import { keypadFor } from '@/lib/keypad';
import type { KeyboardLayoutLike, MathFieldLike, VirtualKeyboardLike } from '@/lib/math-editor-loader';
import type { AnswerContractHint } from '@/api/types';

declare module 'react' {
  // eslint-disable-next-line @typescript-eslint/no-namespace
  namespace JSX {
    interface IntrinsicElements {
      'math-field': React.DetailedHTMLProps<React.HTMLAttributes<HTMLElement>, HTMLElement> & {
        'math-virtual-keyboard-policy'?: string;
      };
    }
  }
}

/** The handle that `AnswerField` reads. */
export interface MathEditorHandle {
  /** The LaTeX of the field, as it stands now. */
  value: () => string;
  clear: () => void;
  focus: () => void;
  /** Insert LaTeX at the caret. */
  insert: (latex: string) => void;
}

export interface MathEditorProps {
  /** The LaTeX to show at mount. */
  value: string;
  contract: AnswerContractHint | undefined;
  disabled: boolean;
  onInput: (latex: string) => void;
  onSubmit: (() => void) | undefined;
  ariaLabel?: string;
  ref?: Ref<MathEditorHandle> | undefined;
}

declare global {
  interface Window {
    /** MathLive puts its keypad object here when the module loads. */
    mathVirtualKeyboard?: VirtualKeyboardLike;
  }
}

function keyboard(): VirtualKeyboardLike | undefined {
  return window.mathVirtualKeyboard;
}

export function MathEditor({
  value, contract, disabled, onInput, onSubmit, ariaLabel = 'Answer', ref,
}: MathEditorProps) {
  const fieldRef = useRef<MathFieldLike | null>(null);
  const onInputRef = useRef(onInput);
  useEffect(() => { onInputRef.current = onInput; });

  useImperativeHandle(ref, () => ({
    value: () => fieldRef.current?.getValue('latex') ?? '',
    clear: () => { fieldRef.current?.setValue(''); },
    focus: () => { fieldRef.current?.focus(); },
    insert: (latex) => {
      const field = fieldRef.current;
      if (!field || field.readOnly) return;
      field.insert(latex);
      field.focus();
      onInputRef.current(field.getValue('latex'));
    },
  }), []);

  // Mount: push the first value in, and listen for the field events.
  useEffect(() => {
    const field = fieldRef.current;
    if (!field) return;
    if (value !== '') field.setValue(value);
    const input = () => { onInputRef.current(field.getValue('latex')); };
    const blur = () => { keyboard()?.hide(); };
    field.addEventListener('input', input);
    field.addEventListener('focusout', blur);
    return () => {
      field.removeEventListener('input', input);
      field.removeEventListener('focusout', blur);
      keyboard()?.hide();
    };
    // The value is the FIRST value only. A later change of the prop must not reset the field.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (fieldRef.current) fieldRef.current.readOnly = disabled;
  }, [disabled]);

  const openKeypad = () => {
    const kb = keyboard();
    if (!kb || disabled) return;
    const layouts: Array<string | KeyboardLayoutLike> = [...keypadFor(contract), 'alphabetic'];
    kb.layouts = layouts;
    kb.show();
  };

  const keydown = (e: KeyboardEvent<HTMLElement>) => {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    if (disabled) return;
    onSubmit?.();
  };

  return (
    <math-field
      ref={(el) => { fieldRef.current = el as MathFieldLike | null; }}
      className="answer-editor"
      aria-label={ariaLabel}
      math-virtual-keyboard-policy="manual"
      onPointerUp={openKeypad}
      onKeyDown={keydown}
    />
  );
}
