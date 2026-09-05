/**
 * The Cytoscape stylesheet.
 *
 * A Cytoscape stylesheet holds LITERAL colors: the canvas is a `<canvas>`, so `var(--accent)`
 * means nothing to it. The literals are still read out of `tokens.css` at runtime rather
 * than written here, so the palette keeps one home (TOKENS-hex), and the sheet is rebuilt
 * when `prefers-color-scheme` flips.
 */
import { STATES } from './layout';

/** Every color the canvas paints with, read off `:root`. */
export interface MapTokens {
  text: string;
  muted: string;
  /** The fill of each `.st-*` class, keyed by `TopicStatus`. */
  state: Record<string, string>;
}

/**
 * The literal token values off the document element.
 *
 * In jsdom every one of these is '', because jsdom resolves no custom property. Cytoscape
 * ignores an empty color, so the map still builds; a test asserts the stylesheet SHAPE, and
 * the S4 contract test asserts the palette itself against the stylesheet text.
 */
export function readTokens(
  style: Pick<CSSStyleDeclaration, 'getPropertyValue'> = getComputedStyle(document.documentElement),
): MapTokens {
  const value = (name: string) => style.getPropertyValue(name).trim();
  return {
    text: value('--text'),
    muted: value('--muted'),
    state: Object.fromEntries(STATES.map((s) => [s.id, value(s.token)])),
  };
}

/** One rule of a Cytoscape stylesheet: a selector and the properties it paints. */
export interface MapStyleRule {
  selector: string;
  style: Record<string, string | number>;
}

/** The stylesheet, rebuilt whenever the color scheme flips. */
export function buildStyle(tokens: MapTokens): MapStyleRule[] {
  const style: MapStyleRule[] = [
    {
      selector: 'node',
      style: {
        width: 'data(size)',
        height: 'data(size)',
        label: 'data(name)',
        color: tokens.text,
        'font-size': 9,
        // Labels are culled below this rendered size, so a 1,000-node view stays readable.
        'min-zoomed-font-size': 11,
        'text-valign': 'bottom',
        'text-halign': 'center',
        'text-margin-y': 3,
        'text-wrap': 'wrap',
        'text-max-width': 110,
        'text-opacity': 0.85,
        'border-width': 0,
        'overlay-opacity': 0,
      },
    },
    {
      selector: 'edge',
      style: {
        'curve-style': 'straight',
        width: 1,
        opacity: 0.25,
        'line-color': tokens.muted,
        'target-arrow-color': tokens.muted,
        'target-arrow-shape': 'triangle',
        'arrow-scale': 0.6,
      },
    },
  ];

  for (const state of STATES) {
    style.push({
      selector: `.st-${state.id}`,
      style: { 'background-color': tokens.state[state.id] },
    });
  }

  style.push(
    // What the placement assumed, and what nothing has reached yet, both recede.
    { selector: '.st-floor', style: { 'background-opacity': 0.45 } },
    { selector: '.st-untouched', style: { 'background-opacity': 0.5 } },
    // What to study next is the one thing the map rings.
    {
      selector: '.st-frontier',
      style: { 'border-width': 2, 'border-color': tokens.state.frontier },
    },
    // The selected topic, whose panel is open beside the canvas.
    { selector: 'node.pick', style: { 'border-width': 3, 'border-color': tokens.text, 'text-opacity': 1, 'min-zoomed-font-size': 0 } },
  );
  return style;
}
