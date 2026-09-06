/**
 * One mathematical visual beside a problem (unit f9).
 *
 * The component draws the server figure and the TEXT EQUIVALENT together. A learner who
 * reads the picture and a learner who reads the words answer the same question:
 *
 *  - The `<svg>` carries `role="img"` and points at its own `<title>` and `<desc>`, so a
 *    screen reader announces the figure and reads its facts.
 *  - A `<details>` beside it prints the same facts on the page, for a learner who reads
 *    text but uses no screen reader.
 *
 * A figure the sanitizer refuses draws nothing, and the text equivalent stays. The learner
 * keeps a usable question in place of a picture that this document must not run.
 */
import { memo, useEffect, useRef } from 'react';
import type { RenderedVisual } from '@/lib/visual';
import { sanitizeVisualSvg } from '@/lib/visual';

export interface MathVisualProps {
  /** The rendered figure the API sent. */
  visual: RenderedVisual;
}

export const MathVisual = memo(function MathVisual({ visual }: MathVisualProps) {
  const holder = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const node = holder.current;
    if (node === null) return;
    node.replaceChildren();
    const figure = sanitizeVisualSvg(visual.svg);
    if (figure !== null) node.appendChild(figure);
  }, [visual.svg]);

  return (
    <figure className={`math-visual math-visual-${visual.kind}`} data-kind={visual.kind}>
      <div className="math-visual-frame" ref={holder} />
      <figcaption className="math-visual-text">
        <details>
          <summary>Text description</summary>
          <p>{visual.text}</p>
        </details>
      </figcaption>
    </figure>
  );
});

export interface MathVisualsProps {
  /** Every figure of one problem, in authored order. The API omits the key for none. */
  visuals: RenderedVisual[] | undefined;
}

/** Every figure of one problem, or nothing when the problem carries none. */
export function MathVisuals({ visuals = [] }: MathVisualsProps) {
  if (visuals.length === 0) return null;
  return (
    <div className="math-visuals">
      {visuals.map((visual, index) => (
        <MathVisual key={`${visual.kind}-${index}`} visual={visual} />
      ))}
    </div>
  );
}
