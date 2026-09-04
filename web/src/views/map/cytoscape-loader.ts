/**
 * The memoized Cytoscape import.
 *
 * NOT `React.lazy`, and the difference is the whole point of this file. `React.lazy` caches
 * a REJECTION for the life of the page: after one chunk 404 every later "Try again" fails
 * instantly, and only a full reload recovers. The memo below CLEARS ITSELF before it
 * rethrows, so the next mount re-enters the import.
 *
 * `React.lazy` for the canvas MODULE is a different case and is correct — `Map.tsx` uses it.
 * That chunk ships with this build, and the error boundary above the view resets by key. It
 * is the 434 KB VENDORED LIBRARY, served from `/vendor/`, that needs the retry.
 *
 * The memo is also what keeps two overlapping mounts to ONE download: the second caller
 * awaits the promise the first one started (F-38-1).
 */

import type { MapElement } from './layout';
import type { MapStyleRule } from './mapStyle';

/**
 * The slice of Cytoscape the map calls, hand-written.
 *
 * The library is a runtime `/vendor/` import with no package entry, so `@types/cytoscape`
 * would be a dependency the build cannot see. Narrow on purpose: anything absent here is
 * something the map does not call, which gives the double in `test/mocks/cytoscape.ts` a
 * closed set to implement.
 */
export interface CyCollection {
  id: () => string;
  empty: () => boolean;
  addClass: (name: string) => CyCollection;
  removeClass: (name: string) => CyCollection;
}

/** A tap anywhere. The target is the instance on the background, or the node under it. */
interface CyTapEvent {
  target: CyLike | CyCollection;
}

/** A tap bound through a `node` selector. The target is always the node. */
interface CyNodeTapEvent {
  target: CyCollection;
}

export type CyTapHandler = (evt: CyTapEvent) => void;
export type CyNodeTapHandler = (evt: CyNodeTapEvent) => void;

export interface CyLike {
  destroy: () => void;
  resize: () => void;
  style: (sheet: MapStyleRule[]) => void;
  /** Read the level, or set it when `level` is given. */
  zoom: (level?: number) => number;
  minZoom: (value: number) => number;
  maxZoom: (value: number) => number;
  fit: () => void;
  center: (target: CyCollection) => void;
  panBy: (delta: { x: number; y: number }) => void;
  /** Bind a handler to every element (`selector` absent) or to the ones it names. */
  on: {
    (event: string, handler: CyTapHandler): void;
    (event: string, selector: 'node', handler: CyNodeTapHandler): void;
  };
  elements: () => CyCollection;
  getElementById: (id: string) => CyCollection;
}

/** What the map hands the factory. Every key is read by the island or by its double. */
export interface CytoscapeOptions {
  container: HTMLElement;
  elements: MapElement[];
  style: MapStyleRule[];
  layout: { name: string; fit: boolean; padding: number };
  boxSelectionEnabled: boolean;
  autoungrabify: boolean;
  autounselectify: boolean;
  pixelRatio: number | 'auto';
  hideEdgesOnViewport: boolean;
  textureOnViewport: boolean;
  motionBlur: boolean;
}

/** The `cytoscape` factory. The library's sole export is `default`. */
export type CytoscapeFactory = (opts: CytoscapeOptions) => CyLike;

/** What the loader awaits. The test seam replaces it; production never does. */
export type CytoscapeImporter = () => Promise<{ default: CytoscapeFactory }>;

/**
 * The runtime URL of the vendored library.
 *
 * ROOT-ABSOLUTE on purpose: `/vendor/**` is SERVED, out of `public/` in dev and out of
 * `dist/` behind Caddy in production, and it is never bundled or hashed.
 * `scripts/check-bundle-csp.mjs` discovers this literal in the sources and fails the build
 * when `dist/vendor/cytoscape/cytoscape.esm.min.mjs` is absent — the 1.0 failure where six
 * views worked, the map was dead, and every gate stayed green.
 */
const CYTOSCAPE_URL = '/vendor/cytoscape/cytoscape.esm.min.mjs';

/**
 * The one production importer.
 *
 * `@vite-ignore` on a CONSTANT specifier, and both halves are needed. Vite refuses a static
 * import of a file inside `public/` outright — "Cannot import non-asset file … which is
 * inside /public" — because such a file is copied verbatim and has no module graph entry.
 * The directive tells Vite to emit the import untouched, which is exactly what a
 * served-not-bundled library needs. The specifier stays a named constant, so the path is
 * still one literal that the CSP gate can find and that this file can state a reason for.
 */
const vendorImport: CytoscapeImporter = () =>
  import(/* @vite-ignore */ CYTOSCAPE_URL) as Promise<{ default: CytoscapeFactory }>;

let importer: CytoscapeImporter = vendorImport;
let pending: Promise<CytoscapeFactory> | null = null;

/** The library, loaded once per page and shared by every caller after that. */
export function loadCytoscape(): Promise<CytoscapeFactory> {
  if (!pending) {
    pending = importer()
      .then((module) => module.default)
      .catch((err: Error) => {
        // A rejected promise is still truthy. Keeping it makes every later retry await the
        // same dead promise, which is the failure `React.lazy` has and this file does not.
        pending = null;
        throw err;
      });
  }
  return pending;
}

/**
 * Test seam: drop the memo, and optionally swap the importer.
 *
 * Call it with no argument to restore the vendored import.
 */
export function resetCytoscapeLoader(next?: CytoscapeImporter): void {
  pending = null;
  importer = next ?? vendorImport;
}
