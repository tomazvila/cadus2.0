/**
 * The curriculum map (S11).
 *
 * The map is the only screen with a THIRD-PARTY IMPERATIVE LIBRARY in it, so what is under
 * test is the split: `<Map>` owns the payload, the legend, the detail panel, and the list
 * view; `<CyCanvas>` owns the one Cytoscape instance. One invariant lives here:
 *
 *   F-38-1  An overlapping load builds EXACTLY ONE instance, and the outgoing one is
 *           destroyed before the replacement is built. Two live instances over one payload
 *           means two canvases, two handler sets, and a camera the controls cannot reach.
 *
 * Every instance assertion reads `test/mocks/cytoscape.ts`, which records `aliveAtBuild` —
 * how many instances were still undestroyed when this one was constructed. It must always
 * be 0, and that number is what makes destroy-before-build a literal rather than a story
 * about React keys.
 *
 * The library load itself is driven through `resetCytoscapeLoader(importer)`: the memo is a
 * module singleton, and the failure paths need an import that fails on demand.
 */
import { StrictMode } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axe } from 'vitest-axe';
import {
  CurriculumMap,
  MAP_CANVAS_LABEL,
  MAP_EMPTY,
  MAP_PAN_STEP,
} from '@/views/map/Map';
import { MAP_RENDERER_FAILED } from '@/views/map/CyCanvas';
import {
  STATES,
  X_STEP,
  Y_STEP,
  countByStatus,
  layerOf,
  positionsOf,
  sizeOf,
  toElements,
} from '@/views/map/layout';
import { loadCytoscape, resetCytoscapeLoader } from '@/views/map/cytoscape-loader';
import { createDemoApi } from '@/api';
import { resetToasts, toastStore } from '@/app/toast';
import { AXE_IN_JSDOM } from './axe';
import cytoscape, { instances, resetCytoscape } from './mocks/cytoscape';
import type { ApiClient, GraphEdge, GraphNode, GraphResponse } from '@/api/types';

// ---------------------------------------------------------------------------
// Fixtures — one payload with four states, two modules, and three layers.
// ---------------------------------------------------------------------------

const node = (over: Partial<GraphNode> & Pick<GraphNode, 'id'>): GraphNode => ({
  name: over.id,
  module: 'Arithmetic',
  course: 'foundations',
  status: 'untouched',
  ability: 0,
  ...over,
});

const NODES: GraphNode[] = [
  node({ id: 'whole-numbers', name: 'Whole numbers', status: 'floor', ability: 0.9 }),
  node({ id: 'fractions', name: 'Fractions', status: 'frontier', ability: 0.1 }),
  node({ id: 'decimals', name: 'Decimals', status: 'learning', ability: 0.5 }),
  node({ id: 'ratios', name: 'Ratios', module: 'Ratio', status: 'untouched' }),
];

const EDGES: GraphEdge[] = [
  { from: 'whole-numbers', to: 'fractions' },
  { from: 'whole-numbers', to: 'decimals' },
  { from: 'fractions', to: 'ratios' },
  { from: 'decimals', to: 'ratios' },
];

function graph(over: Partial<GraphResponse> = {}): GraphResponse {
  return {
    now: '2026-08-30T09:00:00+00:00',
    scope: null,
    courses: [
      { id: 'foundations', name: 'Foundations', current: true },
      { id: 'proofs', name: 'Proofs', current: false },
    ],
    modules: ['Arithmetic', 'Ratio'],
    counts: { nodes: NODES.length, edges: EDGES.length, mastered: 1 },
    nodes: NODES,
    edges: EDGES,
    ...over,
  };
}

function stubApi(over: Partial<ApiClient> = {}): ApiClient {
  return { ...createDemoApi(), demo: false, getGraph: async () => graph(), ...over };
}

/** The importer that hands over the double, as the vendored module would. */
const okImport = () => Promise.resolve({ default: cytoscape });

/** An importer whose promise the test releases by hand. */
function deferredImport() {
  let release!: () => void;
  const gate = new Promise<void>((r) => { release = r; });
  const calls = { n: 0 };
  return {
    importer: () => { calls.n += 1; return gate.then(okImport); },
    release,
    calls,
  };
}

async function mount(
  over: Partial<Parameters<typeof CurriculumMap>[0]> = {},
  wrapper?: (p: { children: React.ReactNode }) => React.ReactNode,
) {
  resetToasts();
  resetCytoscape();
  const handlers = { onUnauthorized: vi.fn(), onExit: vi.fn() };
  const props = { api: stubApi(), ...handlers, ...over };
  const tree = <CurriculumMap {...props} />;
  let view!: ReturnType<typeof render>;
  await act(async () => {
    view = render(wrapper ? wrapper({ children: tree }) : tree, {
      container: document.getElementById('view')!,
    });
  });
  // The lazy canvas module and the library memo are two awaits, not one.
  await act(async () => { await Promise.resolve(); });
  return { ...view, ...handlers };
}

const flush = async () => { await act(async () => { await Promise.resolve(); }); };
const last = () => instances[instances.length - 1];
const canvas = () => document.querySelector<HTMLElement>('.map-canvas')!;
const listButton = () => screen.getByRole('button', { name: /List view|Map view/ });
const label = (el: Element | null) =>
  el?.getAttribute('aria-label') ?? el?.textContent?.trim() ?? '';

// ---------------------------------------------------------------------------

describe('the map layout — pure', () => {
  it('stacks prerequisite layers bottom to top, one step per layer', () => {
    // 2.0 sends no coordinates at all, so these numbers are the client's own.
    const depth = layerOf(NODES, EDGES);
    expect(depth.get('whole-numbers')).toBe(0);
    expect(depth.get('fractions')).toBe(1);
    expect(depth.get('decimals')).toBe(1);
    expect(depth.get('ratios')).toBe(2);

    const at = positionsOf(NODES, EDGES);
    // y is negated, so the foundation row sits at the BOTTOM of the drawing.
    expect(at.get('whole-numbers')).toEqual({ x: 0, y: 0 });
    expect(at.get('ratios')).toEqual({ x: 0, y: -2 * Y_STEP });
    // A layer of two is centered on x = 0, ordered by module then id.
    expect(at.get('decimals')).toEqual({ x: -X_STEP / 2, y: -Y_STEP });
    expect(at.get('fractions')).toEqual({ x: X_STEP / 2, y: -Y_STEP });
  });

  it('takes the LONGEST path to a topic, never the first one found', () => {
    // b is reachable at depth 1 through the direct edge and at depth 2 through a. A
    // shortest-path layering would draw the arrow a → b pointing DOWN the map.
    const nodes = [node({ id: 'a' }), node({ id: 'b' }), node({ id: 'root' })];
    const depth = layerOf(nodes, [
      { from: 'root', to: 'a' },
      { from: 'root', to: 'b' },
      { from: 'a', to: 'b' },
    ]);
    expect(depth.get('b')).toBe(2);
  });

  it('keeps a cyclic pair on the foundation row instead of hanging', () => {
    // The curriculum is a validated DAG, so this cannot arrive through the API. A defect
    // upstream must not hang the map or drop the topic off it.
    const nodes = [node({ id: 'a' }), node({ id: 'b' })];
    const depth = layerOf(nodes, [{ from: 'a', to: 'b' }, { from: 'b', to: 'a' }]);
    expect([...depth.entries()].sort()).toEqual([['a', 0], ['b', 0]]);
  });

  it('sizes a node from ability and clamps every value outside [0, 1]', () => {
    expect(sizeOf(0)).toBe(12);
    expect(sizeOf(0.5)).toBe(21);
    expect(sizeOf(1)).toBe(30);
    expect(sizeOf(-3)).toBe(12);
    expect(sizeOf(4)).toBe(30);
    expect(sizeOf(Number.NaN)).toBe(12);
  });

  it('rides the state as a class and the prerequisite as source → target', () => {
    const elements = toElements(NODES, EDGES);
    const first = elements[0];
    expect(first.classes).toBe('st-floor');
    expect(first.data.state).toBeUndefined();
    expect(first.data.size).toBe(sizeOf(0.9));

    const edge = elements.find((e) => e.group === 'edges')!;
    // `from` is the prerequisite, so the arrow reads "unlocks".
    expect(edge.data.source).toBe('whole-numbers');
    expect(edge.data.target).toBe('fractions');
    expect(edge.position).toBeUndefined();
  });

  it('counts every state, the empty ones included', () => {
    expect(countByStatus(NODES)).toEqual({
      frontier: 1, learning: 1, placed: 0, floor: 1, untouched: 1,
    });
  });
});

describe('the renderer memo', () => {
  it('F-38-1: two overlapping callers share ONE library download', async () => {
    const { importer, release, calls } = deferredImport();
    resetCytoscapeLoader(importer);

    const a = loadCytoscape();
    const b = loadCytoscape();
    release();
    expect(await a).toBe(await b);
    expect(calls.n).toBe(1);
  });

  it('a failed library load retries on the next mount', async () => {
    // The whole reason this is not `React.lazy`: lazy caches the REJECTION for the life of
    // the page, so every later Try again would fail instantly against a dead promise.
    let n = 0;
    resetCytoscapeLoader(() => {
      n += 1;
      return n === 1 ? Promise.reject(new Error('chunk 404')) : okImport();
    });

    await expect(loadCytoscape()).rejects.toThrow('chunk 404');
    await expect(loadCytoscape()).resolves.toBeTypeOf('function');
    expect(n).toBe(2);
  });
});

describe('the map view — the instance lifecycle', () => {
  it('F-38-1: an overlapping load builds exactly one Cytoscape instance', async () => {
    // The race: the library is still in flight when the learner changes the scope. Both
    // islands are inside `loadCytoscape()` when it resolves, and the outgoing one must
    // build nothing.
    const { importer, release } = deferredImport();
    resetCytoscapeLoader(importer);
    const view = await mount();
    expect(instances).toHaveLength(0);          // still waiting on the library

    const scope = screen.getByLabelText('Scope') as HTMLSelectElement;
    await act(async () => {
      scope.value = 'all';
      scope.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await flush();

    await act(async () => { release(); });
    await flush();

    expect(instances).toHaveLength(1);
    expect(last().destroyed).toBe(false);
    expect(last().aliveAtBuild).toBe(0);
    view.unmount();
  });

  it('F-38-1: a StrictMode remount leaves one live instance, never two', async () => {
    // The case the `cancelled` flag exists for, and the ONLY one that needs it. React 19
    // development mounts, tears down and remounts every component, and `useLifetime.revive`
    // makes the second pass a real mount again — so `life.alive()` is true again by the
    // time the FIRST pass's library promise resolves. Without the flag that continuation
    // builds an instance whose cleanup has already run: it is alive, unreachable, and
    // never destroyed. `main.tsx` mounts the app inside `<StrictMode>`.
    resetCytoscapeLoader(okImport);
    const view = await mount({}, ({ children }) => <StrictMode>{children}</StrictMode>);

    expect(instances.map((i) => i.aliveAtBuild)).toEqual(instances.map(() => 0));
    expect(instances.filter((i) => !i.destroyed)).toHaveLength(1);
    view.unmount();
    expect(instances.filter((i) => !i.destroyed)).toHaveLength(0);
  });

  it('destroys the outgoing instance BEFORE it builds the replacement', async () => {
    resetCytoscapeLoader(okImport);
    const view = await mount();
    expect(instances).toHaveLength(1);
    const first = last();

    const scope = screen.getByLabelText('Scope') as HTMLSelectElement;
    await act(async () => {
      scope.value = 'all';
      scope.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await flush();

    expect(instances).toHaveLength(2);
    expect(first.destroyed).toBe(true);
    expect(last().destroyed).toBe(false);
    // The literal that makes the ORDER assertable: nothing was alive when the second one
    // was constructed.
    expect(instances.map((i) => i.aliveAtBuild)).toEqual([0, 0]);
    view.unmount();
  });

  it('destroys the instance when the view unmounts', async () => {
    resetCytoscapeLoader(okImport);
    const view = await mount();
    const cy = last();
    expect(cy.destroyed).toBe(false);
    view.unmount();
    expect(cy.destroyed).toBe(true);
  });

  it('builds nothing for a view that left before the library landed', async () => {
    const { importer, release } = deferredImport();
    resetCytoscapeLoader(importer);
    const view = await mount();
    view.unmount();
    await act(async () => { release(); });
    await flush();
    expect(instances).toHaveLength(0);
  });

  it('builds the instance with preset positions and no layout extension', async () => {
    resetCytoscapeLoader(okImport);
    const view = await mount();
    const { options } = last();
    // Every Cytoscape layout extension needs eval or a blob worker, and the CSP grants
    // neither: the coordinates come from `layout.ts` and the layout is `preset`.
    expect(options.layout.name).toBe('preset');
    expect(options.elements).toHaveLength(NODES.length + EDGES.length);
    view.unmount();
  });
});

describe('the map view — a dead renderer', () => {
  it('reports a dead renderer, and the next mount retries the load', async () => {
    let n = 0;
    resetCytoscapeLoader(() => {
      n += 1;
      return n === 1 ? Promise.reject(new Error('chunk 404')) : okImport();
    });
    const view = await mount();

    expect(screen.getByText(MAP_RENDERER_FAILED)).toBeTruthy();
    expect(instances).toHaveLength(0);
    // An actionable toast never auto-dismisses (F-36-1b), so the recovery survives a
    // learner who looked away.
    await waitFor(() => expect(toastStore.getSnapshot()).toHaveLength(1));
    expect(toastStore.getSnapshot()[0].message).toBe(MAP_RENDERER_FAILED);
    expect(toastStore.getSnapshot()[0].onAction).toBeTypeOf('function');

    await act(async () => { screen.getByRole('button', { name: 'Try again' }).click(); });
    await flush();
    expect(instances).toHaveLength(1);
    expect(screen.queryByText(MAP_RENDERER_FAILED)).toBeNull();
    view.unmount();
  });

  it('does not let a dead renderer on one scope poison the next', async () => {
    let n = 0;
    resetCytoscapeLoader(() => {
      n += 1;
      return n === 1 ? Promise.reject(new Error('chunk 404')) : okImport();
    });
    const view = await mount();
    expect(screen.getByText(MAP_RENDERER_FAILED)).toBeTruthy();

    const scope = screen.getByLabelText('Scope') as HTMLSelectElement;
    await act(async () => {
      scope.value = 'all';
      scope.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await flush();

    expect(screen.queryByText(MAP_RENDERER_FAILED)).toBeNull();
    expect(instances).toHaveLength(1);
    view.unmount();
  });
});

describe('the map view — the accessible list view', () => {
  it('reaches the list view with the keyboard alone', async () => {
    resetCytoscapeLoader(okImport);
    const user = userEvent.setup();
    const view = await mount();

    // Walk the tab order from the top of the document, exactly as a keyboard reader does.
    const order: string[] = [];
    for (let i = 0; i < 5; i += 1) {
      await user.tab();
      order.push(label(document.activeElement));
    }
    expect(order).toEqual(['Scope', 'Fit', 'List view', 'Done', MAP_CANVAS_LABEL]);

    // Back to the control, and open it with the keyboard.
    await user.tab({ shift: true });
    await user.tab({ shift: true });
    expect(label(document.activeElement)).toBe('List view');
    await user.keyboard('{Enter}');

    expect(document.querySelector('.map-list')).not.toBeNull();
    expect(screen.getByText('Fractions')).toBeTruthy();
    // And the canvas leaves the tab order, so the reader is not walking a dead control.
    expect(canvas().hidden).toBe(true);
    view.unmount();
  });

  it('lists every topic of the payload, grouped by state', async () => {
    resetCytoscapeLoader(okImport);
    const view = await mount();
    await act(async () => { listButton().click(); });

    const groups = document.querySelectorAll('.map-list-group');
    // Four states hold a topic; `placed` holds none and prints no group.
    expect(groups).toHaveLength(4);
    expect(groups[0].querySelector('summary')!.textContent).toBe('Ready to learn1');
    expect(document.querySelectorAll('.map-list-group li')).toHaveLength(NODES.length);
    expect(document.querySelector('.map-list-group li')!.textContent)
      .toBe('Fractions — Arithmetic · ability 10%');
    view.unmount();
  });

  it('sends a screen reader from the canvas to the list view', async () => {
    resetCytoscapeLoader(okImport);
    const view = await mount();
    // A canvas is not screen-readable and no ARIA attribute makes it so. The label is the
    // pointer to the equivalent, and the tab stop is what makes the pan keys reachable.
    expect(canvas().getAttribute('role')).toBe('img');
    expect(canvas().getAttribute('aria-label')).toBe(MAP_CANVAS_LABEL);
    expect(canvas().tabIndex).toBe(0);
    view.unmount();
  });

  it('reports no axe violation in either mode', async () => {
    resetCytoscapeLoader(okImport);
    const view = await mount();
    expect(await axe(document.getElementById('view')!, AXE_IN_JSDOM)).toHaveNoViolations();
    await act(async () => { listButton().click(); });
    expect(await axe(document.getElementById('view')!, AXE_IN_JSDOM)).toHaveNoViolations();
    view.unmount();
  });
});

describe('the map view — the payload on screen', () => {
  it('reads out the counts the service sent, and derives none of them', async () => {
    resetCytoscapeLoader(okImport);
    const view = await mount();
    expect(document.querySelector('.map-readout')!.textContent)
      .toBe('4 topics · 4 links · 1 mastered');
    const chips = document.querySelectorAll('.map-legend .legend-chip');
    expect(chips).toHaveLength(STATES.length);
    expect(chips[0].textContent).toBe('Ready to learn1');
    view.unmount();
  });

  it('opens the detail panel on a node tap and closes it on the background', async () => {
    resetCytoscapeLoader(okImport);
    const view = await mount();
    const cy = last();

    await act(async () => { cy.emit('tap', cy.getElementById('fractions')); });
    expect(document.querySelector('.map-panel h2')!.textContent).toBe('Fractions');
    expect(document.querySelector('.map-panel .mono')!.textContent).toBe('fractions');
    // The ring is a class on the instance, not a React render.
    expect(cy.classesOf('fractions')).toEqual(['pick', 'st-frontier']);

    await act(async () => { cy.emit('tap', cy); });
    expect(document.querySelector('.map-panel')).toBeNull();
    expect(cy.classesOf('fractions')).toEqual(['st-frontier']);
    view.unmount();
  });

  it('pans on the arrow keys and fits on 0', async () => {
    resetCytoscapeLoader(okImport);
    const user = userEvent.setup();
    const view = await mount();
    canvas().focus();

    await user.keyboard('{ArrowLeft}{ArrowDown}0');
    expect(last().pans).toEqual([{ x: MAP_PAN_STEP, y: 0 }, { x: 0, y: -MAP_PAN_STEP }]);
    expect(last().fits).toBe(1);
    view.unmount();
  });

  it('says so when the scope holds no topic, instead of an empty canvas', async () => {
    resetCytoscapeLoader(okImport);
    const empty = graph({ nodes: [], edges: [], counts: { nodes: 0, edges: 0, mastered: 0 } });
    const view = await mount({ api: stubApi({ getGraph: async () => empty }) });
    expect(screen.getByText(MAP_EMPTY)).toBeTruthy();
    expect(instances).toHaveLength(0);
    view.unmount();
  });

  it('never paints a slower scope reply over the newer one', async () => {
    resetCytoscapeLoader(okImport);
    let release!: (g: GraphResponse) => void;
    const slow = new Promise<GraphResponse>((r) => { release = r; });
    const getGraph = vi.fn<ApiClient['getGraph']>()
      .mockImplementationOnce(async () => graph())
      .mockImplementationOnce(() => slow)
      .mockImplementation(async () => graph({ scope: 'all', counts: { nodes: 1, edges: 0, mastered: 0 } }));

    const view = await mount({ api: stubApi({ getGraph }) });
    const scope = screen.getByLabelText('Scope') as HTMLSelectElement;

    // Pick Proofs, then the whole curriculum before Proofs answers.
    await act(async () => {
      scope.value = 'proofs';
      scope.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await act(async () => {
      scope.value = 'all';
      scope.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await waitFor(() => expect(document.querySelector('.map-readout')).not.toBeNull());
    expect(document.querySelector('.map-readout')!.textContent).toBe('1 topics · 0 links · 0 mastered');

    // The late reply for Proofs lands last, and must change nothing.
    await act(async () => { release(graph({ scope: 'proofs' })); });
    await flush();
    expect(document.querySelector('.map-readout')!.textContent).toBe('1 topics · 0 links · 0 mastered');
    view.unmount();
  });
});
