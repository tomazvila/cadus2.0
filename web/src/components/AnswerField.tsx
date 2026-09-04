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
import { useImperativeHandle, useRef, type Ref } from 'react';

/** The symbols a plain keyboard does not produce. `√(` carries its opening paren. */
const MATH_SYMBOLS = ['∞', 'π', '√(', '^', '≤', '≥', '≠', '±', '×', '÷', '°', 'θ'] as const;

export interface AnswerFieldHandle {
  /** The trimmed answer, read synchronously at submit time. */
  value: () => string;
  clear: () => void;
  focus: () => void;
}

export interface AnswerFieldProps {
  placeholder?: string;
  onSubmit?: () => void;
  onHint?: () => void;
  disabled?: boolean;
  ref?: Ref<AnswerFieldHandle>;
}

/** Replace the selection with `text` and leave the caret after it. */
function insertAtCursor(input: HTMLInputElement, text: string): void {
  const start = input.selectionStart ?? input.value.length;
  const end = input.selectionEnd ?? start;
  input.value = input.value.slice(0, start) + text + input.value.slice(end);
  const pos = start + text.length;
  input.setSelectionRange(pos, pos);
  input.focus();
}

export function AnswerField({
  placeholder = 'Your answer',
  onSubmit,
  onHint,
  disabled = false,
  ref,
}: AnswerFieldProps) {
  const inputRef = useRef<HTMLInputElement>(null);

  // NO `setDisabled` on the handle. `disabled` has exactly ONE owner — the prop. With both,
  // React never rewrites an unchanged prop, so an imperative `setDisabled(true)` survives a
  // re-render and the prop then lies. The result is either an Enter dropped in a frame that
  // looks live, or an Enter accepted while the feedback panel is on screen. The keydown guard
  // below reads the same attribute, so it inherits the lie.
  //
  // A view drives it from its phase instead: `disabled={phase !== 'ready'}`.
  useImperativeHandle(ref, () => ({
    value: () => inputRef.current?.value.trim() ?? '',
    clear: () => { if (inputRef.current) inputRef.current.value = ''; },
    focus: () => inputRef.current?.focus(),
  }), []);

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
            onClick={() => { insertAtCursor(inputRef.current!, sym); }}
          >
            {sym}
          </button>
        ))}
      </div>
      <p className="field-hint">answers like 3/4, 2x+1, sqrt(2) are fine</p>
    </div>
  );
}
