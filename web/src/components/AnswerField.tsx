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
import { useCallback, useEffect, useImperativeHandle, useRef, useState, type KeyboardEvent, type Ref } from 'react';
import '../styles/fix-answer.css';
import { AnswerPreview } from '@/components/AnswerPreview';
import {
  ALL_SYMBOL_KEYS, clearDraft, contextKeys, isPlainNumber, readDraft, readMoreOpen, registerInserter,
  setNotationOpen, useNotationOpen, writeDraft, writeMoreOpen, type SymbolKey,
} from '@/lib/notation';
import type { AnswerContractHint } from '@/api/types';

/** The wait after the last keystroke before the preview redraws, in milliseconds. */
export const PREVIEW_DEBOUNCE_MS = 150;

/** The mixed-number template: whole, space, numerator, slash, denominator. The caret starts on the whole. */
const MIXED_TEMPLATE = '  /';

/** The hint under the input, from the served contract. Empty when the answer is one plain number. */
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
  return '';
}

/** The root indexes the root key offers. Index 2 is the key `√(`. */
const ROOT_INDEXES = [3, 4, 5, 6, 7, 8, 9] as const;

/** The line under the field when the learner submits nothing. */
export const EMPTY_ANSWER_LINE = 'Type an answer, then press Submit.';

export interface AnswerFieldHandle {
  /** The trimmed answer, read synchronously at submit time. */
  value: () => string;
  clear: () => void;
  focus: () => void;
  /** Show the line "Type an answer, then press Submit." under the field. */
  remindEmpty?: () => void;
}

export interface AnswerFieldProps {
  placeholder?: string;
  onSubmit?: () => void;
  onHint?: (() => void) | undefined;
  disabled?: boolean;
  /** The answer contract of the served problem, when it has one. */
  contract?: AnswerContractHint | undefined;
  /** The id of the served problem. The typed text is kept per id, so a return finds it. */
  draftKey?: string | undefined;
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

/** The label and the spoken name of each key. */
function keyLabel(key: SymbolKey): { text: string; name: string } {
  if (key === 'mixed') return { text: 'a b/c', name: 'Insert mixed number' };
  if (key === 'root') return { text: 'ⁿ√', name: 'Insert indexed root' };
  return { text: key, name: `Insert ${key}` };
}

export function AnswerField({
  placeholder = 'Your answer',
  onSubmit,
  onHint,
  disabled = false,
  contract,
  draftKey,
  ref,
}: AnswerFieldProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [shown, setShown] = useState('');
  const [rootIndex, setRootIndex] = useState<number>(3);
  const [moreOpen, setMoreOpen] = useState<boolean>(readMoreOpen);
  const [emptyNote, setEmptyNote] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  // The preview follows the input through a debounce; it is never the source of the value.
  const refresh = () => {
    clearTimeout(timer.current);
    timer.current = setTimeout(() => { setShown(inputRef.current?.value ?? ''); }, PREVIEW_DEBOUNCE_MS);
  };
  // The typed text of this problem survives a return to the same problem.
  const saveDraft = useCallback(() => {
    if (draftKey && inputRef.current) writeDraft(draftKey, inputRef.current.value);
  }, [draftKey]);
  useEffect(() => () => { clearTimeout(timer.current); }, []);
  useEffect(() => {
    if (!draftKey || !inputRef.current || inputRef.current.value !== '') return;
    const saved = readDraft(draftKey);
    if (saved === '') return;
    inputRef.current.value = saved;
    refresh();
  }, [draftKey]);
  // A tap on a chip of the "How to type answers" panel writes into this field at the caret.
  useEffect(() => registerInserter((text) => {
    if (!inputRef.current || inputRef.current.disabled) return;
    insertAtCursor(inputRef.current, text);
    saveDraft();
    refresh();
  }), [saveDraft]);
  const helpOpen = useNotationOpen();

  // NO `setDisabled` on the handle. `disabled` has exactly ONE owner — the prop. With both,
  // React never rewrites an unchanged prop, so an imperative `setDisabled(true)` survives a
  // re-render and the prop then lies. The result is either an Enter dropped in a frame that
  // looks live, or an Enter accepted while the feedback panel is on screen. The keydown guard
  // below reads the same attribute, so it inherits the lie.
  //
  // A view drives it from its phase instead: `disabled={phase !== 'ready'}`.
  useImperativeHandle(ref, () => ({
    value: () => inputRef.current?.value.trim() ?? '',
    clear: () => {
      if (inputRef.current) inputRef.current.value = '';
      if (draftKey) clearDraft(draftKey);
      clearTimeout(timer.current);
      setShown('');
      setEmptyNote(false);
    },
    focus: () => inputRef.current?.focus(),
    remindEmpty: () => { setEmptyNote(true); },
  }));

  const context = contextKeys(contract);
  const rest = ALL_SYMBOL_KEYS.filter((k) => !context.includes(k));
  // ONE tab stop: the first key takes the tab, and the arrow keys move between the keys.
  const firstKey = context[0] ?? rest[0];

  const press = (key: SymbolKey) => {
    const input = inputRef.current!;
    if (key === 'mixed') insertAtCursor(input, MIXED_TEMPLATE, 0);
    else if (key === 'root') insertAtCursor(input, `root(${rootIndex}, `);
    else insertAtCursor(input, key);
    saveDraft();
    refresh();
  };

  const renderKey = (key: SymbolKey) => {
    const { text, name } = keyLabel(key);
    return (
      <span className="sym-item" key={key}>
        {key === 'root' ? (
          <select
            className="sym-select"
            aria-label="Root index"
            value={rootIndex}
            onChange={(e) => { setRootIndex(Number(e.target.value)); }}
          >
            {ROOT_INDEXES.map((n) => <option key={n} value={n}>{n}</option>)}
          </select>
        ) : null}
        <button
          type="button"
          className="sym-key"
          aria-label={name}
          tabIndex={key === firstKey ? 0 : -1}
          // A mousedown steals focus from the input before the click lands, and the caret
          // position goes with it. The suppression is what makes the tap survive.
          onMouseDown={(e) => { e.preventDefault(); }}
          // The input is on screen for as long as the key beside it is.
          onClick={() => { press(key); }}
        >
          {text}
        </button>
      </span>
    );
  };

  // Left and Right move the focus between the keys; Home and End jump to the first and last.
  const moveFocus = (e: KeyboardEvent<HTMLDivElement>) => {
    const target = e.target as HTMLElement;
    if (!target.classList.contains('sym-key')) return;
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) return;
    const keys = Array.from(e.currentTarget.closest('.sym-area')!.querySelectorAll<HTMLButtonElement>('.sym-key'))
      .filter((k) => !k.closest('details:not([open]) > .sym-palette'));
    const at = keys.indexOf(target as HTMLButtonElement);
    if (at < 0) return;
    let next = at;
    if (e.key === 'ArrowRight') next = (at + 1) % keys.length;
    else if (e.key === 'ArrowLeft') next = (at - 1 + keys.length) % keys.length;
    else if (e.key === 'Home') next = 0;
    else next = keys.length - 1;
    e.preventDefault();
    keys[next]?.focus();
  };

  const hint = inputHint(contract);

  return (
    <div className="answer-field">
      <input
        ref={inputRef}
        type="text"
        className={`answer-input${isPlainNumber(contract) ? ' is-short' : ''}`}
        placeholder={placeholder}
        aria-label="Answer"
        autoComplete="off"
        autoCapitalize="off"
        autoCorrect="off"
        spellCheck={false}
        defaultValue=""
        disabled={disabled}
        onInput={() => { setEmptyNote(false); saveDraft(); refresh(); }}
        onKeyDown={(e) => {
          const input = e.currentTarget;
          if (e.key === 'Enter') {
            e.preventDefault();
            // Enter is the SAME action as the primary Submit button, and it does NOT route
            // through it: a busy view disables the BUTTON while a grade of several seconds
            // runs, and a disabled button does nothing to a keydown on the input. So this
            // path never decides whether a submit starts — the view's phase check decides
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
      <p className="field-note" role="status">{emptyNote ? EMPTY_ANSWER_LINE : ''}</p>
      <div className="sym-area">
        {context.length > 0 ? (
          <div className="sym-palette" role="toolbar" aria-label="Symbol keys. Use the arrow keys to move." onKeyDown={moveFocus}>
            {context.map(renderKey)}
          </div>
        ) : null}
        <details
          className="sym-more"
          open={moreOpen}
          onToggle={(e) => {
            const next = e.currentTarget.open;
            if (next === moreOpen) return;
            setMoreOpen(next);
            writeMoreOpen(next);
          }}
        >
          <summary>More symbols</summary>
          <div className="sym-palette" role="toolbar" aria-label="Symbol keys. Use the arrow keys to move." onKeyDown={moveFocus}>
            {rest.map(renderKey)}
          </div>
        </details>
        <button
          type="button"
          className="link-btn sym-help"
          aria-pressed={helpOpen}
          onMouseDown={(e) => { e.preventDefault(); }}
          onClick={() => { setNotationOpen(!helpOpen); }}
        >
          How to type answers
        </button>
      </div>
      <AnswerPreview text={shown} />
      {hint ? <p className="field-hint">{hint}</p> : null}
    </div>
  );
}
