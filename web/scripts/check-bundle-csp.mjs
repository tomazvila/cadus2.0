#!/usr/bin/env node
/**
 * CSP + asset audit of the BUILT bundle.
 *
 * The deployed CSP is (crates/web/src/security.rs:25-29):
 *   default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline';
 *   font-src 'self'; base-uri 'none'; frame-ancestors 'none';
 *   connect-src 'self' http://localhost:* http://127.0.0.1:*
 *
 * There is no `script-src`, so scripts fall back to `'self'`: no inline script, no eval.
 * BOTH halves are enforced here. `eval` and its family are token checks over the emitted
 * scripts; the two shapes of inline script — a `<script>` element with a body and no `src`,
 * and an `on*` handler attribute — are element checks over every emitted document, because
 * neither one carries a `src=` for a script-tag scan to see.
 * `font-src 'self'` has no `data:`, so an inlined font is a silent runtime failure.
 *
 * The `data:` rule here is stricter than the header: the S1 acceptance check is that the
 * build emits ZERO `data:` URIs, in script, style, and document alike. `img-src` still
 * allows one, so this gate is the only thing that keeps them out.
 *
 * IMPORTANT: this script is itself tested against a known-bad input (`--selftest`). An
 * assertion over build output that has never failed is an assertion that passes forever —
 * 1.0's first `data:` check reported FAIL on a passing build, because a shell pipeline's
 * exit status comes from its LAST command.
 */
import { readFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join, dirname, extname } from 'node:path';
import { walk } from './lib/walk.mjs';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const dist = join(root, 'dist');
const publicVendor = join(root, 'public/vendor');

/** Tokens that would need a CSP relaxation to run. */
const SCRIPT_VIOLATIONS = [
  { pattern: /\beval\s*\(/, why: "eval() needs 'unsafe-eval'" },
  { pattern: /new\s+Function\s*\(/, why: "new Function() needs 'unsafe-eval'" },
  { pattern: /\bimportScripts\s*\(/, why: 'importScripts needs a worker-src relaxation' },
  { pattern: /new\s+Worker\s*\(/, why: 'Worker() needs worker-src' },
  { pattern: /new\s+SharedWorker\s*\(/, why: 'SharedWorker() needs worker-src' },
];

/**
 * A `data:` URI, and NOT the word `data:` in a property list.
 *
 * The vendored `auto-render.min.js` carries `{data:e.slice(0)}` three times. A bare
 * /data:/ search fails the build on that, so the pattern needs the media type or the
 * `;base64` marker — the two shapes a real URI has.
 */
const DATA_URI = [
  { pattern: /\bdata:[a-zA-Z][\w.+-]*\/[\w.+-]+[;,]/, why: 'a data: URI (set assetsInlineLimit: 0)' },
  { pattern: /\bdata:;base64,/, why: 'an untyped base64 data: URI' },
];

/**
 * An HTML comment, removed before every element scan.
 *
 * `index.html` explains the vendor-tag injection in prose that spells `<script src>`. A
 * scanner that reads comments pairs that text with the NEXT real `</script>` and reports
 * the entry module as an inline body. A gate that fails a correct build teaches its reader
 * to turn the gate off, so the comments come out first.
 */
const HTML_COMMENT = /<!--[\s\S]*?-->/g;

/** One start tag, with its attribute text. A quoted attribute value may hold `>`. */
const TAG = /<([a-zA-Z][\w:-]*)((?:"[^"]*"|'[^']*'|[^>"'])*)>/g;

/**
 * One `name=value` attribute.
 *
 * The value is consumed WHOLE, quotes and all, so `data-note="onclick=x"` reports one
 * attribute named `data-note` and not a handler.
 */
const ATTR = /([a-zA-Z_:][\w:.-]*)\s*=\s*(?:"[^"]*"|'[^']*'|[^\s"'>]*)/g;

/** One `<script>` element, with its attribute text and its body. */
const SCRIPT_ELEMENT = /<script\b((?:"[^"]*"|'[^']*'|[^>"'])*)>([\s\S]*?)<\/script\s*>/gi;

const attributeNames = (attrs) => [...attrs.matchAll(ATTR)].map((a) => a[1].toLowerCase());

/**
 * Every inline script body in one document: a `<script>` with a non-empty body and no
 * `src`. An empty element is not one — Vite emits none, and a body is what runs.
 */
export function inlineScriptBodies(html) {
  const found = [];
  for (const m of html.replace(HTML_COMMENT, '').matchAll(SCRIPT_ELEMENT)) {
    if (attributeNames(m[1]).includes('src')) continue;
    const body = m[2].trim();
    if (body !== '') found.push(body.slice(0, 60));
  }
  return found;
}

/** Every inline event handler attribute in one document: `onclick=`, `onload=`, and rest. */
export function inlineHandlerAttributes(html) {
  const found = [];
  for (const tag of html.replace(HTML_COMMENT, '').matchAll(TAG)) {
    for (const name of attributeNames(tag[2])) {
      if (/^on[a-z]/.test(name)) found.push(`<${tag[1].toLowerCase()} ${name}=>`);
    }
  }
  return found;
}

/** The two inline shapes `default-src 'self'` refuses. Documents only. */
const INLINE_SCRIPT = [
  {
    find: (t) => inlineScriptBodies(t).length > 0,
    why: "an inline <script> body needs 'unsafe-inline'",
  },
  {
    find: (t) => inlineHandlerAttributes(t).length > 0,
    why: "an inline event handler attribute needs 'unsafe-inline'",
  },
];

const RULES = {
  '.js': [...SCRIPT_VIOLATIONS, ...DATA_URI],
  '.mjs': [...SCRIPT_VIOLATIONS, ...DATA_URI],
  '.css': DATA_URI,
  '.html': [...DATA_URI, ...INLINE_SCRIPT],
};

/** Does one rule fire on this text? A rule carries a `pattern` OR a `find`. */
function fires(rule, text) {
  return rule.pattern ? rule.pattern.test(text) : rule.find(text);
}

export function audit(files, read) {
  const problems = [];
  for (const file of files) {
    const rules = RULES[extname(file)];
    if (!rules) continue;
    const text = read(file);
    for (const rule of rules) {
      if (fires(rule, text)) problems.push({ file, why: rule.why });
    }
  }
  return problems;
}

/**
 * Every `/vendor/...` path the sources ask for.
 *
 * DISCOVERED, never hand-written. `vite.config.ts` marks `/vendor/**` EXTERNAL, which is
 * correct — the specifier must survive into the bundle as a runtime url — but external
 * means Rollup also stops caring whether the file exists. 1.0 shipped a build that emitted
 * a correct `import("/vendor/cytoscape/cytoscape.esm.min.mjs")` at a path the server
 * answered 404 for: six views fine, the curriculum map dead, and every gate green. A
 * hand-written list is exactly what let that through.
 */
const VENDOR_REF = /["'(](\/vendor\/[A-Za-z0-9._/-]+)["')]/g;

export function referencedVendorPaths(sources, read) {
  const found = new Set();
  for (const file of sources) {
    for (const m of read(file).matchAll(VENDOR_REF)) found.add(m[1].slice(1));
  }
  return [...found].sort();
}

// --- self-test: prove the check can actually fail -----------------------------
if (process.argv.includes('--selftest')) {
  const fails = [];

  const bad = {
    '/x/bad.js': 'const f = new Function("return 1"); eval("2");',
    '/x/bad.css': '@font-face{src:url(data:font/woff2;base64,AA)}',
    '/x/bad.html': '<link rel="icon" href="data:image/svg+xml,%3Csvg%3E">',
    // The two shapes with no `src=` on them. Neither one reaches a script-tag scan, and
    // both run under `default-src 'self'` only with `'unsafe-inline'` added to the header.
    '/x/inline-body.html': '<head><script>window.__boot = 1;</script></head>',
    '/x/inline-handler.html': '<body><button onclick="go()">Go</button></body>',
  };
  const found = audit(Object.keys(bad), (f) => bad[f]);
  const flagged = new Set(found.map((p) => p.file));
  const why = (f) => found.filter((p) => p.file === f).map((p) => p.why).join('; ');
  if (!flagged.has('/x/bad.js')) fails.push('new Function()/eval() in a script was not caught');
  if (!flagged.has('/x/bad.css')) fails.push('a data: font in a stylesheet was not caught');
  if (!flagged.has('/x/bad.html')) fails.push('a data: icon in the document was not caught');
  if (!/inline <script> body/.test(why('/x/inline-body.html'))) {
    fails.push('an inline <script> body in the document was not caught');
  }
  if (!/inline event handler/.test(why('/x/inline-handler.html'))) {
    fails.push('an inline event handler attribute in the document was not caught');
  }

  const clean = {
    '/x/ok.js': 'const a = 1; const o = {data: a.slice(0)};',
    '/x/ok.css': '@font-face{src:url(/vendor/katex/fonts/KaTeX_Main-Regular.woff2)}',
    '/x/ok.html': '<link rel="icon" href="/favicon.svg">',
    // The document the build really emits: two deferred vendor tags, the hashed entry
    // module, a comment that spells `<script src>` in prose, an EMPTY script element, and
    // an attribute whose VALUE holds the text `onclick=`. None of the five is a violation,
    // and a scanner that reports any of them fails a correct build.
    '/x/ok-doc.html': '<!-- Vite rewrites a <script src> tag into a module graph entry. -->'
      + '<script defer src="/vendor/katex/katex.min.js"></script>'
      + '<script type="module" crossorigin src="/assets/index-B0ILUY8-.js"></script>'
      + '<script></script>'
      + '<div data-note="onclick=never" class="one"></div>',
  };
  const noise = audit(Object.keys(clean), (f) => clean[f]);
  if (noise.length !== 0) {
    fails.push(`clean input reported ${noise.length} violations: ${noise.map((p) => p.why).join(', ')}`);
  }

  // The discovery half. A build that emits a `/vendor/**` specifier the tree does not hold
  // is the failure mode this gate exists for, so the finder itself is tested.
  const refs = referencedVendorPaths(['/x/src.ts', '/x/index.html'], (f) =>
    f.endsWith('.ts')
      ? 'await import("/vendor/cytoscape/cytoscape.esm.min.mjs");'
      : '<link href="/vendor/katex/katex.min.css">');
  if (refs.join(',') !== 'vendor/cytoscape/cytoscape.esm.min.mjs,vendor/katex/katex.min.css') {
    fails.push(`vendor discovery returned [${refs.join(', ')}]`);
  }
  if (referencedVendorPaths(['/x/none.ts'], () => 'const a = 1;').length !== 0) {
    fails.push('vendor discovery invented a path from source that references none');
  }

  if (fails.length) {
    console.error('SELFTEST FAIL:');
    for (const f of fails) console.error(`  ${f}`);
    process.exit(1);
  }
  console.log('csp selftest: PASS (catches eval, new Function, data: in css/js/html, an inline script body and an inline handler attribute; passes clean input; discovers vendor refs)');
  process.exit(0);
}

if (!existsSync(dist)) {
  console.error('FAIL — dist/ is absent. Run `npm run build` before the CSP audit.');
  process.exit(1);
}

const files = walk(dist);
const read = (f) => readFileSync(f, 'utf8');
const problems = audit(files, read);

// The sources that may name a `/vendor/**` path: the document, the tag list the build
// injects, and everything under src/.
const sources = [join(root, 'index.html'), join(root, 'vendor-tags.ts'), ...walk(join(root, 'src'))]
  .filter((f) => ['.html', '.ts', '.tsx', '.css'].includes(extname(f)));
const required = referencedVendorPaths(sources, read);
const missing = required.filter((r) => !existsSync(join(dist, r)));

// `public/vendor` is the ONE tree — Vite copies it into `dist/` verbatim, so the two are
// the same bytes by construction rather than by policy.
//
// The comparison stays anyway, and it is not redundant: it proves the COPY RAN. A
// `publicDir` that stops matching, an `emptyOutDir` that wipes the tree after the copy, a
// changed output path — each produces a dist/ that is missing or stale, and each is what
// this file exists to catch.
const drift = [];
const builtVendor = join(dist, 'vendor');

if (!existsSync(builtVendor)) {
  drift.push('dist/vendor is absent — the public/ copy did not run');
} else {
  const rel = (base, p) => p.slice(base.length + 1);
  const sourceFiles = new Map(walk(publicVendor).map((p) => [rel(publicVendor, p), p]));
  const distFiles = new Map(walk(builtVendor).map((p) => [rel(builtVendor, p), p]));

  for (const [name, p] of sourceFiles) {
    const other = distFiles.get(name);
    if (!other) { drift.push(`${name} — in public/vendor, missing from the build`); continue; }
    if (!readFileSync(p).equals(readFileSync(other))) drift.push(`${name} — bytes differ`);
  }
  for (const name of distFiles.keys()) {
    if (!sourceFiles.has(name)) drift.push(`${name} — in the build, missing from public/vendor`);
  }
}

console.log(`csp audit: ${files.length} built files scanned`);

if (problems.length) {
  console.error('\nFAIL — CSP violations in the built bundle:');
  for (const { file, why } of problems) console.error(`  ${file.replace(dist, 'dist')} — ${why}`);
}
if (missing.length) {
  console.error('\nFAIL — the sources name these vendor paths, and the build does not hold them:');
  for (const m of missing) console.error(`  dist/${m}`);
}
if (drift.length) {
  console.error('\nFAIL — dist/vendor has drifted from public/vendor:');
  for (const d of drift) console.error(`  ${d}`);
}

// The EMITTED script order, which no test over index.html can see.
//
// Vite hoists its module script into <head>. Deferred classic scripts and module scripts
// execute in DOCUMENT order, so KaTeX tags left at the end of <body> — `defer` and all —
// run AFTER the application module.
const order = [];
const orderProblems = [];
const html = readFileSync(join(dist, 'index.html'), 'utf8');
for (const m of html.matchAll(/<script[^>]*\ssrc="([^"]+)"/g)) order.push(m[1]);

// A JS chunk must never IMPORT a stylesheet. Vite rewrites `<link rel="stylesheet">` in
// index.html into a module-graph entry so it can hash and bundle it, and with `/vendor/**`
// external that came out as `import "/vendor/katex/katex.min.css"` at the top of the entry
// chunk. Chrome refuses a CSS file as a module, the entry never evaluates, and the app is a
// BLANK PAGE — with every asset answering 200, so no HTTP check and no jsdom test sees it.
for (const file of files.filter((f) => extname(f) === '.js' || extname(f) === '.mjs')) {
  for (const m of read(file).matchAll(/\bimport\s*\(?\s*["']([^"']+\.css)["']/g)) {
    orderProblems.push(`${file.slice(dist.length + 1)} imports a stylesheet as a module: ${m[1]}`);
  }
}

if (!/<link[^>]+rel="stylesheet"[^>]+\/vendor\/katex\/katex\.min\.css/.test(html)) {
  orderProblems.push('dist/index.html does not link the vendored KaTeX stylesheet');
}

const katexAt = order.findIndex((sr) => sr.includes('/vendor/katex/katex.min.js'));
const autoAt = order.findIndex((sr) => sr.includes('/vendor/katex/auto-render.min.js'));
const appAt = order.findIndex((sr) => sr.includes('/assets/'));
if (katexAt === -1 || autoAt === -1) orderProblems.push('dist/index.html loads no KaTeX UMD script');
else {
  if (appAt !== -1 && (appAt < katexAt || appAt < autoAt)) {
    orderProblems.push('the app module is emitted BEFORE the KaTeX globals it depends on');
  }
  // auto-render is a KaTeX EXTENSION: it reads `katex.ParseError` at load. Loaded first it
  // captures `undefined`, every render throws, the app falls back to escaped plain text,
  // and every problem prints raw `$\dfrac{1}{2}$` — silently. jsdom stubs the global, so no
  // test in the suite sees the order; only the built document shows it.
  if (autoAt < katexAt) {
    orderProblems.push('auto-render.min.js is emitted BEFORE katex.min.js, the global it reads');
  }
}

if (orderProblems.length) {
  console.error('\nFAIL — script order in dist/index.html:');
  for (const o of orderProblems) console.error(`  ${o}`);
}

if (problems.length || missing.length || drift.length || orderProblems.length) process.exit(1);

console.log(
  `csp audit: PASS (no eval/Function/Worker, no inline script, no data: URIs, ${required.length} `
  + 'referenced vendor files present and byte-identical to public/vendor)',
);
