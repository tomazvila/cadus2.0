/**
 * The keypad of the structural answer editor.
 *
 * `keypadFor(contract)` picks its extra keys from `contractFamily` in `lib/notation.ts`, the
 * same table that picks the palette keys of the plain field. One table decides both.
 *
 * Structural keys insert a structure with slots, not a glyph: the fraction key inserts
 * `\frac{#@}{#0}`. `#@` takes the selection or the content before the caret, `#0` is the
 * first empty slot, `#?` is a slot the learner fills in turn. A key that names a MathLive
 * shortcut (`[backspace]`, `[0]`) carries a `label` and NO `latex`: with `latex`, the
 * backspace key types the text `[backspace]` into the field.
 *
 * No key inserts `\div`. The checker lexer does not list it, and the fraction key covers
 * division. `crates/core/tests/editor_latex.rs` grades the LaTeX of every key.
 */
import type { KeyboardLayoutLike, KeycapLike } from '@/lib/math-editor-loader';
import { contractFamily, type ContractFamily } from '@/lib/notation';
import type { AnswerContractHint } from '@/api/types';

/** The widest row, in keycap widths. Eight keys of 44 px fit at 320 px. */
export const MAX_ROW_WIDTH = 8;

/** One key: what MathLive draws, plus the tokens that fill its slots in the tests. */
interface Key {
  cap: KeycapLike;
  /** The tokens for the slots of `cap.latex`, in order of appearance. */
  fill?: readonly string[];
  /** A whole expression that uses the key, when the bare key is no answer: `3\times4` for the times key. */
  sample?: string;
}

const shortcut = (label: string, tooltip?: string): Key => ({
  cap: tooltip === undefined ? { label } : { label, tooltip },
});
const glyph = (latex: string, tooltip: string, fill: readonly string[] = [], sample?: string): Key => ({
  cap: { latex, tooltip, class: 'MLK__tex' },
  fill,
  ...(sample === undefined ? {} : { sample }),
});

const FRACTION = glyph('\\frac{#@}{#0}', 'Fraction: a box over a box', ['3', '4']);
const ROOT = glyph('\\sqrt{#0}', 'Square root: a box under a root sign', ['2']);
const POWER = glyph('#@^{#?}', 'Power: a small raised box', ['x', '2']);
const TIMES = glyph('\\times', 'Times', [], '3\\times4');
const MIXED = glyph('#@\\frac{#?}{#?}', 'Mixed number: whole number, then a fraction', ['4', '2', '5']);
const INDEXED_ROOT = glyph('\\sqrt[#?]{#0}', 'Root with an index: a small box and a box', ['3', '8']);

/** The keys that each family adds in a row of its own. */
const EXTRA: Record<ContractFamily, Key[]> = {
  mixed: [MIXED],
  fraction: [],
  radical: [INDEXED_ROOT],
  trig: [glyph('\\pi', 'Pi', [], '2\\pi'), glyph('\\theta', 'Theta', [], '2\\theta'),
    glyph('^\\circ', 'Degree sign', [], '90^\\circ')],
  inequality: [
    glyph('\\le', 'Less than or equal', [], 'x\\le3'), glyph('\\ge', 'Greater than or equal', [], 'x\\ge3'),
    glyph('\\ne', 'Not equal', [], 'x\\ne3'), glyph('\\infty', 'Infinity', [], '(-\\infty,4]'),
  ],
  interval: [
    glyph('\\infty', 'Infinity', [], '(-\\infty,4]'), glyph('\\pm', 'Plus or minus', [], 'x=\\pm3'),
    glyph('[', 'Left square bracket', [], '[1,4]'), glyph(']', 'Right square bracket', [], '[1,4]'),
    glyph('\\{', 'Left brace', [], '\\{1,2\\}'), glyph('\\}', 'Right brace', [], '\\{1,2\\}'),
  ],
  exponent: [],
  expression: [],
};

const BASE_ROWS: Key[][] = [
  [
    shortcut('[7]'), shortcut('[8]'), shortcut('[9]'), shortcut('[+]', 'Plus'),
    TIMES, FRACTION, ROOT, POWER,
  ],
  [
    shortcut('[4]'), shortcut('[5]'), shortcut('[6]'), shortcut('[-]', 'Minus'),
    shortcut('[(]', 'Round bracket, open'), shortcut('[)]', 'Round bracket, close'), shortcut('[,]', 'Comma'),
    shortcut('[backspace]', 'Delete the character before the cursor'),
  ],
  [
    shortcut('[1]'), shortcut('[2]'), shortcut('[3]'), shortcut('[0]'), shortcut('[.]', 'Decimal point'),
    shortcut('[left]', 'Move left'), shortcut('[right]', 'Move right'),
    shortcut('[hide-keyboard]', 'Close the keypad'),
  ],
];

/** The rows of the keypad for a served contract. */
function rowsFor(contract: AnswerContractHint | undefined): Key[][] {
  const family = contractFamily(contract);
  const extra = family ? EXTRA[family] : [];
  return extra.length > 0 ? [...BASE_ROWS, extra] : BASE_ROWS;
}

/** The keypad layouts of a served contract. One layout; the editor appends MathLive's `alphabetic`. */
export function keypadFor(contract: AnswerContractHint | undefined): KeyboardLayoutLike[] {
  return [{
    label: '123',
    tooltip: 'Numbers and math keys',
    rows: rowsFor(contract).map((row) => row.map((k) => k.cap)),
  }];
}

/** The width of one keycap, in keycap widths. */
export function capWidth(cap: KeycapLike): number {
  return cap.width ?? 1;
}

/** One key LaTeX with its slots filled by the sample tokens. */
export interface KeySample {
  /** The LaTeX of the key as the keypad inserts it. */
  key: string;
  /** The LaTeX after the editor fills the slots with the sample tokens. */
  latex: string;
}

/** The LaTeX of every inserting key of a contract, slots filled with sample tokens. */
export function keypadSamples(contract: AnswerContractHint | undefined): KeySample[] {
  const samples: KeySample[] = [];
  for (const key of rowsFor(contract).flat()) {
    const template = key.cap.latex;
    if (template === undefined) continue;
    const tokens = [...(key.fill ?? [])];
    const latex = key.sample ?? template.replace(/#[@0?]/g, () => tokens.shift() ?? '');
    samples.push({ key: template, latex });
  }
  return samples;
}
