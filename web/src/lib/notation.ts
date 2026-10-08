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
