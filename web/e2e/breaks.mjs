/**
 * The three deliberate breaks of the S13 acceptance check.
 *
 * "the three failures 1.0's click-through found are each reproduced by a deliberately
 * broken build and caught; a clean build passes all flows"
 * (`docs/reference/authoring-and-spa-1.0-spec.md` section 7.2, row S13).
 *
 * AN ASSERTION THAT HAS NEVER FAILED IS AN ASSERTION THAT PASSES FOREVER. The three checks
 * in `checks.mjs` are the whole reason this click-through exists, and a green run against a
 * correct build proves nothing about them. Each break below reproduces the MECHANISM of one
 * 1.0 failure, not a symptom staged for the detector.
 *
 * Every break is applied to a COPY of the source tree under `e2e/work/`, never to the tree
 * itself. `run.mjs` builds the copy, serves it, and expects the named failure.
 *
 * The copy sits under `web/` on purpose: Vite and its plugins are resolved by walking up to
 * `web/node_modules`, so a copy anywhere else fails to load the config at all.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { FAIL_BLANK, FAIL_LATEX, FAIL_SERVE } from './checks.mjs';

/** Rewrite one file of the copy, and refuse a substitution that matched nothing. */
function edit(dir, file, from, to) {
  const path = join(dir, file);
  const before = readFileSync(path, 'utf8');
  const after = before.replace(from, to);
  if (after === before) {
    throw new Error(`break: ${file} does not hold ${String(from)} — the break is stale`);
  }
  writeFileSync(path, after);
}

export const BREAKS = [
  {
    id: 'blank-page',
    expect: FAIL_BLANK,
    what:
      'the entry chunk imports a stylesheet as a module, so Chrome never evaluates it and '
      + 'the page is white with every asset answering 200',
    /**
     * 1.0 reached this by writing the vendored `<link rel="stylesheet">` into index.html:
     * Vite rewrote it into a module-graph entry, and with `/vendor/**` external it came out
     * as `import "/vendor/katex/katex.min.css"` at the top of the entry chunk.
     *
     * The break states that OUTPUT directly. Vite's HTML handling has moved on — it now
     * leaves a `public/` href alone — so reproducing the old input would reproduce nothing,
     * while the emitted artifact is what Chrome actually refuses. `external: [/^\/vendor\//]`
     * in `vite.config.ts` carries the specifier into the bundle untouched.
     */
    apply: (dir) => edit(
      dir,
      'src/main.tsx',
      "import { StrictMode } from 'react';",
      "import '/vendor/katex/katex.min.css';\nimport { StrictMode } from 'react';",
    ),
  },
  {
    id: 'raw-latex',
    expect: FAIL_LATEX,
    what:
      'auto-render.min.js loads before katex.min.js, so it captures an undefined global and '
      + 'every problem falls back to escaped raw TeX',
    /**
     * The UMD header of the extension is `e.renderMathInElement=t(e.katex)`: it reads the
     * global AT LOAD. Reversed, `lib/katex.ts` catches the throw and shows the escaped
     * source, which is the documented graceful degradation — so the build is green, the
     * page renders, and every statement reads `$\frac{6}{8}$`.
     */
    apply: (dir) => edit(
      dir,
      'vendor-tags.ts',
      /\{ tag: 'script'([^\n]*)katex\.min\.js'([^\n]*)\},\n(\s*)\{ tag: 'script'([^\n]*)auto-render\.min\.js'([^\n]*)\},/,
      "{ tag: 'script'$4auto-render.min.js'$5 },\n$3{ tag: 'script'$1katex.min.js'$2 },",
    ),
  },
  {
    id: 'wrong-problem',
    expect: FAIL_SERVE,
    what:
      'the demo backend advances its cursor on every serve, so a second serve hands the '
      + 'learner a problem the real server never served',
    /**
     * `POST /task/{id}/serve` is idempotent on the real server and re-stamps `started_at`.
     * The 1.0 demo advanced a cursor per call, and the lesson then practised problem 2
     * while showing the worked example for problem 1. `demo.ts` keeps `cursor` read-only in
     * `taskServe` for exactly this reason; the break moves the write back into it.
     */
    apply: (dir) => edit(
      dir,
      'src/api/demo.ts',
      '      return reply(served());\n    },',
      '      const now = served();\n      cursor += 1;\n      return reply(now);\n    },',
    ),
  },
];
