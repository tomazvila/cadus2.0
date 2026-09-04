/**
 * The fixtures and the moves of the curriculum-map tests: one payload with four states,
 * two modules, and three layers.
 *
 * Every instance assertion reads `test/mocks/cytoscape.ts`, which records `aliveAtBuild` —
 * how many instances were still undestroyed when this one was constructed. It must always
 * be 0, and that number is what makes destroy-before-build a literal rather than a story
 * about React keys. The library load itself is driven through `resetCytoscapeLoader`: the
 * memo is a module singleton, and the failure paths need an import that fails on demand.
 */
import { vi } from 'vitest';
import { act, screen } from '@testing-library/react';
import { createDemoApi } from '@/api';
import { CurriculumMap, type CurriculumMapProps } from '@/views/map/Map';
import { resetCytoscapeLoader } from '@/views/map/cytoscape-loader';
// The canvas module is loaded here, ahead of every `React.lazy` import of it: a module the
// registry already holds resolves in a microtask, and `mount` waits exactly two of them.
import '@/views/map/CyCanvas';
import { resetToasts } from '@/app/toast';
import { renderInView } from './render';
import cytoscape, { instances, resetCytoscape } from '../mocks/cytoscape';
import type { ApiClient, GraphEdge, GraphNode, GraphResponse } from '@/api/types';

export const node = (over: Partial<GraphNode> & Pick<GraphNode, 'id'>): GraphNode => ({
  name: over.id,
  module: 'Arithmetic',
  course: 'foundations',
  status: 'untouched',
  ability: 0,
  ...over,
});

export const NODES: GraphNode[] = [
  node({ id: 'whole-numbers', name: 'Whole numbers', status: 'floor', ability: 0.9 }),
  node({ id: 'fractions', name: 'Fractions', status: 'frontier', ability: 0.1 }),
  node({ id: 'decimals', name: 'Decimals', status: 'learning', ability: 0.5 }),
  node({ id: 'ratios', name: 'Ratios', module: 'Ratio', status: 'untouched' }),
];

export const EDGES: GraphEdge[] = [
  { from: 'whole-numbers', to: 'fractions' },
  { from: 'whole-numbers', to: 'decimals' },
  { from: 'fractions', to: 'ratios' },
  { from: 'decimals', to: 'ratios' },
];

export function graph(over: Partial<GraphResponse> = {}): GraphResponse {
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

export function stubApi(over: Partial<ApiClient> = {}): ApiClient {
  return { ...createDemoApi(), demo: false, getGraph: async () => graph(), ...over };
}

/** The importer that hands over the double, as the vendored module would. */
export const okImport = () => Promise.resolve({ default: cytoscape });

/** An importer whose promise the test releases by hand. */
export function deferredImport() {
  let release!: () => void;
  const gate = new Promise<void>((r) => { release = r; });
  const calls = { n: 0 };
  return {
    importer: () => { calls.n += 1; return gate.then(okImport); },
    release,
    calls,
  };
}

/** An importer whose FIRST call rejects with a chunk 404 and whose later calls resolve. */
export function failOnceImporter() {
  let n = 0;
  resetCytoscapeLoader(() => {
    n += 1;
    return n === 1 ? Promise.reject(new Error('chunk 404')) : okImport();
  });
  return () => n;
}

export async function mount(
  over: Partial<CurriculumMapProps> = {},
  wrapper?: (p: { children: React.ReactNode }) => React.ReactElement,
) {
  resetToasts();
  resetCytoscape();
  const handlers = { onUnauthorized: vi.fn(), onExit: vi.fn() };
  const props = { api: stubApi(), ...handlers, ...over };
  const tree = <CurriculumMap {...props} />;
  const view = await renderInView(wrapper ? wrapper({ children: tree }) : tree);
  // The lazy canvas module and the library memo are two awaits, not one.
  await act(async () => { await Promise.resolve(); });
  return { ...view, ...handlers };
}

/** Mount over the double, with the library already loadable. */
export async function mountLoaded(over: Partial<CurriculumMapProps> = {}) {
  resetCytoscapeLoader(okImport);
  return mount(over);
}

export const flush = async () => { await act(async () => { await Promise.resolve(); }); };
export const last = () => instances[instances.length - 1];
export const canvas = () => document.querySelector<HTMLElement>('.map-canvas')!;
export const listButton = () => screen.getByRole('button', { name: /List view|Map view/ });
export const label = (el: Element | null) =>
  el?.getAttribute('aria-label') ?? el?.textContent?.trim() ?? '';

/** Pick one scope in the select, and let the reply land. */
export async function changeScope(value: string): Promise<void> {
  const scope = screen.getByLabelText('Scope') as HTMLSelectElement;
  await act(async () => {
    scope.value = value;
    scope.dispatchEvent(new Event('change', { bubbles: true }));
  });
  await flush();
}
