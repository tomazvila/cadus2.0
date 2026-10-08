/**
 * The answer input and its symbol palette.
 *
 * UNCONTROLLED, deliberately, and the reasons compound (spec section 4.4):
 *
 *  1. `insertAtCursor` writes `input.value` and then calls `setSelectionRange`. Under a
 *     controlled input React re-renders and puts the caret at the END, so `√(` no longer
 *     leaves the radicand inside its paren. Nothing about that edit looks wrong in review.
 *  2. Every submit path reads the value SYNCHRONOUSLY, before its first await. A controlled
 *     value goes stale exactly as a `useState` phase does, for the same reason.
 *  3. The problem card re-renders about once a second while a clock ticks. A controlled value
 *     puts every keystroke on that render path, beside the KaTeX-rendered problem text.
 *
 * The caller reads the answer through the handle, never through state.
 *
 * THE KEY HANDLERS LIVE IN THE JSX, not in an effect. React attaches them in the first
 * commit, so an Enter in the frame after mount submits once. Installed from a passive effect,
 * that first Enter reaches nothing and the learner's first submit is silently dropped.
 */
import { useEffect, useImperativeHandle, useRef, useState, type Ref } from 'react';
import { AnswerPreview } from '@/components/AnswerPreview';
import type { AnswerContractHint } from '@/api/types';

/** The wait after the last keystroke before the preview redraws, in milliseconds. */
export const PREVIEW_DEBOUNCE_MS = 150;

/** The mixed-number template: whole, space, numerator, slash, denominator. The caret starts on the whole. */
const MIXED_TEMPLATE = '  /';

/** The hint under the input, from the served contract. At most three examples, no system words. */
export function inputHint(contract: AnswerContractHint | undefined): string {
  const form = contract?.form ?? '';
  if (form === 'mixed_number') return 'Write a mixed number as 4 2/5: the whole number, a space, then the fraction.';
  if (contract?.kind === 'list' && contract.ordered === true) {
    return 'Separate the numbers with commas or <, for example 5136, 5316, 5361';
  }
  if (/fraction/.test(form) || /fraction/.test(contract?.kind ?? '')) return 'Write a fraction like 3/4.';
  if (form === 'radical' || form === 'simplest_radical') {
    return 'answers like 3/4, 2x+1, sqrt(2) or root(5, y) are fine';
  }
  return 'answers like 3/4, 2x+1, sqrt(2) are fine';
}

/** The symbols a plain keyboard does not produce. `√(` carries its opening paren. */
const MATH_SYMBOLS = ['∞', 'π', '√(', '^', '≤', '≥', '≠', '±', '×', '÷', '°', 'θ'] as const;

/** The root indexes the root key offers. Index 2 is the key `√(`. */
const ROOT_INDEXES = [3, 4, 5, 6, 7, 8, 9] as const;

export interface AnswerFieldHandle {
  /** The trimmed answer, read synchronously at submit time. */
  value: () => string;
  clear: () => void;
  focus: () => void;
}

export interface AnswerFieldProps {
  placeholder?: string;
  onSubmit?: () => void;
  onHint?: (() => void) | undefined;
  disabled?: boolean;
  /** The answer contract of the served problem, when it has one. */
  contract?: AnswerContractHint | undefined;
  ref?: Ref<AnswerFieldHandle> | undefined;
}

/** Replace the selection with `text` and leave the caret after it. */
function insertAtCursor(input: HTMLInputElement, text: string, caret = text.length): void {
  const start = input.selectionStart ?? input.value.length;
  const end = input.selectionEnd ?? start;
  input.value = input.value.slice(0, start) + text + input.value.slice(end);
  const pos = start + caret;
  input.setSelectionRange(pos, pos);
  input.focus();
}

export function AnswerField({
  placeholder = 'Your answer',
  onSubmit,
  onHint,
  disabled = false,
  contract,
  ref,
}: AnswerFieldProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [shown, setShown] = useState('');
  const [rootIndex, setRootIndex] = useState<number>(3);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  // The preview follows the input through a debounce; it is never the source of the value.
  const refresh = () => {
    clearTimeout(timer.current);
    timer.current = setTimeout(() => { setShown(inputRef.current?.value ?? ''); }, PREVIEW_DEBOUNCE_MS);
  };
  useEffect(() => () => { clearTimeout(timer.current); }, []);

  // NO `setDisabled` on the handle. `disabled` has exactly ONE owner — the prop. With both,
  // React never rewrites an unchanged prop, so an imperative `setDisabled(true)` survives a
  // re-render and the prop then lies. The result is either an Enter dropped in a frame that
  // looks live, or an Enter accepted while the feedback panel is on screen. The keydown guard
  // below reads the same attribute, so it inherits the lie.
  //
  // A view drives it from its phase instead: `disabled={phase !== 'ready'}`.
  useImperativeHandle(ref, () => ({
    value: () => inputRef.current?.value.trim() ?? '',
    clear: () => { if (inputRef.current) inputRef.current.value = ''; clearTimeout(timer.current); setShown(''); },
    focus: () => inputRef.current?.focus(),
  }));

  return (
    <div className="answer-field">
      <input
        ref={inputRef}
        type="text"
        className="answer-input"
        placeholder={placeholder}
        aria-label="Answer"
        autoComplete="off"
        autoCapitalize="off"
        autoCorrect="off"
        spellCheck={false}
        defaultValue=""
        disabled={disabled}
        onInput={refresh}
        onKeyDown={(e) => {
          const input = e.currentTarget;
          if (e.key === 'Enter') {
            e.preventDefault();
            // Enter is the SAME action as the primary Submit button, and it does NOT route
            // through it: a busy view disables the BUTTON while a grade of several seconds
            // runs, and a disabled button does nothing to a keydown on the input. So this
            // path never decides whether a submit starts — the view's phase gate decides
            // that, synchronously, before its first await. The guard here covers the field's
            // own terminal state alone.
            if (input.disabled || input.readOnly) return;
            onSubmit?.();
            return;
          }
          if ((e.key === 'h' || e.key === 'H') && onHint) {
            // Only while the field is empty: `sqrt` contains an h. A modifier means a browser
            // or reader shortcut — Ctrl+H is the history — so leave those alone.
            if (input.value !== '' || e.ctrlKey || e.metaKey || e.altKey) return;
            e.preventDefault();
            onHint();
          }
        }}
      />
      <div className="sym-palette" role="toolbar" aria-label="Math symbols">
        {MATH_SYMBOLS.map((sym) => (
          <button
            key={sym}
            type="button"
            className="sym-key"
            aria-label={`Insert ${sym}`}
            // A mousedown steals focus from the input before the click lands, and the caret
            // position goes with it. The suppression is what makes the tap survive.
            onMouseDown={(e) => { e.preventDefault(); }}
            // The input is on screen for as long as the key beside it is.
            onClick={() => { insertAtCursor(inputRef.current!, sym); refresh(); }}
          >
            {sym}
          </button>
        ))}
        <button
          type="button"
          className="sym-key"
          aria-label="Insert mixed number"
          onMouseDown={(e) => { e.preventDefault(); }}
          // The caret goes to the start of the template: the whole number is typed first.
          onClick={() => { insertAtCursor(inputRef.current!, MIXED_TEMPLATE, 0); refresh(); }}
        >
          a b/c
        </button>
        <select
          className="sym-select"
          aria-label="Root index"
          value={rootIndex}
          onChange={(e) => { setRootIndex(Number(e.target.value)); }}
        >
          {ROOT_INDEXES.map((n) => <option key={n} value={n}>{n}</option>)}
        </select>
        <button
          type="button"
          className="sym-key"
          aria-label="Insert indexed root"
          onMouseDown={(e) => { e.preventDefault(); }}
          // `root(5, ` with the chosen index: the caret lands inside the bracket, where the radicand goes.
          onClick={() => { insertAtCursor(inputRef.current!, `root(${rootIndex}, `); refresh(); }}
        >
          ⁿ√
        </button>
      </div>
      <AnswerPreview text={shown} />
      <p className="field-hint">{inputHint(contract)}</p>
    </div>
  );
}
