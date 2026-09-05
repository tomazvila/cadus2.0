/**
 * The curriculum map (S11), part 2: a dead renderer, the accessible list view, and the
 * payload on screen.
 *
 * `map.test.tsx` carries the module note and the fixtures live in `test/helpers/map.tsx`.
 */
import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axe } from 'vitest-axe';
import { MAP_CANVAS_LABEL, MAP_EMPTY, MAP_PAN_STEP } from '@/views/map/Map';
import { MAP_RENDERER_FAILED } from '@/views/map/CyCanvas';
import { STATES } from '@/views/map/layout';
import { toastStore } from '@/app/toast';
import { ApiError } from '@/api';
import { resetCytoscapeLoader } from '@/views/map/cytoscape-loader';
import { AXE_IN_JSDOM } from './axe';
import { instances } from './mocks/cytoscape';
import {
  NODES, canvas, changeScope, deferredImport, failOnceImporter, flush, graph, label, last,
  listButton, mount, mountLoaded, stubApi,
} from './helpers/map';
import type { ApiClient, GraphResponse } from '@/api/types';

describe('the map view — a dead renderer', () => {
  it('reports a dead renderer, and the next mount retries the load', async () => {
    failOnceImporter();
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
    failOnceImporter();
    const view = await mount();
    expect(screen.getByText(MAP_RENDERER_FAILED)).toBeTruthy();

    await changeScope('all');

    expect(screen.queryByText(MAP_RENDERER_FAILED)).toBeNull();
    expect(instances).toHaveLength(1);
    view.unmount();
  });
});

describe('the map view — the accessible list view', () => {
  it('reaches the list view with the keyboard alone', async () => {
    const user = userEvent.setup();
    const view = await mountLoaded();

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
    const view = await mountLoaded();
    await act(async () => { listButton().click(); });

    const groups = document.querySelectorAll<HTMLDetailsElement>('.map-list-group');
    // Four states hold a topic; `placed` holds none and prints no group.
    expect(groups).toHaveLength(4);
    expect(groups[0].querySelector('summary')!.textContent).toBe('Ready to learn1');
    // The frontier group opens by itself; the rest wait for the reader.
    expect(Array.from(groups).map((g) => g.open)).toEqual([true, false, false, false]);
    expect(Array.from(groups).map((g) => g.querySelector('.legend-dot')!.className)).toEqual([
      'legend-dot ld-frontier', 'legend-dot ld-learning', 'legend-dot ld-floor', 'legend-dot ld-untouched',
    ]);
    expect(document.querySelectorAll('.map-list-group li')).toHaveLength(NODES.length);
    expect(document.querySelector('.map-list-group li')!.textContent)
      .toBe('Fractions — Arithmetic · ability 10%');
    view.unmount();
  });

  it('sends a screen reader from the canvas to the list view', async () => {
    const view = await mountLoaded();
    // A canvas is not screen-readable and no ARIA attribute makes it so. The label is the
    // pointer to the equivalent, and the tab stop is what makes the pan keys reachable.
    expect(canvas().getAttribute('role')).toBe('img');
    expect(canvas().getAttribute('aria-label')).toBe(MAP_CANVAS_LABEL);
    expect(canvas().tabIndex).toBe(0);
    view.unmount();
  });

  it('reports no axe violation in either mode', async () => {
    const view = await mountLoaded();
    expect(await axe(document.getElementById('view')!, AXE_IN_JSDOM)).toHaveNoViolations();
    await act(async () => { listButton().click(); });
    expect(await axe(document.getElementById('view')!, AXE_IN_JSDOM)).toHaveNoViolations();
    view.unmount();
  });
});

/** Mount the loaded map and tap Fractions, so its panel is open. */
async function tapFractions() {
  const view = await mountLoaded();
  const cy = last();
  await act(async () => { cy.emit('tap', cy.getElementById('fractions')); });
  expect(document.querySelector('.map-panel')).not.toBeNull();
  return { view, cy };
}

describe('the map view — the payload on screen', () => {
  it('reads out the counts the service sent, and derives none of them', async () => {
    const view = await mountLoaded();
    expect(document.querySelector('.map-readout')!.textContent)
      .toBe('4 topics · 4 links · 1 mastered');
    const chips = document.querySelectorAll('.map-legend .legend-chip');
    expect(chips).toHaveLength(STATES.length);
    expect(chips[0].textContent).toBe('Ready to learn1');
    view.unmount();
  });

  it('opens the detail panel on a node tap and closes it on the background', async () => {
    const { view, cy } = await tapFractions();
    expect(document.querySelector('.map-panel h2')!.textContent).toBe('Fractions');
    expect(document.querySelector('.map-panel .mono')!.textContent).toBe('fractions');
    // The state line: the dot of the state, its legend label, and the module after a dot.
    expect(document.querySelector('.map-panel-state')!.textContent).toBe('Ready to learn · Arithmetic');
    expect(document.querySelector('.map-panel-state .legend-dot')!.className).toBe('legend-dot ld-frontier');
    // The ring is a class on the instance, not a React render, and the camera moves to it.
    expect(cy.classesOf('fractions')).toEqual(['pick', 'st-frontier']);
    expect(cy.centered).toEqual(['fractions']);

    await act(async () => { cy.emit('tap', cy); });
    expect(document.querySelector('.map-panel')).toBeNull();
    expect(cy.classesOf('fractions')).toEqual(['st-frontier']);
    view.unmount();
  });

  it('pans on the arrow keys and fits on 0', async () => {
    const user = userEvent.setup();
    const view = await mountLoaded();
    canvas().focus();

    await user.keyboard('{ArrowLeft}{ArrowDown}0');
    expect(last().pans).toEqual([{ x: MAP_PAN_STEP, y: 0 }, { x: 0, y: -MAP_PAN_STEP }]);
    expect(last().fits).toBe(1);
    view.unmount();
  });

  it('closes the panel and drops the ring when the list view opens, and keeps them closed after', async () => {
    const { view, cy } = await tapFractions();
    await act(async () => { listButton().click(); });
    expect(cy.classesOf('fractions')).toEqual(['st-frontier']);
    await act(async () => { listButton().click(); });
    expect(document.querySelector('.map-panel')).toBeNull();
    view.unmount();
  });

  it('closes the panel when a new payload lands', async () => {
    const { view } = await tapFractions();
    await changeScope('all');
    expect(document.querySelector('.map-panel')).toBeNull();
    view.unmount();
  });

  it('asks for the own course with no scope, and for a scope by its id', async () => {
    const getGraph = vi.fn<ApiClient['getGraph']>(async () => graph());
    const view = await mountLoaded({ api: stubApi({ getGraph }) });
    await changeScope('proofs');
    expect(getGraph.mock.calls).toEqual([[undefined], ['proofs']]);
    view.unmount();
  });

  it('lists the own course, every course with the current one marked, and the whole curriculum', async () => {
    const view = await mountLoaded();
    const options = Array.from((screen.getByLabelText('Scope') as HTMLSelectElement).options);
    expect(options.map((o) => [o.value, o.textContent])).toEqual([
      ['', 'Your course'], ['foundations', 'Foundations ·'], ['proofs', 'Proofs'], ['all', 'Entire curriculum'],
    ]);
    view.unmount();
  });

  it('routes a 401 on the graph read to sign-in', async () => {
    const view = await mountLoaded({
      api: stubApi({ getGraph: async () => { throw new ApiError(401, 'unauthorized', 'No session.'); } }),
    });
    await waitFor(() => expect(view.onUnauthorized).toHaveBeenCalledTimes(1));
    view.unmount();
  });

  it('takes the Fit control and the list toggle before the instance exists', async () => {
    const { importer } = deferredImport();
    resetCytoscapeLoader(importer);
    const view = await mount();
    await act(async () => { screen.getByRole('button', { name: 'Fit' }).click(); });
    await act(async () => { listButton().click(); });
    expect(document.querySelector('.map-list')).not.toBeNull();
    view.unmount();
  });

  it('swallows a bound key on the canvas', async () => {
    const view = await mountLoaded();
    expect(fireEvent.keyDown(canvas(), { key: '0' })).toBe(false);
    expect(last().fits).toBe(1);
    view.unmount();
  });

  it('paints one legend dot per state, in the class of that state', async () => {
    const view = await mountLoaded();
    expect(Array.from(document.querySelectorAll('.map-legend .legend-dot')).map((d) => d.className)).toEqual([
      'legend-dot ld-frontier', 'legend-dot ld-learning', 'legend-dot ld-placed', 'legend-dot ld-floor', 'legend-dot ld-untouched',
    ]);
    view.unmount();
  });

  it('says so when the scope holds no topic, instead of an empty canvas', async () => {
    const empty = graph({ nodes: [], edges: [], counts: { nodes: 0, edges: 0, mastered: 0 } });
    const view = await mountLoaded({ api: stubApi({ getGraph: async () => empty }) });
    expect(screen.getByText(MAP_EMPTY)).toBeTruthy();
    expect(instances).toHaveLength(0);
    view.unmount();
  });

  it('never paints a slower scope reply over the newer one', async () => {
    let release!: (g: GraphResponse) => void;
    const slow = new Promise<GraphResponse>((r) => { release = r; });
    const getGraph = vi.fn<ApiClient['getGraph']>()
      .mockImplementationOnce(async () => graph())
      .mockImplementationOnce(() => slow)
      .mockImplementation(async () => graph({ scope: 'all', counts: { nodes: 1, edges: 0, mastered: 0 } }));

    const view = await mountLoaded({ api: stubApi({ getGraph }) });

    // Pick Proofs, then the whole curriculum before Proofs answers.
    await changeScope('proofs');
    await changeScope('all');
    await waitFor(() => expect(document.querySelector('.map-readout')).not.toBeNull());
    expect(document.querySelector('.map-readout')!.textContent).toBe('1 topics · 0 links · 0 mastered');

    // The late reply for Proofs lands last, and must change nothing.
    await act(async () => { release(graph({ scope: 'proofs' })); });
    await flush();
    expect(document.querySelector('.map-readout')!.textContent).toBe('1 topics · 0 links · 0 mastered');
    view.unmount();
  });
});
