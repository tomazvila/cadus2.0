/**
 * The entry document itself, and the vendored files it needs.
 *
 * Every other test runs against a jsdom document that `setup.ts` builds and a
 * `renderMathInElement` that `setup.ts` stubs. That makes the whole suite blind to the one
 * thing only `index.html` can get wrong — and 1.0 got it wrong: the first version shipped
 * the KaTeX stylesheet without its two script tags, so `window.renderMathInElement` was
 * undefined in production, the render idiom took its escaped-plain-text fallback, and EVERY
 * problem, hint, feedback body and solution rendered as raw `$\dfrac{1}{2}$`. Silently,
 * because that fallback is the documented graceful degradation.
 *
 * So these assertions read the FILES. A stub cannot satisfy them.
 */
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { VENDOR_TAGS } from '../vendor-tags';

const WEB = join(dirname(fileURLToPath(import.meta.url)), '..');
const entry = readFileSync(join(WEB, 'index.html'), 'utf8');

describe('the entry document', () => {
  it('carries the three shell ids the app mounts into', () => {
    expect(entry).toContain('id="topbar"');
    expect(entry).toContain('id="view"');
    expect(entry).toContain('id="toasts"');
  });

  it('marks the toast region as a live region before its first toast', () => {
    // A region that mounts together with its content announces nothing.
    expect(entry).toMatch(/id="toasts"[^>]*aria-live="polite"/);
  });

  it('declares both themes for the browser chrome', () => {
    expect(entry).toContain('<meta name="color-scheme" content="dark light">');
    expect(entry).toContain('content="#1a1b26" media="(prefers-color-scheme: dark)"');
    expect(entry).toContain('content="#e1e2e7" media="(prefers-color-scheme: light)"');
  });

  it('links the icon as a file, never as a data: URI', () => {
    expect(entry).toContain('href="/favicon.svg"');
    expect(entry).not.toMatch(/data:[a-zA-Z][\w.+-]*\/[\w.+-]+[;,]/);
    expect(existsSync(join(WEB, 'public/favicon.svg'))).toBe(true);
  });

  it('writes no KaTeX tag of its own', () => {
    // Written as plain tags they are rewritten by Vite's HTML plugin into module-graph
    // entries. With `/vendor/**` external the stylesheet became
    // `import "/vendor/katex/katex.min.css"` at the top of the entry chunk, Chrome refused
    // a CSS file as a module, and the app rendered a BLANK PAGE with every asset
    // answering 200.
    expect(entry).not.toContain('/vendor/katex/');
  });
});

/**
 * The KaTeX tags are INJECTED by the vendorTags() plugin of vite.config.ts, so these assert
 * the injection rather than the document.
 *
 * The EMITTED order is checked where it can be seen — `scripts/check-bundle-csp.mjs`, which
 * reads dist/index.html after the build. A test over the source cannot catch it.
 */
describe('KaTeX must actually load', () => {
  const injected = VENDOR_TAGS.map((t) => ({
    tag: t.tag,
    url: 'href' in t.attrs ? t.attrs.href : t.attrs.src,
  }));

  it('injects both UMD scripts, which define the global the render idiom reads', () => {
    const scripts = injected.filter((t) => t.tag === 'script').map((t) => t.url);
    expect(scripts).toEqual(['/vendor/katex/katex.min.js', '/vendor/katex/auto-render.min.js']);
  });

  it('injects the stylesheet as a LINK, never as a script', () => {
    const css = injected.filter((t) => t.url?.endsWith('.css'));
    expect(css).toHaveLength(1);
    // A `script` here is the blank page: Chrome refuses a stylesheet as a module.
    expect(css[0].tag).toBe('link');
  });

  it('puts every tag in head-prepend, ahead of the module script Vite injects', () => {
    for (const t of VENDOR_TAGS) expect(t.injectTo).toBe('head-prepend');
  });

  it('holds every injected file in the vendored tree', () => {
    // External means Rollup stops caring whether the file exists. 1.0 shipped a build whose
    // vendor specifier the server answered 404 for, and every gate stayed green.
    for (const { url } of injected) {
      expect(existsSync(join(WEB, 'public', url!))).toBe(true);
    }
  });

  it('vendors the woff2 faces and no other font format', () => {
    // The npm package ships woff2 AND woff AND ttf: 1.5 MB of fonts where this tree needs
    // 600 KB. `font-src 'self'` also forbids a data: URI, so nothing may be inlined.
    const css = readFileSync(join(WEB, 'public/vendor/katex/katex.min.css'), 'utf8');
    expect(css).not.toMatch(/data:[a-zA-Z][\w.+-]*\/[\w.+-]+[;,]/);
    expect(existsSync(join(WEB, 'public/vendor/katex/fonts/KaTeX_Main-Regular.woff2'))).toBe(true);
    expect(existsSync(join(WEB, 'public/vendor/katex/fonts/KaTeX_Main-Regular.woff'))).toBe(false);
    expect(existsSync(join(WEB, 'public/vendor/katex/fonts/KaTeX_Main-Regular.ttf'))).toBe(false);
  });

  it('pins KaTeX 0.17.0', () => {
    const lib = readFileSync(join(WEB, 'public/vendor/katex/katex.min.js'), 'utf8');
    expect(lib).toContain('version:"0.17.0"');
  });
});
