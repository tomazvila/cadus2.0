import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

const here = (p: string) => fileURLToPath(new URL(p, import.meta.url));

export default defineConfig({
  plugins: [react()],

  resolve: {
    alias: {
      '@': here('./src'),

      // `src/views/map/cytoscape-loader.ts` imports this ROOT-ABSOLUTE specifier, which the
      // browser resolves against the origin Caddy serves `/vendor/` from. Vitest resolves
      // nothing there, so without this alias every map test fails to load. It also hands
      // the map a deterministic double instead of a 434 KB canvas library in jsdom.
      //
      // A TEST-config fix only. Production imports the vendored file (`vite.config.ts`
      // marks `/vendor/**` external, and `scripts/check-bundle-csp.mjs` proves the build
      // holds it).
      '/vendor/cytoscape/cytoscape.esm.min.mjs': here('./test/mocks/cytoscape.ts'),
    },
  },

  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: [here('./test/setup.ts')],
    include: ['test/**/*.test.{ts,tsx}'],
    restoreMocks: true,
    // `restoreMocks` restores spies but does NOT undo `vi.stubGlobal`. Without this, a test
    // that stubs a global poisons every test declared after it in the same file.
    unstubGlobals: true,
    // A module registry per file. The toast store and the KaTeX memo are module-scope
    // singletons by design, so their state leaks between files without this.
    isolate: true,
  },
});
