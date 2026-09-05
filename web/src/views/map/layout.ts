/**
 * The curriculum map's layout and display vocabulary — pure data, no React, no Cytoscape.
 *
 * THE CLIENT LAYS THE GRAPH OUT, and that is the one structural difference from 1.0. 1.0's
 * server sent `x` and `y` with every node and the client fed them straight to the `preset`
 * layout. 2.0's `GET /api/graph` sends neither (`crates/web/src/session.rs:509-517`), so
 * the coordinates are computed here.
 *
 * It is still `preset` in the browser. No layout ALGORITHM runs there: `layerOf` is a
 * longest-path pass over the prerequisite edges, it is deterministic, and it is tested on
 * literal coordinates. The CSP blocks every Cytoscape layout extension worth having
 * (cytoscape-dagre, cola, spread all use eval or a blob worker), so a pure pass here is
 * also the only option the header leaves open.
 *
 * jsdom returns '' for every CSS custom property, so `readTokens` can never be asserted on
 * color. The layout and the size math can, and they are what this file exists for.
 */
import type { GraphEdge, GraphNode, TopicStatus } from '@/api/types';

/** Horizontal gap between two topics of the same layer, in graph units. */
export const X_STEP = 90;

/** Vertical gap between two layers, in graph units. */
export const Y_STEP = 120;

/**
 * The five mastery states, in the order the legend and the list view show them.
 *
 * The order is the learner's order of interest, not the enum's: what to study next comes
 * first, and what the placement assumed comes last. `token` names the CSS custom property
 * the canvas paints that state with — `--good` and the rest live in `tokens.css`, which is
 * the one home of the palette (TOKENS-hex).
 */
export const STATES: ReadonlyArray<{ id: TopicStatus; label: string; token: string }> = [
  { id: 'frontier', label: 'Ready to learn', token: '--accent' },
  { id: 'learning', label: 'Learning', token: '--info' },
  { id: 'placed', label: 'Placed', token: '--good' },
  { id: 'floor', label: 'Assumed known', token: '--accent-2' },
  { id: 'untouched', label: 'Not reached', token: '--muted' },
];

/**
 * Node diameter from ability: 12 px at zero, 30 px at one.
 *
 * `ability` is the core's per-topic estimate in [0, 1]. A non-finite or out-of-range value
 * clamps rather than producing a NaN width, which Cytoscape renders as an invisible node.
 */
export function sizeOf(ability: number): number {
  const a = Number.isFinite(ability) ? Math.min(Math.max(ability, 0), 1) : 0;
  return 12 + 18 * a;
}

/**
 * The prerequisite depth of every node: 0 for a topic with no in-scope prerequisite, and
 * one more than its deepest prerequisite otherwise.
 *
 * Kahn's order, so the pass is linear and needs no recursion over a 1,090-node scope. A
 * node the queue never reaches sits in a cycle. The curriculum is a validated DAG, so that
 * cannot happen through the API — but a defect upstream must not hang the map or hide the
 * topic, so such a node keeps depth 0 and draws on the foundation row.
 */
export function layerOf(nodes: GraphNode[], edges: GraphEdge[]): Map<string, number> {
  const depth = new Map<string, number>(nodes.map((n) => [n.id, 0]));
  const outgoing = new Map<string, string[]>(nodes.map((n) => [n.id, []]));
  const indegree = new Map<string, number>(nodes.map((n) => [n.id, 0]));

  for (const edge of edges) {
    // `from` is the prerequisite and `to` is the dependent, so the arrow reads "unlocks".
    if (!depth.has(edge.from) || !depth.has(edge.to)) continue;
    // Every in-scope node was seeded above, so the three maps hold every id read below.
    outgoing.get(edge.from)!.push(edge.to);
    indegree.set(edge.to, indegree.get(edge.to)! + 1);
  }

  const queue = nodes.filter((n) => indegree.get(n.id) === 0).map((n) => n.id);
  // `for..of` over an array visits the ids pushed while it runs, so the queue drains.
  for (const id of queue) {
    const here = depth.get(id)!;
    for (const next of outgoing.get(id)!) {
      depth.set(next, Math.max(depth.get(next)!, here + 1));
      const left = indegree.get(next)! - 1;
      indegree.set(next, left);
      if (left === 0) queue.push(next);
    }
  }
  return depth;
}

/**
 * A position per node: layers stacked bottom to top, each layer centered on x = 0.
 *
 * `y` is NEGATED, because the Cytoscape y axis points down the screen. Foundations sit at
 * the bottom and the map reads "up = more advanced", exactly as 1.0's map does.
 *
 * Within a layer the order is module, then id — a total order over the payload, so the same
 * payload always draws the same picture and the coordinates are assertable.
 */
export function positionsOf(
  nodes: GraphNode[],
  edges: GraphEdge[],
): Map<string, { x: number; y: number }> {
  const depth = layerOf(nodes, edges);
  const layers = new Map<number, GraphNode[]>();
  for (const node of nodes) {
    const at = depth.get(node.id)!;
    const row = layers.get(at);
    if (row) row.push(node);
    else layers.set(at, [node]);
  }

  const out = new Map<string, { x: number; y: number }>();
  for (const [at, row] of layers) {
    row.sort((a, b) => a.module.localeCompare(b.module) || a.id.localeCompare(b.id));
    // `-0` is not `0` to a deep-equality assertion, and the foundation row is the one that
    // produces it. Spell the zero out.
    const y = at === 0 ? 0 : -at * Y_STEP;
    for (let i = 0; i < row.length; i += 1) {
      out.set(row[i].id, { x: (i - (row.length - 1) / 2) * X_STEP, y });
    }
  }
  return out;
}

/** One Cytoscape element. `position` is absent on an edge. */
export interface MapElement {
  group: 'nodes' | 'edges';
  /** The node id, name, module and size, or the edge id, source and target. */
  data: Record<string, string | number>;
  classes: string;
  position?: { x: number; y: number };
}

/** The element list Cytoscape is built from. */
export function toElements(nodes: GraphNode[], edges: GraphEdge[]): MapElement[] {
  const positions = positionsOf(nodes, edges);
  const elements: MapElement[] = nodes.map((node) => ({
    group: 'nodes',
    data: {
      id: node.id,
      // The label is the name, and the id when the payload carries no name.
      name: node.name ?? node.id,
      module: node.module,
      size: sizeOf(node.ability),
    },
    // The state rides as a CLASS, not as node data: a Cytoscape stylesheet selects on
    // classes, and a `data(state)` mapper cannot pick a color per value.
    classes: `st-${node.status}`,
    position: positions.get(node.id)!,
  }));

  for (const edge of edges) {
    elements.push({
      group: 'edges',
      data: { id: `${edge.from}->${edge.to}`, source: edge.from, target: edge.to },
      classes: 'prereq',
    });
  }
  return elements;
}

/** How many topics sit in each state. Every state gets a key, including the empty ones. */
export function countByStatus(nodes: GraphNode[]): Record<TopicStatus, number> {
  const counts = Object.fromEntries(STATES.map((s) => [s.id, 0])) as Record<TopicStatus, number>;
  // `STATES` names every `TopicStatus`, so every status has its key.
  for (const node of nodes) counts[node.status] += 1;
  return counts;
}
