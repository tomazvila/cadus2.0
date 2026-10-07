/**
 * The live preview of a typed answer.
 *
 * A small pure parser for the grammar the app documents: numbers, fractions, mixed numbers,
 * + − × ÷, ^, sqrt and variables. It reads the text the way a learner means it. It does NOT
 * grade, and it does not claim the grader reads the text the same way. When the text does not
 * parse, `parsePreview` returns null and the view shows the raw text.
 */

export type PreviewNode =
  | { t: 'num'; v: string }
  | { t: 'var'; v: string }
  | { t: 'op'; v: string }
  | { t: 'frac'; n: PreviewNode; d: PreviewNode }
  | { t: 'mixed'; w: string; n: string; d: string }
  | { t: 'sqrt'; x: PreviewNode }
  | { t: 'pow'; b: PreviewNode; e: PreviewNode }
  | { t: 'paren'; x: PreviewNode }
  | { t: 'seq'; xs: PreviewNode[] };

type Tok = { k: 'num' | 'id' | 'op' | 'mix' | 'sqrt' | '(' | ')' | '/' | '^'; v: string };

const OPS: Record<string, string> = {
  '+': '+', '-': '−', '−': '−', '*': '×', '×': '×', '÷': '÷',
  '=': '=', '<': '<', '>': '>', '≤': '≤', '≥': '≥', '≠': '≠', '±': '±', ',': ',',
};

class Unreadable extends Error {}

/** The token of the word at the start of a run of letters, and its length. */
function readWord(w: string): { tok: Tok; len: number } {
  if (w === 'and') return { tok: { k: 'mix', v: w }, len: 3 };
  if (w.startsWith('sqrt')) return { tok: { k: 'sqrt', v: 'sqrt' }, len: 4 };
  const first = String.fromCodePoint(w.codePointAt(0)!);
  return { tok: { k: 'id', v: first }, len: first.length };
}

function lex(text: string): Tok[] {
  const toks: Tok[] = [];
  let i = 0;
  while (i < text.length) {
    const c = text[i]!;
    if (/\s/.test(c)) { i += 1; continue; }
    const num = /^\d+(\.\d+)?/.exec(text.slice(i));
    if (num) { toks.push({ k: 'num', v: num[0] }); i += num[0].length; continue; }
    if (c in OPS) { toks.push({ k: 'op', v: OPS[c]! }); i += 1; continue; }
    if (c === '(' || c === ')' || c === '/' || c === '^') { toks.push({ k: c, v: c }); i += 1; continue; }
    if (c === '_') { toks.push({ k: 'mix', v: c }); i += 1; continue; }
    if (c === '√') { toks.push({ k: 'sqrt', v: c }); i += 1; continue; }
    const word = /^\p{L}+/u.exec(text.slice(i));
    const w = word ? readWord(word[0]) : c === '∞' || c === '°' ? { tok: { k: 'id', v: c } as Tok, len: 1 } : null;
    if (!w) throw new Unreadable();
    toks.push(w.tok);
    i += w.len;
  }
  return toks;
}

const isInt = (n: PreviewNode | undefined): n is { t: 'num'; v: string } =>
  n?.t === 'num' && !n.v.includes('.');

/** Fold `item` into the whole number last in `xs` as a mixed number. False when it does not fit. */
function joinMixed(xs: PreviewNode[], item: PreviewNode): boolean {
  const last = xs[xs.length - 1];
  if (!isInt(last) || item.t !== 'frac' || !isInt(item.n) || !isInt(item.d)) return false;
  xs[xs.length - 1] = { t: 'mixed', w: last.v, n: item.n.v, d: item.d.v };
  return true;
}

class Parser {
  private pos = 0;
  constructor(private readonly toks: Tok[]) {}

  private peek(): Tok | undefined { return this.toks[this.pos]; }

  done(): boolean { return this.pos >= this.toks.length; }

  seq(nested: boolean): PreviewNode {
    const xs: PreviewNode[] = [];
    let join = false;
    for (;;) {
      const t = this.peek();
      if (!t || (nested && t.k === ')')) break;
      if (t.k === ')') throw new Unreadable();
      if (t.k === 'op') { this.pos += 1; xs.push({ t: 'op', v: t.v }); join = false; continue; }
      if (t.k === 'mix') {
        // `4 and 2/5` and `4_2/5`: the joiner is only valid between a whole number and a fraction.
        this.pos += 1;
        const next = this.term();
        if (!join || !joinMixed(xs, next)) throw new Unreadable();
        continue;
      }
      const item = this.term();
      if (!(join && joinMixed(xs, item))) xs.push(item);
      join = true;
    }
    if (xs.length === 0) throw new Unreadable();
    return xs.length === 1 ? xs[0]! : { t: 'seq', xs };
  }

  private term(): PreviewNode {
    let left = this.power();
    while (this.peek()?.k === '/') {
      this.pos += 1;
      left = { t: 'frac', n: left, d: this.power() };
    }
    return left;
  }

  private power(): PreviewNode {
    const base = this.atom();
    if (this.peek()?.k !== '^') return base;
    this.pos += 1;
    const sign = this.peek();
    if (sign?.k === 'op' && sign.v === '−') {
      this.pos += 1;
      return { t: 'pow', b: base, e: { t: 'seq', xs: [{ t: 'op', v: '−' }, this.atom()] } };
    }
    return { t: 'pow', b: base, e: this.atom() };
  }

  private atom(): PreviewNode {
    const t = this.peek();
    if (!t) throw new Unreadable();
    this.pos += 1;
    if (t.k === 'num') return { t: 'num', v: t.v };
    if (t.k === 'id') return { t: 'var', v: t.v };
    if (t.k === '(') {
      const x = this.seq(true);
      if (this.peek()?.k !== ')') throw new Unreadable();
      this.pos += 1;
      return { t: 'paren', x };
    }
    if (t.k === 'sqrt') {
      const a = this.atom();
      return { t: 'sqrt', x: a.t === 'paren' ? a.x : a };
    }
    throw new Unreadable();
  }
}

/** The tree of `text`, or null when the text is blank or the grammar does not read it. */
export function parsePreview(text: string): PreviewNode | null {
  try {
    const p = new Parser(lex(text));
    const tree = p.seq(false);
    return p.done() ? tree : null;
  } catch (e) {
    if (e instanceof Unreadable) return null;
    throw e;
  }
}

/** A flat one-line form of a tree: the stable shape the unit tests read. */
export function signature(n: PreviewNode): string {
  switch (n.t) {
    case 'num': case 'var': case 'op': return n.v;
    case 'frac': return `frac(${signature(n.n)},${signature(n.d)})`;
    case 'mixed': return `mixed(${n.w},${n.n},${n.d})`;
    case 'sqrt': return `√${signature(n.x)}`;
    case 'pow': return `pow(${signature(n.b)},${signature(n.e)})`;
    case 'paren': return `(${signature(n.x)})`;
    case 'seq': return n.xs.map(signature).join(' ');
  }
}
