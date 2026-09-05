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
 * The fixtures and the double live in `test/helpers/map.tsx` and `test/mocks/cytoscape.ts`.
 * This part holds the pure layout, the renderer memo and the instance lifecycle;
 * `map.view.test.tsx` holds the dead renderer, the list view and the payload on screen.
 */
import { StrictMode } from 'react';
import { describe, expect, it } from 'vitest';
import { act } from '@testing-library/react';
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
import { instances } from './mocks/cytoscape';
import {
  EDGES, NODES, changeScope, deferredImport, failOnceImporter, flush, last, mount,
  mountLoaded, node, okImport,
} from './helpers/map';

describe('the map layout — pure', () => {
  it('stacks prerequisite layers bottom to top, one step per layer', () => {
    // 2.0 sends no coordinates at all, so these numbers are the client's own.
    const depth = layerOf(NODES, EDGES);
    // Every topic and nothing else: one entry per node of the payload.
    expect(depth).toEqual(new Map([
      ['whole-numbers', 0], ['fractions', 1], ['decimals', 1], ['ratios', 2],
    ]));

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

  it('walks on past a topic with two prerequisites', () => {
    // `ratios` waits for both of its prerequisites before it is placed, and only then does
    // the topic after it get its row.
    const nodes = [...NODES, node({ id: 'percent' })];
    const depth = layerOf(nodes, [...EDGES, { from: 'ratios', to: 'percent' }]);
    expect(depth.get('percent')).toBe(3);
  });

  it('keeps a cycle fed by a root on the foundation row instead of hanging', () => {
    const nodes = [node({ id: 'root' }), node({ id: 'a' }), node({ id: 'b' })];
    const depth = layerOf(nodes, [
      { from: 'root', to: 'a' },
      { from: 'a', to: 'b' },
      { from: 'b', to: 'a' },
    ]);
    // The root places `a` one row up; the pair itself is never reached, so `b` stays down.
    expect([...depth.entries()].sort()).toEqual([['a', 1], ['b', 0], ['root', 0]]);
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

  it('builds the element list literally: two groups, one class per state, one id per edge', () => {
    const a = node({ id: 'a', name: 'A', status: 'frontier', ability: 0.5 });
    const b = node({ id: 'b', name: 'B', module: 'Ratio' });
    expect(toElements([a, b], [{ from: 'a', to: 'b' }])).toEqual([
      {
        group: 'nodes',
        data: { id: 'a', name: 'A', module: 'Arithmetic', size: sizeOf(0.5) },
        classes: 'st-frontier',
        position: { x: 0, y: 0 },
      },
      {
        group: 'nodes',
        data: { id: 'b', name: 'B', module: 'Ratio', size: sizeOf(0) },
        classes: 'st-untouched',
        position: { x: 0, y: -Y_STEP },
      },
      {
        group: 'edges',
        data: { id: 'a->b', source: 'a', target: 'b' },
        classes: 'prereq',
      },
    ]);
  });

  it('counts every state, the empty ones included', () => {
    expect(countByStatus(NODES)).toEqual({
      frontier: 1, learning: 1, placed: 0, floor: 1, untouched: 1,
    });
  });

  it('names the five states in the order of the legend', () => {
    expect(STATES.map((s) => s.id)).toEqual(['frontier', 'learning', 'placed', 'floor', 'untouched']);
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
    const attempts = failOnceImporter();

    await expect(loadCytoscape()).rejects.toThrow('chunk 404');
    await expect(loadCytoscape()).resolves.toBeTypeOf('function');
    expect(attempts()).toBe(2);
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

    await changeScope('all');

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
    const view = await mountLoaded();
    expect(instances).toHaveLength(1);
    const first = last();

    await changeScope('all');

    expect(instances).toHaveLength(2);
    expect(first.destroyed).toBe(true);
    expect(last().destroyed).toBe(false);
    // The literal that makes the ORDER assertable: nothing was alive when the second one
    // was constructed.
    expect(instances.map((i) => i.aliveAtBuild)).toEqual([0, 0]);
    view.unmount();
  });

  it('destroys the instance when the view unmounts', async () => {
    const view = await mountLoaded();
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
    const view = await mountLoaded();
    const { options } = last();
    // Every Cytoscape layout extension needs eval or a blob worker, and the CSP grants
    // neither: the coordinates come from `layout.ts` and the layout is `preset`.
    expect(options.layout.name).toBe('preset');
    expect(options.elements).toHaveLength(NODES.length + EDGES.length);
    view.unmount();
  });
});
