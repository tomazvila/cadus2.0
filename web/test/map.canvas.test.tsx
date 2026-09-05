/**
 * The curriculum map (S11), part 3: the island under load, a renderer that fails late, the
 * keys, the color scheme, and the payload shapes.
 *
 * `map.test.tsx` carries the module note and the fixtures live in `test/helpers/map.tsx`.
 */
import { describe, expect, it } from 'vitest';
import { act, cleanup, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { loadCytoscape, resetCytoscapeLoader } from '@/views/map/cytoscape-loader';
import { layerOf, toElements } from '@/views/map/layout';
import { buildStyle, readTokens } from '@/views/map/mapStyle';
import { MAP_RENDERER_FAILED } from '@/views/map/CyCanvas';
import { fireToastAction, toastStore } from '@/app/toast';
import { mediaListenerCount, resizeObservers } from './setup';
import cytoscape, { instances } from './mocks/cytoscape';
import {
  EDGES, NODES, canvas, deferredImport, failOnceImporter, flush, graph, last, listButton,
  mount, mountLoaded, node, stubApi,
} from './helpers/map';
import type { GraphNode } from '@/api/types';

const SCHEME = '(prefers-color-scheme: light)';

describe('the island under load', () => {
  it('trades pixels for speed past four hundred topics', async () => {
    const many: GraphNode[] = Array.from({ length: 401 }, (_, i) => node({ id: `t${i}` }));
    const view = await mountLoaded({
      api: stubApi({ getGraph: async () => graph({ nodes: many, edges: [] }) }),
    });
    expect(last().options.pixelRatio).toBe(1);
    expect(last().options.hideEdgesOnViewport).toBe(true);
    expect(last().options.textureOnViewport).toBe(true);
    view.unmount();
  });

  it('keeps the default pixel ratio for a small scope, four hundred topics included', async () => {
    const view = await mountLoaded();
    expect(last().options.pixelRatio).toBe('auto');
    expect(last().options.hideEdgesOnViewport).toBe(false);
    view.unmount();
    cleanup();

    const four = Array.from({ length: 400 }, (_, i) => node({ id: `t${i}` }));
    const edge = await mountLoaded({
      api: stubApi({ getGraph: async () => graph({ nodes: four, edges: [] }) }),
    });
    expect(last().options.pixelRatio).toBe('auto');
    edge.unmount();
  });

  it('builds the instance with the options the CSP and the camera need, literally', async () => {
    const view = await mountLoaded();
    const cy = last();
    expect(cy.options).toMatchObject({
      layout: { name: 'preset', fit: true, padding: 30 },
      boxSelectionEnabled: false,
      autoungrabify: true,
      autounselectify: true,
      motionBlur: false,
    });
    expect(cy.options.container).toBe(document.querySelector('.map-host'));
    // A little further out than the fitted view of zoom 1, close enough in to read a label.
    expect(cy.zoomBounds).toEqual({ min: 0.6, max: 2.5 });
    view.unmount();
  });

  it('shows the renderer loading until the instance exists, and no longer', async () => {
    const { importer, release } = deferredImport();
    resetCytoscapeLoader(importer);
    const view = await mount();
    expect(screen.getByText('Loading the map renderer…')).toBeTruthy();
    await act(async () => { release(); });
    await flush();
    expect(screen.queryByText('Loading the map renderer…')).toBeNull();
    expect(screen.queryByText(MAP_RENDERER_FAILED)).toBeNull();
    view.unmount();
  });
});

describe('a renderer that fails late', () => {
  it('reports nothing for a library that fails after the view left', async () => {
    const { importer, calls } = deferredImport();
    let fail!: () => void;
    resetCytoscapeLoader(() => {
      const p = importer();
      return new Promise((resolve, reject) => { fail = () => reject(new Error('chunk 404')); p.then(resolve, reject); });
    });
    const view = await mount();
    view.unmount();
    await act(async () => { fail(); });
    await flush();
    expect(calls.n).toBe(1);
    expect(toastStore.getSnapshot()).toEqual([]);
    expect(screen.queryByText(MAP_RENDERER_FAILED)).toBeNull();
  });

  it('retries the load from the Retry of the toast', async () => {
    failOnceImporter();
    const view = await mount();
    expect(instances).toHaveLength(0);
    expect(screen.queryByText('Loading the map renderer…')).toBeNull();
    expect(MAP_RENDERER_FAILED).toBe('Could not load the map renderer.');
    expect(toastStore.getSnapshot()).toEqual([
      { id: 1, message: MAP_RENDERER_FAILED, kind: 'error', onAction: expect.any(Function) },
    ]);
    await act(async () => { fireToastAction(toastStore.getSnapshot()[0].id); });
    await flush();
    expect(instances).toHaveLength(1);
    expect(toastStore.getSnapshot()).toEqual([]);
    view.unmount();
  });

  it('survives an instance that throws on teardown', async () => {
    const view = await mountLoaded();
    const cy = last();
    cy.destroy = () => { throw new Error('half-built'); };
    expect(() => view.unmount()).not.toThrow();
  });
});

describe('the keys and the scheme', () => {
  it('pans right and up, zooms on + = and -, and does nothing before the instance exists', async () => {
    const { importer } = deferredImport();
    resetCytoscapeLoader(importer);
    const user = userEvent.setup();
    const view = await mount();
    canvas().focus();
    // No instance yet: the keys reach nothing and throw nothing.
    await user.keyboard('{ArrowLeft}');
    expect(instances).toHaveLength(0);
    view.unmount();
    cleanup();

    const loaded = await mountLoaded();
    canvas().focus();
    await user.keyboard('{ArrowRight}{ArrowUp}+=-');
    expect(last().pans).toEqual([{ x: -60, y: 0 }, { x: 0, y: 60 }]);
    // 1 × 1.25 × 1.25 × 0.8, on the instance's own zoom.
    expect(last().zoom()).toBeCloseTo(1.25);
    loaded.unmount();
  });

  it('leaves an unbound key to the browser', async () => {
    const user = userEvent.setup();
    const view = await mountLoaded();
    canvas().focus();
    await user.keyboard('x');
    expect(last().pans).toEqual([]);
    expect(last().fits).toBe(0);
    view.unmount();
  });

  it('takes a color-scheme flip before the instance exists', async () => {
    const { importer } = deferredImport();
    resetCytoscapeLoader(importer);
    const view = await mount();
    expect(mediaListenerCount(SCHEME)).toBe(1);
    expect(() => {
      act(() => { window.matchMedia(SCHEME).dispatchEvent(new Event('change')); });
    }).not.toThrow();
    view.unmount();
  });

  it('rebuilds the stylesheet when the color scheme flips, and lets go on unmount', async () => {
    const view = await mountLoaded();
    expect(last().styles).toHaveLength(1);
    expect(mediaListenerCount(SCHEME)).toBe(1);

    act(() => { window.matchMedia(SCHEME).dispatchEvent(new Event('change')); });
    expect(last().styles).toHaveLength(2);

    view.unmount();
    expect(mediaListenerCount(SCHEME)).toBe(0);
  });
});

describe('the resize and the fit', () => {
  it('resizes the canvas when its box changes, and not while the list view hides it', async () => {
    const { importer, release } = deferredImport();
    resetCytoscapeLoader(importer);
    const view = await mount();
    const observer = resizeObservers[resizeObservers.length - 1]!;
    // The box changes before the library landed: nothing to resize, nothing thrown.
    expect(() => { act(() => { observer.fire(); }); }).not.toThrow();
    await act(async () => { release(); });
    await flush();
    expect(observer.targets).toEqual([document.querySelector('.map-host')]);
    act(() => { observer.fire(); });
    expect(last().resizes).toBe(1);

    // `hidden` in list mode: a resize against a zero box leaves the canvas blank.
    await act(async () => { listButton().click(); });
    act(() => { observer.fire(); });
    expect(last().resizes).toBe(1);
    view.unmount();
    expect(observer.targets).toEqual([]);
  });

  it('fits the whole map from the Fit control', async () => {
    const view = await mountLoaded();
    await act(async () => { screen.getByRole('button', { name: 'Fit' }).click(); });
    expect(last().fits).toBe(1);
    view.unmount();
  });
});

describe('the payload shapes', () => {
  it('names a topic with no name by its id, on the canvas, in the panel and in the list', async () => {
    const nameless = [node({ id: 'bare', name: null, status: 'frontier' })];
    const view = await mountLoaded({
      api: stubApi({ getGraph: async () => graph({ nodes: nameless, edges: [] }) }),
    });
    const cy = last();
    expect(cy.options.elements[0].data.name).toBe('bare');

    await act(async () => { cy.emit('tap', cy.getElementById('bare')); });
    expect(document.querySelector('.map-panel h2')!.textContent).toBe('bare');

    await act(async () => { listButton().click(); });
    expect(document.querySelector('.map-list-group li span')!.textContent).toBe('bare');
    view.unmount();
  });

  it('ignores a tap on an element the payload does not hold', async () => {
    const view = await mountLoaded();
    const cy = last();
    await act(async () => { cy.emit('tap', cy.getElementById('ghost')); });
    expect(document.querySelector('.map-panel')).toBeNull();
    expect(cy.centered).toEqual([]);
    view.unmount();
  });

  it('ignores an edge that names a topic outside the scope', () => {
    const depth = layerOf(NODES, [...EDGES, { from: 'whole-numbers', to: 'elsewhere' }]);
    expect(depth.get('ratios')).toBe(2);
    expect(depth.has('elsewhere')).toBe(false);
    // An edge FROM outside the scope counts for nothing either: the topic it points at
    // is placed by its in-scope prerequisites alone, and the topics after it follow.
    const chain = [node({ id: 'a' }), node({ id: 'b' }), node({ id: 'c' })];
    const fed = layerOf(chain, [
      { from: 'elsewhere', to: 'b' },
      { from: 'a', to: 'b' },
      { from: 'b', to: 'c' },
    ]);
    expect(fed).toEqual(new Map([['a', 0], ['b', 1], ['c', 2]]));
    expect(toElements(NODES, [{ from: 'nowhere', to: 'ratios' }]).filter((e) => e.group === 'edges'))
      .toHaveLength(1);
  });

  it('imports the vendored module when the seam is reset with no argument', async () => {
    resetCytoscapeLoader();
    // Under the test config the vendored specifier resolves to the double.
    expect(await loadCytoscape()).toBe(cytoscape);
  });

  it('builds the stylesheet literally, from the tokens it is given', () => {
    const tokens = readTokens({
      getPropertyValue: (name: string) => ({ '--text': ' #111 ', '--muted': '#888', '--accent': '#f00' })[name] ?? '',
    });
    expect(tokens).toEqual({
      text: '#111',
      muted: '#888',
      // `untouched` reads the muted token, so it shares the edge color.
      state: { frontier: '#f00', learning: '', placed: '', floor: '', untouched: '#888' },
    });
    expect(buildStyle(tokens)).toEqual([
      {
        selector: 'node',
        style: {
          width: 'data(size)',
          height: 'data(size)',
          label: 'data(name)',
          color: '#111',
          'font-size': 9,
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
          'line-color': '#888',
          'target-arrow-color': '#888',
          'target-arrow-shape': 'triangle',
          'arrow-scale': 0.6,
        },
      },
      { selector: '.st-frontier', style: { 'background-color': '#f00' } },
      { selector: '.st-learning', style: { 'background-color': '' } },
      { selector: '.st-placed', style: { 'background-color': '' } },
      { selector: '.st-floor', style: { 'background-color': '' } },
      { selector: '.st-untouched', style: { 'background-color': '#888' } },
      { selector: '.st-floor', style: { 'background-opacity': 0.45 } },
      { selector: '.st-untouched', style: { 'background-opacity': 0.5 } },
      { selector: '.st-frontier', style: { 'border-width': 2, 'border-color': '#f00' } },
      {
        selector: 'node.pick',
        style: { 'border-width': 3, 'border-color': '#111', 'text-opacity': 1, 'min-zoomed-font-size': 0 },
      },
    ]);
  });
});
