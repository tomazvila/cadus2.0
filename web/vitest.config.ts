import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

const here = (p: string) => fileURLToPath(new URL(p, import.meta.url));

export default defineConfig({
  plugins: [react()],

  resolve: {
    alias: {
      '@': here('./src'),
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
