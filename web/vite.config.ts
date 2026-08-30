import { fileURLToPath } from 'node:url';
import { defineConfig, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';
import { VENDOR_TAGS } from './vendor-tags';

const here = (p: string) => fileURLToPath(new URL(p, import.meta.url));

/**
 * Put the vendored KaTeX tags into the document, in array order.
 *
 * `head-prepend` inserts the whole batch at the top of `<head>` and keeps the array order,
 * so `katex.min.js` loads before `auto-render.min.js` — see `vendor-tags.ts` for why both
 * the injection and the order are load-bearing.
 */
export function vendorTags(): Plugin {
  return {
    name: 'cadus-vendor-tags',
    transformIndexHtml() {
      return VENDOR_TAGS as unknown as Array<{ tag: string }>;
    },
  };
}

// The build contract. Every value below is load-bearing; spec section 4.6 gives the reason
// for each one.
export default defineConfig({
  plugins: [react(), vendorTags()],

  // `/vendor/**` and `/favicon.svg` live under `public/`, so Vite serves them in dev and
  // copies them into `dist/` verbatim at build. ONE tree, never rewritten and never hashed:
  // 1.0 kept a second copy and it drifted twice in one commit.
  publicDir: 'public',

  // Assets are referenced root-absolute. Caddy serves `dist/` and proxies `/api` to the
  // axum service on the SAME origin, so no path rewriting happens anywhere.
  base: '/',

  build: {
    outDir: 'dist',
    emptyOutDir: true,

    // The deployed CSP is `font-src 'self'` with NO `data:` (crates/web/src/security.rs:25).
    // Vite inlines assets under 4 KB as `data:` URIs by default, which silently breaks the
    // three smallest KaTeX fonts. Inline nothing.
    assetsInlineLimit: 0,

    // Not shipped: a source map at the edge exposes the whole tree.
    sourcemap: false,

    rollupOptions: {
      // `/vendor/**` is served, not bundled.
      //
      // The curriculum map does `import('/vendor/cytoscape/cytoscape.esm.min.mjs')` — a
      // RUNTIME url the browser resolves against the same origin. Rollup otherwise tries to
      // read it off disk relative to the project root and fails the build. External also
      // keeps the 434 KB library out of the entry chunk, so the views that never open the
      // map never pay for it.
      external: [/^\/vendor\//],
    },
  },

  // `vite dev` serves the SPA while the axum service serves the API on 8080
  // (crates/web/src/bin/cadus-web.rs: BIND_ADDR defaults to 0.0.0.0:8080). Without this
  // proxy every /api call 404s against the dev server.
  server: {
    proxy: {
      '/api': { target: 'http://127.0.0.1:8080', changeOrigin: true },
    },
  },

  resolve: {
    alias: {
      '@': here('./src'),
    },
  },
});
