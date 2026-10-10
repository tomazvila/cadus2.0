/**
 * The notation cheat sheet: its rows, its search, and the small store that holds whether the
 * panel is open and which answer field a tapped chip writes into.
 *
 * The rows come from one file, `notation-examples.json`. A Rust test grades every row, so the
 * panel never shows a spelling the grader refuses.
 */
import { useSyncExternalStore } from 'react';
import data from './notation-examples.json';

export interface NotationRow {
  /** What the learner wants to write, in plain words. */
  want: string;
  /** The plain keyboard spelling. */
  type: string;
  /** The LaTeX spelling. */
  latex: string;
}

export interface NotationSection {
  name: string;
  rows: NotationRow[];
}

export const NOTATION_SECTIONS: NotationSection[] = (data as { sections: NotationSection[] }).sections;

/** The sections whose rows match every word of `query`, in any of the row's texts. */
export function filterSections(sections: NotationSection[], query: string): NotationSection[] {
  const words = query.toLowerCase().split(/\s+/).filter((w) => w !== '');
  if (words.length === 0) return sections;
  return sections
    .map((section) => ({
      name: section.name,
      rows: section.rows.filter((row) => {
        const text = `${section.name} ${row.want} ${row.type} ${row.latex}`.toLowerCase();
        return words.every((w) => text.includes(w));
      }),
    }))
    .filter((section) => section.rows.length > 0);
}

// ---- the store ----

const OPEN_KEY = 'cadus.notation.open';
const listeners = new Set<() => void>();
let open = readOpen();
let inserter: ((text: string) => void) | null = null;

function readOpen(): boolean {
  try { return window.localStorage.getItem(OPEN_KEY) === '1'; } catch { return false; }
}

function emit(): void {
  listeners.forEach((fn) => { fn(); });
}

/** Open or close the panel. The choice is kept for the next problem and the next visit. */
export function setNotationOpen(next: boolean): void {
  if (open === next) return;
  open = next;
  try { window.localStorage.setItem(OPEN_KEY, next ? '1' : '0'); } catch { /* storage is optional */ }
  emit();
}

/** Read the stored choice again. A test calls it after it clears the storage. */
export function resetNotationStore(): void {
  open = readOpen();
  inserter = null;
  emit();
}

export function useNotationOpen(): boolean {
  return useSyncExternalStore(
    (fn) => { listeners.add(fn); return () => { listeners.delete(fn); }; },
    () => open,
  );
}

/** The answer field on screen gives its insert function here. It returns the undo. */
export function registerInserter(fn: (text: string) => void): () => void {
  inserter = fn;
  return () => { if (inserter === fn) inserter = null; };
}

/** Write `text` into the answer field at the caret. False when no typed field is on screen. */
export function insertNotation(text: string): boolean {
  if (!inserter) return false;
  inserter(text);
  return true;
}

/** The panel sits beside the problem at this width and above it. Below, it is a full sheet. */
export const WIDE_QUERY = '(min-width: 1024px)';

// ---- the symbol keys that one answer contract needs ----

/** One key of the palette: a plain symbol, the mixed-number template, or the indexed root. */
export type SymbolKey = '∞' | 'π' | '√(' | '^' | '≤' | '≥' | '≠' | '±' | '×' | '÷' | '°' | 'θ' | 'mixed' | 'root';

/** Every key, in the order the "More symbols" list shows them. */
export const ALL_SYMBOL_KEYS: readonly SymbolKey[] = [
  '∞', 'π', '√(', '^', '≤', '≥', '≠', '±', '×', '÷', '°', 'θ', 'mixed', 'root',
];

/** The most keys the answer field shows outside "More symbols". */
export const MAX_CONTEXT_KEYS = 5;

interface ContractLike {
  kind: string;
  form?: string | undefined;
}

/**
 * The keys that the answer of one problem is likely to need, from the served contract.
 * A fraction needs none. A learner with no contract sees no key outside "More symbols".
 */
export function contextKeys(contract: ContractLike | undefined): SymbolKey[] {
  if (!contract) return [];
  const text = `${contract.kind} ${contract.form ?? ''}`;
  let keys: SymbolKey[] = [];
  if (/mixed_number/.test(text)) keys = ['mixed'];
  else if (/fraction/.test(text)) keys = [];
  else if (/radical/.test(text)) keys = ['√(', 'root'];
  else if (/trig|angle/.test(text)) keys = ['π', 'θ', '°'];
  else if (/inequal/.test(text)) keys = ['≤', '≥', '≠', '∞'];
  else if (/interval|set/.test(text)) keys = ['∞', '±'];
  else if (/exponent/.test(text)) keys = ['^', '√('];
  else if (/polynomial|expression|relation|function/.test(text)) keys = ['^', '×', '÷'];
  return keys.slice(0, MAX_CONTEXT_KEYS);
}

/** True if the answer is one plain number, so the field needs only a short width. */
export function isPlainNumber(contract: ContractLike | undefined): boolean {
  if (!contract) return false;
  const form = contract.form ?? '';
  return form === 'integer' || form === 'decimal' || contract.kind === 'integer'
    || contract.kind === 'decimal' || contract.kind === 'approx';
}

// ---- the "More symbols" choice ----

const MORE_KEY = 'cadus.symbols.more';

/** True if the learner left "More symbols" open at the last visit. */
export function readMoreOpen(): boolean {
  try { return window.localStorage.getItem(MORE_KEY) === '1'; } catch { return false; }
}

export function writeMoreOpen(next: boolean): void {
  try { window.localStorage.setItem(MORE_KEY, next ? '1' : '0'); } catch { /* storage is optional */ }
}

// ---- the draft of one problem ----

/** The sessionStorage key of the typed text of one problem. */
export function draftStorageKey(problemId: string): string {
  return `cadus.draft.${problemId}`;
}

export function readDraft(problemId: string): string {
  try { return window.sessionStorage.getItem(draftStorageKey(problemId)) ?? ''; } catch { return ''; }
}

export function writeDraft(problemId: string, text: string): void {
  try {
    if (text === '') window.sessionStorage.removeItem(draftStorageKey(problemId));
    else window.sessionStorage.setItem(draftStorageKey(problemId), text);
  } catch { /* storage is optional */ }
}

export function clearDraft(problemId: string): void {
  writeDraft(problemId, '');
}
