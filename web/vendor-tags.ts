/**
 * The vendored KaTeX tags the built document must carry.
 *
 * Its OWN module, imported by both `vite.config.ts` and `test/document.test.ts`. The test
 * cannot import the Vite config directly — that pulls Vite into a jsdom environment and
 * breaks its TextEncoder — and the list must not exist in two places.
 *
 * WHY THEY ARE INJECTED RATHER THAN WRITTEN IN index.html. Vite's HTML plugin rewrites
 * `<link rel="stylesheet">` and `<script src>` into module-graph entries so it can hash and
 * bundle them. `/vendor/**` is external, so the stylesheet came out the other side as
 * `import "/vendor/katex/katex.min.css"` at the top of the entry chunk. Chrome refuses a CSS
 * file as a module — "Expected a JavaScript-or-Wasm module script" — the entry never
 * evaluates, and the app renders a BLANK PAGE while every asset answers 200. No HTTP check
 * and no jsdom test sees that; a real browser sees it immediately.
 *
 * `head-prepend` also puts the tags ahead of the module script Vite injects, which is the
 * order the KaTeX idiom needs: it reads `window.renderMathInElement`, and without it every
 * problem, hint, feedback body and solution renders as raw `$\dfrac{1}{2}$`.
 *
 * ORDER IS LOAD-BEARING: `katex.min.js` first, `auto-render.min.js` second. auto-render is a
 * KaTeX extension that reads `katex.ParseError` at load; reversed, it captures `undefined`,
 * every render throws, and the only symptom is raw LaTeX on every problem.
 */
export const VENDOR_TAGS = [
  { tag: 'link', attrs: { rel: 'stylesheet', href: '/vendor/katex/katex.min.css' }, injectTo: 'head-prepend' },
  { tag: 'script', attrs: { defer: true, src: '/vendor/katex/katex.min.js' }, injectTo: 'head-prepend' },
  { tag: 'script', attrs: { defer: true, src: '/vendor/katex/auto-render.min.js' }, injectTo: 'head-prepend' },
] as const;
