/**
 * The keypad of the structural editor: the keys, the widths, and the LaTeX each key makes.
 * `crates/core/tests/editor_latex.rs` grades the same fixture through the checker.
 */
import { describe, expect, it } from 'vitest';
import fixture from './fixtures/editor-latex.json';
import { MAX_ROW_WIDTH, capWidth, keypadFor, keypadSamples } from '@/lib/keypad';
import { contextKeys } from '@/lib/notation';
import type { AnswerContractHint } from '@/api/types';

const CONTRACTS: Array<[string, AnswerContractHint | undefined]> = [
  ['none', undefined],
  ['fraction', { kind: 'required_form', form: 'reduced_fraction' }],
  ['mixed number', { kind: 'required_form', form: 'mixed_number' }],
  ['radical', { kind: 'required_form', form: 'radical' }],
  ['trig', { kind: 'unit', form: 'angle' }],
  ['inequality', { kind: 'inequality_union' }],
  ['interval', { kind: 'set' }],
  ['exponent', { kind: 'exact', form: 'rational_exponent' }],
  ['expression', { kind: 'polynomial_relation' }],
];

const keysOf = (contract: AnswerContractHint | undefined) => keypadFor(contract).flatMap((l) => l.rows.flat());
const hasLabel = (contract: AnswerContractHint | undefined, label: string) =>
  keysOf(contract).some((k) => k.label === label);

describe.each(CONTRACTS)('the keypad for %s', (_name, contract) => {
  it('holds the digits, a fraction key, a root key, a power key, backspace and hide', () => {
    for (const digit of '0123456789') expect(hasLabel(contract, `[${digit}]`)).toBe(true);
    const latex = keysOf(contract).map((k) => k.latex);
    expect(latex).toContain('\\frac{#@}{#0}');
    expect(latex).toContain('\\sqrt{#0}');
    expect(latex).toContain('#@^{#?}');
    expect(hasLabel(contract, '[backspace]')).toBe(true);
    expect(hasLabel(contract, '[hide-keyboard]')).toBe(true);
    for (const label of ['[.]', '[+]', '[-]', '[(]', '[)]', '[,]', '[left]', '[right]']) {
      expect(hasLabel(contract, label)).toBe(true);
    }
    expect(latex).toContain('\\times');
  });

  it('has no row wider than eight keycap widths', () => {
    for (const layout of keypadFor(contract)) {
      for (const row of layout.rows) {
        expect(row.reduce((sum, cap) => sum + capWidth(cap), 0)).toBeLessThanOrEqual(MAX_ROW_WIDTH);
      }
    }
  });

  it('gives a shortcut key a label and no latex, and never inserts \\div', () => {
    for (const key of keysOf(contract)) {
      if (key.label?.startsWith('[')) expect(key.latex).toBeUndefined();
      expect(key.latex ?? '').not.toContain('\\div');
    }
  });

  it('makes the structural keys insert structures with slots', () => {
    const byLatex = (latex: string) => keysOf(contract).find((k) => k.latex === latex);
    for (const latex of ['\\frac{#@}{#0}', '\\sqrt{#0}', '#@^{#?}']) {
      expect(byLatex(latex)?.latex).toMatch(/#[@0?]/);
    }
  });

  it('names every inserting key for a screen reader', () => {
    for (const key of keysOf(contract)) {
      if (key.latex !== undefined) expect(key.tooltip ?? '').not.toBe('');
    }
  });
});

describe('the keys that a contract adds', () => {
  const latexOf = (contract: AnswerContractHint) => keysOf(contract).map((k) => k.latex);

  it('a mixed number gets the mixed-number key', () => {
    expect(latexOf({ kind: 'required_form', form: 'mixed_number' })).toContain('#@\\frac{#?}{#?}');
  });

  it('a radical gets the indexed root', () => {
    expect(latexOf({ kind: 'required_form', form: 'radical' })).toContain('\\sqrt[#?]{#0}');
  });

  it('a trig contract gets \\pi, \\theta and ^\\circ', () => {
    const latex = latexOf({ kind: 'unit', form: 'angle' });
    for (const k of ['\\pi', '\\theta', '^\\circ']) expect(latex).toContain(k);
  });

  it('an inequality gets \\le, \\ge, \\ne and \\infty', () => {
    const latex = latexOf({ kind: 'inequality_union' });
    for (const k of ['\\le', '\\ge', '\\ne', '\\infty']) expect(latex).toContain(k);
  });

  it('an interval or a set gets \\infty, \\pm, brackets and braces', () => {
    const latex = latexOf({ kind: 'set' });
    for (const k of ['\\infty', '\\pm', '[', ']', '\\{', '\\}']) expect(latex).toContain(k);
  });

  it('a plain contract gets none of them', () => {
    const latex = latexOf({ kind: 'exact' });
    for (const k of ['\\pi', '\\le', '\\infty', '\\sqrt[#?]{#0}']) expect(latex).not.toContain(k);
  });

  it('one table decides the palette and the keypad', () => {
    expect(contextKeys({ kind: 'required_form', form: 'mixed_number' })).toEqual(['mixed']);
    expect(contextKeys({ kind: 'required_form', form: 'radical' })).toContain('root');
  });
});

describe('the fixture of key LaTeX', () => {
  const rows = fixture.rows as Array<{
    key: string; latex: string; answer: string; contract: { kind: string }; hint?: AnswerContractHint; source?: string;
  }>;

  it('holds a row for every inserting key with its sample tokens, under every contract', () => {
    for (const [name, contract] of CONTRACTS) {
      for (const sample of keypadSamples(contract)) {
        const row = rows.find((r) => r.key === sample.key && r.latex === sample.latex);
        expect(row, `${name}: no fixture row for ${sample.key} as ${sample.latex}`).toBeDefined();
      }
    }
  });

  it('holds a row whose hint gives the keypad that makes the key', () => {
    for (const row of rows.filter((r) => r.source === undefined)) {
      const hint = row.hint;
      const samples = keypadSamples(hint);
      expect(samples.some((s) => s.key === row.key && s.latex === row.latex), `${row.key}`).toBe(true);
    }
  });

  it('puts no \\div in the fixture', () => {
    for (const row of rows) expect(row.latex).not.toContain('\\div');
  });
});
