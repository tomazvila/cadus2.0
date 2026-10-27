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
 *
 * A figure that carries a `motion` table also gets a slider (decision row D-FA1). The table
 * is read AFTER the sanitizer has rebuilt the SVG, and a table that does not fit the SVG
 * gives a still figure with no slider.
 */
import { memo, useCallback, useMemo, useRef } from 'react';
import { ParameterSlider } from '@/components/ParameterSlider';
import { readMotion, type Motion } from '@/lib/motion';
import type { RenderedVisual } from '@/lib/visual';
import { sanitizeVisualSvg } from '@/lib/visual';
import '../styles/fix-figure.css';

export interface MathVisualProps {
  /** The rendered figure the API sent. */
  visual: RenderedVisual;
}

export const MathVisual = memo(function MathVisual({ visual }: MathVisualProps) {
  const holder = useRef<HTMLDivElement | null>(null);
  const figure = useMemo(() => sanitizeVisualSvg(visual.svg), [visual.svg]);
  const motion = useMemo<Motion | null>(
    () => (figure !== null && visual.motion !== undefined ? readMotion(visual.motion, figure) : null),
    [figure, visual.motion],
  );
  // The ref callback runs before the layout effect of the slider below it, so the SVG is in
  // place when the tween engine looks for its groups.
  const attach = useCallback(
    (node: HTMLDivElement | null) => {
      holder.current = node;
      if (node === null) return;
      node.replaceChildren();
      if (figure !== null) node.appendChild(figure);
    },
    [figure],
  );

  return (
    <figure className={`math-visual math-visual-${visual.kind}`} data-kind={visual.kind}>
      <div className="math-visual-frame" ref={attach} />
      {motion !== null && <ParameterSlider key={visual.svg} motion={motion} frame={holder} />}
      <figcaption className="math-visual-text">
        <details>
          <summary>Text description</summary>
          <p className="math-visual-summary">{visual.text}</p>
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
