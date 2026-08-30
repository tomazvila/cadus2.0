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

export interface CyElementStub {
  id: () => string;
  empty: () => boolean;
  classes: Set<string>;
  addClass: (name: string) => CyElementStub;
  removeClass: (name: string) => CyElementStub;
}

export interface CyStub {
  /** The options the island constructed it with. */
  readonly options: Record<string, unknown>;
  destroyed: boolean;
  /** Instances still alive at the moment this one was built. It must always be 0. */
  readonly aliveAtBuild: number;
  /** Every `cy.resize()` call. */
  resizes: number;
  /** Every stylesheet handed over, the constructor's included. */
  readonly styles: unknown[];
  zoom: (arg?: unknown) => number;
  minZoom: (v: number) => number;
  maxZoom: (v: number) => number;
  fit: () => void;
  center: (target?: unknown) => void;
  panBy: (delta: { x: number; y: number }) => void;
  pans: Array<{ x: number; y: number }>;
  fits: number;
  centered: string[];
  resize: () => void;
  destroy: () => void;
  style: (sheet: unknown) => void;
  on: (event: string, a?: unknown, b?: unknown) => void;
  elements: () => CyElementStub;
  getElementById: (id: string) => CyElementStub;
  /** Test affordance: fire a handler bound through `cy.on`. */
  emit: (event: string, target?: unknown) => void;
  /** Test affordance: the classes one element carries. */
  classesOf: (id: string) => string[];
}

/** Every instance built, in order. `resetCytoscape()` clears it. */
export const instances: CyStub[] = [];

/** Drop the record between tests. */
export function resetCytoscape(): void {
  instances.length = 0;
}

interface Handler {
  event: string;
  selector: string | null;
  fn: (evt: { target: unknown }) => void;
}

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
export default function cytoscape(options: Record<string, unknown>): CyStub {
  const elements = (options.elements ?? []) as Array<{
    group?: string;
    data?: { id?: string };
    classes?: string;
  }>;
  const classes = new Map<string, Set<string>>();
  for (const el of elements) {
    const id = String(el.data?.id ?? '');
    classes.set(id, new Set((el.classes ?? '').split(' ').filter(Boolean)));
  }

  const handlers: Handler[] = [];
  let zoomLevel = 1;

  const cy: CyStub = {
    options,
    destroyed: false,
    aliveAtBuild: instances.filter((i) => !i.destroyed).length,
    resizes: 0,
    styles: [options.style],
    pans: [],
    fits: 0,
    centered: [],

    zoom: (arg?: unknown) => {
      if (typeof arg === 'number') zoomLevel = arg;
      return zoomLevel;
    },
    minZoom: (v) => v,
    maxZoom: (v) => v,
    fit: () => { cy.fits += 1; },
    center: (target?: unknown) => {
      cy.centered.push(typeof target === 'object' && target !== null && 'id' in target
        ? (target as CyElementStub).id()
        : '');
    },
    panBy: (delta) => { cy.pans.push(delta); },
    resize: () => { cy.resizes += 1; },
    destroy: () => { cy.destroyed = true; },
    style: (sheet) => { cy.styles.push(sheet); },

    on: (event, a, b) => {
      const selector = typeof a === 'string' ? a : null;
      const fn = (typeof a === 'function' ? a : b) as Handler['fn'];
      handlers.push({ event, selector, fn });
    },

    elements: () => element('', [...classes.values()], true),
    getElementById: (id) => {
      const own = classes.get(id);
      return element(id, own ? [own] : [], own !== undefined);
    },

    emit: (event, target) => {
      // A tap on the background carries the instance itself as the target, which is exactly
      // how the island tells the two taps apart. A tap on a NODE fires both the selective
      // and the unselective handler, as the library does — so an island that forgot the
      // `evt.target === cy` guard clears its own selection here, and a test sees it.
      const isNode = target !== undefined && target !== cy;
      for (const h of handlers) {
        if (h.event !== event) continue;
        if (h.selector === 'node' && !isNode) continue;
        h.fn({ target: target ?? cy });
      }
    },

    classesOf: (id) => [...(classes.get(id) ?? new Set<string>())].sort(),
  };

  instances.push(cy);
  return cy;
}
