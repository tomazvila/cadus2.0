/**
 * A deterministic Cytoscape double.
 *
 * `vitest.config.ts` aliases `/vendor/cytoscape/cytoscape.esm.min.mjs` to this file, for two
 * reasons. Vitest cannot resolve a root-absolute runtime URL at all, so without the alias
 * every map test fails to load. And a 434 KB canvas library in jsdom would render nothing
 * and assert nothing: the island drives the instance by method call, so a double with the
 * same closed surface tests the whole island.
 *
 * WHAT IT RECORDS is the point of the file. `instances` is every instance ever built, in
 * order, and each one remembers whether it is destroyed and HOW MANY OTHER INSTANCES WERE
 * STILL ALIVE when it was built. That second number is what makes F-38-1 and
 * destroy-before-build assertable as literals instead of as a story about `key`.
 *
 * What is deliberately NOT modelled: layout, pixels, and hit testing. jsdom reports every
 * box as 0, so anything that depends on rendered geometry is covered as a pure function
 * (`layout.ts`) rather than pretended at here.
 */
import type {
  CyCollection,
  CyLike,
  CyNodeTapHandler,
  CyTapHandler,
  CytoscapeOptions,
} from '@/views/map/cytoscape-loader';
import type { MapStyleRule } from '@/views/map/mapStyle';

interface CyElementStub extends CyCollection {
  classes: Set<string>;
}

/** The instance the island drives, plus what the tests read off it. */
export interface CyStub extends CyLike {
  /** The options the island constructed it with. */
  readonly options: CytoscapeOptions;
  /** The camera bounds the island set. */
  readonly zoomBounds: { min?: number; max?: number };
  destroyed: boolean;
  /** Instances still alive at the moment this one was built. It must always be 0. */
  readonly aliveAtBuild: number;
  /** Every `cy.resize()` call. */
  resizes: number;
  /** Every stylesheet handed over, the constructor's included. */
  readonly styles: MapStyleRule[][];
  pans: Array<{ x: number; y: number }>;
  fits: number;
  centered: string[];
  /** Test affordance: fire a handler bound through `cy.on`. */
  emit: (event: string, target?: CyCollection | CyStub) => void;
  /** Test affordance: the classes one element carries. */
  classesOf: (id: string) => string[];
}

/** Every instance built, in order. `resetCytoscape()` clears it. */
export const instances: CyStub[] = [];

/** Drop the record between tests. */
export function resetCytoscape(): void {
  instances.length = 0;
}

type Handler =
  | { event: string; selector: 'node'; fn: CyNodeTapHandler }
  | { event: string; selector: null; fn: CyTapHandler };

/**
 * One element, or a collection standing in for several.
 *
 * `sets` is every class set the operation applies to: one for `getElementById`, all of them
 * for `elements()`. A double whose `elements().removeClass('pick')` reached nothing would
 * certify a map that rings every topic it has ever selected.
 */
function element(id: string, sets: Array<Set<string>>, present: boolean): CyElementStub {
  const self: CyElementStub = {
    id: () => id,
    empty: () => !present,
    classes: sets[0] ?? new Set<string>(),
    addClass: (name) => { for (const s of sets) s.add(name); return self; },
    removeClass: (name) => { for (const s of sets) s.delete(name); return self; },
  };
  return self;
}

/** The `cytoscape` factory, exactly as the library exports it: one default function. */
export default function cytoscape(options: CytoscapeOptions): CyStub {
  const classes = new Map<string, Set<string>>();
  for (const el of options.elements) {
    const id = String(el.data.id ?? '');
    classes.set(id, new Set((el.classes ?? '').split(' ').filter(Boolean)));
  }

  const handlers: Handler[] = [];
  let zoomLevel = 1;

  const cy: CyStub = {
    options,
    zoomBounds: {},
    destroyed: false,
    aliveAtBuild: instances.filter((i) => !i.destroyed).length,
    resizes: 0,
    styles: [options.style],
    pans: [],
    fits: 0,
    centered: [],

    zoom: (level?: number) => {
      if (typeof level === 'number') zoomLevel = level;
      return zoomLevel;
    },
    minZoom: (v) => { cy.zoomBounds.min = v; return v; },
    maxZoom: (v) => { cy.zoomBounds.max = v; return v; },
    fit: () => { cy.fits += 1; },
    center: (target) => { cy.centered.push(target.id()); },
    panBy: (delta) => { cy.pans.push(delta); },
    resize: () => { cy.resizes += 1; },
    destroy: () => { cy.destroyed = true; },
    style: (sheet) => { cy.styles.push(sheet); },

    on: (event: string, a: 'node' | CyTapHandler, b?: CyNodeTapHandler) => {
      // The one selector the island binds. Any other string selects nothing here.
      if (a === 'node' && b) handlers.push({ event, selector: 'node', fn: b });
      if (typeof a === 'function') handlers.push({ event, selector: null, fn: a });
    },

    elements: () => element('', [...classes.values()], true),
    getElementById: (id) => {
      // The library tolerates a non-string; the double holds the island to the declared type.
      if (typeof id !== 'string') throw new TypeError('getElementById takes a string id');
      const own = classes.get(id);
      return element(id, own ? [own] : [], own !== undefined);
    },

    emit: (event, target) => {
      // A tap on the background carries the instance itself as the target, which is exactly
      // how the island tells the two taps apart. A tap on a NODE fires both the selective
      // and the unselective handler, as the library does — so an island that forgot the
      // `evt.target === cy` guard clears its own selection here, and a test sees it.
      const node = target !== undefined && target !== cy ? (target as CyCollection) : null;
      for (const h of handlers) {
        if (h.event !== event) continue;
        if (h.selector === 'node') { if (node) h.fn({ target: node }); continue; }
        h.fn({ target: target ?? cy });
      }
    },

    classesOf: (id) => [...(classes.get(id) ?? new Set<string>())].sort(),
  };

  instances.push(cy);
  return cy;
}
