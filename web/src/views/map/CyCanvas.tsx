/**
 * The imperative island: one Cytoscape instance and nothing else.
 *
 * THE OWNERSHIP BOUNDARY, which the whole unit rests on:
 *
 *   `<Map>`       ordinary React — the payload, the scope, the legend, the detail panel,
 *                 and the accessible list view (pure data to DOM).
 *   `<CyCanvas>`  this file — the instance, its stylesheet, its handlers, and the resize
 *                 observer. React never renders anything inside its container.
 *
 * DESTROY BEFORE BUILD. `<Map>` gives this component a `key` that changes with the payload
 * and with the retry count, so React UNMOUNTS the outgoing island — running the cleanup,
 * which destroys the instance — before it mounts the replacement. The rule is structural,
 * not a hand-rolled generation counter: there is no code path on which two instances are
 * alive at once (F-38-1).
 *
 * The `key` is also the only thing that resets `failed`. Without it, one dead renderer
 * would render the failure block for every later scope the learner picks.
 */
import { useEffect, useRef, useState } from 'react';
import { LoadingBlock } from '@/components/primitives';
import { toast } from '@/app/toast';
import { loadCytoscape, type CyLike } from './cytoscape-loader';
import { buildStyle, readTokens } from './mapStyle';
import { toElements } from './layout';
import type { GraphEdge, GraphNode } from '@/api/types';

/** The line the learner reads when the renderer will not load. */
export const MAP_RENDERER_FAILED = 'Could not load the map renderer.';

/** Everything `<Map>` drives imperatively, instead of reaching for the instance. */
export interface CyHandle {
  fit: () => void;
  panBy: (delta: { x: number; y: number }) => void;
  zoomBy: (factor: number) => void;
  /** Ring one topic and center the camera on it. `null` clears the ring. */
  pick: (id: string | null) => void;
}

interface CyCanvasProps {
  nodes: GraphNode[];
  edges: GraphEdge[];
  /** Filled once the instance exists, and cleared on teardown. */
  handleRef: React.RefObject<CyHandle | null>;
  /** A tap on a node, or `null` for a tap on the background. */
  onSelect: (id: string | null) => void;
  /** Re-enter the renderer memo. `<Map>` bumps the key, which remounts this component. */
  onRetry: () => void;
}

/** The two callbacks of the parent, read at event time. */
interface Callbacks {
  onSelect: (id: string | null) => void;
  onRetry: () => void;
}

/** Where the island is: waiting on the library, drawn, or told the library is dead. */
type Island = 'loading' | 'ready' | 'failed';

function CyCanvas({ nodes, edges, handleRef, onSelect, onRetry }: CyCanvasProps) {
  const cyRef = useRef<CyLike | null>(null);
  // Cytoscape's OWN container, never shared with React. Handing it the wrapper that React
  // also paints the spinner and the failure block into has the two of them inserting and
  // removing siblings in one node.
  const hostRef = useRef<HTMLDivElement>(null);
  const [island, setIsland] = useState<Island>('loading');

  // The callbacks ride in a ref, so the instance effect never re-runs because the parent
  // re-rendered. Rebuilding a 1,090-element store because a panel opened would be absurd.
  // The write is in an effect, never during render, and it runs before any tap can land.
  const cb = useRef<Callbacks | null>(null);
  useEffect(() => { cb.current = { onSelect, onRetry }; });

  // ONE effect owns the instance, and the two listeners that serve it.
  //
  // There is no reset to `loading` at the top: the parent's `key` remounts this component
  // for every new payload, so the initial state IS the reset state.
  useEffect(() => {
    // Set by the cleanup, which runs on every unmount and on every StrictMode teardown: a
    // continuation that lands after it builds nothing and reports nothing.
    let cancelled = false;
    // The host div is on the page for the life of this effect, so the ref is never null.
    const host = hostRef.current!;

    void loadCytoscape()
      .then((cytoscape) => {
        // The guard that makes an overlapping load build ONE instance: the outgoing island
        // is already unmounted here, so its continuation never constructs anything.
        if (cancelled) return;
        const big = nodes.length > 400;
        const cy = cytoscape({
          container: host,
          elements: toElements(nodes, edges),
          style: buildStyle(readTokens()),
          // `preset` and nothing else: `layout.ts` computed the coordinates, and every
          // Cytoscape layout extension needs a CSP relaxation this service does not grant.
          layout: { name: 'preset', fit: true, padding: 30 },
          boxSelectionEnabled: false,
          autoungrabify: true,
          autounselectify: true,
          pixelRatio: big ? 1 : 'auto',
          hideEdgesOnViewport: big,
          textureOnViewport: big,
          motionBlur: false,
        });
        cyRef.current = cy;
        setIsland('ready');

        // Bound the camera around the fitted view: a little further out than "fit", close
        // enough in to read a label.
        const fitZoom = cy.zoom();
        cy.minZoom(Math.max(fitZoom * 0.6, 0.02));
        cy.maxZoom(2.5);

        cy.on('tap', 'node', (evt) => {
          cb.current!.onSelect(evt.target.id());
        });
        cy.on('tap', (evt) => {
          if (evt.target === cy) cb.current!.onSelect(null);
        });

        handleRef.current = {
          fit: () => cy.fit(),
          panBy: (delta) => cy.panBy(delta),
          zoomBy: (factor) => cy.zoom(cy.zoom() * factor),
          pick: (id) => {
            cy.elements().removeClass('pick');
            if (id === null) return;
            const node = cy.getElementById(id);
            if (node.empty()) return;
            node.addClass('pick');
            cy.center(node);
          },
        };
      })
      .catch(() => {
        if (cancelled) return;
        setIsland('failed');
        // An ACTIONABLE toast (F-36-1b): one that carries an action never auto-dismisses,
        // so the recovery survives a learner who looked away. A bare error toast expires in
        // six seconds and takes the only prompt with it. The host names the action Retry.
        toast(MAP_RENDERER_FAILED, { onAction: () => { cb.current!.onRetry(); } });
      });

    // A color-scheme flip needs the sheet rebuilt: Cytoscape holds literal colors, not
    // tokens. The instance arrives later than the listener, so it may be absent.
    const query = matchMedia('(prefers-color-scheme: light)');
    const restyle = () => cyRef.current?.style(buildStyle(readTokens()));
    query.addEventListener('change', restyle);

    // The canvas is sized by CSS, and Cytoscape reads pixels at construction, so a container
    // that changes size needs an explicit resize.
    const observer = new ResizeObserver(() => {
      // `hidden` in list mode, on the canvas the host sits in: a resize against a zero box
      // leaves the canvas blank on the way back, so skip it and let the mode switch resize
      // instead.
      if (host.closest('[hidden]') === null) cyRef.current?.resize();
    });
    observer.observe(host);

    return () => {
      cancelled = true;
      handleRef.current = null;
      query.removeEventListener('change', restyle);
      observer.disconnect();
      // The destroy is NOT guarded by liveness: on a payload change this cleanup is the
      // only thing that frees the outgoing instance, and a liveness check in front of it
      // would leak exactly what it exists to release.
      //
      // It is wrapped, because `useLifetime` names `cy.destroy()` as the call that throws:
      // a throw here would escape the unmount and leave `cyRef` pointing at a half-torn
      // instance while the effect builds the replacement. The pointer goes first.
      const cy = cyRef.current;
      cyRef.current = null;
      try {
        cy?.destroy();
      } catch {
        /* a half-built instance can throw on teardown */
      }
    };
  }, [nodes, edges, handleRef]);

  return (
    <>
      <div ref={hostRef} className="map-host" data-state={island} />
      {island === 'loading' ? <LoadingBlock label="Loading the map renderer…" /> : null}
      {island === 'failed' ? (
        <div className="empty">
          <p>{MAP_RENDERER_FAILED}</p>
          <button type="button" className="btn btn-primary" onClick={onRetry}>
            Try again
          </button>
        </div>
      ) : null}
    </>
  );
}

// The default export is what `React.lazy` in `Map.tsx` awaits.
export default CyCanvas;
