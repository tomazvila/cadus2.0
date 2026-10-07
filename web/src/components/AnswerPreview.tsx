/** The rendered preview of a typed answer (see `lib/preview.ts`). Raw text when it cannot parse. */
import { parsePreview, type PreviewNode } from '@/lib/preview';

function Node({ n }: { n: PreviewNode }) {
  switch (n.t) {
    case 'num': case 'var': return <span>{n.v}</span>;
    case 'op': return <span className="pv-op">{n.v}</span>;
    case 'frac': return <Frac n={<Node n={n.n} />} d={<Node n={n.d} />} />;
    case 'mixed': return <span className="pv-mixed"><span>{n.w}</span><Frac n={n.n} d={n.d} /></span>;
    case 'sqrt': return <span className="pv-sqrt">√<span className="pv-radicand"><Node n={n.x} /></span></span>;
    case 'pow': return <span><Node n={n.b} /><sup><Node n={n.e.t === 'paren' ? n.e.x : n.e} /></sup></span>;
    case 'paren': return <span>(<Node n={n.x} />)</span>;
    case 'seq': return <span className="pv-seq">{n.xs.map((x, i) => <Node key={i} n={x} />)}</span>;
  }
}

function Frac({ n, d }: { n: React.ReactNode; d: React.ReactNode }) {
  return <span className="pv-frac"><span className="pv-num">{n}</span><span className="pv-den">{d}</span></span>;
}

export function AnswerPreview({ text }: { text: string }) {
  const trimmed = text.trim();
  if (trimmed === '') return null;
  const tree = parsePreview(trimmed);
  return (
    <div className="answer-preview" aria-live="off" data-testid="answer-preview">
      <span className="muted small">You wrote:</span>{' '}
      {tree ? <Node n={tree} /> : <span className="pv-raw">{trimmed}</span>}
    </div>
  );
}
