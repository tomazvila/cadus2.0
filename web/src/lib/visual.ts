/**
 * The client half of the mathematical visuals (unit f9).
 *
 * The SERVER draws the picture. `crates/core/src/visual/` validates the figure and renders
 * deterministic SVG with a `<title>` and a `<desc>`, so the browser never repeats the
 * geometry and the two halves never disagree.
 *
 * The browser still treats those bytes as untrusted. The figure carries author text — a
 * caption, a point label — and a mistake in the escape upstream must not become script in
 * this document. `sanitizeVisualSvg` therefore REBUILDS the picture from an allowlist of
 * elements and attributes. It never assigns `innerHTML`, and it fails closed: one unknown
 * element or one unknown attribute drops the whole figure, and the learner reads the text
 * equivalent instead.
 */

/** One rendered figure, as the API sends it. */
export interface RenderedVisual {
  /** The family: `number_line`, `fraction`, `coordinate`, or `geometry`. */
  kind: string;
  /** The SVG bytes the server drew. */
  svg: string;
  /** The accessible equivalent: the facts of the figure in words. */
  text: string;
}

/** The SVG elements a figure is allowed to hold. */
const ELEMENTS = new Set([
  'svg',
  'g',
  'title',
  'desc',
  'line',
  'circle',
  'rect',
  'path',
  'polygon',
  'polyline',
  'text',
  'tspan',
]);

/** The attributes a figure is allowed to carry. */
const ATTRIBUTES = new Set([
  'class',
  'id',
  'role',
  'aria-labelledby',
  'viewBox',
  'xmlns',
  'x',
  'y',
  'x1',
  'y1',
  'x2',
  'y2',
  'cx',
  'cy',
  'r',
  'd',
  'points',
  'width',
  'height',
  'text-anchor',
]);

const SVG_NS = 'http://www.w3.org/2000/svg';

/**
 * Rebuild one element and its children from the allowlist.
 *
 * The return is `null` when the element, one of its attributes, or one of its descendants
 * sits outside the allowlist.
 */
function rebuild(source: Element, doc: Document): SVGElement | null {
  const name = source.tagName.toLowerCase();
  if (!ELEMENTS.has(name) || source.namespaceURI !== SVG_NS) return null;

  const copy = doc.createElementNS(SVG_NS, name);
  for (const attribute of Array.from(source.attributes)) {
    if (!ATTRIBUTES.has(attribute.name)) return null;
    copy.setAttribute(attribute.name, attribute.value);
  }
  for (const child of Array.from(source.childNodes)) {
    if (child.nodeType === Node.TEXT_NODE) {
      copy.appendChild(doc.createTextNode(child.nodeValue ?? ''));
      continue;
    }
    if (child.nodeType !== Node.ELEMENT_NODE) return null;
    const built = rebuild(child as Element, doc);
    if (built === null) return null;
    copy.appendChild(built);
  }
  return copy;
}

/**
 * Parse server SVG and give back a rebuilt node this document owns.
 *
 * The answer is `null` for markup that does not parse, for markup whose root is not an
 * `<svg>`, and for markup that names anything outside the allowlist.
 */
export function sanitizeVisualSvg(markup: string): SVGElement | null {
  let parsed: Document;
  try {
    parsed = new DOMParser().parseFromString(markup, 'image/svg+xml');
  } catch {
    return null;
  }
  if (parsed.getElementsByTagName('parsererror').length > 0) return null;
  const root = parsed.documentElement;
  if (root === null || root.tagName.toLowerCase() !== 'svg') return null;
  return rebuild(root, document);
}
