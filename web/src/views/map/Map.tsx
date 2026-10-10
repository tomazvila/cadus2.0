/**
 * The curriculum map (`/map`, `GET /api/graph`).
 *
 * The React half of the view: the payload, the scope, the legend, the detail panel, and the
 * accessible list view. The Cytoscape instance lives in `<CyCanvas>`, which this file mounts
 * behind a `<Suspense>` boundary and gives a `key` to.
 *
 * TWO LAZY LOADS, AND THEY ARE NOT THE SAME LOAD.
 *
 *   the canvas MODULE   `React.lazy` + `<Suspense>`. It is a chunk of this build, served
 *                       from the same origin as the entry, and the error boundary above the
 *                       view resets by key. `lazy` is right for it, and it keeps Cytoscape's
 *                       adapter out of every screen that never opens the map.
 *   the LIBRARY         the memo of `cytoscape-loader.ts`. `React.lazy` caches a rejection
 *                       for the life of the page, so one 404 on the 434 KB vendored file
 *                       would make every later Try again fail instantly. The memo clears
 *                       itself, so a failed load retries on the next mount.
 *
 * THE CANVAS IS NOT SCREEN-READABLE, and no aria attribute makes a `<canvas>` readable. The
 * List control switches to a real DOM list of the same payload, grouped by state, and the
 * canvas's own label points at it (spec section 4.5). Both controls are ordinary buttons, so
 * the list is reachable with the keyboard alone.
 *
 * THE COMPONENT IS `CurriculumMap`, NOT `Map`. A component named `Map` shadows the global
 * `Map` inside its own module, so `new Map(...)` in this file constructs the COMPONENT and
 * recurses until the stack goes. It is measured, not theoretical: the first draft of this
 * file died that way, and React reported it as "Do not call Hooks inside useEffect".
 */
import { Suspense, lazy, useEffect, useMemo, useRef, useState } from 'react';
import { useCall } from '@/hooks/useCall';
import { LoadingBlock } from '@/components/primitives';
import { pct } from '@/lib/format';
import '../../styles/fix-map.css';
import { STATES, countByStatus } from './layout';
import { TopicProofs } from '@/views/proofs/Proofs';
import type { CyHandle } from './CyCanvas';
import type { ApiClient, GraphEdge, GraphNode, GraphResponse, TopicStatus } from '@/api/types';

/** The label of the canvas, which sends a screen reader to the list view. */
export const MAP_CANVAS_LABEL =
  'Curriculum map, drawn as a picture. Use the List view to read it as text.';

/** The line under an empty scope. */
export const MAP_EMPTY = 'This part of the course has no topics yet.';

/** One arrow press pans the camera by this many rendered pixels. */
export const MAP_PAN_STEP = 60;

// The canvas module, not the library. See the file docstring for why the two differ.
const CyCanvas = lazy(() => import('./CyCanvas'));

export interface CurriculumMapProps {
  api: ApiClient;
  /** Demo mode. A 401 then keeps the learner on the screen. */
  demo: boolean;
  /** The session-expired path of `useCall`. */
  onUnauthorized: () => void;
  /** Leave the map. */
  onExit: () => void;
  /** The words on the leave button. Name where it goes: the dashboard, or the lesson. */
  backLabel?: string;
}

/** Where the learner's choice of view is kept. */
const LIST_KEY = 'cadus.map.list';

/** A phone screen: the list reads better there than a graph does. */
const PHONE_QUERY = '(max-width: 600px)';

/** The stored view choice: '1' list, '0' map, anything else none. */
function readListMode(): boolean {
  try {
    const stored = window.localStorage.getItem(LIST_KEY);
    if (stored === '1') return true;
    if (stored === '0') return false;
  } catch { /* storage is optional */ }
  try { return window.matchMedia(PHONE_QUERY).matches; } catch { return false; }
}

function writeListMode(on: boolean): void {
  try { window.localStorage.setItem(LIST_KEY, on ? '1' : '0'); } catch { /* storage is optional */ }
}

/** The topics whose name or id holds the typed words. An empty query holds every topic. */
function matches(node: GraphNode, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q === '') return true;
  return (node.name ?? '').toLowerCase().includes(q) || node.id.toLowerCase().includes(q);
}

/** No payload yet. One array for every render before the first reply, so nothing rebuilds. */
const NO_NODES: GraphNode[] = [];
const NO_EDGES: GraphEdge[] = [];

export function CurriculumMap({
  api, demo, onUnauthorized, onExit, backLabel = 'Back to dashboard',
}: CurriculumMapProps) {
  const call = useCall({ demo, onUnauthorized });
  const [data, setData] = useState<GraphResponse | null>(null);
  const [listMode, setListMode] = useState(readListMode);
  const [query, setQuery] = useState('');
  const [picked, setPicked] = useState<string | null>(null);
  // Bumped by a retry. It rides in the canvas key, so a retry REMOUNTS the island and the
  // renderer memo is entered again.
  const [attempt, setAttempt] = useState(0);
  // Bumped by every payload that reaches the screen, and it rides in the same key.
  //
  // The counter is the identity, NOT the payload object and not `data.scope`. Two replies
  // can carry the same scope, and a memoized or reused array is the same object twice: an
  // island keyed on either of those would keep a dead renderer, or an instance built from
  // the previous payload, with nothing on screen to say so.
  const [revision, setRevision] = useState(0);
  const handleRef = useRef<CyHandle | null>(null);
  // The scope a reply must still belong to. Written SYNCHRONOUSLY when the learner picks,
  // and read by the continuation.
  const scopeRef = useRef('');

  // One loader per mount: it reads the client, the call and a ref, and all three hold.
  const [load] = useState(() => (next: string) => {
    void call(
      () => api.getGraph(next || undefined),
      (payload) => {
        // The continuation reads a ref, never captured render state: a Retry arrives renders
        // later (the React rule of `useCall`). Two scope changes in flight land in either
        // order, and the slower reply must not paint over the newer one. A retry of the
        // scope still on screen still applies, and a reply after the view left writes state
        // nobody renders.
        if (scopeRef.current !== next) return;
        setData(payload);
        setRevision((n) => n + 1);
        setPicked(null);
      },
    );
  });

  useEffect(() => { load(''); }, [load]);

  const nodes = data === null ? NO_NODES : data.nodes;
  const counts = useMemo(() => countByStatus(nodes), [nodes]);
  const selected = nodes.find((n) => n.id === picked) ?? null;
  const edges = data === null ? NO_EDGES : data.edges;
  const before = useMemo(() => neighborIds(edges, picked, 'to'), [edges, picked]);
  const after = useMemo(() => neighborIds(edges, picked, 'from'), [edges, picked]);

  /** Type in the find field: the list filters, and the canvas centers on the first match. */
  function find(next: string): void {
    setQuery(next);
    if (listMode || next.trim() === '') return;
    const first = nodes.find((n) => matches(n, next));
    if (first) pick(first.id);
  }

  function pick(id: string | null): void {
    setPicked(id);
    handleRef.current?.pick(id);
  }

  /**
   * Pick a scope.
   *
   * The payload on screen STAYS until the next one lands. Blanking it while the request is
   * in flight takes the scope picker down with it, so the learner cannot change their mind,
   * and a slow reply leaves them looking at a spinner where a map was.
   */
  function changeScope(next: string): void {
    scopeRef.current = next;
    load(next);
  }

  /** Arrows pan, `+` and `-` zoom, `0` fits. The canvas is in the tab order for this. */
  function onCanvasKey(e: React.KeyboardEvent<HTMLDivElement>): void {
    const handle = handleRef.current;
    if (!handle) return;
    const moves: Record<string, () => void> = {
      ArrowLeft: () => handle.panBy({ x: MAP_PAN_STEP, y: 0 }),
      ArrowRight: () => handle.panBy({ x: -MAP_PAN_STEP, y: 0 }),
      ArrowUp: () => handle.panBy({ x: 0, y: MAP_PAN_STEP }),
      ArrowDown: () => handle.panBy({ x: 0, y: -MAP_PAN_STEP }),
      '+': () => handle.zoomBy(1.25),
      '=': () => handle.zoomBy(1.25),
      '-': () => handle.zoomBy(0.8),
      '0': () => handle.fit(),
    };
    const move = moves[e.key];
    if (move) { move(); e.preventDefault(); }
  }

  if (!data) {
    return (
      <section className="view-map">
        <LoadingBlock label="Loading your map…" />
      </section>
    );
  }

  return (
    <section className="view-map">
      <div className="map-head">
        <button type="button" className="btn btn-ghost" onClick={onExit}>
          {backLabel}
        </button>
        <h1 className="h-screen">Map</h1>
        <span className="map-readout mono">
          {`${String(data.counts.nodes)} topics · ${String(data.counts.edges)} links · ${String(data.counts.mastered)} mastered`}
        </span>
      </div>

      <div className="map-controls">
        {/* Uncontrolled: the select keeps the scope the learner picked, and the payload
            on screen follows it through `load`. */}
        <select
          className="map-select"
          aria-label="Show topics from"
          onChange={(e) => { changeScope(e.target.value); }}
        >
          <option value="">Your course</option>
          {data.courses.map((course) => (
            <option key={course.id} value={course.id}>
              {course.current ? `${course.name} ·` : course.name}
            </option>
          ))}
          <option value="all">Every course</option>
        </select>

        <span className="map-spacer" />

        {listMode ? null : (
          <>
            <button
              type="button"
              className="btn btn-ghost"
              title="Zoom out to show the whole map (0)"
              onClick={() => handleRef.current?.fit()}
            >
              Show all
            </button>
            <button
              type="button"
              className="btn btn-ghost map-zoom"
              aria-label="Zoom out"
              onClick={() => handleRef.current?.zoomBy(0.8)}
            >
              −
            </button>
            <button
              type="button"
              className="btn btn-ghost map-zoom"
              aria-label="Zoom in"
              onClick={() => handleRef.current?.zoomBy(1.25)}
            >
              +
            </button>
          </>
        )}
        <button
          type="button"
          className="btn btn-ghost"
          aria-pressed={listMode}
          onClick={() => {
            writeListMode(!listMode);
            setListMode(!listMode);
            pick(null);
          }}
        >
          {listMode ? 'Map view' : 'List view'}
        </button>
      </div>

      <div className="map-legend" role="status">
        {STATES.map((state) => (
          <span key={state.id} className="legend-chip">
            <i className={`legend-dot ld-${state.id}`} aria-hidden="true" />
            <span>{state.label}</span>
            <b className="mono">{String(counts[state.id])}</b>
          </span>
        ))}
      </div>

      <div className="map-find">
        <label htmlFor="map-find-input">Find a topic</label>
        <input
          id="map-find-input"
          type="search"
          className="map-find-input"
          autoComplete="off"
          value={query}
          onChange={(e) => { find(e.target.value); }}
        />
      </div>

      <div className="map-body">
        {/* The canvas is genuinely keyboard-operable — arrows pan, + and - zoom, 0 fits —
            so it must be focusable AND must take key events. It keeps `role="img"` rather
            than an interactive role, because no screen reader can read a canvas: the label
            points at the list view, which is the accessible equivalent, and the list is
            where a keyboard reader belongs. That pairing is what trips both rules, and it
            is the pairing spec section 4.5 asks for. */}
        {/* eslint-disable-next-line jsx-a11y/no-noninteractive-element-interactions */}
        <div
          className="map-canvas"
          // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex
          tabIndex={0}
          role="img"
          aria-label={MAP_CANVAS_LABEL}
          hidden={listMode}
          onKeyDown={onCanvasKey}
        >
          {nodes.length === 0 ? (
            <p className="muted map-empty">{MAP_EMPTY}</p>
          ) : (
            <Suspense fallback={<LoadingBlock label="Loading the map…" />}>
              {/* The key ties the instance's lifetime to the payload and to the retry
                  count: React unmounts the outgoing island — which destroys the instance —
                  before it mounts the replacement (F-38-1). */}
              <CyCanvas
                key={`${String(revision)}:${String(attempt)}`}
                nodes={data.nodes}
                edges={data.edges}
                handleRef={handleRef}
                onSelect={pick}
                onRetry={() => { setAttempt((n) => n + 1); }}
              />
            </Suspense>
          )}
        </div>

        {listMode ? <MapList nodes={nodes} query={query} /> : null}

        {selected && !listMode ? (
          <aside className="map-panel" aria-label="Topic detail">
            <h2>{selected.name ?? selected.id}</h2>
            <p className="map-panel-id mono">{selected.id}</p>
            <p className="map-panel-state">
              <i className={`legend-dot ld-${selected.status}`} aria-hidden="true" />
              {labelOf(selected.status)}
              <span className="muted">{` · ${selected.module}`}</span>
            </p>
            <p className="mono">{abilityLine(selected)}</p>
            <Neighbors title="Learn first" ids={before} nodes={nodes} onPick={pick} />
            <Neighbors title="Opens next" ids={after} nodes={nodes} onPick={pick} />
            {/* D-PR1: the revision chains of this topic, versions and verdicts. */}
            <TopicProofs key={selected.id} api={api} topic={selected.id} />
          </aside>
        ) : null}
      </div>
    </section>
  );
}

/**
 * The accessible list view: the same payload as real DOM, grouped by state.
 *
 * Rendered ONLY in list mode. 1.0 kept it mounted and `hidden`, which was 1,090 list items
 * rebuilt on every hover and every pan frame.
 */
function MapList({ nodes, query }: { nodes: GraphNode[]; query: string }) {
  const searching = query.trim() !== '';
  const total = nodes.filter((n) => matches(n, query)).length;
  return (
    <div className="map-list">
      {searching && total === 0 ? <p className="muted">{`No topic matches "${query.trim()}".`}</p> : null}
      {STATES.map((state) => {
        const rows = nodes.filter((n) => n.status === state.id && matches(n, query));
        if (rows.length === 0) return null;
        return (
          <details
            key={`${state.id}:${String(searching)}`}
            className="map-list-group"
            open={searching || state.id === 'frontier'}
          >
            <summary>
              <i className={`legend-dot ld-${state.id}`} aria-hidden="true" />
              <span>{state.label}</span>
              <span className="mono muted">{String(rows.length)}</span>
            </summary>
            <ul>
              {rows.map((node) => (
                <li key={node.id}>
                  <span>{node.name ?? node.id}</span>
                  {/* Everything the sighted reader gets from the detail panel. A row of
                      name alone makes the canvas's promise of a screen-readable version
                      false. */}
                  <span className="muted small">{`: ${node.module} · ${abilityLine(node)}`}</span>
                </li>
              ))}
            </ul>
          </details>
        );
      })}
    </div>
  );
}

/** The ids on the other end of every edge that touches `id` on the `end` side. */
function neighborIds(edges: GraphEdge[], id: string | null, end: 'from' | 'to'): string[] {
  if (id === null) return [];
  const other = end === 'to' ? 'from' : 'to';
  return edges.filter((e) => e[end] === id).map((e) => e[other]);
}

/** A titled row of buttons, each of which selects that topic. Nothing renders for none. */
function Neighbors({ title, ids, nodes, onPick }: {
  title: string;
  ids: string[];
  nodes: GraphNode[];
  onPick: (id: string) => void;
}) {
  const named = ids
    .map((id) => nodes.find((n) => n.id === id))
    .filter((n): n is GraphNode => n !== undefined);
  if (named.length === 0) return null;
  return (
    <div className="map-neighbors">
      <h3>{title}</h3>
      <ul>
        {named.map((n) => (
          <li key={n.id}>
            <button type="button" className="map-link" onClick={() => { onPick(n.id); }}>
              {n.name ?? n.id}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** The legend label of one state. `STATES` names every `TopicStatus`, so the find holds. */
function labelOf(status: TopicStatus): string {
  return STATES.find((s) => s.id === status)!.label;
}

/** The one number the 2.0 payload carries per topic. */
function abilityLine(node: GraphNode): string {
  return `ability ${String(pct(node.ability))}%`;
}
