/**
 * Boot.
 *
 * S1 is the scaffold: this mounts an empty shell and nothing else. S2 adds the API client,
 * S3 the hooks, S4 the design system, and S6 the single-use `?verify=` and `?reset=` tokens
 * — which must be read HERE, before `createRoot`, because React 19 StrictMode spends an
 * effect twice and both tokens are single-use.
 *
 * KaTeX is NOT imported from npm. It stays a `<link>` and two `<script>` tags to
 * `/vendor/katex/*`, injected by the build (see `vendor-tags.ts`). The npm package ships
 * woff2 AND woff AND ttf — a measured 1.5 MB of fonts where the vendored tree needs 600 KB
 * of woff2 — and the vendored tree is the CSP-audited 0.17.0 artifact. The `katex` package
 * stays a dev dependency for TESTS only: S5 runs the real auto-render against the render
 * idiom. Nothing in `src/` imports it.
 */
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './app/App';
import './styles/app.css';

/** The ids index.html provides. Each absence is a silent failure on its own. */
export const SHELL_IDS = ['view', 'topbar', 'toasts'] as const;

/**
 * Resolve the mount point, or say which part of the document is missing.
 *
 * Two of the three are worse than a missing view: without `#topbar` there is no way out of
 * a screen, and without `#toasts` every error the app reports — a failed grade, a lost
 * session — disappears with nothing on screen. A host that returns null for each of them
 * degrades three times over, so the precondition lives in one place.
 */
export function resolveMount(doc: Document): HTMLElement {
  const missing = SHELL_IDS.filter((id) => !doc.getElementById(id));
  if (missing.length) {
    throw new Error(`index.html is missing ${missing.map((id) => `#${id}`).join(', ')}`);
  }
  return doc.getElementById('view')!;
}

export function boot(): void {
  createRoot(resolveMount(document)).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

// The guard keeps the module importable by the suite: a test drives `boot()` and
// `resolveMount()` directly, against its own DOM.
if (import.meta.env.MODE !== 'test') boot();
