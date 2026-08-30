/**
 * The design-token contract (S4) — invariant TOKENS-hex.
 *
 * WHY THIS READS THE STYLESHEET TEXT AND NOT THE DOM. jsdom resolves no CSS custom
 * property: `getComputedStyle(document.documentElement).getPropertyValue('--bg')` returns
 * the empty string, in every theme, forever. A test written against the DOM therefore
 * asserts `'' === ''` and passes with the whole palette deleted. Parsing the file is the
 * only assertion available here that can fail.
 *
 * WHAT IT PINS. Every row of the table in spec section 4.2, in both themes, as a literal.
 * The table below is transcribed from the spec, not read back out of the stylesheet, so an
 * edit to one of them disagrees with the other.
 *
 * It also holds the two rules that make one token file worth having: `app.css` declares no
 * color of its own, and the light theme redefines exactly the tokens the dark theme
 * declares — no more, and none missing.
 */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const read = (p: string) => readFileSync(resolve(process.cwd(), p), 'utf8');

const TOKENS_CSS = read('src/styles/tokens.css');
const APP_CSS = read('src/styles/app.css');
const INDEX_HTML = read('index.html');

/** Comments carry example values; none of them is a declaration. */
const stripComments = (css: string) => css.replace(/\/\*[\s\S]*?\*\//g, ' ');

/**
 * Split the file into the dark block and the light block.
 *
 * The dark `:root` is everything before the media query and the light `:root` is everything
 * inside it. That holds only while the file carries exactly ONE media query — a second one
 * would put part of the light theme in the dark bucket and every assertion below would
 * still pass. A test of its own asserts the count, because a helper that calls `expect()`
 * at module scope is a helper the runner cannot report on.
 */
function themes(css: string): { dark: string; light: string } {
  const body = stripComments(css);
  const at = body.indexOf('@media');
  return { dark: body.slice(0, at), light: body.slice(at) };
}

/**
 * Every `--name: value;` and every `color-scheme:` in one block, whitespace normalized.
 *
 * The lookbehind is load-bearing: without it `prefers-color-scheme: light) { :root {
 * color-scheme: light` matches as one `color-scheme` declaration, and the light theme
 * reports its own media condition as its value.
 */
function declarations(block: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const m of block.matchAll(/(?<![\w-])(--[\w-]+|color-scheme)\s*:\s*([^;]+);/g)) {
    out[m[1]] = m[2].replace(/\s+/g, ' ').trim();
  }
  return out;
}

const { dark: DARK_BLOCK, light: LIGHT_BLOCK } = themes(TOKENS_CSS);
const DARK = declarations(DARK_BLOCK);
const LIGHT = declarations(LIGHT_BLOCK);

/**
 * The table of spec section 4.2, transcribed. `[token, dark, light]`.
 *
 * `--muted` is lightened from Tokyo Night's own `#565f89`, which scores contrast 2.76 and
 * fails WCAG AA. `--border` scores 1.27 and is decorative only. Neither is a typo.
 */
const TABLE: ReadonlyArray<readonly [string, string, string]> = [
  ['--bg', '#1a1b26', '#e1e2e7'],
  ['--bg-dark', '#16161e', '#d0d5e3'],
  ['--surface', '#1f2335', '#ffffff'],
  ['--surface-2', '#24283b', '#d5d6db'],
  ['--text', '#c0caf5', '#24283b'],
  ['--muted', '#7e88b4', '#6b7089'],
  ['--border', '#292e42', '#c4c8da'],
  ['--accent', '#7aa2f7', '#2e7de9'],
  ['--accent-2', '#bb9af7', '#9854f1'],
  ['--accent-ink', '#1a1b26', '#ffffff'],
  ['--accent-weak', 'rgba(122, 162, 247, 0.14)', 'rgba(46, 125, 233, 0.10)'],
  ['--good', '#9ece6a', '#587539'],
  ['--good-weak', 'rgba(158, 206, 106, 0.13)', 'rgba(88, 117, 57, 0.10)'],
  ['--bad', '#f7768e', '#f52a65'],
  ['--bad-weak', 'rgba(247, 118, 142, 0.13)', 'rgba(245, 42, 101, 0.10)'],
  ['--warn', '#e0af68', '#8f5e15'],
  ['--info', '#7dcfff', '#007197'],
  ['--scrim', 'rgba(10, 12, 20, 0.55)', 'rgba(16, 24, 40, 0.35)'],
  ['--ring-bg', '#292e42', '#d5d6db'],
];

/** Defined in both themes, and not in the section 4.2 table. */
const THEMED_EXTRAS = ['--shadow'];
/** Defined once, because they do not change with the theme. */
const GEOMETRY = ['--radius', '--maxw', '--mono'];

describe('TOKENS-hex: the dark theme', () => {
  it('TOKENS-hex: the file carries exactly one media query, so the split is sound', () => {
    // The dark/light split above is a text slice at the first `@media`. A second one puts
    // part of the light theme in the dark bucket, and every hex assertion still passes.
    expect(stripComments(TOKENS_CSS).match(/@media/g)).toHaveLength(1);
  });

  it.each(TABLE)('TOKENS-hex: %s is %s in dark', (token, dark) => {
    expect(DARK[token]).toBe(dark);
  });

  it('TOKENS-hex: dark declares color-scheme: dark', () => {
    expect(DARK['color-scheme']).toBe('dark');
  });
});

describe('TOKENS-hex: the light theme', () => {
  it('TOKENS-hex: light is reached only when the OS asks for light', () => {
    // Dark is primary. A bare `prefers-color-scheme` or a `light dark` default inverts the
    // whole app for every reader who has expressed no preference.
    expect(LIGHT_BLOCK).toContain('@media (prefers-color-scheme: light)');
  });

  // Mapped to [token, light] so the reported title names the value under test. `%s` fills
  // the placeholders in order, so passing the whole row would print the DARK value beside
  // the word "light" in every one of these names.
  it.each(TABLE.map(([token, , light]) => [token, light]))(
    'TOKENS-hex: %s is %s in light',
    (token, light) => { expect(LIGHT[token]).toBe(light); },
  );

  it('TOKENS-hex: light declares color-scheme: light', () => {
    expect(LIGHT['color-scheme']).toBe('light');
  });
});

describe('TOKENS-hex: the two themes agree on their token set', () => {
  it('TOKENS-hex: every themed token is redefined in light', () => {
    const expected = [...TABLE.map(([t]) => t), ...THEMED_EXTRAS].sort();
    const inLight = Object.keys(LIGHT).filter((k) => k.startsWith('--')).sort();
    // A token declared in dark and forgotten in light keeps its dark value on a light
    // desktop — one unreadable element, and nothing reports it.
    expect(inLight).toEqual(expected);
  });

  it('TOKENS-hex: dark declares the themed tokens plus the geometry, and nothing else', () => {
    const expected = [...TABLE.map(([t]) => t), ...THEMED_EXTRAS, ...GEOMETRY].sort();
    const inDark = Object.keys(DARK).filter((k) => k.startsWith('--')).sort();
    expect(inDark).toEqual(expected);
  });

  it('TOKENS-hex: the geometry tokens are theme-independent', () => {
    expect(DARK['--radius']).toBe('14px');
    expect(DARK['--maxw']).toBe('720px');
    expect(DARK['--mono']).toContain('ui-monospace');
    // Redefining these per theme would resize the study column when the OS flips.
    for (const token of GEOMETRY) expect(LIGHT[token]).toBeUndefined();
  });
});

describe('TOKENS-hex: the palette has exactly one home', () => {
  it('TOKENS-hex: app.css writes no color literal of its own', () => {
    const body = stripComments(APP_CSS);
    const literals = [
      ...(body.match(/#[0-9a-fA-F]{3,8}\b/g) ?? []),
      ...(body.match(/\b(?:rgba?|hsla?)\(/g) ?? []),
    ];
    // Every color in app.css reads a token. One literal here and the palette starts
    // drifting a component at a time, in one theme only.
    expect(literals).toEqual([]);
  });

  it('TOKENS-hex: the browser chrome is painted with --bg in both themes', () => {
    // The two theme-color values in index.html cannot read a custom property, so they are
    // the one place the hexes are duplicated. Pin them to the same table.
    expect(INDEX_HTML).toContain(
      '<meta name="theme-color" content="#1a1b26" media="(prefers-color-scheme: dark)">',
    );
    expect(INDEX_HTML).toContain(
      '<meta name="theme-color" content="#e1e2e7" media="(prefers-color-scheme: light)">',
    );
    expect(INDEX_HTML).toContain('<meta name="color-scheme" content="dark light">');
  });
});

describe('the stylesheet contract', () => {
  const body = stripComments(APP_CSS);

  it('gives every focus ring 2px solid var(--accent) with outline-offset 2px', () => {
    const rings = body.match(/:focus-visible\s*\{[^}]*\}/g) ?? [];
    expect(rings.length).toBeGreaterThan(0);
    for (const rule of rings) {
      expect(rule).toContain('outline: 2px solid var(--accent)');
      expect(rule).toContain('outline-offset: 2px');
    }
  });

  it('gives every button a 44px minimum target height', () => {
    const btn = /\.btn\s*\{([^}]*)\}/.exec(body);
    expect(btn).not.toBeNull();
    expect(btn![1]).toContain('min-height: 44px');
  });

  it('renders every technical readout in the mono face with tabular figures', () => {
    const mono = /\.stat-value,[^{]*\{([^}]*)\}/.exec(body);
    expect(mono).not.toBeNull();
    expect(mono![1]).toContain('font-family: var(--mono)');
    expect(mono![1]).toContain('font-variant-numeric: tabular-nums');
  });

  it('strips every animation and transition under prefers-reduced-motion', () => {
    const block = /@media \(prefers-reduced-motion: reduce\)\s*\{([\s\S]*)\}/.exec(body);
    expect(block).not.toBeNull();
    expect(block![1]).toContain('animation-duration: 0.01ms !important');
    expect(block![1]).toContain('transition-duration: 0.01ms !important');
  });

  it('holds the study column at 720px and the card at the shared radius', () => {
    expect(body).toContain('max-width: var(--maxw)');
    expect(body).toContain('border-radius: var(--radius)');
    // Buttons take 10px and chips 999px, per spec section 4.2.
    expect(/\.btn\s*\{[^}]*border-radius: 10px/.test(body)).toBe(true);
    expect(/\.chip\s*\{[^}]*border-radius: 999px/.test(body)).toBe(true);
  });
});
