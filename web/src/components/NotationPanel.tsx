/**
 * "How to type answers": the cheat sheet of spellings the answer field reads.
 *
 * At 1400 px and wider it is a panel in the margin beside the problem: the problem and the
 * field stay where they are and the panel scrolls. Below that it is a full-screen sheet with
 * a close button. The page column never moves in either mode. The
 * open state lives in `lib/notation.ts`, so the panel stays open from one problem to the next.
 *
 * Each row shows the rendered form, the plain spelling and the LaTeX spelling. A tap on a
 * chip writes that spelling into the answer field at the caret.
 */
import { useEffect, useState } from 'react';
import { MathBlock } from '@/components/MathBlock';
import {
  NOTATION_SECTIONS, WIDE_QUERY, filterSections, insertNotation, setNotationOpen, useNotationOpen,
  type NotationRow,
} from '@/lib/notation';

function isWide(): boolean {
  if (typeof window.matchMedia === 'function') return window.matchMedia(WIDE_QUERY).matches;
  return window.innerWidth >= 1400;
}

function useWide(): boolean {
  const [wide, setWide] = useState(isWide);
  useEffect(() => {
    const update = () => { setWide(isWide()); };
    const list = typeof window.matchMedia === 'function' ? window.matchMedia(WIDE_QUERY) : null;
    list?.addEventListener('change', update);
    window.addEventListener('resize', update);
    return () => { list?.removeEventListener('change', update); window.removeEventListener('resize', update); };
  }, []);
  return wide;
}

function Chip({ label, text }: { label: string; text: string }) {
  return (
    <button
      type="button"
      className="notation-chip mono"
      aria-label={`Insert ${label} spelling ${text}`}
      title={`Write ${text} in your answer`}
      // A mousedown steals focus from the answer field and the caret goes with it.
      onMouseDown={(e) => { e.preventDefault(); }}
      onClick={() => { insertNotation(text); }}
    >
      {text}
    </button>
  );
}

function Row({ row }: { row: NotationRow }) {
  return (
    <li className="notation-row">
      <div className="notation-want">{row.want}</div>
      <MathBlock as="span" className="notation-rendered">{`$${row.latex}$`}</MathBlock>
      <div className="notation-chips">
        <span className="notation-chip-label muted small">Type</span>
        <Chip label="typed" text={row.type} />
        <span className="notation-chip-label muted small">LaTeX</span>
        <Chip label="LaTeX" text={row.latex} />
      </div>
    </li>
  );
}

export function NotationPanel() {
  const open = useNotationOpen();
  const wide = useWide();
  const [query, setQuery] = useState('');

  useEffect(() => {
    if (!open || wide) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setNotationOpen(false); };
    window.addEventListener('keydown', onKey);
    return () => { window.removeEventListener('keydown', onKey); };
  }, [open, wide]);

  if (!open) return null;
  const sections = filterSections(NOTATION_SECTIONS, query);
  return (
    <aside
      className={`notation-panel ${wide ? 'notation-side' : 'notation-sheet'}`}
      aria-label="How to type answers"
      {...(wide ? {} : { role: 'dialog', 'aria-modal': true })}
    >
      <div className="notation-head">
        <h2>How to type answers</h2>
        <button type="button" className="btn btn-ghost" onClick={() => { setNotationOpen(false); }}>
          Close
        </button>
      </div>
      <p className="muted small">Tap a spelling to write it in your answer at the cursor.</p>
      <input
        type="search"
        className="notation-search"
        aria-label="Search the examples"
        placeholder="Search, for example root or fraction"
        value={query}
        onChange={(e) => { setQuery(e.target.value); }}
      />
      <div className="notation-body">
        {sections.length === 0 ? <p className="muted">No example matches. Try one word.</p> : null}
        {sections.map((section) => (
          <section key={section.name} className="notation-section">
            <h3>{section.name}</h3>
            <ul className="notation-rows">
              {section.rows.map((row) => <Row key={`${row.type}|${row.latex}`} row={row} />)}
            </ul>
          </section>
        ))}
      </div>
    </aside>
  );
}
